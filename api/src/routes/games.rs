use crate::{authenticate_db, extract_auth, html_with_csrf, validate_csrf};
use api::AppState;
use api::auth::require_auth;
use api::cookies::{SESSION_COOKIE, read_cookie};
use api::templates::game_detail;
use api::templates::tera_engine;
use axum::Form;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use shared::ListDisplayGame;
use std::str::FromStr;
use surrealdb_types::SerdeWrapper;

// ── Game list types ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct GamesListQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub status: Option<String>,
}

fn default_limit() -> u32 {
    10
}

#[derive(Deserialize)]
pub struct CreateGameRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub private: Option<String>,
    #[serde(default)]
    pub csrf_token: String,
    /// Goblins checked in the roster picker; empty = bot fallback.
    #[serde(default)]
    pub characters: Vec<String>,
    /// Teams in the game (default 8, range 2-16).
    #[serde(default)]
    pub team_count: Option<u32>,
    /// Goblins each team fields (default 3, range 1-8).
    #[serde(default)]
    pub goblins_per_team: Option<u32>,
    /// Goblins each joining player may bring (default 1).
    #[serde(default)]
    pub max_goblins_per_player: Option<u32>,
}

// ── HTMX page handlers ──────────────────────────────────────────────

/// GET / — home page.
pub async fn home_handler(headers: axum::http::HeaderMap) -> Response {
    let (auth, csrf) = extract_auth(&headers);
    let mut ctx = tera_engine::base_context("Home", &auth);
    ctx.insert(
        "stats",
        &serde_json::json!({"running": 0, "waiting": 0, "finished": 0, "total": 0}),
    );
    html_with_csrf(tera_engine::render("home.html", &ctx), &csrf)
}

/// GET /games — list paginated games.
pub async fn games_list_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(params): Query<GamesListQuery>,
) -> Response {
    let (auth, csrf) = extract_auth(&headers);
    let limit = params.limit.min(100);
    let offset = params.offset.min(10000);

    let result = state
        .db
        .query("SELECT * FROM fn::get_list_games($limit, $offset)")
        .bind(("limit", limit))
        .bind(("offset", offset))
        .await;

    let all_games: Vec<ListDisplayGame> = match result {
        Ok(mut result) => result
            .take::<Vec<SerdeWrapper<ListDisplayGame>>>(0)
            .unwrap_or_default()
            .into_iter()
            .map(|w| w.0)
            .collect(),
        Err(_) => vec![],
    };

    let games = filter_games_by_status(&all_games, params.status.as_deref());
    let total = all_games.len() as u32;
    let has_more = (offset + limit) < total;
    let active_filter = params.status.as_deref().unwrap_or("");

    // Compute stats
    let running = all_games
        .iter()
        .filter(|g| g.status == shared::GameStatus::InProgress)
        .count() as u32;
    let waiting = all_games
        .iter()
        .filter(|g| g.status == shared::GameStatus::NotStarted)
        .count() as u32;
    let finished = all_games
        .iter()
        .filter(|g| g.status == shared::GameStatus::Finished)
        .count() as u32;

    let mut ctx = tera_engine::base_context("Games", &auth);
    ctx.insert("stats", &serde_json::json!({"running": running, "waiting": waiting, "finished": finished, "total": total}));
    ctx.insert("games", &games);
    ctx.insert("active_filter", active_filter);
    ctx.insert(
        "pagination",
        &shared::PaginationMetadata {
            total,
            limit,
            offset: offset + limit,
            has_more,
        },
    );

    html_with_csrf(tera_engine::render("games_list.html", &ctx), &csrf)
}

fn filter_games_by_status(games: &[ListDisplayGame], status: Option<&str>) -> Vec<ListDisplayGame> {
    match status {
        Some("running") => games
            .iter()
            .filter(|g| g.status == shared::GameStatus::InProgress)
            .cloned()
            .collect(),
        Some("waiting") => games
            .iter()
            .filter(|g| g.status == shared::GameStatus::NotStarted)
            .cloned()
            .collect(),
        Some("finished") => games
            .iter()
            .filter(|g| g.status == shared::GameStatus::Finished)
            .cloned()
            .collect(),
        Some(_) | None => games.to_vec(),
    }
}

