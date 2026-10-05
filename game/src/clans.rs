use crate::terrain::BaseTerrain;
use rand::Rng;
use rand::RngExt;

/// Represents a clan's industry and terrain preferences
#[derive(Debug, Clone, Copy)]
pub struct ClanProfile {
    pub number: u8,
    pub industry: &'static str,
    pub primary_affinity: BaseTerrain,
    pub bonus_affinity_pool: [BaseTerrain; 2],
}

/// All 8 clan profiles with their industries and terrain affinities
pub const CLAN_PROFILES: [ClanProfile; 8] = [
    ClanProfile {
        number: 1,
        industry: "Luxury",
        primary_affinity: BaseTerrain::UrbanRuins,
        bonus_affinity_pool: [BaseTerrain::Clearing, BaseTerrain::Grasslands],
    },
    ClanProfile {
        number: 2,
        industry: "Masonry",
        primary_affinity: BaseTerrain::Mountains,
        bonus_affinity_pool: [BaseTerrain::UrbanRuins, BaseTerrain::Badlands],
    },
    ClanProfile {
        number: 3,
        industry: "Technology",
        primary_affinity: BaseTerrain::UrbanRuins,
        bonus_affinity_pool: [BaseTerrain::Mountains, BaseTerrain::Clearing],
    },
    ClanProfile {
        number: 4,
        industry: "Fishing",
        primary_affinity: BaseTerrain::Wetlands,
        bonus_affinity_pool: [BaseTerrain::Forest, BaseTerrain::Jungle],
    },
    ClanProfile {
        number: 5,
        industry: "Power",
        primary_affinity: BaseTerrain::Geothermal,
        bonus_affinity_pool: [BaseTerrain::UrbanRuins, BaseTerrain::Mountains],
    },
    ClanProfile {
        number: 6,
        industry: "Transportation",
        primary_affinity: BaseTerrain::Grasslands,
        bonus_affinity_pool: [BaseTerrain::Clearing, BaseTerrain::Highlands],
    },
    ClanProfile {
        number: 7,
        industry: "Lumber",
        primary_affinity: BaseTerrain::Forest,
        bonus_affinity_pool: [BaseTerrain::Jungle, BaseTerrain::Wetlands],
    },
    ClanProfile {
        number: 8,
        industry: "Textiles",
        primary_affinity: BaseTerrain::Grasslands,
        bonus_affinity_pool: [BaseTerrain::Clearing, BaseTerrain::Wetlands],
    },
];

/// Assigns terrain affinity to a character based on their clan
///
/// # Arguments
/// * `clan` - Clan number (1-8)
/// * `rng` - Random number generator
///
/// # Returns
/// Vec containing 1-2 terrain types:
/// - Always includes the primary affinity
/// - 40% chance to add one random terrain from the bonus pool
pub fn assign_terrain_affinity(clan: u8, rng: &mut impl Rng) -> Vec<BaseTerrain> {
    // Handle invalid clans by returning empty vec
    if !(1..=8).contains(&clan) {
        return vec![];
    }

    let profile = &CLAN_PROFILES[(clan - 1) as usize];
    let mut affinities = vec![profile.primary_affinity];

    // 40% chance to add a bonus terrain
    if rng.random_bool(0.4) {
        let bonus_index = rng.random_range(0..profile.bonus_affinity_pool.len());
        affinities.push(profile.bonus_affinity_pool[bonus_index]);
    }

    affinities
}

/// Display epithets for the 8 clans, indexed by clan number minus one.
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

/// Epithet for a clan number (1..=8). Unknown numbers fall back to the
/// first epithet so bad data still renders.
pub fn clan_epithet(clan: u32) -> &'static str {
    CLAN_EPITHETS
        .get(clan.wrapping_sub(1) as usize)
        .copied()
        .unwrap_or(CLAN_EPITHETS[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::prelude::SmallRng;
    use rstest::rstest;

    #[test]
    fn test_clan_profiles_count() {
        assert_eq!(CLAN_PROFILES.len(), 8);
    }

    #[rstest]
    #[case(1, "Luxury", BaseTerrain::UrbanRuins)]
    #[case(2, "Masonry", BaseTerrain::Mountains)]
    #[case(3, "Technology", BaseTerrain::UrbanRuins)]
    #[case(4, "Fishing", BaseTerrain::Wetlands)]
    #[case(5, "Power", BaseTerrain::Geothermal)]
    #[case(6, "Transportation", BaseTerrain::Grasslands)]
    #[case(7, "Lumber", BaseTerrain::Forest)]
    #[case(8, "Textiles", BaseTerrain::Grasslands)]
    fn test_clan_profile_data(
        #[case] clan: u8,
        #[case] expected_industry: &str,
        #[case] expected_primary: BaseTerrain,
    ) {
        let profile = &CLAN_PROFILES[(clan - 1) as usize];
        assert_eq!(profile.number, clan);
        assert_eq!(profile.industry, expected_industry);
        assert_eq!(profile.primary_affinity, expected_primary);
        assert_eq!(profile.bonus_affinity_pool.len(), 2);
    }

    #[test]
    fn test_assign_terrain_affinity_always_includes_primary() {
        let mut rng = SmallRng::seed_from_u64(42);

        for clan in 1..=8 {
            let affinities = assign_terrain_affinity(clan, &mut rng);
            assert!(!affinities.is_empty());

            let profile = &CLAN_PROFILES[(clan - 1) as usize];
            assert_eq!(affinities[0], profile.primary_affinity);
        }
    }

    #[test]
    fn test_assign_terrain_affinity_returns_one_or_two() {
        let mut rng = SmallRng::seed_from_u64(42);

        for clan in 1..=8 {
            let affinities = assign_terrain_affinity(clan, &mut rng);
            assert!(!affinities.is_empty() && affinities.len() <= 2);
        }
    }

    #[test]
    fn test_assign_terrain_affinity_bonus_probability() {
        let iterations = 1000;
        let mut bonus_count = 0;

        for i in 0..iterations {
            let mut rng = SmallRng::seed_from_u64(i);
            let affinities = assign_terrain_affinity(1, &mut rng);

            if affinities.len() == 2 {
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

    #[test]
    fn test_assign_terrain_affinity_invalid_clan() {
        let mut rng = SmallRng::seed_from_u64(42);

        assert_eq!(assign_terrain_affinity(0, &mut rng).len(), 0);
        assert_eq!(assign_terrain_affinity(9, &mut rng).len(), 0);
        assert_eq!(assign_terrain_affinity(255, &mut rng).len(), 0);
    }
}
