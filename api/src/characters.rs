use crate::games::game_characters;
use crate::storage::{UploadConstraints, validate_upload};
use crate::{AppError, AppState, AuthDb};
use axum::extract::{Extension, Multipart, Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use game::characters::Character;
use game::items::Item;
use game::messages::GameMessage;
use game::naming::{clan_name, goblin_name};
use serde::{Deserialize, Serialize};
use shared::EditCharacter;
use std::sync::LazyLock;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb_types::{RecordId, SerdeWrapper};
use uuid::Uuid;
use validator::Validate;

pub static CHARACTERS_ROUTER: LazyLock<Router<AppState>> = LazyLock::new(|| {
    Router::new()
        .route("/", get(game_characters))
        .route(
            "/{identifier}",
            get(character_detail)
                .delete(character_delete)
                .put(character_update),
        )
        .route("/{identifier}/log", get(character_log))
});

/// Top-level CRUD for a player's own persistent characters, mounted at
/// `/api/characters`. Distinct from [`CHARACTERS_ROUTER`], which nests
/// under a game at `/api/games/{game}/characters`. Both mount behind the
/// `surreal_jwt` middleware, so handlers run on an authenticated
/// connection where `$auth` is the calling user — required by the
/// ownership checks below.
pub static OWNED_CHARACTERS_ROUTER: LazyLock<Router<AppState>> = LazyLock::new(|| {
    Router::new()
        .route("/", get(list_owned_characters).post(create_owned_character))
        .route(
            "/{identifier}",
            get(owned_character_detail)
                .put(owned_character_update)
                .delete(owned_character_delete),
        )
        .route("/{identifier}/avatar", post(upload_owned_character_avatar))
});

/// Payload for `POST /api/characters`. Missing or blank strings both
/// mean "generate it server-side": a random goblin name for `name`, a
/// goblin-flavored clan name for `clan_name`.
#[derive(Debug, Deserialize, Validate)]
pub struct CreateOwnedCharacter {
    #[validate(length(max = 50, message = "Name must be at most 50 characters"))]
    pub name: Option<String>,
    #[validate(length(max = 100, message = "Clan name must be at most 100 characters"))]
    pub clan_name: Option<String>,
}

/// Payload for `PUT /api/characters/{identifier}` (owner-only edit).
///
/// - `name`: optional; when present it must be 1-50 characters — a
///   character's name cannot be blanked.
/// - `clan_name`: optional; a blank value regenerates the clan name
///   (the same "blank means generate" rule as creation).
#[derive(Debug, Deserialize, Validate)]
pub struct EditOwnedCharacter {
    #[validate(length(min = 1, max = 50, message = "Name must be 1-50 characters"))]
    pub name: Option<String>,
    #[validate(length(max = 100, message = "Clan name must be at most 100 characters"))]
    pub clan_name: Option<String>,
}

/// Response body for `GET /api/characters`.
#[derive(Debug, Serialize)]
pub struct OwnedCharacterList {
    pub characters: Vec<Character>,
}

/// Trim a supplied string and treat blank as absent, so "not sent" and
/// "sent blank" behave identically for creation payloads.
fn generated_unless_present(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Load a character row for mutation, enforcing ownership: the row must
/// exist and `created_by` must equal `$auth` on this request's
/// authenticated connection.
///
/// The schema (`schemas/character.surql`) already restricts
/// create/update/delete to `created_by = $auth`, but SurrealDB enforces
/// that by *silently skipping* unauthorized rows — a foreign UPDATE
/// affects 0 rows and a foreign DELETE removes nothing, neither with an
/// error. Without this check the handlers would report success (or a
/// misleading 500) for writes that did nothing, so ownership is asked
/// explicitly. Requires an [`AuthDb`]-backed connection: on a root
/// connection `$auth` is NONE and every character looks foreign.
async fn require_owned_character(
    db: &Surreal<Any>,
    identifier: &str,
) -> Result<serde_json::Value, AppError> {
    let mut response = db
        .query(
            "SELECT name, clan_name, (created_by = $auth) AS is_owner
             FROM character WHERE identifier = $identifier",
        )
        .bind(("identifier", identifier.to_owned()))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to load character: {e}")))?;
    let rows: Vec<serde_json::Value> = response
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to read character: {e}")))?;
    let row = rows
        .into_iter()
        .next()
        .ok_or_else(|| AppError::NotFound("Character not found".to_string()))?;
    if row["is_owner"].as_bool() == Some(true) {
        Ok(row)
    } else {
        Err(AppError::Forbidden(
            "You do not own this character".to_string(),
        ))
    }
}

/// Resolve a page path parameter to a character UUID owned by the caller:
/// either the UUID itself or a slug of the character's name (`stinky`,
/// `page-goblin`). Slugs match against the caller's own roster only.
pub async fn resolve_owned_character_identifier(
    db: &Surreal<Any>,
    param: &str,
) -> Result<Uuid, AppError> {
    if let Ok(id) = Uuid::parse_str(param) {
        require_owned_character(db, &id.to_string()).await?;
        return Ok(id);
    }
    let mut response = db
        .query("SELECT identifier, name FROM character WHERE created_by = $auth")
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to resolve character: {e}")))?;
    let rows: Vec<serde_json::Value> = response
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to read characters: {e}")))?;
    rows.into_iter()
        .find(|row| {
            crate::pages::slugify(row["name"].as_str().unwrap_or(""))
                == crate::pages::slugify(param)
        })
        .and_then(|row| row["identifier"].as_str().map(str::to_owned))
        .and_then(|id| Uuid::parse_str(&id).ok())
        .ok_or_else(|| AppError::NotFound("Character not found".to_string()))
}

/// Fetch a character through `fn::get_full_character` (items, log, and
/// `editable` computed from the game's status).
async fn fetch_full_character(db: &Surreal<Any>, identifier: &str) -> Result<Character, AppError> {
    let mut result = db
        .query("SELECT * FROM fn::get_full_character($identifier);")
        .bind(("identifier", identifier.to_owned()))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to fetch character: {e}")))?;

    // Take as raw JSON to bypass the SurrealDB SDK custom deserializer
    // (chokes on null fields like `game_day: null` inside Option<T>).
    let raw: Vec<serde_json::Value> = result
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to take character: {e}")))?;

    raw.into_iter()
        .next()
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(|| AppError::NotFound("Character not found".to_string()))
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CharacterItemEdge {
    #[serde(rename = "in")]
    pub character: RecordId,
    #[serde(rename = "out")]
    pub item: RecordId,
}

/// Team (1..=8) with the fewest members in the game right now; ties go to
/// the lowest slot, so joins and bot-fill together divide a full roster
/// into exactly 3 per team (the ready rule in `schemas/game.surql`).
pub async fn fewest_team(db: &Surreal<Any>, game_identifier: &str) -> Result<u32, AppError> {
    let mut response = db
        .query("SELECT in.team AS team FROM playing_in WHERE out.identifier = $game")
        .bind(("game", game_identifier.to_owned()))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to read roster: {e}")))?;
    let rows: Vec<serde_json::Value> = response
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to read roster: {e}")))?;
    let mut counts = [0u32; 9];
    for row in rows {
        if let Some(team) = row["team"].as_u64()
            && (1..=8).contains(&team)
        {
            counts[team as usize] += 1;
        }
    }
    Ok((1..=8u32).min_by_key(|&t| counts[t as usize]).unwrap())
}

/// Owned goblins that have not entered any game yet — the roster picker
/// on create-game and the join form on a not-started game's page.
pub async fn list_joinable_goblins(db: &Surreal<Any>) -> Result<Vec<Character>, AppError> {
    let mut owned = db
        .query("SELECT * FROM character WHERE created_by = $auth ORDER BY name")
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to list goblins: {e}")))?;
    let owned_rows: Vec<serde_json::Value> = owned
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to read goblins: {e}")))?;
    let characters: Vec<Character> = owned_rows
        .into_iter()
        .filter_map(|row| serde_json::from_value(row).ok())
        .collect();

    let mut entered = db
        .query("SELECT in.identifier AS identifier FROM playing_in")
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to read entries: {e}")))?;
    let entered_rows: Vec<serde_json::Value> = entered
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to read entries: {e}")))?;
    let entered_ids: std::collections::HashSet<String> = entered_rows
        .iter()
        .filter_map(|row| row["identifier"].as_str().map(str::to_owned))
        .collect();
    Ok(characters
        .into_iter()
        .filter(|c| !entered_ids.contains(&c.identifier.to_string()))
        .collect())
}

/// Enter an owned character into a game that hasn't started.
///
/// Gates, in order: game exists and is `NotStarted`; caller owns the
/// character (404 missing / 403 foreign); the character isn't in another
/// game; the caller has no goblin in this game yet (bot fillers have no
/// `created_by` and are exempt); roster under 24. Assigns the team with
/// the fewest members and records the `playing_in` edge.
pub async fn join_game(
    db: &Surreal<Any>,
    game_identifier: &str,
    character_identifier: &str,
) -> Result<(), AppError> {
    let mut status_response = db
        .query("SELECT status FROM game WHERE identifier = $game")
        .bind(("game", game_identifier.to_owned()))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to load game: {e}")))?;
    let status_rows: Vec<serde_json::Value> = status_response
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to read game: {e}")))?;
    let status = status_rows
        .first()
        .and_then(|row| row["status"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| AppError::NotFound("Game not found".to_string()))?;
    if status != "NotStarted" {
        return Err(AppError::Conflict(
            "The game has already started".to_string(),
        ));
    }

    require_owned_character(db, character_identifier).await?;

    let mut in_game_response = db
        .query("RETURN count(SELECT id FROM playing_in WHERE in.identifier = $character)")
        .bind(("character", character_identifier.to_owned()))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to check goblin: {e}")))?;
    let in_game: u32 = in_game_response
        .take::<Option<u32>>(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to check goblin: {e}")))?
        .unwrap_or(0);
    if in_game > 0 {
        return Err(AppError::Conflict(
            "That goblin is already in a game".to_string(),
        ));
    }

    let mut mine_response = db
        .query(
            "RETURN count(SELECT id FROM playing_in WHERE out.identifier = $game AND in.created_by = $auth)",
        )
        .bind(("game", game_identifier.to_owned()))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to check roster: {e}")))?;
    let mine: u32 = mine_response
        .take::<Option<u32>>(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to check roster: {e}")))?
        .unwrap_or(0);
    if mine > 0 {
        return Err(AppError::Conflict(
            "You already have a goblin in this game".to_string(),
        ));
    }

    let mut count_response = db
        .query("RETURN count(SELECT id FROM playing_in WHERE out.identifier = $game)")
        .bind(("game", game_identifier.to_owned()))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to count roster: {e}")))?;
    let roster: u32 = count_response
        .take::<Option<u32>>(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to count roster: {e}")))?
        .unwrap_or(0);
    if roster >= 24 {
        return Err(AppError::GameFull("Game is full".to_string()));
    }

    let team = fewest_team(db, game_identifier).await?;
    db.query(
        "UPDATE character SET team = $team, statistics.game = $game WHERE identifier = $character",
    )
    .bind(("team", team))
    .bind(("game", game_identifier.to_owned()))
    .bind(("character", character_identifier.to_owned()))
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to enter game: {e}")))?;

    db.query("RELATE $character->playing_in->$game")
        .bind((
            "character",
            RecordId::new("character", character_identifier),
        ))
        .bind(("game", RecordId::new("game", game_identifier)))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to enter game: {e}")))?;

    Ok(())
}

pub async fn create_character(
    character: Option<Character>,
    game_identifier: &str,
    db: &Surreal<Any>,
    team: u32,
) -> Result<Character, AppError> {
    let game_id = RecordId::new("game", game_identifier.to_owned());
    let mut character_count_resp = db
        .query("RETURN count(SELECT id FROM playing_in WHERE out.identifier=$game)")
        .bind(("game", game_identifier.to_owned()))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to count characters: {}", e)))?;
    let character_count: Option<u32> = character_count_resp.take(0).map_err(|e| {
        AppError::InternalServerError(format!("Failed to parse character count: {}", e))
    })?;
    if character_count >= Some(24) {
        return Err(AppError::GameFull("Game is full".to_string()));
    }

    let mut character = character.unwrap_or_else(Character::random);
    character.team = team + 1;
    character.statistics.game = game_identifier.to_owned();

    let id = RecordId::new("character", character.identifier.as_str());

    // Bind via serde_json::Value to bypass the SurrealDB SDK's bespoke type
    // serializer, which collapses externally-tagged enums and Option fields.
    // The generic JSON bind path round-trips cleanly. Mirrors the pattern in
    // save_game in api/src/games.rs.
    let body = serde_json::to_value(&character)
        .map_err(|e| AppError::InternalServerError(format!("Failed to encode character: {}", e)))?;
    db.query("UPSERT $rid CONTENT $body")
        .bind(("rid", id.clone()))
        .bind(("body", body))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to create character: {}", e)))?;
    let new_character = Some(character);

    db.query("RELATE $character->playing_in->$game")
        .bind(("character", id.clone()))
        .bind(("game", game_id.clone()))
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Failed to connect character to game: {}", e))
        })?;

    let new_object: Item = Item::new_random(None);
    let new_object_id: RecordId = RecordId::new("item", new_object.identifier.as_str());
    let item_body = serde_json::to_value(&new_object)
        .map_err(|e| AppError::InternalServerError(format!("Failed to encode item: {}", e)))?;
    db.query("UPSERT $rid CONTENT $body")
        .bind(("rid", new_object_id.clone()))
        .bind(("body", item_body))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to create item: {}", e)))?;
    db.query("RELATE $character->owns->$item")
        .bind(("character", id.clone()))
        .bind(("item", new_object_id.clone()))
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Failed to create owns relation: {}", e))
        })?;

    if let Some(character) = new_character {
        Ok(character)
    } else {
        Err(AppError::InternalServerError(
            "Failed to create character".to_string(),
        ))
    }
}

pub async fn character_delete(
    Path((game_identifier, character_identifier)): Path<(String, String)>,
    Extension(AuthDb(db)): Extension<AuthDb>,
) -> Result<StatusCode, AppError> {
    // Only the owner may delete. The schema makes foreign deletes silent
    // no-ops, so ask explicitly: 404 when missing, 403 when not ours.
    require_owned_character(&db, &character_identifier).await?;

    // The character was created with RecordId::new("character", identifier),
    // so its record ID is `character:<identifier>`. Delete by RecordId.
    let character_rid = surrealdb_types::RecordId::new("character", character_identifier.as_str());

    // Delete the playing_in edge (character->playing_in->game: in =
    // character, out = game).
    let _ = db
        .query(
            "DELETE playing_in WHERE in.identifier = $character_id AND out.identifier = $game_id",
        )
        .bind(("game_id", game_identifier.to_string()))
        .bind(("character_id", character_identifier.to_string()))
        .await;

    let _: Option<serde_json::Value> = db
        .delete(character_rid)
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to delete character: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn character_update(
    Path((_game_identifier, _character_identifier)): Path<(Uuid, Uuid)>,
    Extension(AuthDb(db)): Extension<AuthDb>,
    Json(payload): Json<EditCharacter>,
) -> Result<StatusCode, AppError> {
    // Only the owner may mutate. The schema makes foreign updates silent
    // no-ops (0 rows), which this handler would otherwise report as a
    // 500 — ask explicitly instead: 404 when missing, 403 when not ours.
    require_owned_character(&db, &payload.identifier).await?;

    // Validate input
    if let Err(e) = validator::Validate::validate(&payload) {
        return Err(AppError::ValidationError(format!("{}", e)));
    }

    // Use raw JSON to bypass SDK deserializer
    let mut response = db
        .query("UPDATE character SET name = $name, avatar = $avatar WHERE identifier = $identifier")
        .bind(("identifier", payload.identifier.clone()))
        .bind(("name", payload.name.clone()))
        .bind(("avatar", Some(payload.avatar.clone())))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to update character: {}", e)))?;

    let updated: Vec<serde_json::Value> = response
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to check update: {}", e)))?;

    if updated.is_empty() {
        Err(AppError::InternalServerError(
            "Failed to update character".into(),
        ))
    } else {
        Ok(StatusCode::OK)
    }
}

pub async fn character_detail(
    Path((_, character_identifier)): Path<(Uuid, Uuid)>,
    Extension(AuthDb(db)): Extension<AuthDb>,
) -> Result<Json<Character>, AppError> {
    Ok(Json(
        fetch_full_character(&db, &character_identifier.to_string()).await?,
    ))
}

pub async fn character_log(
    Path((_, identifier)): Path<(Uuid, Uuid)>,
    Extension(AuthDb(db)): Extension<AuthDb>,
) -> Result<Json<Vec<GameMessage>>, AppError> {
    let identifier = identifier.to_string();
    let mut result = db
        .query("SELECT * FROM fn::get_messages_by_character_id($identifier)")
        .bind(("identifier", identifier))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to fetch logs: {}", e)))?;

    let rows: Vec<SerdeWrapper<crate::games::GameLog>> = result
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to take logs: {}", e)))?;
    let rows: Vec<crate::games::GameLog> = rows.into_iter().map(|w| w.0).collect();
    let logs: Vec<GameMessage> = rows.into_iter().map(GameMessage::from).collect();
    Ok(Json(logs))
}

/// `POST /api/characters/{identifier}/avatar` — avatar upload for a
/// standalone owned character, so pages like My Goblins can offer the
/// action outside a game. Same storage rules as the game-scoped route.
pub async fn upload_owned_character_avatar(
    Path(identifier): Path<Uuid>,
    State(state): State<AppState>,
    Extension(AuthDb(db)): Extension<AuthDb>,
    multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    store_character_avatar(&state, &db, identifier.to_string(), multipart).await
}

/// Shared body of both avatar upload routes: enforce ownership, then
/// read the `avatar` file field, validate it, store it, and record the
/// storage path on the character row.
async fn store_character_avatar(
    state: &AppState,
    db: &Surreal<Any>,
    character_identifier: String,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    // Only the owner may change the avatar; same explicit gate as the
    // other mutation paths (404 missing / 403 foreign).
    require_owned_character(db, &character_identifier).await?;

    // Find the file field
    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart field: {}", e)))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "avatar" {
            filename = field.file_name().map(|s| s.to_string());
            file_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read file data: {}", e)))?
                    .to_vec(),
            );
            break;
        }
    }

    let file_data = file_data.ok_or_else(|| {
        AppError::BadRequest("No file uploaded. Field 'avatar' required.".to_string())
    })?;

    let filename =
        filename.ok_or_else(|| AppError::BadRequest("No filename provided".to_string()))?;

    // Validate upload
    let constraints = UploadConstraints::default();
    validate_upload(&file_data, &filename, &constraints)?;

    // Generate storage path: avatars/{character_id}.{extension}
    let extension = std::path::Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| AppError::ValidationError("Invalid filename".to_string()))?;

    let storage_path = format!("avatars/{}.{}", character_identifier, extension);

    // Save file
    let saved_path = state.storage.save(&storage_path, &file_data).await?;
    let public_url = state.storage.public_url(&saved_path);

    // Update character avatar field in database
    let mut response = db
        .query("UPDATE character SET avatar = $avatar WHERE identifier = $identifier;")
        .bind(("identifier", character_identifier.clone()))
        .bind(("avatar", Some(saved_path.clone())))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to update character: {}", e)))?;

    let updated: Vec<serde_json::Value> = response
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to check update: {}", e)))?;

    if updated.is_empty() {
        Err(AppError::InternalServerError(
            "Failed to update character avatar".into(),
        ))
    } else {
        Ok(Json(serde_json::json!({
            "url": public_url,
            "path": saved_path
        })))
    }
}

