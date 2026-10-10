use axum::Json;
use game::games::Game;
use shared::messages::GameMessage;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb_types::{RecordId, SerdeWrapper};

use super::items::{save_area_items, save_character_items};
use super::{GameLog, MAX_MESSAGES};
use crate::AppError;
use crate::websocket::{GameBroadcaster, broadcast_game_message};

/// Persistence partition — the single seam over
/// [`shared::messages::Importance`]. Returns (persist, server-log-only).
pub(crate) fn partition_persist(logs: Vec<GameMessage>) -> (Vec<GameMessage>, Vec<GameMessage>) {
    logs.into_iter()
        .partition(|log| log.payload.importance() == shared::messages::Importance::Persist)
}

/// Full debug-feed line for one message (the server-log debugging channel
/// carries every message, both tiers). `CharacterMoved` gets its previous
/// position spelled out so `from -> to` is one grep away.
pub(crate) fn format_debug_line(game_id: &str, game_day: u32, log: &GameMessage) -> String {
    let mut line = format!(
        "message game={game_id} day={game_day} phase={} tick={} emit={} kind={:?} content={:?}",
        log.phase,
        log.tick,
        log.emit_index,
        log.payload.kind(),
        log.content
    );
    if let shared::messages::MessagePayload::CharacterMoved {
        character,
        from,
        to,
    } = &log.payload
    {
        line.push_str(&format!(
            " character={} from={}({}) to={}({})",
            character.name, from.name, from.identifier, to.name, to.identifier
        ));
    }
    line
}