#[derive(Deserialize, Default)]
pub struct DayQuery {
    pub day: Option<u32>,
    pub phase: Option<String>,
    /// `?error=` after a failed join (POST/Redirect/GET).
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct GameJoinForm {
    pub character_id: String,
    pub csrf_token: String,
}

/// POST /games/{id}/join — enter one of your goblins into a not-started
/// game, then redirect back to the game page (POST/Redirect/GET).
pub async fn game_join_post_handler(
    State(state): State<AppState>,
    axum::extract::Path(game_identifier): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
    Form(form): Form<GameJoinForm>,
) -> Response {
    if !validate_csrf(&headers, &form.csrf_token) {
        return Redirect::to("/auth").into_response();
    }
    let token = match read_cookie(&headers, SESSION_COOKIE) {
        Some(t) => t.to_owned(),
        None => return Redirect::to("/auth").into_response(),
    };
    let user_db = match authenticate_db(&state, &token).await {
        Ok(db) => db,
        Err(redirect) => return redirect.into_response(),
    };
    match api::characters::join_game(&user_db, &game_identifier, &form.character_id).await {
        Ok(()) => Redirect::to(&format!("/games/{game_identifier}")).into_response(),
        Err(error) => {
            let query = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("error", &error.to_string())
                .finish();
            Redirect::to(&format!("/games/{game_identifier}?{query}")).into_response()
        }
    }
}

/// GET /games/{id} — game detail page (broadcast interface).
pub async fn game_detail_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(game_identifier): axum::extract::Path<uuid::Uuid>,
    Query(query): Query<DayQuery>,
) -> Response {
    let (auth, csrf) = extract_auth(&headers);
    let identifier = game_identifier.to_string();

    // Authenticate DB for $auth-gated queries (is_mine)
    let token = read_cookie(&headers, SESSION_COOKIE)
        .map(|s| s.to_owned())
        .unwrap_or_default();
    let db = if !token.is_empty() {
        match authenticate_db(&state, &token).await {
            Ok(db) => db,
            Err(_) => (*state.db).clone(), // fallback to unauthenticated
        }
    } else {
        (*state.db).clone()
    };

    let result = db
        .query("SELECT * FROM fn::get_display_game($identifier);")
        .bind(("identifier", identifier.clone()))
        .await;

    let game = match result {
        Ok(mut result) => {
            let game: Option<SerdeWrapper<shared::DisplayGame>> =
                result.take(0).unwrap_or_default();
            game.map(|w| w.0)
        }
        Err(_) => None,
    };

    let Some(game) = game else {
        let mut ctx = tera_engine::base_context("Not Found", &auth);
        ctx.insert("message", "The game you're looking for doesn't exist.");
        return html_with_csrf(tera_engine::render("not_found.html", &ctx), &csrf);
    };

    let characters_result = state
        .db
        .query("SELECT * FROM fn::get_characters_by_game($identifier);")
        .bind(("identifier", identifier.clone()))
        .await;

    let characters: Vec<game::characters::Character> = match characters_result {
        Ok(mut result) => {
            let raw_rows: Vec<serde_json::Value> = result.take(0).unwrap_or_default();
            raw_rows
                .into_iter()
                .filter_map(|row| row["characters"].as_array().cloned())
                .flatten()
                .filter_map(|t| serde_json::from_value(t).ok())
                .collect()
        }
        Err(_) => vec![],
    };

    // Fetch areas with terrain + character slot assignments
    let areas_result = state
        .db
        .query(
            r#"
SELECT (
    SELECT *, ->items->item[*] AS items
    FROM ->areas->area
) AS areas FROM game WHERE identifier = $identifier;
"#,
        )
        .bind(("identifier", identifier.clone()))
        .await;

    let areas: Vec<game::areas::AreaDetails> = match areas_result {
        Ok(mut result) => {
            let rows: Vec<Vec<SerdeWrapper<game::areas::AreaDetails>>> =
                result.take("areas").unwrap_or_default();
            rows.into_iter()
                .next()
                .map(|inner| inner.into_iter().map(|w| w.0).collect())
                .unwrap_or_default()
        }
        Err(_) => vec![],
    };

    let current_day = game.day.unwrap_or(0);
    // Override current_day if query param provided
    let current_day = query.day.unwrap_or(current_day);

    let current_phase = query.phase.as_deref().unwrap_or("all");

    let messages_result = if current_day > 0 {
        if current_phase != "all" {
            state
                .db
                .query(
                    r#"SELECT * FROM message
                    WHERE string::starts_with(subject, $identifier)
                    AND game_day = $day
                    AND phase = $phase
                    ORDER BY game_day, phase, tick, emit_index;"#,
                )
                .bind(("identifier", identifier.clone()))
                .bind(("day", current_day))
                .bind(("phase", current_phase))
                .await
        } else {
            state
                .db
                .query(
                    r#"SELECT * FROM message
                    WHERE string::starts_with(subject, $identifier)
                    AND game_day = $day
                    ORDER BY game_day, phase, tick, emit_index;"#,
                )
                .bind(("identifier", identifier.clone()))
                .bind(("day", current_day))
                .await
        }
    } else {
        state
            .db
            .query(
                r#"SELECT * FROM message
                WHERE string::starts_with(subject, $identifier)
                ORDER BY game_day, phase, tick, emit_index;"#,
            )
            .bind(("identifier", identifier.clone()))
            .await
    };

    let messages: Vec<shared::messages::GameMessage> = match messages_result {
        Ok(mut logs) => {
            let rows: Vec<SerdeWrapper<api::games::GameLog>> = logs.take(0).unwrap_or_default();
            rows.into_iter()
                .map(|w| shared::messages::GameMessage::from(w.0))
                .collect()
        }
        Err(_) => vec![],
    };

    let commentary_result = state
        .db
        .query(
            r#"SELECT game_id, day, phase, lines, generated_at, model_used
            FROM commentary_segments
            WHERE game_id = $identifier
            ORDER BY day, phase;"#,
        )
        .bind(("identifier", identifier.clone()))
        .await;

    let segments: Vec<announcers::CommentarySegment> = match commentary_result {
        Ok(mut result) => result
            .take::<Vec<SerdeWrapper<announcers::CommentarySegment>>>(0)
            .unwrap_or_default()
            .into_iter()
            .map(|w| w.0)
            .collect(),
        Err(_) => vec![],
    };

    // Pre-compute context data
    let alive = characters.iter().filter(|t| t.is_alive()).count() as u32;
    let fallen = characters.len() as u32 - alive;
    let total = characters.len() as u32;

    let (phase_class, phase_label) = game_detail::current_broadcast_phase(&game, &messages);

    // Sort characters: alive first, then alphabetically
    let mut sorted_characters: Vec<_> = characters.iter().collect();
    sorted_characters.sort_by(|a, b| b.is_alive().cmp(&a.is_alive()).then(a.name.cmp(&b.name)));

    // Day numbers: generate range 1..=game.day (includes all days, not just days with messages)
    let max_day = game.day.unwrap_or(0);
    let day_numbers: Vec<u32> = if max_day > 0 {
        (1..=max_day).collect()
    } else {
        vec![]
    };

    // Pre-render event cards
    let mut event_cards = String::new();
    for msg in &messages {
        event_cards.push_str(&game_detail::render_event_card(msg));
    }
    for seg in &segments {
        event_cards.push_str(&game_detail::render_commentary_card(seg));
    }

    // Pre-render character rows — grouped by team
    let team_groups = game_detail::build_team_groups(&sorted_characters);
    let mut character_rows = String::new();
    for group in &team_groups {
        character_rows.push_str(&game_detail::render_team_group(group, &identifier));
    }

    // Build hex arena map SVG
    let hex_map = game_detail::render_hex_map(&areas, &sorted_characters);

    // SSE events string
    let sse_events = "death,wound,attack,combat,alliance_formed,alliance_proposed,alliance_dissolved,betrayal,trust_shock_break,patron_gift,movement,hidden,area_closed,area_event,item_found,item_used,item_dropped,rested,starved,dehydrated,sanity_break,hunger_band_changed,thirst_band_changed,stamina_band_changed,shelter_sought,foraged,drank,ate,cycle_start,cycle_end,phase_started,phase_ended,slept,woke,game_ended,wounded,attacked,affliction_acquired,affliction_progressed,affliction_healed,affliction_cascaded,trauma_acquired,trauma_reinforced,trauma_escalated,trauma_flashback,trauma_avoidance,trauma_observed,trauma_forgotten,trauma_habituated,phobia_acquired,phobia_triggered,phobia_escalated,phobia_habituated,phobia_observed,phobia_forgotten,fixation_acquired,fixation_escalated,fixation_fired,fixation_consummated,fixation_thwarted,fixation_faded,generic,trapped,struggling,trapped_escaped,died_while_trapped,trap_set,trap_triggered,rescue_attempted,sleep_incident,partial_rescue_progress";

    // Team victory: label of the only team with living members, if exactly
    // one team stands (the engine ends the game at that point).
    let winning_team: Option<String> = {
        let teams: std::collections::BTreeSet<u32> = sorted_characters
            .iter()
            .filter(|c| c.is_alive())
            .map(|c| c.team)
            .collect();
        match teams.len() {
            1 => Some(world::clans::team_label(*teams.iter().next().unwrap())),
            _ => None,
        }
    };

    let joinable_goblins = if game.status == shared::GameStatus::NotStarted {
        api::characters::list_joinable_goblins(&db)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut ctx = tera_engine::base_context(&game.name, &auth);
    ctx.insert("body_class", "broadcast");
    ctx.insert("error", &query.error);
    ctx.insert("joinable_goblins", &joinable_goblins);
    ctx.insert("game", &game);
    ctx.insert("winning_team", &winning_team);
    ctx.insert("alive", &alive);
    ctx.insert("fallen", &fallen);
    ctx.insert("total", &total);
    ctx.insert("phase_class", phase_class);
    ctx.insert("phase_label", phase_label);
    ctx.insert("day_numbers", &day_numbers);
    ctx.insert("current_day", &current_day);
    ctx.insert("current_phase", current_phase);
    ctx.insert("sse_events", sse_events);
    ctx.insert("character_rows", &character_rows);
    ctx.insert("hex_map", &hex_map);
    ctx.insert("event_cards", &event_cards);
    ctx.insert("messages", &messages);
    ctx.insert("segments", &segments);
    let day_count = game.day.unwrap_or(0) + 1;
    ctx.insert("day_count", &day_count);

    html_with_csrf(tera_engine::render("game_detail.html", &ctx), &csrf)
}

/// GET /games/{id}/characters — characters for a game.
pub async fn game_characters_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(game_identifier): axum::extract::Path<uuid::Uuid>,
) -> Response {
    let (auth, csrf) = extract_auth(&headers);
    let identifier = game_identifier.to_string();
    let result = state
        .db
        .query("SELECT * FROM fn::get_characters_by_game($identifier);")
        .bind(("identifier", identifier.clone()))
        .await;

    let characters = match result {
        Ok(mut result) => {
            let raw_rows: Vec<serde_json::Value> = result.take(0).unwrap_or_default();
            raw_rows
                .into_iter()
                .filter_map(|row| row["characters"].as_array().cloned())
                .flatten()
                .filter_map(|t| serde_json::from_value(t).ok())
                .collect()
        }
        Err(_) => vec![],
    };

    // Pre-render character cards grouped by team
    let mut character_cards = String::new();
    for character in &characters {
        character_cards.push_str(&game_detail::render_character_card(character));
    }

    let mut ctx = tera_engine::base_context("Characters", &auth);
    ctx.insert("game_id", &identifier);
    ctx.insert("character_cards", &character_cards);
    html_with_csrf(tera_engine::render("characters.html", &ctx), &csrf)
}