/// `POST /api/characters` — create a persistent character owned by the
/// caller (`created_by = $auth` via the schema's `VALUE $auth`).
///
/// Standalone by design: traits, brain, and terrain affinity are rolled
/// by [`Character::new`], but no team, game relation, or items attach —
/// those arrive on game entry. `team` stays 0 until a game assigns one.
pub async fn create_owned_character(
    Extension(AuthDb(db)): Extension<AuthDb>,
    Json(payload): Json<CreateOwnedCharacter>,
) -> Result<(StatusCode, Json<Character>), AppError> {
    let payload = CreateOwnedCharacter {
        name: generated_unless_present(payload.name),
        clan_name: generated_unless_present(payload.clan_name),
    };
    if let Err(e) = validator::Validate::validate(&payload) {
        return Err(AppError::ValidationError(format!("{}", e)));
    }

    // Generation happens before any await, so the RNG is never held
    // across one.
    let (name, generated_clan) = {
        let mut rng = rand::rng();
        (
            payload.name.unwrap_or_else(|| goblin_name(&mut rng)),
            payload.clan_name.unwrap_or_else(|| clan_name(&mut rng)),
        )
    };
    let mut character = Character::new(name, None, None);
    character.clan_name = generated_clan;

    let body = serde_json::to_value(&character)
        .map_err(|e| AppError::InternalServerError(format!("Failed to encode character: {e}")))?;
    let id = RecordId::new("character", character.identifier.as_str());
    db.query("UPSERT $rid CONTENT $body")
        .bind(("rid", id))
        .bind(("body", body))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to create character: {e}")))?;

    Ok((StatusCode::CREATED, Json(character)))
}

