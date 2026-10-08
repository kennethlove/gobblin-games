//! Server-rendered page handlers and the request plumbing they share.
//!
//! `extract_auth`, `html_with_csrf`, `validate_csrf`, and
//! `authenticate_db` live here so the page handlers built on them can
//! also be mounted by integration
//! tests, which link this library and not the binary. The binary
//! re-exports them (`pub use api::pages::…`) so the handlers in
//! `routes/` keep importing them through `crate::` unchanged.

use crate::auth::require_auth;
use crate::characters::{
    CreateOwnedCharacter, EditOwnedCharacter, create_owned_character, owned_character_delete,
    owned_character_update, resolve_owned_character_identifier,
};
use crate::cookies::{CSRF_COOKIE, SESSION_COOKIE, generate_csrf_token, read_cookie};
use crate::templates::{AuthState, tera_engine};
use crate::{AppState, AuthDb};
use axum::Json;
use axum::extract::{Extension, Form, Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use characters::Character;
use serde::Deserialize;
use serde_json::Value;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::opt::auth::Token;
use time::OffsetDateTime;

// ── Page plumbing ─────────────────────────────

/// Render an HTML string as a response with the CSRF cookie attached.
pub fn html_with_csrf(html: String, csrf: &str) -> Response {
    let mut response = axum::response::Html(html).into_response();
    crate::cookies::set_csrf_cookie(&mut response, csrf);
    response
}

/// Compare the form's CSRF token against the `gg_csrf` cookie.
pub fn validate_csrf(headers: &axum::http::HeaderMap, form_token: &str) -> bool {
    let cookie_token = read_cookie(headers, CSRF_COOKIE);
    cookie_token.is_some_and(|t| t == form_token)
}

/// Extract authentication state from request cookies, paired with the CSRF token.
///
/// The CSRF token is read from the existing `gg_csrf` cookie when present and
/// only minted fresh when no cookie exists. This keeps the token stable across
/// tabs and page loads so a form rendered on one page still validates after
/// the user navigates elsewhere and back.
pub fn extract_auth(headers: &axum::http::HeaderMap) -> (AuthState, String) {
    let csrf = read_cookie(headers, CSRF_COOKIE)
        .map(|s| s.to_owned())
        .unwrap_or_else(generate_csrf_token);
    let auth = extract_auth_state(headers, &csrf);
    (auth, csrf)
}

fn extract_auth_state(headers: &axum::http::HeaderMap, csrf: &str) -> AuthState {
    let token = match read_cookie(headers, SESSION_COOKIE) {
        Some(t) => t.to_owned(),
        None => return AuthState::guest(csrf),
    };

    let token_parts: Vec<&str> = token.split('.').collect();
    if token_parts.len() != 3 {
        return AuthState::guest(csrf);
    }

    let payload_base64 = token_parts[1].trim_start_matches('=');
    let payload_bytes = match base64_url::decode(payload_base64) {
        Ok(b) => b,
        Err(_) => return AuthState::guest(csrf),
    };

    let payload_str = match String::from_utf8(payload_bytes) {
        Ok(s) => s,
        Err(_) => return AuthState::guest(csrf),
    };

    let payload: Value = match serde_json::from_str(&payload_str) {
        Ok(v) => v,
        Err(_) => return AuthState::guest(csrf),
    };

    let exp = payload.get("exp").and_then(|v| v.as_u64()).unwrap_or(0);
    let now = OffsetDateTime::now_utc().unix_timestamp() as u64;
    if exp < now {
        return AuthState::guest(csrf);
    }

    let id = payload.get("id").and_then(|v| v.as_str()).map(String::from);
    let username = payload
        .get("sub")
        .and_then(|v| v.as_str())
        .map(String::from);

    match (id, username) {
        (Some(id), Some(name)) if !name.is_empty() => AuthState::authenticated(id, name, csrf),
        _ => {
            // No `sub` claim — this is a raw SurrealDB-issued JWT (only
            // carries `ID: user:<record-id>` but encoded as lowercase `id`
            // in our tokens). We refuse to surface the record id as a
            // display name. The cookie will be replaced with a
            // `sub`-bearing token on the next login or refresh.
            tracing::warn!(
                "session JWT missing `sub` or `id` claim; treating as guest for display"
            );
            AuthState::guest(csrf)
        }
    }
}

/// Clone the shared DB and authenticate with the given JWT.
pub async fn authenticate_db(
    state: &AppState,
    token: &str,
) -> Result<surrealdb::Surreal<Any>, Redirect> {
    let user_db = (*state.db).clone();
    if user_db
        .use_ns(&state.namespace)
        .use_db(&state.database)
        .await
        .is_err()
    {
        return Err(Redirect::to("/auth"));
    }
    if user_db.authenticate(Token::from(token)).await.is_err() {
        return Err(Redirect::to("/auth"));
    }
    Ok(user_db)
}

// ── My Goblins page ─────────────────────────────────────────────────

/// Query params accepted by `GET /my-goblins`. Form-post failures redirect
/// back with the API's message in `?error=`.
#[derive(Deserialize, Default)]
pub struct MyGoblinsQuery {
    #[serde(default)]
    pub error: Option<String>,
}

/// One persistent goblin rendered by `my_goblins.html`.
#[derive(Debug, serde::Serialize)]
pub struct MyGoblin {
    pub identifier: String,
    pub name: String,
    pub clan_name: String,
    pub status: String,
    pub avatar_url: Option<String>,
}

/// Form payload shared by the create and edit posts. Both fields are
/// optional so "blank means generate" matches `POST /api/characters`.
#[derive(Deserialize)]
pub struct GoblinForm {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub clan_name: Option<String>,
    pub csrf_token: String,
}

/// Form payload for the delete post (confirmation happens in the browser).
#[derive(Deserialize)]
pub struct DeleteGoblinForm {
    pub csrf_token: String,
}

/// GET /my-goblins — the caller's persistent goblins (requires auth).
///
/// Follows the account page pattern: `require_auth` guard, then an
/// authenticated connection for the `created_by = $auth` list query
/// (same query as `GET /api/characters`).
pub async fn my_goblins_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(params): Query<MyGoblinsQuery>,
) -> Response {
    let (auth, csrf) = extract_auth(&headers);
    if require_auth(&state, &headers).await.is_err() {
        return Redirect::to("/auth?tab=login").into_response();
    }

    let token = read_cookie(&headers, SESSION_COOKIE)
        .unwrap_or_default()
        .to_owned();
    let user_db = match authenticate_db(&state, &token).await {
        Ok(user_db) => user_db,
        Err(redirect) => return redirect.into_response(),
    };

    let characters: Vec<Character> = user_db
        .query("SELECT * FROM character WHERE created_by = $auth ORDER BY identifier")
        .await
        .ok()
        .and_then(|mut response| response.take::<Vec<Value>>(0).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| serde_json::from_value(row).ok())
        .collect();

    let goblins: Vec<MyGoblin> = characters
        .into_iter()
        .map(|character| MyGoblin {
            identifier: character.identifier,
            name: character.name,
            clan_name: character.clan_name,
            status: character.status.to_string(),
            avatar_url: character
                .avatar
                .as_deref()
                .map(|path| state.storage.public_url(path)),
        })
        .collect();

    let mut ctx = tera_engine::base_context("My Goblins", &auth);
    ctx.insert("goblins", &goblins);
    if let Some(error) = params.error.as_deref().filter(|error| !error.is_empty()) {
        ctx.insert("error", error);
    }
    html_with_csrf(tera_engine::render("my_goblins.html", &ctx), &csrf)
}

