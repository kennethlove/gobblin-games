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
use serde::{Deserialize, Serialize};
use shared::EditCharacter;
use std::sync::LazyLock;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb_types::{RecordId, SerdeWrapper};
use uuid::Uuid;

pub static CHARACTERS_ROUTER: LazyLock<Router<AppState>> = LazyLock::new(|| {
    Router::new()
        .route("/", get(game_characters))
        .route(
            "/{identifier}",
            get(character_detail)
                .delete(character_delete)
                .put(character_update),
        )
        .route("/{identifier}/avatar", post(upload_avatar))
        .route("/{identifier}/log", get(character_log))
});

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CharacterItemEdge {
    #[serde(rename = "in")]
    pub character: RecordId,
    #[serde(rename = "out")]
    pub item: RecordId,
}

pub async fn create_character(
    character: Option<Character>,
    game_identifier: &str,
    db: &Surreal<Any>,
    clan: u32,
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
    character.clan = clan + 1;
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
    // The character was created with RecordId::new("character", identifier),
    // so its record ID is `character:<identifier>`. Delete by RecordId.
    let character_rid = surrealdb_types::RecordId::new("character", character_identifier.as_str());

    // Delete the playing_in edge (character->playing_in->game)
    let _ = db
        .query(
            "DELETE playing_in WHERE in.identifier = $game_id AND out.identifier = $character_id",
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
    let character_identifier = character_identifier.to_string();
    let mut result = db
        .query("SELECT * FROM fn::get_full_character($identifier);")
        .bind(("identifier", character_identifier))
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to fetch character: {}", e)))?;

    // Take as raw JSON to bypass SurrealDB SDK custom deserializer (chokes
    // on null fields like `game_day: null` inside Option<T>).
    let raw: Vec<serde_json::Value> = result
        .take(0)
        .map_err(|e| AppError::InternalServerError(format!("Failed to take character: {}", e)))?;

    let character: Option<Character> = raw
        .into_iter()
        .next()
        .and_then(|v| serde_json::from_value(v).ok());

    match character {
        Some(character) => Ok(Json(character)),
        None => Err(AppError::NotFound("Character not found".to_string())),
    }
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

/// Upload avatar for a character
/// Accepts multipart/form-data with a file field named "avatar"
pub async fn upload_avatar(
    Path((_, character_identifier)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Extension(AuthDb(db)): Extension<AuthDb>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    let character_identifier = character_identifier.to_string();

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