pub(crate) async fn save_game(
    game: &mut Game,
    db: &Surreal<Any>,
    broadcaster: &GameBroadcaster,
) -> Result<Json<Game>, AppError> {
    let game_identifier = RecordId::new("game", game.identifier.clone());

    // Start transaction
    db.query("BEGIN TRANSACTION").await.map_err(|e| {
        AppError::InternalServerError(format!("Failed to start transaction: {}", e))
    })?;

    // Drain events accumulated during the most recent run_day_night_cycle.
    let logs: Vec<GameMessage> = std::mem::take(&mut game.messages);
    let game_day = game.day.unwrap_or_default();

    // Full debug feed: every message, both tiers, lands in server logs so
    // the dropped/derived stream stays debuggable (RUST_LOG=...=debug).
    if tracing::enabled!(tracing::Level::DEBUG) {
        for log in &logs {
            tracing::debug!("{}", format_debug_line(&game.identifier, game_day, log));
        }
    }

    // Persist + broadcast only state-dramatic messages; everything else
    // lives in the debug feed above. Tier source: shared importance().
    let (persist_logs, dropped_logs) = partition_persist(logs);
    if !dropped_logs.is_empty() {
        tracing::debug!(
            game_id = %game.identifier,
            dropped = dropped_logs.len(),
            "server-log-only messages not persisted this cycle"
        );
    }

    if !persist_logs.is_empty() {
        // Broadcast first so clients see updates even if persistence is slow.
        for log in &persist_logs {
            broadcast_game_message(broadcaster, &game.identifier, log.clone());
        }

        let game_logs: Vec<GameLog> = persist_logs
            .into_iter()
            .map(|log| GameLog {
                id: RecordId::new("message", log.identifier.as_str()),
                identifier: log.identifier,
                source: log.source,
                game_day,
                subject: log.subject,
                timestamp: log.timestamp,
                content: log.content,
                phase: log.phase,
                tick: log.tick,
                emit_index: log.emit_index,
                payload: serde_json::to_string(&log.payload).unwrap_or_else(|_| "null".to_string()),
            })
            .collect();

        // Check current message count for this game
        let current_count: u32 = db
            .query(
                "RETURN count(SELECT id FROM message WHERE string::starts_with(subject, $game_id))",
            )
            .bind(("game_id", game.identifier.clone()))
            .await
            .ok()
            .and_then(|mut r| r.take(0).ok().flatten())
            .unwrap_or(0);

        let new_messages_count = game_logs.len();
        let total_after_insert = current_count as usize + new_messages_count;

        // Log warning if approaching limit
        if current_count >= 9000 && current_count < MAX_MESSAGES as u32 {
            tracing::warn!(
                game_id = %game.identifier,
                current_count = %current_count,
                "Game message count approaching limit (9000+)"
            );
        }

        // Rotate messages if total would exceed MAX_MESSAGES
        if total_after_insert > MAX_MESSAGES {
            let messages_to_delete = total_after_insert - MAX_MESSAGES;
            tracing::info!(
                game_id = %game.identifier,
                current_count = %current_count,
                new_count = %new_messages_count,
                deleting = %messages_to_delete,
                "Rotating old messages to maintain {} message limit", MAX_MESSAGES
            );

            if let Err(e) = db
                .query(
                    r#"
                    DELETE message
                    WHERE string::starts_with(subject, $game_id)
                    ORDER BY timestamp ASC
                    LIMIT $delete_count
                    "#,
                )
                .bind(("game_id", game.identifier.clone()))
                .bind(("delete_count", messages_to_delete))
                .await
            {
                tracing::error!(
                    game_id = %game.identifier,
                    error = %e,
                    "Failed to rotate old messages"
                );
                // Continue anyway - this is not critical enough to fail the save
            }
        }

        // The structured `event` payload rides as a plain JSON string
        // on the `GameLog.event` field, sidestepping the SurrealDB SDK
        // serializer's habit of collapsing externally-tagged enums and
        // `serde_json::Value::Object` payloads to `{}` when bound into
        // an `object` column. mqi.3.
        let wrapped_logs: Vec<SerdeWrapper<GameLog>> =
            game_logs.into_iter().map(SerdeWrapper).collect();
        if let Err(e) = db
            .insert::<Vec<SerdeWrapper<GameLog>>>("message")
            .content(wrapped_logs)
            .await
        {
            let _ = db.query("ROLLBACK").await;
            return Err(AppError::InternalServerError(format!(
                "Failed to save game logs: {}",
                e
            )));
        }
    }

    let area_results = futures::future::join_all(game.areas.iter().map(|area| async {
        let id = RecordId::new("area", area.identifier.as_str());
        save_area_items(&area.items, id.clone(), db).await?;

        let mut area_without_items = area.clone();
        area_without_items.items = vec![];
        // Bind via serde_json::Value to bypass the SurrealDB SDK's bespoke
        // type serializer, which collapses externally-tagged enums and Option
        // fields. The generic JSON bind path round-trips cleanly.
        let body = serde_json::to_value(&area_without_items)
            .map_err(|e| AppError::InternalServerError(format!("Failed to encode area: {}", e)))?;
        db.query("UPSERT $rid CONTENT $body")
            .bind(("rid", id.clone()))
            .bind(("body", body))
            .await
            .map_err(|e| AppError::InternalServerError(format!("Failed to update area: {}", e)))
            .map(|_| ())
    }))
    .await;

    if let Some(err) = area_results.iter().find_map(|r| r.as_ref().err()) {
        let msg = format!("{}", err);
        let _ = db.query("ROLLBACK").await;
        return Err(AppError::InternalServerError(format!(
            "Failed to save area items: {}",
            msg
        )));
    }

    let character_results =
        futures::future::join_all(game.characters.iter().map(|character| async {
            let id = RecordId::new("character", character.identifier.as_str());
            if character.is_alive() {
                save_character_items(&character.items, id.clone(), db).await?;
            }

            let mut character_without_items = character.clone();
            character_without_items.items = vec![];
            // Same workaround as the area UPDATE above: serde_json::Value bypasses
            // the SDK's enum-collapsing serializer.
            let body = serde_json::to_value(&character_without_items).map_err(|e| {
                AppError::InternalServerError(format!("Failed to encode character: {}", e))
            })?;
            db.query("UPSERT $rid CONTENT $body")
                .bind(("rid", id.clone()))
                .bind(("body", body))
                .await
                .map_err(|e| {
                    AppError::InternalServerError(format!("Failed to update character: {}", e))
                })
                .map(|_| ())
        }))
        .await;

    if let Some(err) = character_results.iter().find_map(|r| r.as_ref().err()) {
        let msg = format!("{}", err);
        let _ = db.query("ROLLBACK").await;
        return Err(AppError::InternalServerError(format!(
            "Failed to save character items: {}",
            msg
        )));
    }

    // Persist mutable game fields explicitly. `db.update().content()` with the
    // SurrealDB SDK's bespoke serializer drops `Option<u32>` fields like `day`
    // (same family of bugs as the externally-tagged-enum collapse called out
    // around the message payload above), so we use a plain UPDATE query that
    // names the fields we want written.
    if let Err(e) = db
        .query("UPDATE $record_id SET day = $day, status = $status, config = $config")
        .bind(("record_id", game_identifier.clone()))
        .bind(("day", game.day.unwrap_or(0) as i64))
        .bind(("status", game.status.to_string()))
        .bind((
            "config",
            serde_json::to_value(&game.config).unwrap_or(serde_json::Value::Null),
        ))
        .await
    {
        let _ = db.query("ROLLBACK").await;
        return Err(AppError::InternalServerError(format!(
            "Failed to update game: {}",
            e
        )));
    }

    db.query("COMMIT").await.map_err(|e| {
        AppError::InternalServerError(format!("Failed to commit transaction: {}", e))
    })?;
    Ok(Json(game.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::messages::{AreaRef, CharacterRef, CharacterRef as C, MessageSource, Phase};

    fn log(payload: shared::messages::MessagePayload) -> GameMessage {
        GameMessage::new(
            MessageSource::Game("g".into()),
            1,
            Phase::DAY_START,
            3,
            4,
            "game:g".into(),
            "content".into(),
            payload,
        )
    }

    #[test]
    fn moved_log_line_carries_from_to() {
        use shared::messages::MessagePayload;
        let moved = log(MessagePayload::CharacterMoved {
            character: C {
                identifier: "c1".into(),
                name: "Glimmer".into(),
            },
            from: AreaRef {
                identifier: "a1".into(),
                name: "Hub".into(),
            },
            to: AreaRef {
                identifier: "a2".into(),
                name: "East Mire".into(),
            },
        });
        let line = format_debug_line("game-1", 2, &moved);
        assert!(
            line.contains("from=Hub") && line.contains("to=East Mire"),
            "previous position must be explicit: {line}"
        );
        assert!(line.contains("kind=CharacterMoved"), "{line}");
        assert!(
            line.contains("day=2") && line.contains("phase=06"),
            "{line}"
        );
        assert!(line.contains("character=Glimmer"), "{line}");
        // Identifiers too, for precise greps.
        assert!(line.contains("a1") && line.contains("a2"), "{line}");
    }

    #[test]
    fn debug_line_covers_plain_messages() {
        use shared::messages::MessagePayload;
        let killed = log(MessagePayload::CharacterKilled {
            victim: CharacterRef {
                identifier: "v".into(),
                name: "Victim".into(),
            },
            killer: None,
            cause: shared::afflictions::DeathCause::Combat,
        });
        let line = format_debug_line("game-1", 1, &killed);
        assert!(line.contains("kind=CharacterKilled"), "{line}");
        assert!(line.contains("content="), "{line}");
    }

    #[test]
    fn partition_drops_server_only_and_keeps_important() {
        use shared::messages::MessagePayload;
        let server_only = vec![
            log(MessagePayload::CharacterMoved {
                character: CharacterRef {
                    identifier: "c".into(),
                    name: "C".into(),
                },
                from: AreaRef {
                    identifier: "a".into(),
                    name: "A".into(),
                },
                to: AreaRef {
                    identifier: "b".into(),
                    name: "B".into(),
                },
            }),
            log(MessagePayload::PhaseStarted {
                day: 1,
                phase: Phase::DAY,
                weather_summary: None,
            }),
            log(MessagePayload::HungerBandChanged {
                character: CharacterRef {
                    identifier: "c".into(),
                    name: "C".into(),
                },
                from: shared::messages::HungerBand::Sated,
                to: shared::messages::HungerBand::Starving,
            }),
        ];
        let important = vec![
            log(MessagePayload::CycleStart {
                day: 1,
                phase: Phase::DAY_START,
            }),
            log(MessagePayload::CharacterKilled {
                victim: CharacterRef {
                    identifier: "v".into(),
                    name: "V".into(),
                },
                killer: None,
                cause: shared::afflictions::DeathCause::Combat,
            }),
        ];
        let mut all = server_only.clone();
        all.extend(important.clone());
        let (persist, dropped) = partition_persist(all);
        assert_eq!(persist.len(), 2);
        assert_eq!(dropped.len(), 3);
        assert!(
            persist
                .iter()
                .all(|l| l.payload.importance() == shared::messages::Importance::Persist)
        );
        assert!(
            dropped
                .iter()
                .all(|l| l.payload.importance() == shared::messages::Importance::ServerLogOnly)
        );
    }
}