/// GET /games/{id}/areas — areas for a game.
pub async fn game_areas_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(game_identifier): axum::extract::Path<uuid::Uuid>,
) -> Response {
    let (auth, csrf) = extract_auth(&headers);
    let identifier = game_identifier.to_string();
    let result = state
        .db
        .query(
            r#"
SELECT (
    SELECT *, ->items->item[*] AS items
    FROM ->areas->area
) AS areas FROM game WHERE identifier = $identifier;
"#,
        )
        .bind(("identifier", identifier.clone()))
        .await;

    let areas = match result {
        Ok(mut result) => {
            let areas: Vec<Vec<SerdeWrapper<game::areas::AreaDetails>>> =
                result.take("areas").unwrap_or_default();
            areas
                .into_iter()
                .next()
                .map(|inner| inner.into_iter().map(|w| w.0).collect())
                .unwrap_or_default()
        }
        Err(_) => vec![],
    };

    // Pre-render area cards
    let mut area_cards = String::new();
    for area in &areas {
        area_cards.push_str(&game_detail::render_area_card(area));
    }

    let mut ctx = tera_engine::base_context("Areas", &auth);
    ctx.insert("game_id", &identifier);
    ctx.insert("area_cards", &area_cards);
    html_with_csrf(tera_engine::render("areas.html", &ctx), &csrf)
}

