//! Terrain affinity rolls per goblin from that goblin's own RNG — team
//! never seeds it. Shape: one primary terrain drawn
//! uniformly from every `BaseTerrain`, plus a 40% chance of a second,
//! distinct terrain.

use game::characters::Character;
use game::clans::roll_terrain_affinity;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use strum::IntoEnumIterator;
use world::terrain::BaseTerrain;

#[test]
fn test_affinity_has_one_or_two_distinct_valid_terrains() {
    let mut rng = SmallRng::seed_from_u64(42);

    for _ in 0..500 {
        let affinities = roll_terrain_affinity(&mut rng);
        assert!(
            (1..=2).contains(&affinities.len()),
            "shape is 1 primary (+ optional 2nd), got {}",
            affinities.len()
        );
        if affinities.len() == 2 {
            assert_ne!(affinities[0], affinities[1], "terrains are distinct");
        }
        for terrain in affinities {
            assert!(
                BaseTerrain::iter().any(|t| t == terrain),
                "unknown terrain {terrain:?}"
            );
        }
    }
}

#[test]
fn test_affinity_primary_is_uniform_not_clan_keyed() {
    // Every BaseTerrain must show up as a primary over many seeded rolls —
    // affinity is uniformly random across all terrains, not a per-clan table.
    let mut seen = std::collections::HashSet::new();
    for i in 0..1000 {
        let mut rng = SmallRng::seed_from_u64(i);
        let affinities = roll_terrain_affinity(&mut rng);
        seen.insert(affinities[0]);
    }
    assert_eq!(seen.len(), BaseTerrain::iter().count());
}

#[test]
fn test_bonus_affinity_probability() {
    // Use seeded RNG for determinism (mirrors the unit test in clans.rs).
    // 1000 iterations gives tighter ±5% bound vs flaky 100-iter ±10%.
    let iterations = 1000;
    let mut bonus_count = 0;

    for i in 0..iterations {
        let mut rng = SmallRng::seed_from_u64(i);
        if roll_terrain_affinity(&mut rng).len() == 2 {
            bonus_count += 1;
        }
    }

    let percentage = (bonus_count as f64 / iterations as f64) * 100.0;
    assert!(
        (35.0..=45.0).contains(&percentage),
        "Expected ~40% bonus rate, got {:.1}%",
        percentage
    );
}

#[test]
fn test_character_affinity_rolls_regardless_of_team() {
    // Characters always carry a 1-2 terrain affinity — even with no team
    // assigned — because affinity belongs to the goblin, not the grouping.
    let teams: [Option<u32>; 9] = [
        None,
        Some(1),
        Some(2),
        Some(3),
        Some(4),
        Some(5),
        Some(6),
        Some(7),
        Some(8),
    ];
    for team in teams {
        let character = Character::new(format!("Goblin on team {team:?}"), team, None);

        assert!(
            (1..=2).contains(&character.terrain_affinity.len()),
            "team {team:?} should have 1-2 terrain affinities, got {}",
            character.terrain_affinity.len()
        );
        for terrain in &character.terrain_affinity {
            assert!(
                matches!(
                    terrain,
                    BaseTerrain::Clearing
                        | BaseTerrain::Forest
                        | BaseTerrain::Desert
                        | BaseTerrain::Tundra
                        | BaseTerrain::Wetlands
                        | BaseTerrain::Mountains
                        | BaseTerrain::UrbanRuins
                        | BaseTerrain::Jungle
                        | BaseTerrain::Grasslands
                        | BaseTerrain::Badlands
                        | BaseTerrain::Highlands
                        | BaseTerrain::Geothermal
                ),
                "Invalid terrain type for team {team:?}: {terrain:?}"
            );
        }
    }
}

#[test]
fn test_same_seed_same_affinity() {
    // Affinity depends only on the RNG — same seed, same roll, every time.
    let mut a = SmallRng::seed_from_u64(4242);
    let mut b = SmallRng::seed_from_u64(4242);
    assert_eq!(roll_terrain_affinity(&mut a), roll_terrain_affinity(&mut b));
}
