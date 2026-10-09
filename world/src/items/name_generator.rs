use rand::prelude::*;

// Adjectives to come before "shield"
// _____ shield
const SHIELD_ADJECTIVES: &[&str] = &[
    "bone",
    "rusty",
    "scrap",
    "wicker",
    "stone",
    "iron",
    "mud-caked",
    "snagged",
];

// Weapon nouns
// <descriptor> _____
const WEAPON_NOUNS: &[&str] = &[
    "shiv",
    "spear",
    "cleaver",
    "club",
    "sling",
    "sickle",
    "gouger",
    "snag",
    "bone-dart",
    "axe",
];

// Adjectives to come before a weapon noun
// _____ <weapon noun>
const WEAPON_ADJECTIVES: &[&str] = &[
    "jagged", "rusty", "wicked", "heavy", "sharp", "notched", "gnawed", "flint", "bone", "iron",
];

pub fn generate_shield_name() -> String {
    generate_shield_name_with_rng(&mut rand::rng())
}

/// Generate a shield name, drawing the word choices from `rng` so callers
/// can reproduce the name with a fixed seed.
pub fn generate_shield_name_with_rng(rng: &mut impl Rng) -> String {
    let adjective = SHIELD_ADJECTIVES.choose(rng).unwrap().to_owned();
    format!("{} {}", adjective, "shield")
}

pub fn generate_weapon_name() -> String {
    generate_weapon_name_with_rng(&mut rand::rng())
}

/// Generate a weapon name, drawing the word choices from `rng` so callers
/// can reproduce the name with a fixed seed.
pub fn generate_weapon_name_with_rng(rng: &mut impl Rng) -> String {
    let adjective = WEAPON_ADJECTIVES.choose(rng).unwrap().to_owned();
    let noun = WEAPON_NOUNS.choose(rng).unwrap().to_owned();
    format!("{} {}", adjective, noun)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shield_name() {
        let name = generate_shield_name();
        assert!(name.contains(" shield"));
    }

    #[test]
    fn weapon_name() {
        let name = generate_weapon_name();
        assert!(name.contains(" "));

        let mut name = name.as_str().split(" ");
        let adjective = name.next().unwrap();
        assert!(WEAPON_ADJECTIVES.contains(&adjective));
        let noun = name.next().unwrap();
        assert!(WEAPON_NOUNS.contains(&noun));
    }
}