/// GET /games/{id}/log — event log for a game with commentary.
pub async fn game_log_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(game_identifier): axum::extract::Path<uuid::Uuid>,
) -> Response {
    let (auth, csrf) = extract_auth(&headers);
    let identifier = game_identifier.to_string();
    let result = state
        .db
        .query(
            r#"SELECT * FROM message
            WHERE string::starts_with(subject, $identifier)
            ORDER BY game_day, phase, tick, emit_index;"#,
        )
        .bind(("identifier", identifier.clone()))
        .await;

    let messages = match result {
        Ok(mut logs) => {
            let rows: Vec<SerdeWrapper<api::games::GameLog>> = logs.take(0).unwrap_or_default();
            rows.into_iter()
                .map(|w| shared::messages::GameMessage::from(w.0))
                .collect()
        }
        Err(_) => vec![],
    };

    let commentary_result = state
        .db
        .query(
            r#"SELECT game_id, day, phase, lines, generated_at, model_used
            FROM commentary_segments
            WHERE game_id = $identifier
            ORDER BY day, phase;"#,
        )
        .bind(("identifier", identifier.clone()))
        .await;

    let segments = match commentary_result {
        Ok(mut result) => result
            .take::<Vec<SerdeWrapper<announcers::CommentarySegment>>>(0)
            .unwrap_or_default()
            .into_iter()
            .map(|w| w.0)
            .collect(),
        Err(_) => vec![],
    };

    // Pre-render event cards
    let mut event_cards = String::new();
    for msg in &messages {
        event_cards.push_str(&game_detail::render_event_card(msg));
    }
    for seg in &segments {
        event_cards.push_str(&game_detail::render_commentary_card(seg));
    }

    let mut ctx = tera_engine::base_context("Event Log", &auth);
    ctx.insert("game_id", &identifier);
    ctx.insert("event_cards", &event_cards);
    html_with_csrf(tera_engine::render("log.html", &ctx), &csrf)
}

