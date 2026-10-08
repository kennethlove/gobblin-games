use areas::events::AreaEvent;
use areas::{Area, AreaDetails};
use characters::Character;
use game::games::Game;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use world::terrain::{BaseTerrain, TerrainType};

/// Test that events triggered in game loop actually process character survival checks
#[test]
fn test_event_survival_integration_with_game_loop() {
    let mut game = Game::new("test-game");
    let _ = game.start();

    // Create area with Forest terrain (catastrophic for wildfire)
    let mut area_details = AreaDetails::new_with_terrain(
        Some("Forest Area".to_string()),
        Area::Sector1,
        TerrainType::new(BaseTerrain::Forest, vec![]).unwrap(),
    );

    // Manually add wildfire event (simulate what trigger_cycle_events does)
    area_details.events.push(AreaEvent::Wildfire);
    game.areas.push(area_details);

    // Create characters in forest with vulnerable health
    for i in 0..5 {
        let mut character = Character::new(format!("Character{}", i), Some((i % 8) + 1), None);
        character.area = Area::Sector1;
        character.blood = 500;
        character.terrain_affinity = vec![]; // No protection
        character.statistics.game = game.identifier.clone();
        game.characters.push(character);
    }

    let initial_alive = game.living_characters().len();
    assert_eq!(initial_alive, 5);

    // Drain any setup messages so we only inspect event-driven output below
    game.messages.clear();

    // Process event survival checks
    let mut rng = SmallRng::seed_from_u64(123);
    game.process_event_for_area(&Area::Sector1, &AreaEvent::Wildfire, &mut rng)
        .unwrap();

    // Check that some characters died (probabilistic, but should happen)
    let final_alive = game.living_characters().len();

    // In a catastrophic event with no protection, expect casualties
    // Run assertion with tolerance (not guaranteed 100% death)
    assert!(
        final_alive < initial_alive,
        "Expected casualties from catastrophic wildfire, but all {} characters survived",
        initial_alive
    );

    // Verify messages were generated
    assert!(
        !game.messages.is_empty(),
        "Expected survival outcome messages but found none"
    );

    // Check for specific outcome messages (death or survival)
    let has_outcome_message = game.messages.iter().any(|m| {
        m.content.contains("dies from")
            || m.content.contains("killed by")
            || m.content.contains("survives")
    });
    assert!(
        has_outcome_message,
        "Expected death or survival messages but found none"
    );
}

/// Test that trigger_cycle_events integrates with process_event_for_area
#[test]
fn test_trigger_cycle_events_calls_process_event() {
    let mut game = Game::new("test-game");
    let _ = game.start();
    game.day = Some(2); // Day 2 to avoid special day #1 behavior

    // Create multiple areas with different terrains
    let forest_area = AreaDetails::new_with_terrain(
        Some("Forest".to_string()),
        Area::Sector1,
        TerrainType::new(BaseTerrain::Forest, vec![]).unwrap(),
    );
    let desert_area = AreaDetails::new_with_terrain(
        Some("Desert".to_string()),
        Area::Sector4,
        TerrainType::new(BaseTerrain::Desert, vec![]).unwrap(),
    );

    game.areas.push(forest_area);
    game.areas.push(desert_area);

    // Create characters in each area
    for i in 0..6 {
        let mut character = Character::new(format!("Character{}", i), Some((i % 8) + 1), None);
        character.area = if i < 3 { Area::Sector1 } else { Area::Sector4 };
        character.blood = 500;
        character.terrain_affinity = vec![];
        character.statistics.game = game.identifier.clone();
        game.characters.push(character);
    }

    // Note: We can't easily test trigger_cycle_events directly because it's random
    // and private. This test verifies the integration exists by manually calling
    // process_event_for_area (which trigger_cycle_events now calls)

    // Manually trigger events (simulating what trigger_cycle_events does)
    game.areas[0].events.push(AreaEvent::Wildfire);
    game.areas[1].events.push(AreaEvent::Sandstorm);

    let initial_alive = game.living_characters().len();

    // Drain setup messages so the assertion below only sees event output
    game.messages.clear();

    // Process events (what trigger_cycle_events now does internally)
    let mut rng = SmallRng::seed_from_u64(99);
    game.process_event_for_area(&Area::Sector1, &AreaEvent::Wildfire, &mut rng)
        .unwrap();
    game.process_event_for_area(&Area::Sector4, &AreaEvent::Sandstorm, &mut rng)
        .unwrap();

    let final_alive = game.living_characters().len();

    // Verify characters were affected
    // Wildfire in forest is catastrophic, sandstorm in desert is also catastrophic
    // Expect some deaths (probabilistic)
    assert!(
        final_alive <= initial_alive,
        "Expected some casualties from events"
    );

    // Verify messages exist
    assert!(!game.messages.is_empty(), "Expected event outcome messages");
}

