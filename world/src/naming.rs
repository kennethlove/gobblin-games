//! Goblin name generation.
//!
//! Discworld-style goblin phrase names: short evocative phrases rather than
//! personal names, e.g. "Tears of the Mushroom", "Shine on the Moon", or
//! "Of the Wind Regretfully Blown"; rarely a nickname like "Stinky" or a
//! human-style name like "Billy Slick".
//! Everything is RNG-driven so callers stay deterministic when seeded.

use rand::Rng;
use rand::RngExt;

/// Leading nouns for the `<Noun> of the <Thing>` pattern, mostly plural
/// ("Tears", "Shines") with a mass noun or two ("Shine").
const PLURALS: &[&str] = &[
    "Tears", "Drops", "Puddles", "Spores", "Warts", "Teeth", "Bones", "Shines", "Cinders",
    "Whispers", "Giggles", "Bubbles", "Grubs", "Beetles", "Sparks", "Crumbs", "Shine",
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
    "Rainbow",
];

/// Opening nouns for the `Of the <Noun> ...` poetic patterns.
const POETIC_NOUNS: &[&str] = &[
    "Twilight", "Wind", "Lathe", "Wheel", "Dawn", "Dusk", "Storm", "Tide", "Ember", "Frost",
];

/// Adverb middles for the poetic pattern: "Of the Wind Regretfully Blown".
const POETIC_ADVERBS: &[&str] = &[
    "Regretfully",
    "Silently",
    "Slowly",
    "Gently",
    "Patiently",
    "Wearily",
    "Endlessly",
    "Reluctantly",
];

/// Noun closers after the article middle: "Of the Twilight the Darkness".
const POETIC_NOUN_FORMS: &[&str] = &[
    "Darkness", "Swarf", "Spoke", "Silence", "Gloom", "Splendor", "Havoc",
];

/// Poetic verb-forms after an adverb middle: "Of the Wind Regretfully Blown".
const POETIC_VERB_FORMS: &[&str] = &[
    "Blown",
    "Glowing",
    "Waning",
    "Scattered",
    "Whispered",
    "Forgotten",
    "Shattered",
];

/// Sensory heads and their noun pools for
/// `<Sense> of the <Noun> on <Phrase>` names.
const SENSE_POOLS: [(&str, &[&str]); 5] = [
    ("Sound", &["Rain", "Thunder", "Hail", "Bell"]),
    ("Sight", &["Dawn", "Dusk", "Moon", "Horizon"]),
    ("Smell", &["Smoke", "Mildew", "Tar", "Rot"]),
    ("Taste", &["Salt", "Honey", "Ash", "Iron"]),
    ("Touch", &["Wind", "Frost", "Stone", "Briar"]),
];

/// Ground phrases closing `<Sense> of the <Noun> on <Phrase>` names.
const GROUND_PHRASES: &[&str] = &["Hard Ground", "Soft Mud", "Wet Stone"];

/// Adjectives for `The <Adjective> <Noun> <Verb>` names.
const THE_ADJECTIVES: &[&str] = &["Cold", "Long", "Hollow", "Quiet", "Bitter", "Restless"];

/// Verbs for `The <Adjective> <Noun> <Verb>` names.
const THE_VERBS: &[&str] = &[
    "Wakes", "Falls", "Waits", "Hums", "Turns", "Bites", "Creeps",
];

/// One-word goblin nicknames.
const NICKNAMES: &[&str] = &["Stinky"];

/// Rare human-style given names, e.g. "Billy".
const HUMAN_FIRST: &[&str] = &["Billy"];

/// Surnames for rare human-style names, e.g. "Slick".
const HUMAN_LAST: &[&str] = &["Slick"];

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

