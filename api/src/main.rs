extern crate core;

use api::auth::AUTH_ROUTER;
use api::cleanup::start_cleanup_scheduler;
use api::games::GAMES_ROUTER;
use api::users::{USERS_PROTECTED_ROUTER, USERS_PUBLIC_ROUTER};
use api::{AppState, AuthDb};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::http::header::{
    ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_MAX_AGE, AUTHORIZATION, CACHE_CONTROL,
    CONTENT_TYPE, EXPIRES, HeaderName,
};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Json, Router, middleware};
use base64_url::decode;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::hash::{Hash, Hasher};
use std::string::String;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::opt::auth::{Root, Token};
use time::OffsetDateTime;
use tower::ServiceBuilder;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::KeyExtractor;
use tower_http::cors::CorsLayer;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod routes;
use routes::*;

// Page-rendering helpers live in the library (`api::pages`) so integration
// tests can mount the same handlers the binary serves; re-exported here so
// the handlers in `routes/` keep importing them through `crate::`.
pub use api::pages::{authenticate_db, extract_auth, html_with_csrf, validate_csrf};

pub static DATABASE: LazyLock<Arc<Surreal<Any>>> = LazyLock::new(|| Arc::new(Surreal::init()));

/// Cooldown cache for resend verification requests.
/// Key: "resend:<email>", Value: Instant of last send.
static RESEND_COOLDOWN: LazyLock<Mutex<HashMap<String, std::time::Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn initialize_logging() {
    // a layer that logs events to stdout
    let stdout_log = tracing_subscriber::fmt::layer().pretty();

    let production = env::var("PRODUCTION").unwrap_or("true".to_string());
    let tracing_level = if production == "true" {
        "info"
    } else {
        "debug"
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("{}={tracing_level},tower_http={tracing_level},surrealdb={tracing_level},surrealdb_client={tracing_level}",
                        env!("CARGO_CRATE_NAME")).into()
            })
        )
        .with(stdout_log)
        .init()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_logging();

    let surreal_host =
        env::var("SURREAL_HOST").map_err(|_| "SURREAL_HOST environment variable not set")?;
    let db = Arc::new(
        surrealdb::engine::any::connect(surreal_host)
            .await
            .map_err(|e| format!("Failed to connect to database: {}", e))?,
    );
    tracing::debug!("connected to SurrealDB");

    let surreal_user =
        env::var("SURREAL_USER").map_err(|_| "SURREAL_USER environment variable not set")?;
    let surreal_pass =
        env::var("SURREAL_PASS").map_err(|_| "SURREAL_PASS environment variable not set")?;

    db.signin(Root {
        username: surreal_user.clone(),
        password: surreal_pass.clone(),
    })
    .await
    .map_err(|e| format!("Failed to authenticate to database: {}", e))?;
    tracing::debug!("authenticated to SurrealDB");

    let surreal_namespace =
        env::var("APP_SURREAL_NAMESPACE").unwrap_or_else(|_| "gobblin-games".to_string());
    let surreal_database = env::var("APP_SURREAL_DATABASE").unwrap_or_else(|_| "games".to_string());

    db.use_ns(&surreal_namespace)
        .use_db(&surreal_database)
        .await
        .map_err(|e| format!("Failed to use database: {}", e))?;
    tracing::debug!(
        "Using '{}' namespace and '{}' database",
        surreal_namespace,
        surreal_database
    );

    // ── Schema & migration helper ──────────────────────────────────────
    async fn apply_files(
        db: &Surreal<Any>,
        dir_rel: &str,
        kind: &str,
        critical: &[&str],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir_abs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(dir_rel);
        let mut entries = match tokio::fs::read_dir(&dir_abs).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Cannot read {dir_rel} directory: {e}");
                return Ok(());
            }
        };
        let mut files = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| format!("Failed to read entry in {dir_rel}: {e}"))?
        {
            if entry
                .file_type()
                .await
                .map(|t| t.is_file())
                .unwrap_or(false)
                && let Some(name) = entry.file_name().to_str()
                && name.ends_with(".surql")
            {
                files.push(name.to_string());
            }
        }
        files.sort();
        for fname in &files {
            let path = dir_abs.join(fname);
            let sql = tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| format!("Failed to read {dir_rel}/{fname}: {e}"))?;
            if let Err(e) = db.query(&sql).await {
                tracing::error!("Failed to apply {kind} {fname}: {e}");
                if critical.contains(&fname.as_str()) {
                    return Err(format!("Critical {kind} {fname} failed: {e}").into());
                }
            } else {
                tracing::info!("Applied {kind}: {fname}");
            }
        }
        Ok(())
    }

    tracing::info!("Applying schema definitions...");
    apply_files(
        &db,
        "../schemas",
        "schema",
        &["users.surql", "refresh_tokens.surql"],
    )
    .await?;

    tracing::info!("Applying migrations...");
    apply_files(&db, "../migrations", "migration", &[]).await?;

    // CORS Configuration
    let env_mode = env::var("ENV").unwrap_or_else(|_| "production".to_string());
    let is_production = env_mode == "production";

    let allowed_origins_str = env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:8080,http://127.0.0.1:8080".to_string());

    let allowed_origins: Vec<String> = allowed_origins_str
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    // Validate CORS configuration in production
    if is_production {
        for origin in &allowed_origins {
            if origin == "*" || origin.contains("*") {
                return Err(format!(
                    "Wildcard CORS origins are not allowed in production. Found: {}. \
                    Please set ALLOWED_ORIGINS to specific domains.",
                    origin
                )
                .into());
            }
        }
        tracing::info!(
            "Production CORS validation passed. Allowed origins: {:?}",
            allowed_origins
        );
    } else {
        tracing::debug!("Development mode. Allowed origins: {:?}", allowed_origins);
    }

    // These are safe to unwrap as they are static HTTP method strings that are guaranteed valid
    let cors_layer = CorsLayer::new()
        .allow_methods(vec![
            "DELETE".parse()?,
            "GET".parse()?,
            "HEAD".parse()?,
            "OPTIONS".parse()?,
            "POST".parse()?,
            "PUT".parse()?,
        ])
        .allow_origin(
            allowed_origins
                .iter()
                .map(|o| o.parse())
                .collect::<Result<Vec<_>, _>>()?,
        )
        .allow_credentials(true)
        .allow_headers([
            ACCEPT,
            ACCEPT_ENCODING,
            ACCEPT_LANGUAGE,
            ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN,
            ACCESS_CONTROL_MAX_AGE,
            AUTHORIZATION,
            CACHE_CONTROL,
            CONTENT_TYPE,
            EXPIRES,
        ]);

    // Rate Limiting Configuration
    let rate_limit_per_second = env::var("RATE_LIMIT_PER_SECOND")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(2); // Default: 2 requests per second (~120 per minute)

    let rate_limit_burst = env::var("RATE_LIMIT_BURST")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(50); // Default: burst size of 50

    tracing::info!(
        "Rate limiting configured: {} req/sec, burst size: {}",
        rate_limit_per_second,
        rate_limit_burst
    );

    let governor_config = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(rate_limit_per_second)
            .burst_size(rate_limit_burst)
            .key_extractor(CompoundKeyExtractor)
            .finish()
            .ok_or("Failed to build GovernorConfig")?,
    );

    // Initialize storage backend
    use api::storage::LocalStorage;
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let storage_path = env::var("STORAGE_PATH").unwrap_or_else(|_| "uploads".to_string());
    let storage = Arc::new(LocalStorage::new(&storage_path, "/uploads"));
    storage.init().await?;
    tracing::info!("Storage initialized at: {}", storage_path);

    // Initialize WebSocket broadcaster
    use api::websocket::GameBroadcaster;
    let broadcaster = Arc::new(GameBroadcaster::default());
    tracing::info!("WebSocket broadcaster initialized");

    let app_state = AppState {
        db: db.clone(),
        storage,
        broadcaster,
        commentator: Some(
            Arc::new(announcers::Chronicler::new()) as Arc<dyn announcers::Commentator>
        ),
        namespace: surreal_namespace,
        database: surreal_database,
    };

    // Start cleanup scheduler for refresh tokens
    let _cleanup_scheduler = start_cleanup_scheduler(app_state.clone())
        .await
        .map_err(|e| format!("Failed to start cleanup scheduler: {}", e))?;
    tracing::info!("Cleanup scheduler initialized");

    let api_routes =
        Router::new()
            .route(
                "/version",
                axum::routing::get(|| async { Json(env!("CARGO_PKG_VERSION")) }),
            )
            .nest(
                "/games",
                GAMES_ROUTER.clone().layer(middleware::from_fn_with_state(
                    app_state.clone(),
                    surreal_jwt,
                )),
            )
            .nest(
                "/characters",
                api::characters::OWNED_CHARACTERS_ROUTER.clone().layer(
                    middleware::from_fn_with_state(app_state.clone(), surreal_jwt),
                ),
            )
            .nest("/users", USERS_PUBLIC_ROUTER.clone())
            .nest(
                "/users",
                USERS_PROTECTED_ROUTER
                    .clone()
                    .layer(middleware::from_fn_with_state(
                        app_state.clone(),
                        surreal_jwt,
                    )),
            )
            .nest("/auth", AUTH_ROUTER.clone());

    let router = Router::new()
        .nest("/api", api_routes)
        .nest_service(
            "/uploads",
            tower_http::services::ServeDir::new(&storage_path),
        )
        .nest_service(
            "/assets",
            tower_http::services::ServeDir::new(
                std::path::PathBuf::from(&manifest_dir)
                    .join("assets")
                    .join("dist"),
            ),
        )
        .nest_service(
            "/icons",
            tower_http::services::ServeDir::new(
                std::path::PathBuf::from(&manifest_dir)
                    .join("assets")
                    .join("icons"),
            ),
        )
        .route(
            "/ws",
            axum::routing::get(api::websocket::websocket_handler).layer(
                middleware::from_fn_with_state(app_state.clone(), surreal_jwt),
            ),
        )
        // TODO: Move to GAMES_ROUTER?
        .route(
            "/api/games/{game_id}/events",
            axum::routing::get(api::sse::sse_handler).layer(middleware::from_fn_with_state(
                app_state.clone(),
                surreal_jwt,
            )),
        )
        .route("/", axum::routing::get(home_handler))
        .route("/account", axum::routing::get(account_handler))
        .route(
            "/account/settings",
            axum::routing::get(account_settings_handler),
        )
        .route(
            "/my-goblins",
            axum::routing::get(my_goblins_handler).post(my_goblins_create_handler),
        )
        .route(
            "/my-goblins/{identifier}",
            axum::routing::post(my_goblins_edit_handler),
        )
        .route(
            "/my-goblins/{identifier}/delete",
            axum::routing::post(my_goblins_delete_handler),
        )
        .route("/auth", axum::routing::get(auth_handler))
        .route("/auth/login", axum::routing::post(login_post_handler))
        .route("/auth/logout", axum::routing::post(logout_handler))
        .route("/auth/register", axum::routing::post(register_post_handler))
        .route("/auth/check-email", axum::routing::get(check_email_handler))
        .route(
            "/auth/verify-email",
            axum::routing::get(verify_email_handler),
        )
        .route(
            "/auth/resend-verification",
            axum::routing::post(resend_verification_handler),
        )
        .route(
            "/auth/email-verified",
            axum::routing::get(email_verified_handler),
        )
        .route("/games", axum::routing::get(games_list_handler))
        .route("/games/{id}", axum::routing::get(game_detail_handler))
        .route(
            "/games/{id}/join",
            axum::routing::post(game_join_post_handler),
        )
        .route("/games/{id}/areas", axum::routing::get(game_areas_handler))
        .route("/games/{id}/log", axum::routing::get(game_log_handler))
        .route("/games/{id}/timeline", axum::routing::get(timeline_handler))
        .route(
            "/games/{id}/characters",
            axum::routing::get(game_characters_handler),
        )
        .route(
            "/games/{game_id}/characters/{character_id}",
            axum::routing::get(game_character_detail_handler),
        )
        .route(
            "/games/new",
            axum::routing::get(create_game_handler).post(create_game_post_handler),
        )
        .route(
            "/health",
            axum::routing::get(|State(state): State<AppState>| async move {
                let db_status = match state.db.health().await {
                    Ok(_) => "connected",
                    Err(_) => "disconnected",
                };
                Json(serde_json::json!({
                    "status": "ok",
                    "version": env!("CARGO_PKG_VERSION"),
                    "db": db_status
                }))
            }),
        )
        .route(
            "/dev/verify-email",
            axum::routing::post(dev_verify_email_handler),
        )
        .with_state(app_state)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(middleware::from_fn(add_rate_limit_headers))
                .layer(GovernorLayer::new(governor_config))
                .layer(cors_layer)
                .into_inner(),
        );

    let port: u16 = std::env::var("API_PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .map_err(|e| format!("Invalid API_PORT: {}", e))?;
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .map_err(|e| format!("Failed to bind to 0.0.0.0:{}: {}", port, e))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local address: {}", e))?;
    tracing::info!("listening on {}", local_addr);

    // Wrap the router in NormalizePathLayer so e.g. `/api/users/` is rewritten
    // to `/api/users` BEFORE route matching. Axum's per-route layers can't do
    // this — the layer must sit outside the Router.
    use axum::ServiceExt;
    use tower::Layer;
    let app = NormalizePathLayer::trim_trailing_slash().layer(router);

    axum::serve(listener, ServiceExt::<Request>::into_make_service(app))
        .await
        .map_err(|e| format!("Server error: {}", e))?;

    Ok(())
}

