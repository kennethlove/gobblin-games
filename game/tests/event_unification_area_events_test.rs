//! Round-trip test for event unification (PR3: area-event survival slice).
//!
//! Verifies that area-event narration emitted by `process_event_for_area`
//! and `announce_area_events` reaches `Game.messages` with the correct
//! `MessageSource::Area(_)` tagging, and that per-character survival
//! outcomes still surface as `MessageSource::Character(_)` entries. Together
//! these confirm the full event slice is unified through `Game.messages`.

use areas::events::AreaEvent;
use areas::{Area, AreaDetails};
use characters::Character;
use game::games::Game;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use shared::messages::{GameMessage, MessageSource};
use world::terrain::{BaseTerrain, TerrainType};

/// Drop two characters into a single area, fire an `AreaEvent` directly via
/// `process_event_for_area`, and assert that:
/// - one `MessageSource::Area(_)` line announces the event itself
/// - per-character survival narration appears as `MessageSource::Character(_)`
///   carrying the matching character identifier
#[test]
fn area_event_survival_narration_reaches_game_messages() {
    let mut game = Game::new("event-unification-area-events-test");

    let area_details = AreaDetails::new_with_terrain(
        Some("Hub".to_string()),
        Area::Hub,
        TerrainType::new(BaseTerrain::Clearing, vec![]).unwrap(),
    );
    game.areas.push(area_details);
    let area = game.areas[0].area.unwrap();

    let mut alpha = Character::random();
    alpha.name = "Alpha".to_string();
    alpha.area = area;
    alpha.blood = 1000;
    alpha.statistics.game = game.identifier.clone();
    let alpha_id = alpha.identifier.clone();

    let mut bravo = Character::random();
    bravo.name = "Bravo".to_string();
    bravo.area = area;
    bravo.blood = 1000;
    bravo.statistics.game = game.identifier.clone();
    bravo.team = (alpha.team % 8) + 1;
    let bravo_id = bravo.identifier.clone();

    game.characters.push(alpha);
    game.characters.push(bravo);

    let mut rng = SmallRng::seed_from_u64(0xC0FFEE);
    let event = AreaEvent::Wildfire;

    let messages_before = game.messages.len();

    game.process_event_for_area(&area, &event, &mut rng)
        .expect("process_event_for_area succeeded");

    let new_messages: Vec<&GameMessage> = game.messages.iter().skip(messages_before).collect();

    assert!(
        !new_messages.is_empty(),
        "expected at least one game message after processing area event"
    );

    // Exactly one MessageSource::Area(_) opening announcement, tagged to
    // our area name and using the canonical area:{name} subject.
    let area_sourced: Vec<&&GameMessage> = new_messages
        .iter()
        .filter(|m| matches!(m.source, MessageSource::Area(_)))
        .collect();
    assert_eq!(
        area_sourced.len(),
        1,
        "expected exactly one MessageSource::Area(_) announcement, got: {:?}",
        new_messages.iter().map(|m| &m.source).collect::<Vec<_>>()
    );
    match &area_sourced[0].source {
        MessageSource::Area(name) => assert_eq!(name, "Hub"),
        other => panic!("expected MessageSource::Area, got {other:?}"),
    }
    assert_eq!(
        area_sourced[0].subject,
        format!("{}:area:Hub", game.identifier)
    );

    // Per-character survival narration: at least one MessageSource::Character(_)
    // entry per character, identifiers must match our two characters.
    let character_sourced: Vec<&&GameMessage> = new_messages
        .iter()
        .filter(|m| matches!(m.source, MessageSource::Character(_)))
        .collect();
    assert!(
        !character_sourced.is_empty(),
        "expected at least one MessageSource::Character(_) survival outcome, \
         got sources: {:?}",
        new_messages.iter().map(|m| &m.source).collect::<Vec<_>>()
    );
    for msg in &character_sourced {
        match &msg.source {
            MessageSource::Character(id) => {
                assert!(
                    id == &alpha_id.to_string() || id == &bravo_id.to_string(),
                    "unexpected character identifier in message source: {id}"
                );
            }
            other => panic!("filter let through non-Character source: {other:?}"),
        }
    }
}