/// A goblin name pattern with explicit word choices, so tests can
/// reconstruct exact example names.
enum GoblinNamePattern<'a> {
    /// `<Noun> of the <Thing>` — "Tears of the Mushroom".
    NounOf(&'a str, &'a str),
    /// `<Verb> on the <Thing>` — "Shine on the Moon".
    VerbOn(&'a str, &'a str),
    /// `Of the <Noun> the <Noun-form>` — "Of the Twilight the Darkness".
    PoeticArticle(&'a str, &'a str),
    /// `Of the <Noun> <Adverb> <Verb-form>` — "Of the Wind Regretfully Blown".
    PoeticAdverb(&'a str, &'a str, &'a str),
    /// `<Sense> of the <Thing> on <Phrase>` — "Sound of the Rain on Hard Ground".
    SenseOn(&'a str, &'a str, &'a str),
    /// `The <Adjective> <Noun> <Verb>` — "The Cold Bone Wakes".
    TheNounVerb(&'a str, &'a str, &'a str),
    /// A one-word goblin nickname — "Stinky".
    Nickname(&'a str),
    /// A rare human-style name — "Billy Slick".
    Human(&'a str, &'a str),
}

/// Build the name for an explicit pattern choice.
fn build_goblin_name(pattern: GoblinNamePattern<'_>) -> String {
    match pattern {
        GoblinNamePattern::NounOf(noun, thing) => format!("{noun} of the {thing}"),
        GoblinNamePattern::VerbOn(verb, thing) => format!("{verb} on the {thing}"),
        GoblinNamePattern::PoeticArticle(noun, form) => format!("Of the {noun} the {form}"),
        GoblinNamePattern::PoeticAdverb(noun, adverb, form) => {
            format!("Of the {noun} {adverb} {form}")
        }
        GoblinNamePattern::SenseOn(sense, noun, phrase) => {
            format!("{sense} of the {noun} on {phrase}")
        }
        GoblinNamePattern::TheNounVerb(adjective, noun, verb) => {
            format!("The {adjective} {noun} {verb}")
        }
        GoblinNamePattern::Nickname(nickname) => nickname.to_string(),
        GoblinNamePattern::Human(first, last) => format!("{first} {last}"),
    }
}

/// Generate a goblin name: usually a short phrase like "Tears of the
/// Mushroom" or "Of the Wind Regretfully Blown", rarely a nickname like
/// "Stinky" or a human-style name like "Billy Slick".
pub fn goblin_name(rng: &mut impl Rng) -> String {
    // Weighted roll; human-style names stay rare.
    let pattern = match rng.random_range(0..100) {
        0..=19 => GoblinNamePattern::NounOf(pick(PLURALS, rng), pick(THINGS, rng)),
        20..=39 => GoblinNamePattern::VerbOn(pick(ACTIONS, rng), pick(THINGS, rng)),
        40..=54 => {
            GoblinNamePattern::PoeticArticle(pick(POETIC_NOUNS, rng), pick(POETIC_NOUN_FORMS, rng))
        }
        55..=69 => GoblinNamePattern::PoeticAdverb(
            pick(POETIC_NOUNS, rng),
            pick(POETIC_ADVERBS, rng),
            pick(POETIC_VERB_FORMS, rng),
        ),
        70..=79 => {
            let (sense, nouns) = SENSE_POOLS[rng.random_range(0..SENSE_POOLS.len())];
            GoblinNamePattern::SenseOn(sense, pick(nouns, rng), pick(GROUND_PHRASES, rng))
        }
        80..=89 => GoblinNamePattern::TheNounVerb(
            pick(THE_ADJECTIVES, rng),
            pick(THINGS, rng),
            pick(THE_VERBS, rng),
        ),
        90..=96 => GoblinNamePattern::Nickname(pick(NICKNAMES, rng)),
        _ => GoblinNamePattern::Human(pick(HUMAN_FIRST, rng), pick(HUMAN_LAST, rng)),
    };
    build_goblin_name(pattern)
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

    /// Noun pool for a sensory head from `SENSE_POOLS`.
    fn nouns_for(sense: &str) -> &'static [&'static str] {
        SENSE_POOLS
            .iter()
            .find(|(s, _)| *s == sense)
            .unwrap_or_else(|| panic!("unknown sense {sense:?}"))
            .1
    }

    #[test]
    fn names_follow_one_of_the_supported_patterns() {
        let mut rng = SmallRng::seed_from_u64(7);
        // [nickname, human, the-noun-verb, noun-of, verb-on,
        //  poetic-article, poetic-adverb, sense-on]
        let mut seen = [false; 8];
        for _ in 0..200 {
            let name = goblin_name(&mut rng);
            let parts: Vec<&str> = name.split_whitespace().collect();
            match (parts.len(), parts[0]) {
                (1, _) => {
                    assert!(NICKNAMES.contains(&parts[0]), "{name}");
                    seen[0] = true;
                }
                (2, _) => {
                    assert!(HUMAN_FIRST.contains(&parts[0]), "{name}");
                    assert!(HUMAN_LAST.contains(&parts[1]), "{name}");
                    seen[1] = true;
                }
                (4, "The") => {
                    assert!(THE_ADJECTIVES.contains(&parts[1]), "{name}");
                    assert!(THINGS.contains(&parts[2]), "{name}");
                    assert!(THE_VERBS.contains(&parts[3]), "{name}");
                    seen[2] = true;
                }
                (4, _) => match parts[1] {
                    "of" => {
                        assert!(PLURALS.contains(&parts[0]), "{name}");
                        seen[3] = true;
                    }
                    "on" => {
                        assert!(ACTIONS.contains(&parts[0]), "{name}");
                        seen[4] = true;
                    }
                    other => panic!("unexpected middle word {other:?} in {name}"),
                },
                (5, "Of") => {
                    assert_eq!(parts[1], "the", "{name}");
                    assert!(POETIC_NOUNS.contains(&parts[2]), "{name}");
                    if parts[3] == "the" {
                        assert!(POETIC_NOUN_FORMS.contains(&parts[4]), "{name}");
                        seen[5] = true;
                    } else {
                        assert!(POETIC_ADVERBS.contains(&parts[3]), "{name}");
                        assert!(POETIC_VERB_FORMS.contains(&parts[4]), "{name}");
                        seen[6] = true;
                    }
                }
                (7, sense) => {
                    assert_eq!(parts[1], "of", "{name}");
                    assert_eq!(parts[2], "the", "{name}");
                    assert!(SENSE_POOLS.iter().any(|(s, _)| *s == sense), "{name}");
                    assert!(nouns_for(sense).contains(&parts[3]), "{name}");
                    assert_eq!(parts[4], "on", "{name}");
                    let phrase = parts[5..].join(" ");
                    assert!(GROUND_PHRASES.contains(&phrase.as_str()), "{name}");
                    seen[7] = true;
                }
                _ => panic!("unexpected shape: {name}"),
            }
        }
        assert!(seen.iter().all(|s| *s), "not all patterns hit: {seen:?}");
    }

    #[test]
    fn example_names_are_generatable() {
        // List membership proves goblin_name can pick that slot's word;
        // build_goblin_name proves the exact example string.
        assert!(POETIC_NOUNS.contains(&"Lathe"));
        assert!(POETIC_NOUN_FORMS.contains(&"Swarf"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::PoeticArticle("Lathe", "Swarf")),
            "Of the Lathe the Swarf"
        );

        assert!(POETIC_NOUNS.contains(&"Wheel"));
        assert!(POETIC_NOUN_FORMS.contains(&"Spoke"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::PoeticArticle("Wheel", "Spoke")),
            "Of the Wheel the Spoke"
        );

        assert!(POETIC_NOUNS.contains(&"Wind"));
        assert!(POETIC_ADVERBS.contains(&"Regretfully"));
        assert!(POETIC_VERB_FORMS.contains(&"Blown"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::PoeticAdverb(
                "Wind",
                "Regretfully",
                "Blown"
            )),
            "Of the Wind Regretfully Blown"
        );

        assert!(PLURALS.contains(&"Shine"));
        assert!(THINGS.contains(&"Rainbow"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::NounOf("Shine", "Rainbow")),
            "Shine of the Rainbow"
        );

        assert!(nouns_for("Sound").contains(&"Rain"));
        assert!(GROUND_PHRASES.contains(&"Hard Ground"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::SenseOn("Sound", "Rain", "Hard Ground")),
            "Sound of the Rain on Hard Ground"
        );

        assert!(nouns_for("Sight").contains(&"Dawn"));
        assert!(GROUND_PHRASES.contains(&"Wet Stone"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::SenseOn("Sight", "Dawn", "Wet Stone")),
            "Sight of the Dawn on Wet Stone"
        );

        assert!(nouns_for("Smell").contains(&"Smoke"));
        assert!(GROUND_PHRASES.contains(&"Soft Mud"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::SenseOn("Smell", "Smoke", "Soft Mud")),
            "Smell of the Smoke on Soft Mud"
        );

        assert!(nouns_for("Taste").contains(&"Salt"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::SenseOn("Taste", "Salt", "Hard Ground")),
            "Taste of the Salt on Hard Ground"
        );

        assert!(nouns_for("Touch").contains(&"Wind"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::SenseOn("Touch", "Wind", "Wet Stone")),
            "Touch of the Wind on Wet Stone"
        );

        assert!(NICKNAMES.contains(&"Stinky"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::Nickname("Stinky")),
            "Stinky"
        );

        assert!(PLURALS.contains(&"Tears"));
        assert!(THINGS.contains(&"Mushroom"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::NounOf("Tears", "Mushroom")),
            "Tears of the Mushroom"
        );

        assert!(THE_ADJECTIVES.contains(&"Cold"));
        assert!(THINGS.contains(&"Bone"));
        assert!(THE_VERBS.contains(&"Wakes"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::TheNounVerb("Cold", "Bone", "Wakes")),
            "The Cold Bone Wakes"
        );

        assert!(POETIC_NOUNS.contains(&"Twilight"));
        assert!(POETIC_NOUN_FORMS.contains(&"Darkness"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::PoeticArticle("Twilight", "Darkness")),
            "Of the Twilight the Darkness"
        );

        assert!(HUMAN_FIRST.contains(&"Billy"));
        assert!(HUMAN_LAST.contains(&"Slick"));
        assert_eq!(
            build_goblin_name(GoblinNamePattern::Human("Billy", "Slick")),
            "Billy Slick"
        );
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
