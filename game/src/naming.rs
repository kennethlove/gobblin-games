//! Goblin name generation.
//!
//! Discworld-style goblin phrase names: short evocative phrases rather than
//! personal names, e.g. "Tears of the Mushroom" or "Shine on the Moon".
//! Everything is RNG-driven so callers stay deterministic when seeded.

use rand::Rng;
use rand::RngExt;

/// Leading plural nouns for the `<Plural> of the <Thing>` pattern.
const PLURALS: &[&str] = &[
    "Tears", "Drops", "Puddles", "Spores", "Warts", "Teeth", "Bones", "Shines", "Cinders",
    "Whispers", "Giggles", "Bubbles", "Grubs", "Beetles", "Sparks", "Crumbs",
];

/// Leading verbs for the `<Verb> on the <Thing>` pattern.
const ACTIONS: &[&str] = &[
    "Shine", "Crawl", "Creep", "Slip", "Slosh", "Gleam", "Gnaw", "Squeak", "Drip", "Stumble",
    "Wander", "Grumble", "Sniffle", "Waddle", "Lurk", "Flicker",
];

/// Trailing things the name points at.
const THINGS: &[&str] = &[
    "Mushroom",
    "Moon",
    "Bog",
    "Puddle",
    "Bone",
    "Rust",
    "Mud",
    "Straw",
    "Cinder",
    "Turnip",
    "Slug",
    "Flea",
    "Toadstool",
    "Candle",
    "Lantern",
    "Pebble",
    "Root",
    "Ditch",
];

fn pick<'a>(options: &'a [&'a str], rng: &mut impl Rng) -> &'a str {
    options[rng.random_range(0..options.len())]
}

/// Beasts for "Clan of the Cave Bear"-style names.
const CLAN_BEASTS: &[&str] = &[
    "Cave Bear",
    "Boar",
    "Wolf",
    "Raven",
    "Elk",
    "Serpent",
    "Stag",
    "Jackal",
    "Badger",
    "Heron",
    "Weasel",
    "Otter",
];

/// Personal names for "Jocko's Crew"-style names.
const CLAN_BOSS_NAMES: &[&str] = &[
    "Jocko", "Snag", "Mudd", "Rend", "Pib", "Tark", "Nib", "Grub", "Worm", "Fen", "Rook", "Bram",
];

/// Group nouns for "The Warriors"-style names.
const CLAN_GROUPS: &[&str] = &[
    "Warriors",
    "Hunters",
    "Drifters",
    "Outcasts",
    "Gatherers",
    "Watchers",
    "Breakers",
    "Skinners",
    "Foragers",
    "Brawlers",
    "Tinkers",
    "Wanderers",
];

/// Directions and places for "Eastern Forest Clan"-style names.
const CLAN_DIRECTIONS: &[&str] = &[
    "Eastern", "Western", "Northern", "Southern", "Upper", "Lower", "High", "Deep",
];
const CLAN_PLACES: &[&str] = &[
    "Forest", "Marsh", "Hills", "River", "Coast", "Valley", "Fens", "Moor", "Ridge", "Thicket",
    "Wold", "Hollow",
];

/// Generate a goblin name like "Tears of the Mushroom" or "Shine on the Moon".
pub fn goblin_name(rng: &mut impl Rng) -> String {
    if rng.random_bool(0.5) {
        format!("{} of the {}", pick(PLURALS, rng), pick(THINGS, rng))
    } else {
        format!("{} on the {}", pick(ACTIONS, rng), pick(THINGS, rng))
    }
}

/// Generate a persistent goblin clan name (lore only, no mechanics),
/// e.g. "Clan of the Cave Bear", "Jocko's Crew", "The Warriors",
/// or "Eastern Forest Clan".
pub fn clan_name(rng: &mut impl Rng) -> String {
    match rng.random_range(0..4) {
        0 => format!("Clan of the {}", pick(CLAN_BEASTS, rng)),
        1 => format!("{}'s Crew", pick(CLAN_BOSS_NAMES, rng)),
        2 => format!("The {}", pick(CLAN_GROUPS, rng)),
        _ => format!(
            "{} {} Clan",
            pick(CLAN_DIRECTIONS, rng),
            pick(CLAN_PLACES, rng)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    #[test]
    fn names_follow_one_of_the_two_patterns() {
        let mut rng = SmallRng::seed_from_u64(7);
        for _ in 0..200 {
            let name = goblin_name(&mut rng);
            let parts: Vec<&str> = name.split_whitespace().collect();
            assert_eq!(parts.len(), 4, "unexpected shape: {name}");
            match parts[1] {
                "of" => assert!(PLURALS.contains(&parts[0]), "{name}"),
                "on" => assert!(ACTIONS.contains(&parts[0]), "{name}"),
                other => panic!("unexpected middle word {other:?} in {name}"),
            }
            assert_eq!(parts[2], "the", "{name}");
            assert!(THINGS.contains(&parts[3]), "{name}");
        }
    }

    #[test]
    fn seeded_rng_is_deterministic() {
        let mut a = SmallRng::seed_from_u64(42);
        let mut b = SmallRng::seed_from_u64(42);
        assert_eq!(goblin_name(&mut a), goblin_name(&mut b));
    }

    #[test]
    fn clan_names_follow_one_of_the_four_patterns() {
        let mut rng = SmallRng::seed_from_u64(11);
        let mut seen = [false; 4];
        for _ in 0..400 {
            let name = clan_name(&mut rng);
            if let Some(beast) = name.strip_prefix("Clan of the ") {
                assert!(CLAN_BEASTS.contains(&beast), "{name}");
                seen[0] = true;
            } else if let Some(boss) = name.strip_suffix("'s Crew") {
                assert!(CLAN_BOSS_NAMES.contains(&boss), "{name}");
                seen[1] = true;
            } else if let Some(group) = name.strip_prefix("The ") {
                assert!(CLAN_GROUPS.contains(&group), "{name}");
                seen[2] = true;
            } else if let Some(place_part) = name.strip_suffix(" Clan") {
                let bits: Vec<&str> = place_part.split_whitespace().collect();
                assert_eq!(bits.len(), 2, "{name}");
                assert!(CLAN_DIRECTIONS.contains(&bits[0]), "{name}");
                assert!(CLAN_PLACES.contains(&bits[1]), "{name}");
                seen[3] = true;
            } else {
                panic!("unexpected clan name shape: {name}");
            }
        }
        assert!(seen.iter().all(|s| *s), "not all patterns hit: {seen:?}");
    }

    #[test]
    fn seeded_rng_makes_clan_names_deterministic() {
        let mut a = SmallRng::seed_from_u64(42);
        let mut b = SmallRng::seed_from_u64(42);
        assert_eq!(clan_name(&mut a), clan_name(&mut b));
    }
}