async fn surreal_jwt(State(state): State<AppState>, request: Request, next: Next) -> Response {
    // Prefer the HttpOnly `gg_session` cookie (browsers attach it on every
    // same-site request, including the WebSocket upgrade). Fall back to
    // `Authorization: Bearer …` so non-browser clients (tests, scripts) still
    // work.
    let token = api::cookies::read_cookie(request.headers(), api::cookies::SESSION_COOKIE)
        .map(|s| s.to_owned())
        .or_else(|| {
            request
                .headers()
                .get(AUTHORIZATION)
                .and_then(|h| h.to_str().ok())
                .and_then(|h| h.strip_prefix("Bearer "))
                .map(|s| s.to_owned())
        });
    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let token_parts: Vec<&str> = token.split('.').collect();
    if token_parts.len() != 3 {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let payload_base64 = token_parts[1].trim_start_matches("=");
    let payload_bytes = decode(payload_base64).map_err(|_| ()).unwrap_or_default();
    let payload_str = String::from_utf8(payload_bytes).unwrap_or_default();
    let payload: Value = serde_json::from_str(&payload_str).unwrap_or_default();

    let exp = payload["exp"].as_u64().unwrap_or_default();
    let now = OffsetDateTime::now_utc().unix_timestamp() as u64;
    if exp < now {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let jwt = Token::from(token.as_str());
    // Per-request session: clone the shared connection (independent
    // session state, same underlying socket per SurrealDB Rust SDK 2.x
    // multi-tenancy) and authenticate the clone. The original
    // root-authenticated `state.db` is untouched, so concurrent requests
    // can no longer race on `$auth`. The clone is injected as a request
    // extension so handlers (extractor `AuthDb`) see it (replaces the
    // global `auth_lock`).
    let user_db = (*state.db).clone();
    if user_db
        .use_ns(&state.namespace)
        .use_db(&state.database)
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if user_db.authenticate(jwt).await.is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut request = request;
    request.extensions_mut().insert(AuthDb(user_db));
    next.run(request).await
}

/// Middleware to add rate limit headers to responses
async fn add_rate_limit_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;

    // Read rate limit config from environment (same as main config)
    let rate_limit_per_second = env::var("RATE_LIMIT_PER_SECOND")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(2);
    let rate_limit_burst = env::var("RATE_LIMIT_BURST")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(50);

    // Calculate the limit per minute for header
    let limit_per_minute = rate_limit_per_second * 60;

    // Add rate limit headers
    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static("x-ratelimit-limit"),
        limit_per_minute
            .to_string()
            .parse()
            .unwrap_or_else(|_| "120".parse().unwrap()),
    );

    // Note: We can't easily get the remaining count from tower-governor without more complex integration
    // For now, we'll document the limit. A future enhancement could track this more precisely.
    headers.insert(
        HeaderName::from_static("x-ratelimit-burst"),
        rate_limit_burst
            .to_string()
            .parse()
            .unwrap_or_else(|_| "50".parse().unwrap()),
    );

    response
}