#[derive(Deserialize)]
pub struct TimelineQuery {
    pub filter: Option<String>,
    pub character: Option<String>,
    pub day: Option<u32>,
    pub phase: Option<String>,
}

/// GET /games/{id}/timeline — timeline view with period grid, filters, and event cards.
pub async fn timeline_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(game_identifier): axum::extract::Path<uuid::Uuid>,
    Query(params): Query<TimelineQuery>,
) -> Response {
    let (auth, csrf) = extract_auth(&headers);
    let identifier = game_identifier.to_string();

    // ── Fetch game ─────────────────────────────────────────────────
    let game_result = state
        .db
        .query("SELECT * FROM fn::get_display_game($identifier);")
        .bind(("identifier", identifier.clone()))
        .await;

    let game = match game_result {
        Ok(mut result) => {
            let game: Option<SerdeWrapper<shared::DisplayGame>> =
                result.take(0).unwrap_or_default();
            game.map(|w| w.0)
        }
        Err(_) => None,
    };

    let game = match game {
        Some(g) => g,
        None => {
            let mut ctx = tera_engine::base_context("Not Found", &auth);
            ctx.insert("message", "Game not found.");
            return html_with_csrf(tera_engine::render("not_found.html", &ctx), &csrf);
        }
    };

    // ── Fetch characters ─────────────────────────────────────────────
    let characters_result = state
        .db
        .query("SELECT * FROM fn::get_characters_by_game($identifier);")
        .bind(("identifier", identifier.clone()))
        .await;

    let character_refs: Vec<shared::messages::CharacterRef> = match characters_result {
        Ok(mut result) => {
            let raw_rows: Vec<serde_json::Value> = result.take(0).unwrap_or_default();
            raw_rows
                .into_iter()
                .filter_map(|row| row["characters"].as_array().cloned())
                .flatten()
                .filter_map(|t| {
                    let id = t.get("identifier")?.as_str()?.to_string();
                    let name = t.get("name")?.as_str()?.to_string();
                    Some(shared::messages::CharacterRef {
                        identifier: id.into(),
                        name,
                    })
                })
                .collect()
        }
        Err(_) => vec![],
    };

    // ── Fetch messages ─────────────────────────────────────────────
    let messages_result = state
        .db
        .query(
            r#"SELECT * FROM message
            WHERE string::starts_with(subject, $identifier)
            ORDER BY game_day, phase, tick, emit_index;"#,
        )
        .bind(("identifier", identifier.clone()))
        .await;

    let messages: Vec<shared::messages::GameMessage> = match messages_result {
        Ok(mut logs) => {
            let rows: Vec<SerdeWrapper<api::games::GameLog>> = logs.take(0).unwrap_or_default();
            rows.into_iter()
                .map(|w| shared::messages::GameMessage::from(w.0))
                .collect()
        }
        Err(_) => vec![],
    };

    let current_day = game.day.unwrap_or(0);
    let current_phase = messages
        .iter()
        .filter(|m| m.game_day == current_day)
        .max_by_key(|m| (m.phase, m.tick, m.emit_index))
        .map(|m| m.phase)
        .unwrap_or(shared::messages::Phase::Day);

    let periods = shared::messages::summarize_periods(&messages, (current_day, current_phase));

    let filter_str = params.filter.as_deref().unwrap_or("");
    let character_filter_str = params.character.as_deref().unwrap_or("");
    let selected_day = params.day;
    let selected_phase = params
        .phase
        .as_deref()
        .and_then(|p| shared::messages::Phase::from_str(p).ok());

    // ── Filter events ──────────────────────────────────────────────
    let filtered_events: Vec<shared::messages::GameMessage> = messages
        .iter()
        .filter(|m| {
            if let (Some(day), Some(phase)) = (selected_day, selected_phase)
                && (m.game_day != day || m.phase != phase)
            {
                return false;
            }
            if !filter_str.is_empty() {
                let kind = m.payload.kind();
                let kind_str = match kind {
                    shared::messages::MessageKind::CharacterKilled => "death",
                    shared::messages::MessageKind::Combat
                    | shared::messages::MessageKind::CombatSwing
                    | shared::messages::MessageKind::CharacterAttacked
                    | shared::messages::MessageKind::CharacterWounded => "combat",
                    shared::messages::MessageKind::AllianceFormed
                    | shared::messages::MessageKind::AllianceProposed
                    | shared::messages::MessageKind::AllianceDissolved
                    | shared::messages::MessageKind::BetrayalTriggered
                    | shared::messages::MessageKind::TrustShockBreak => "alliance",
                    shared::messages::MessageKind::CharacterMoved
                    | shared::messages::MessageKind::CharacterHidden
                    | shared::messages::MessageKind::AreaClosed
                    | shared::messages::MessageKind::AreaEvent => "movement",
                    shared::messages::MessageKind::ItemFound
                    | shared::messages::MessageKind::ItemUsed
                    | shared::messages::MessageKind::ItemDropped
                    | shared::messages::MessageKind::PatronGift => "items",
                    _ => "",
                };
                if kind_str != filter_str {
                    return false;
                }
            }
            if !character_filter_str.is_empty() {
                let character_id = character_refs
                    .iter()
                    .find(|t| t.name == character_filter_str)
                    .map(|t| t.identifier.as_str());
                if let Some(id) = character_id {
                    if !m.payload.involves(id) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    // ── Pre-render event cards ─────────────────────────────────────
    let rendered_events: Vec<String> = filtered_events
        .iter()
        .map(game_detail::render_event_card)
        .collect();

    // ── Build template context ─────────────────────────────────────
    let mut ctx = tera_engine::base_context(&format!("Timeline — {}", game.name), &auth);

    ctx.insert("game_id", &identifier);
    ctx.insert("game_name", &game.name);
    ctx.insert("current_day", &current_day);
    ctx.insert("current_phase", &current_phase.to_string());
    ctx.insert("periods", &periods);
    ctx.insert("filter", filter_str);
    ctx.insert("character_filter", character_filter_str);
    ctx.insert("characters", &character_refs);
    ctx.insert("rendered_events", &rendered_events);
    ctx.insert("selected_day", &selected_day);
    ctx.insert("selected_phase", &selected_phase.map(|p| p.to_string()));

    let filter_options = vec![
        serde_json::json!({"value": "", "label": "All", "icon_name": "list"}),
        serde_json::json!({"value": "death", "label": "Deaths", "icon_name": "skull"}),
        serde_json::json!({"value": "combat", "label": "Combat", "icon_name": "sword"}),
        serde_json::json!({"value": "alliance", "label": "Alliances", "icon_name": "users"}),
        serde_json::json!({"value": "movement", "label": "Movement", "icon_name": "map-pin"}),
        serde_json::json!({"value": "items", "label": "Items", "icon_name": "backpack"}),
    ];
    ctx.insert("filter_options", &filter_options);

    let rendered = tera_engine::render("timeline.html", &ctx);
    html_with_csrf(rendered, &csrf)
}

/// GET /account — account dashboard (requires auth).
pub async fn account_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Response {
    let (auth, csrf) = extract_auth(&headers);
    let session = match require_auth(&state, &headers).await {
        Ok(s) => s,
        Err(_) => return Redirect::to("/auth?tab=login").into_response(),
    };

    let user_db = (*state.db).clone();
    if user_db
        .use_ns(&state.namespace)
        .use_db(&state.database)
        .await
        .is_err()
    {
        return Redirect::to("/auth?tab=login").into_response();
    }

    let games: Vec<ListDisplayGame> = user_db
        .query("SELECT * FROM fn::get_list_games(100, 0)")
        .await
        .ok()
        .and_then(|mut r| r.take::<Vec<SerdeWrapper<ListDisplayGame>>>(0).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|w| w.0)
        .collect();

    let mut ctx = tera_engine::base_context("Account", &auth);
    ctx.insert("session", &session);
    ctx.insert("games", &games);
    html_with_csrf(tera_engine::render("account.html", &ctx), &csrf)
}

/// GET /account/settings — account settings page (requires auth).
pub async fn account_settings_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Response {
    let (auth, csrf) = extract_auth(&headers);
    let session = match require_auth(&state, &headers).await {
        Ok(s) => s,
        Err(_) => return Redirect::to("/auth?tab=login").into_response(),
    };

    // Clone and authenticate DB to query email
    let token = match read_cookie(&headers, SESSION_COOKIE) {
        Some(t) => t.to_owned(),
        None => return Redirect::to("/auth?tab=login").into_response(),
    };
    let user_db = match authenticate_db(&state, &token).await {
        Ok(db) => db,
        Err(_) => return Redirect::to("/auth?tab=login").into_response(),
    };

    #[derive(serde::Deserialize, serde::Serialize)]
    struct EmailRow {
        email: String,
    }
    let current_email: String = user_db
        .query("SELECT email FROM $auth")
        .await
        .ok()
        .and_then(|mut r| r.take::<Option<SerdeWrapper<EmailRow>>>(0).ok())
        .flatten()
        .map(|w| w.0.email)
        .unwrap_or_default();

    // Convert avatar storage path to public URL
    let avatar_url: Option<String> = session
        .avatar
        .as_ref()
        .map(|path| state.storage.public_url(path));

    let mut ctx = tera_engine::base_context("Account Settings", &auth);
    ctx.insert("session", &session);
    ctx.insert("current_email", &current_email);
    if let Some(ref url) = avatar_url {
        ctx.insert("avatar_url", url);
    }
    html_with_csrf(tera_engine::render("account_settings.html", &ctx), &csrf)
}

/// GET /games/{id}/characters/{character_id} — character detail page.
pub async fn game_character_detail_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Path((game_identifier, character_identifier)): axum::extract::Path<(
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Response {
    let (auth, csrf) = extract_auth(&headers);
    let game_id = game_identifier.to_string();
    let character_id = character_identifier.to_string();

    let result = state
        .db
        .query("SELECT * FROM fn::get_full_character($identifier);")
        .bind(("identifier", character_id.clone()))
        .await;

    let character = match result {
        Ok(mut result) => result
            .take::<Vec<serde_json::Value>>(0)
            .unwrap_or_default()
            .into_iter()
            .next()
            .and_then(|v| serde_json::from_value(v).ok()),
        Err(_) => None,
    };

    match character {
        Some(character) => {
            // Pre-render character detail content
            let character_html = game_detail::render_character_detail(&character, &game_id);
            let mut ctx = tera_engine::base_context(&character.name, &auth);
            ctx.insert("game_id", &game_id);
            ctx.insert("character_detail_html", &character_html);
            html_with_csrf(tera_engine::render("character_detail.html", &ctx), &csrf)
        }
        None => {
            let mut ctx = tera_engine::base_context("Not Found", &auth);
            ctx.insert("message", "The character you're looking for doesn't exist.");
            html_with_csrf(tera_engine::render("not_found.html", &ctx), &csrf)
        }
    }
}

/// GET /games/new — create game form (requires auth).
pub async fn create_game_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Response {
    let (auth, csrf) = extract_auth(&headers);
    if !auth.is_authenticated() {
        return Redirect::to("/auth?tab=login").into_response();
    }

    // Owned goblins that haven't entered a game — the roster picker.
    let token = read_cookie(&headers, SESSION_COOKIE)
        .map(|s| s.to_owned())
        .unwrap_or_default();
    let db = if !token.is_empty() {
        match authenticate_db(&state, &token).await {
            Ok(db) => db,
            Err(_) => (*state.db).clone(),
        }
    } else {
        (*state.db).clone()
    };
    let goblins = api::characters::list_joinable_goblins(&db)
        .await
        .unwrap_or_default();

    let mut ctx = tera_engine::base_context("Create Game", &auth);
    ctx.insert("goblins", &goblins);
    html_with_csrf(tera_engine::render("create_game.html", &ctx), &csrf)
}

/// POST /games/new — create game, redirect to /games/{id}.
pub async fn create_game_post_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Form(form): Form<CreateGameRequest>,
) -> Response {
    if !validate_csrf(&headers, &form.csrf_token) {
        return Redirect::to("/auth").into_response();
    }

    let token = match read_cookie(&headers, SESSION_COOKIE) {
        Some(t) => t.to_owned(),
        None => return Redirect::to("/auth").into_response(),
    };

    let user_db = match authenticate_db(&state, &token).await {
        Ok(db) => db,
        Err(redirect) => return redirect.into_response(),
    };

    use game::games::Game;
    let game = Game::default();
    let game_identifier = uuid::Uuid::new_v4().to_string();
    let game_name = form.name.filter(|n| !n.is_empty()).unwrap_or(game.name);
    let is_private = form.private.is_some_and(|v| v == "true");
    let team_count = form
        .team_count
        .unwrap_or(shared::DEFAULT_TEAM_COUNT)
        .clamp(2, 16);
    let goblins_per_team = form
        .goblins_per_team
        .unwrap_or(shared::DEFAULT_GOBLINS_PER_TEAM)
        .clamp(1, 8)
        .min(shared::MAX_ROSTER_CAP / team_count);
    let roster_cap = team_count * goblins_per_team;
    let max_goblins_per_player = form
        .max_goblins_per_player
        .unwrap_or(shared::DEFAULT_MAX_GOBINS_PER_PLAYER)
        .clamp(1, roster_cap);

    use surrealdb_types::RecordId;

    let game_rid = RecordId::new("game", game_identifier.as_str());
    let body = serde_json::json!({
        "identifier": &game_identifier,
        "name": &game_name,
        "status": "NotStarted",
        "day": null,
        "private": is_private,
        "team_count": team_count,
        "goblins_per_team": goblins_per_team,
        "max_goblins_per_player": max_goblins_per_player,
    });

    if user_db
        .query("UPSERT $rid CONTENT $body")
        .bind(("rid", game_rid.clone()))
        .bind(("body", body))
        .await
        .is_err()
    {
        return Redirect::to("/games/new").into_response();
    }

    let mut chosen: Vec<String> = form.characters.clone();
    chosen.retain(|c| !c.is_empty());
    if chosen.is_empty() {
        // Legacy fallback: no goblin chosen, spawn the full bot roster.
        let character_futures = (0..roster_cap).map(|idx| {
            api::characters::create_character(None, &game_identifier, &user_db, idx % team_count)
        });
        let character_results = futures::future::join_all(character_futures).await;
        if character_results.into_iter().any(|r| r.is_err()) {
            return Redirect::to("/games/new").into_response();
        }
    } else {
        for character_id in &chosen {
            if api::characters::join_game(&user_db, &game_identifier, character_id)
                .await
                .is_err()
            {
                let _ = user_db.query("DELETE $rid").bind(("rid", game_rid)).await;
                return Redirect::to("/games/new").into_response();
            }
        }
    }

    use game::areas::Area;
    use strum::IntoEnumIterator;
    let base_item_count = shared::ItemQuantity::default().base_item_count();
    let area_names = game::areas::generate_area_names();
    let area_futures = Area::iter().zip(area_names).map(|(area, name)| {
        api::games::create_area(&game_identifier, area, name, base_item_count, &user_db)
    });
    let area_results = futures::future::join_all(area_futures).await;
    if area_results.into_iter().any(|r| r.is_err()) {
        return Redirect::to("/games/new").into_response();
    }

    Redirect::to(&format!("/games/{game_identifier}")).into_response()
}
