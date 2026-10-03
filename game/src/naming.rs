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

/// Generate a goblin name like "Tears of the Mushroom" or "Shine on the Moon".
pub fn goblin_name(rng: &mut impl Rng) -> String {
    if rng.random_bool(0.5) {
        format!("{} of the {}", pick(PLURALS, rng), pick(THINGS, rng))
    } else {
        format!("{} on the {}", pick(ACTIONS, rng), pick(THINGS, rng))
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
}