/// `GET /api/characters` — every character created by the caller
/// (`created_by = $auth`, backed by the `character_created_by` index).
pub async fn list_owned_characters(
    Extension(AuthDb(db)): Extension<AuthDb>,
) -> Result<Json<OwnedCharacterList>, AppError> {
    let mut response = db
        .query("SELECT * FROM character WHERE created_by = $auth ORDER BY identifier")
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to list characters: {e}")))?;
    let rows: Vec<serde_json::Value> = response
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to take characters: {e}")))?;
    let characters: Vec<Character> = rows
        .into_iter()
        .filter_map(|row| serde_json::from_value(row).ok())
        .collect();
    Ok(Json(OwnedCharacterList { characters }))
}

/// `GET /api/characters/{identifier}` — full character detail. Reads are
/// unrestricted by the schema (`select FULL`), matching the game-scoped
/// detail route; only mutations are owner-gated.
pub async fn owned_character_detail(
    Path(identifier): Path<Uuid>,
    Extension(AuthDb(db)): Extension<AuthDb>,
) -> Result<Json<Character>, AppError> {
    Ok(Json(
        fetch_full_character(&db, &identifier.to_string()).await?,
    ))
}

/// `PUT /api/characters/{identifier}` — owner-only edit of `name` and
/// `clan_name`.
///
/// Standalone characters are always editable. A character that is in a
/// game can only be edited while that game is `NotStarted`, mirroring
/// the `editable` expression in `fn::get_full_character`.
pub async fn owned_character_update(
    Path(identifier): Path<Uuid>,
    Extension(AuthDb(db)): Extension<AuthDb>,
    Json(payload): Json<EditOwnedCharacter>,
) -> Result<Json<Character>, AppError> {
    let identifier = identifier.to_string();
    let row = require_owned_character(&db, &identifier).await?;

    let payload = EditOwnedCharacter {
        name: payload.name.map(|name| name.trim().to_string()),
        clan_name: payload.clan_name.map(|clan| clan.trim().to_string()),
    };
    if let Err(e) = validator::Validate::validate(&payload) {
        return Err(AppError::ValidationError(format!("{}", e)));
    }

    let mut status_response = db
        .query(
            "SELECT (->playing_in->game.status)[0] AS game_status
             FROM character WHERE identifier = $identifier",
        )
        .bind(("identifier", identifier.clone()))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to load game status: {e}")))?;
    let statuses: Vec<serde_json::Value> = status_response
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to take game status: {e}")))?;
    let game_status = statuses
        .first()
        .and_then(|status_row| status_row["game_status"].as_str());
    if let Some(status) = game_status
        && status != "NotStarted"
    {
        return Err(AppError::Conflict(
            "Character is in a game that has already started".to_string(),
        ));
    }

    // Read-modify-write so omitted fields keep their current values.
    let mut name = row["name"].as_str().unwrap_or_default().to_string();
    let mut clan = row["clan_name"].as_str().unwrap_or_default().to_string();
    if let Some(new_name) = payload.name {
        name = new_name;
    }
    match payload.clan_name {
        None => {}
        Some(new_clan) if new_clan.is_empty() => {
            let mut rng = rand::rng();
            clan = clan_name(&mut rng);
        }
        Some(new_clan) => clan = new_clan,
    }

    db.query(
        "UPDATE character SET name = $name, clan_name = $clan_name
         WHERE identifier = $identifier",
    )
    .bind(("identifier", identifier.clone()))
    .bind(("name", name))
    .bind(("clan_name", clan))
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to update character: {e}")))?;

    Ok(Json(fetch_full_character(&db, &identifier).await?))
}

/// `DELETE /api/characters/{identifier}` — owner-only removal. Detaches
/// the character from any game first so no `playing_in` edges are left
/// behind (`playing_in`: in = character, out = game).
pub async fn owned_character_delete(
    Path(identifier): Path<Uuid>,
    Extension(AuthDb(db)): Extension<AuthDb>,
) -> Result<StatusCode, AppError> {
    let identifier = identifier.to_string();
    require_owned_character(&db, &identifier).await?;

    db.query("DELETE playing_in WHERE in.identifier = $identifier")
        .bind(("identifier", identifier.clone()))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to detach character: {e}")))?;

    let character_rid = RecordId::new("character", identifier.as_str());
    let _: Option<serde_json::Value> = db
        .delete(character_rid)
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to delete character: {e}")))?;

    Ok(StatusCode::NO_CONTENT)
}