/// Rate limiting key extractor that uses IP + user_id for JWT-protected routes
/// and IP only for public routes.
#[derive(Clone, Copy, Debug)]
struct CompoundKeyExtractor;

impl KeyExtractor for CompoundKeyExtractor {
    type Key = u64;

    fn extract<T>(
        &self,
        request: &axum::http::Request<T>,
    ) -> Result<Self::Key, tower_governor::GovernorError> {
        // Extract IP from remote_addr
        let ip = request
            .extensions()
            .get::<axum::extract::connect_info::ConnectInfo<std::net::SocketAddr>>()
            .map(|ci| ci.0.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Try to extract user_id from JWT payload in Authorization header
        let user_id = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .and_then(|token| {
                let token_parts: Vec<&str> = token.split('.').collect();
                if token_parts.len() == 3 {
                    let payload_base64 = token_parts[1].trim_start_matches('=');
                    decode(payload_base64)
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                } else {
                    None
                }
            })
            .and_then(|payload_str| serde_json::from_str::<Value>(&payload_str).ok())
            .and_then(|payload| payload["sub"].as_str().map(String::from))
            .unwrap_or_default();

        // Combine IP and user_id into a single key using hash
        let mut hasher = DefaultHasher::new();
        ip.hash(&mut hasher);
        user_id.hash(&mut hasher);
        Ok(hasher.finish())
    }
}

/// Redirect to an auth tab with an error message in the query string.
/// Uses `url::form_urlencoded` for robust query construction and
/// percent-encoding. Handles paths that already contain query strings.
fn redirect_with_error(path: &str, tab: &str, error: &str) -> Response {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("tab", tab)
        .append_pair("error", error)
        .finish();
    let separator = if path.contains('?') { "&" } else { "?" };
    Redirect::to(&format!("{}{}{}", path, separator, query)).into_response()
}
fn urlencoding(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push_str("%20"),
            _ => {
                for b in c.to_string().bytes() {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    result
}
