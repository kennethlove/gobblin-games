//! Round-trip test for hangrier_games-33r (PR1: combat slice).
//!
//! Verifies that combat events emitted by `Character::attacks`,
//! `attack_contest`, and `apply_combat_results` are now collected into
//! `Game.messages` (rather than silently dropped by the old
//! `try_log_action` no-op), with `MessageSource::Character(identifier)`.

use game::areas::{Area, AreaDetails};
use game::characters::Character;
use game::games::Game;
use game::messages::MessageSource;
use game::terrain::{BaseTerrain, TerrainType};

/// Pin two characters in the same area, force one to be obviously stronger,
/// run a single day cycle, and assert combat narration reaches `game.messages`
/// tagged to the attacker via `MessageSource::Character(identifier)`.
#[test]
fn combat_events_reach_game_messages_with_character_source() {
    let mut game = Game::new("event-unification-combat-test");

    // Single-area arena keeps both characters guaranteed-adjacent.
    let area_details = AreaDetails::new_with_terrain(
        Some("Hub".to_string()),
        Area::Hub,
        TerrainType::new(BaseTerrain::Clearing, vec![]).unwrap(),
    );
    game.areas.push(area_details);
    let area = game.areas[0].area.unwrap();

    // Strong attacker, frail defender — combat is overwhelmingly likely.
    let mut attacker = Character::random();
    attacker.name = "Attacker".to_string();
    attacker.area = area;
    attacker.blood = 1000;
    attacker.attributes.strength = 60;
    attacker.statistics.game = game.identifier.clone();

    let mut defender = Character::random();
    defender.name = "Defender".to_string();
    defender.area = area;
    defender.blood = 50;
    defender.attributes.defense = 0;
    defender.statistics.game = game.identifier.clone();
    // Different clan so attacker classifies them as an enemy.
    defender.clan = (attacker.clan % 8) + 1;

    let attacker_id = attacker.identifier.clone();
    let defender_id = defender.identifier.clone();

    game.characters.push(attacker);
    game.characters.push(defender);

    let messages_before = game.messages.len();

    // Run several cycles to give combat plenty of opportunity to fire,
    // even if the brain occasionally picks Rest/Hide/Move.
    for _ in 0..6 {
        game.run_phase(shared::messages::Phase::Day)
            .expect("day cycle ran");
        if game.messages.iter().any(is_character_sourced) {
            break;
        }
    }

    let new_messages: Vec<_> = game.messages.iter().skip(messages_before).collect();

    assert!(
        !new_messages.is_empty(),
        "expected at least one game message after running cycles"
    );

    let character_sourced: Vec<_> = new_messages
        .iter()
        .filter(|m| is_character_sourced(m))
        .collect();

    assert!(
        !character_sourced.is_empty(),
        "expected at least one MessageSource::Character(_) entry, got sources: {:?}",
        new_messages.iter().map(|m| &m.source).collect::<Vec<_>>()
    );

    // Every character-sourced message should carry an identifier matching
    // one of our two characters (no stray identifiers).
    for msg in &character_sourced {
        match &msg.source {
            MessageSource::Character(id) => {
                assert!(
                    id == &attacker_id.to_string() || id == &defender_id.to_string(),
                    "unexpected character identifier in message source: {id}"
                );
            }
            other => panic!("filter let through non-Character source: {other:?}"),
        }
    }
}

fn is_character_sourced(m: &game::messages::GameMessage) -> bool {
    matches!(m.source, MessageSource::Character(_))
}