/// CSRF + session preamble shared by the My Goblins form posts: validate
/// the token, then open an authenticated connection for the `$auth`
/// writes. Returns `None` when either check fails — the caller sends the
/// same `/auth` redirect [`authenticate_db`] uses.
async fn form_write_db(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    csrf_token: &str,
) -> Option<Surreal<Any>> {
    if !validate_csrf(headers, csrf_token) {
        return None;
    }
    let token = read_cookie(headers, SESSION_COOKIE)
        .unwrap_or_default()
        .to_owned();
    if token.is_empty() {
        return None;
    }
    authenticate_db(state, &token).await.ok()
}

/// Redirect back to the page with the API's error message in `?error=`.
fn goblins_error_redirect(error: impl std::fmt::Display) -> Response {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("error", &error.to_string())
        .finish();
    Redirect::to(&format!("/my-goblins?{query}")).into_response()
}

/// POST /my-goblins — create a goblin from the page form, then redirect
/// (POST/Redirect/GET) so refreshing can't resubmit.
pub async fn my_goblins_create_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Form(form): Form<GoblinForm>,
) -> Response {
    let Some(user_db) = form_write_db(&state, &headers, &form.csrf_token).await else {
        return Redirect::to("/auth").into_response();
    };
    let payload = CreateOwnedCharacter {
        name: form.name,
        clan_name: form.clan_name,
    };
    match create_owned_character(Extension(AuthDb(user_db)), Json(payload)).await {
        Ok(_) => Redirect::to("/my-goblins").into_response(),
        Err(error) => goblins_error_redirect(error),
    }
}

/// URL slug for a goblin name: lowercase, runs of non-alphanumerics
/// folded into single hyphens (`Page Goblin` → `page-goblin`). Shared by
/// the `slug` Tera filter and `characters::resolve_owned_character_identifier`
/// so form actions round-trip.
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut pending_sep = false;
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            if pending_sep && !slug.is_empty() {
                slug.push('-');
            }
            pending_sep = false;
            slug.push(c);
        } else {
            pending_sep = true;
        }
    }
    slug
}

/// POST /my-goblins/{identifier} — edit name and clan from the page form.
pub async fn my_goblins_edit_handler(
    Path(param): Path<String>,
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Form(form): Form<GoblinForm>,
) -> Response {
    let Some(user_db) = form_write_db(&state, &headers, &form.csrf_token).await else {
        return Redirect::to("/auth").into_response();
    };
    let identifier = match resolve_owned_character_identifier(&user_db, &param).await {
        Ok(identifier) => identifier,
        Err(error) => return goblins_error_redirect(error),
    };
    let payload = EditOwnedCharacter {
        name: form.name,
        clan_name: form.clan_name,
    };
    match owned_character_update(Path(identifier), Extension(AuthDb(user_db)), Json(payload)).await
    {
        Ok(_) => Redirect::to("/my-goblins").into_response(),
        Err(error) => goblins_error_redirect(error),
    }
}

/// POST /my-goblins/{identifier}/delete — remove a goblin (the template's
/// form asks for confirmation before it submits).
pub async fn my_goblins_delete_handler(
    Path(param): Path<String>,
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Form(form): Form<DeleteGoblinForm>,
) -> Response {
    let Some(user_db) = form_write_db(&state, &headers, &form.csrf_token).await else {
        return Redirect::to("/auth").into_response();
    };
    let identifier = match resolve_owned_character_identifier(&user_db, &param).await {
        Ok(identifier) => identifier,
        Err(error) => return goblins_error_redirect(error),
    };
    match owned_character_delete(Path(identifier), Extension(AuthDb(user_db))).await {
        Ok(_) => Redirect::to("/my-goblins").into_response(),
        Err(error) => goblins_error_redirect(error),
    }
}
