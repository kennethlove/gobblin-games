//! Snapshots patron affinity evolution given a deterministic set of
//! audience events. Regenerate with `cargo insta accept` after intentional
//! rebalances.

use areas::{Area, AreaDetails};
use game::characters::Character;
use game::games::Game;
use game::patrons::{PatronContext, translate, update_affinities};
use rand::SeedableRng;
use shared::messages::{CharacterRef, MessagePayload};
use world::terrain::{BaseTerrain, TerrainType};

fn build_test_game() -> Game {
    let mut game = Game::new("snapshot-test");
    let _ = game.start();

    let area = AreaDetails::new_with_terrain(
        Some("Arena".to_string()),
        Area::Sector1,
        TerrainType::new(BaseTerrain::Clearing, vec![]).unwrap(),
    );
    game.areas.push(area);

    for i in 0..6 {
        let mut character = Character::new(format!("Character{}", i), Some((i % 8) + 1), None);
        character.area = Area::Sector1;
        character.blood = 1000;
        character.statistics.game = game.identifier.clone();
        game.characters.push(character);
    }

    game
}

fn tref(name: &str) -> CharacterRef {
    CharacterRef {
        identifier: name.into(),
        name: name.into(),
    }
}

#[test]
fn three_cycle_affinity() {
    let mut game = build_test_game();

    // Seed patrons deterministically.
    let mut rng = rand::rngs::SmallRng::seed_from_u64(0xDEAD_BEEF);
    game.spawn_patrons(&mut rng);

    // Simulate three cycles worth of events deterministically.
    // Cycle 1: a kill + an alliance
    let cycle1_payloads = vec![
        MessagePayload::CharacterKilled {
            victim: tref("Character0"),
            killer: Some(tref("Character1")),
            cause: shared::afflictions::DeathCause::Character("spear".into()),
        },
        MessagePayload::AllianceFormed {
            members: vec![tref("Character2"), tref("Character3")],
        },
    ];

    // Cycle 2: a betrayal
    let cycle2_payloads = vec![MessagePayload::BetrayalTriggered {
        betrayer: tref("Character2"),
        victim: tref("Character3"),
    }];

    // Cycle 3: another kill (environmental)
    let cycle3_payloads = vec![MessagePayload::CharacterKilled {
        victim: tref("Character4"),
        killer: None,
        cause: shared::afflictions::DeathCause::Hazard(
            shared::afflictions::HazardKind::FallingDebris,
        ),
    }];

    for payloads in [cycle1_payloads, cycle2_payloads, cycle3_payloads] {
        let ctx = PatronContext::new(&game);
        let mut events = Vec::new();
        for p in &payloads {
            events.extend(translate(p, &ctx));
        }
        update_affinities(&mut game, &events);
    }

    let snapshot = game.patron_affinity_snapshot();
    insta::assert_yaml_snapshot!(snapshot);
}
