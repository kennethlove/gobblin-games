use areas::events::AreaEvent;
use areas::{Area, AreaDetails};
use characters::Character;
use game::games::Game;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use world::terrain::{BaseTerrain, TerrainType};

#[test]
fn test_wildfire_in_forest_kills_characters() {
    let mut game = Game::new("test-game");

    // Create area with Forest terrain
    let area_details = AreaDetails::new_with_terrain(
        Some("Forest Area".to_string()),
        Area::Sector1,
        TerrainType::new(BaseTerrain::Forest, vec![]).unwrap(),
    );
    game.areas.push(area_details);

    let area_name = game.areas[0].area.unwrap();

    // Create character in forest area with low health
    let mut character = Character::random();
    character.area = area_name;
    character.blood = 500;
    character.terrain_affinity = vec![]; // No affinity bonus
    character.statistics.game = game.identifier.clone(); // Set game identifier
    game.characters.push(character.clone());

    // Process wildfire event (catastrophic in forest)
    let event = AreaEvent::Wildfire;

    // Run 10 times, expect at least some deaths
    let mut death_count = 0;
    let mut rng = SmallRng::seed_from_u64(42);
    for _ in 0..10 {
        let mut test_game = game.clone();
        test_game
            .process_event_for_area(&area_name, &event, &mut rng)
            .unwrap();
        if test_game.characters[0].effective_health() == 0 {
            death_count += 1;
        }
    }

    assert!(
        death_count > 0,
        "Catastrophic wildfire should kill some characters"
    );
}

#[test]
fn test_wildfire_in_desert_minor_impact() {
    let mut game = Game::new("test-game");

    // Create area with Desert terrain
    let area_details = AreaDetails::new_with_terrain(
        Some("Desert Area".to_string()),
        Area::Sector4,
        TerrainType::new(BaseTerrain::Desert, vec![]).unwrap(),
    );
    game.areas.push(area_details);

    let area_name = game.areas[0].area.unwrap();

    // Create character
    let mut character = Character::random();
    character.area = Area::Sector4;
    character.blood = 500;
    character.terrain_affinity = vec![];
    character.statistics.game = game.identifier.clone();
    game.characters.push(character);

    // Process wildfire (minor in desert)
    let event = AreaEvent::Wildfire;

    // Run 10 times, expect low death rate
    let mut death_count = 0;
    let mut rng = SmallRng::seed_from_u64(7);
    for _ in 0..10 {
        let mut test_game = game.clone();
        test_game
            .process_event_for_area(&area_name, &event, &mut rng)
            .unwrap();
        if test_game.characters[0].effective_health() == 0 {
            death_count += 1;
        }
    }

    assert!(
        death_count < 5,
        "Minor wildfire should rarely kill characters"
    );
}
