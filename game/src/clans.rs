use rand::Rng;
use rand::RngExt;
use strum::IntoEnumIterator;
use world::terrain::BaseTerrain;

/// Display epithets for the 8 team slots, indexed by team number minus one.
/// Teams are labeled "color + epithet" (e.g. "Orange Mangletooths"); the
/// epithet doubles as the team mascot.
pub const CLAN_EPITHETS: [&str; 8] = [
    "Mangletooth",
    "Bogmaw",
    "Rustnib",
    "Duskhollow",
    "Wartknuckle",
    "Mudlark",
    "Bonepick",
    "Scabridge",
];

/// One entry of the 8-color team palette: display name + HTML hex value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeamColor {
    pub name: &'static str,
    pub hex: &'static str,
}

/// Team colors indexed by team slot (team 1 = `TEAM_COLORS[0]`).
pub const TEAM_COLORS: [TeamColor; 8] = [
    TeamColor {
        name: "Orange",
        hex: "#ea580c",
    },
    TeamColor {
        name: "Blue",
        hex: "#2563eb",
    },
    TeamColor {
        name: "Green",
        hex: "#16a34a",
    },
    TeamColor {
        name: "Red",
        hex: "#dc2626",
    },
    TeamColor {
        name: "Purple",
        hex: "#9333ea",
    },
    TeamColor {
        name: "Yellow",
        hex: "#ca8a04",
    },
    TeamColor {
        name: "Teal",
        hex: "#0d9488",
    },
    TeamColor {
        name: "Pink",
        hex: "#db2777",
    },
];

/// Mascot epithet for a team slot (1..=8). Unknown slots fall back to the
/// first epithet so bad data still renders.
pub fn team_epithet(team: u32) -> &'static str {
    CLAN_EPITHETS
        .get(team.wrapping_sub(1) as usize)
        .copied()
        .unwrap_or(CLAN_EPITHETS[0])
}

/// Team color for a team slot (1..=8). Unknown slots fall back to the
/// first color so bad data still renders.
pub fn team_color(team: u32) -> &'static TeamColor {
    TEAM_COLORS
        .get(team.wrapping_sub(1) as usize)
        .unwrap_or(&TEAM_COLORS[0])
}

/// Team label: color name + pluralized mascot epithet, e.g. "Orange
/// Mangletooths".
pub fn team_label(team: u32) -> String {
    format!("{} {}s", team_color(team).name, team_epithet(team))
}

/// Roll a terrain affinity for a single goblin from its own RNG.
///
/// Shape: always one primary terrain drawn uniformly from every
/// [`BaseTerrain`], plus a 40% chance of a second, distinct terrain also
/// drawn uniformly from every [`BaseTerrain`]. Grouping (team/clan) plays
/// no part — terrain affinity belongs to the goblin.
pub fn roll_terrain_affinity(rng: &mut impl Rng) -> Vec<BaseTerrain> {
    let all: Vec<BaseTerrain> = BaseTerrain::iter().collect();
    let primary = all[rng.random_range(0..all.len())];
    let mut affinities = vec![primary];

    // 40% chance to add a second, different terrain.
    if rng.random_bool(0.4) {
        let others: Vec<BaseTerrain> = all.iter().copied().filter(|t| *t != primary).collect();
        let second = others[rng.random_range(0..others.len())];
        affinities.push(second);
    }

    affinities
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    use rstest::rstest;

    #[test]
    fn team_palette_covers_eight_slots() {
        assert_eq!(TEAM_COLORS.len(), 8);
        assert_eq!(CLAN_EPITHETS.len(), 8);
        for color in TEAM_COLORS {
            assert!(color.hex.starts_with('#') && color.hex.len() == 7);
            assert!(!color.name.is_empty());
        }
    }

    #[rstest]
    #[case(1, "Orange", "Mangletooth")]
    #[case(2, "Blue", "Bogmaw")]
    #[case(7, "Teal", "Bonepick")]
    #[case(8, "Pink", "Scabridge")]
    fn team_color_and_epithet_by_slot(
        #[case] team: u32,
        #[case] expected_color: &str,
        #[case] expected_epithet: &str,
    ) {
        assert_eq!(team_color(team).name, expected_color);
        assert_eq!(team_epithet(team), expected_epithet);
        assert_eq!(
            team_label(team),
            format!("{expected_color} {expected_epithet}s")
        );
    }

    #[test]
    fn unknown_team_slot_falls_back_to_first_entry() {
        // Slots outside 1..=8 fall back to the first color/epithet; assert
        // bad data still renders instead of panicking.
        let _ = team_color(0);
        let _ = team_color(9);
        assert_eq!(team_epithet(0), CLAN_EPITHETS[0]);
        assert_eq!(team_epithet(9), CLAN_EPITHETS[0]);
    }

    #[test]
    fn roll_terrain_affinity_returns_one_or_two_distinct_terrains() {
        let mut rng = SmallRng::seed_from_u64(42);

        for _ in 0..500 {
            let affinities = roll_terrain_affinity(&mut rng);
            assert!((1..=2).contains(&affinities.len()));
            if affinities.len() == 2 {
                assert_ne!(affinities[0], affinities[1], "no duplicate terrains");
            }
            for terrain in affinities {
                assert!(BaseTerrain::iter().any(|t| t == terrain));
            }
        }
    }

    #[test]
    fn roll_terrain_affinity_covers_all_terrains_as_primary() {
        // Over many seeded rolls every BaseTerrain must show up as primary —
        // affinity is uniformly random, not keyed to any grouping.
        let mut seen = std::collections::HashSet::new();
        for i in 0..1000 {
            let mut rng = SmallRng::seed_from_u64(i);
            let affinities = roll_terrain_affinity(&mut rng);
            seen.insert(affinities[0]);
        }
        assert_eq!(seen.len(), BaseTerrain::iter().count());
    }

    #[test]
    fn roll_terrain_affinity_bonus_probability() {
        let iterations = 1000;
        let mut bonus_count = 0;

        for i in 0..iterations {
            let mut rng = SmallRng::seed_from_u64(i);
            if roll_terrain_affinity(&mut rng).len() == 2 {
                bonus_count += 1;
            }
        }

        // With 1000 iterations, expect roughly 350-450 bonuses (40% ± 5%)
        let percentage = (bonus_count as f64 / iterations as f64) * 100.0;
        assert!(
            (35.0..=45.0).contains(&percentage),
            "Expected ~40% bonus rate, got {:.1}%",
            percentage
        );
    }
}