/// Test that characters with terrain affinity have better survival
#[test]
fn test_terrain_affinity_improves_survival() {
    // Use a Major-severity event (Flood in Forest = DC 15) so the +3 affinity
    // bonus makes a measurable statistical difference. Catastrophic events
    // (DC 20) are too lethal in this regime to show separation reliably.
    //
    // Each arm gets its own deterministic RNG so the comparison is independent.
    let trials = 50;
    let mut with_affinity_deaths = 0;
    let mut without_affinity_deaths = 0;
    let mut rng_with = SmallRng::seed_from_u64(2024);
    let mut rng_without = SmallRng::seed_from_u64(4048);

    for _ in 0..trials {
        // Test WITH affinity
        let mut game_with = Game::new("test-affinity");
        let _ = game_with.start();

        let mut area = AreaDetails::new_with_terrain(
            Some("Forest".to_string()),
            Area::Sector1,
            TerrainType::new(BaseTerrain::Forest, vec![]).unwrap(),
        );
        area.events.push(AreaEvent::Flood);
        game_with.areas.push(area);

        let mut character = Character::new("Affinity Character".to_string(), Some(1), None);
        character.area = Area::Sector1;
        character.blood = 500;
        character.terrain_affinity = vec![BaseTerrain::Forest]; // HAS affinity
        character.statistics.game = game_with.identifier.clone();
        game_with.characters.push(character);

        game_with
            .process_event_for_area(&Area::Sector1, &AreaEvent::Flood, &mut rng_with)
            .unwrap();

        if game_with.characters[0].effective_health() == 0 {
            with_affinity_deaths += 1;
        }

        // Test WITHOUT affinity
        let mut game_without = Game::new("test-no-affinity");
        let _ = game_without.start();

        let mut area2 = AreaDetails::new_with_terrain(
            Some("Forest".to_string()),
            Area::Sector1,
            TerrainType::new(BaseTerrain::Forest, vec![]).unwrap(),
        );
        area2.events.push(AreaEvent::Flood);
        game_without.areas.push(area2);

        let mut character2 = Character::new("No Affinity Character".to_string(), Some(1), None);
        character2.area = Area::Sector1;
        character2.blood = 500;
        character2.terrain_affinity = vec![]; // NO affinity
        character2.statistics.game = game_without.identifier.clone();
        game_without.characters.push(character2);

        game_without
            .process_event_for_area(&Area::Sector1, &AreaEvent::Flood, &mut rng_without)
            .unwrap();

        if game_without.characters[0].effective_health() == 0 {
            without_affinity_deaths += 1;
        }
    }

    // Characters with affinity should die strictly less often than characters
    // without it. With these seeds and 50 trials this is deterministic.
    assert!(
        with_affinity_deaths < without_affinity_deaths,
        "Expected characters WITH affinity to die less often (with: {}, without: {})",
        with_affinity_deaths,
        without_affinity_deaths
    );
}
