//! Generates a full-day event log (a round of 24 goblins) as JSON at
//! `../commentary/day-log.json` (repo-sibling dir, per request). Plays
//! fresh games until one day contains combat + an area event + a death,
//! then dumps every message of that day with its structured payload.
use super::*;
use shared::messages::MessagePayload;
use std::path::Path;

fn kind_of(payload: &MessagePayload) -> &'static str {
    match payload {
        MessagePayload::Combat(_)
        | MessagePayload::CombatRoundStart { .. }
        | MessagePayload::CombatBeat(_)
        | MessagePayload::CombatJoined { .. }
        | MessagePayload::CombatEnded { .. } => "combat",
        MessagePayload::AreaEvent { .. } => "event",
        MessagePayload::CharacterKilled { .. } => "death",
        _ => "narrative",
    }
}

/// A fresh 24-goblin two-team game on the default 7-area arena (real
/// layout so hazards can fire and movement has a graph).
fn fresh_game() -> Game {
    let mut game = Game::new("Day Log");
    game.areas = areas::hex::default_layout()
        .iter()
        .map(|(a, _)| areas::AreaDetails::new(None, *a))
        .collect();
    for i in 0..24 {
        let mut c = Character::new(format!("Gob{i}"), None, None);
        c.team = if i % 2 == 0 { 1 } else { 2 };
        game.characters.push(c);
    }
    game.start().expect("start game");
    game
}

/// The engine seeds itself per phase, so a qualifying day is a matter
/// of attempts: a game can end (one team wiped) before any single day
/// carries all three conditions. Fresh game per attempt, bounded.
#[test]
fn day_log_json() {
    let mut rows: Option<Vec<serde_json::Value>> = None;
    'attempts: for _ in 0..40 {
        let mut game = fresh_game();
        while game.status == GameStatus::InProgress {
            let start = game.messages.len();
            game.run_full_day().expect("full day");
            let day_msgs = &game.messages[start..];
            let mut combat = false;
            let mut event = false;
            let mut death = false;
            for m in day_msgs {
                match &m.payload {
                    MessagePayload::Combat(_)
                    | MessagePayload::CombatRoundStart { .. }
                    | MessagePayload::CombatBeat(_)
                    | MessagePayload::CombatJoined { .. }
                    | MessagePayload::CombatEnded { .. } => combat = true,
                    MessagePayload::AreaEvent { .. } => event = true,
                    MessagePayload::CharacterKilled { .. } => death = true,
                    _ => {}
                }
            }
            if combat && event && death {
                rows = Some(
                    day_msgs
                        .iter()
                        .map(|m| {
                            // Illustrative in-world clock, derived only from
                            // existing state: day-minute = ord*120 (2h slots)
                            // + tick*30 + emit_index, normalized to a real
                            // HH:MM on a day that starts at 06.
                            let day_minute = (m.phase.ord() as u32) * 120
                                + (m.tick * 30 + m.emit_index).min(119);
                            let game_clock = format!(
                                "{:#02}:{:#02}",
                                (6 + day_minute / 60) % 24,
                                day_minute % 60
                            );
                            serde_json::json!({
                                "day": m.game_day,
                                "hour": m.phase.hour(),
                                "tick": m.tick,
                                "emit_index": m.emit_index,
                                // Real generation time (wall clock at push).
                                "timestamp": m.timestamp.to_rfc3339(),
                                "game_clock": game_clock,
                                "kind": kind_of(&m.payload),
                                "source": format!("{:?}", m.source),
                                "subject": m.subject,
                                "content": m.content,
                                "payload": serde_json::to_value(&m.payload).unwrap_or_default(),
                            })
                        })
                        .collect(),
                );
                break 'attempts;
            }
        }
    }
    let rows = rows.expect("a day with combat + an area event + a death within 40 attempts");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../commentary");
    std::fs::create_dir_all(&dir).expect("create commentary dir");
    let json = serde_json::to_string_pretty(&rows).expect("serialize day log");
    std::fs::write(dir.join("day-log.json"), &json).expect("write day-log.json");
    println!(
        "wrote {} messages to {}",
        rows.len(),
        dir.join("day-log.json").display()
    );
}
