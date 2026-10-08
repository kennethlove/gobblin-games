use serde::{Deserialize, Serialize};
use shared::messages::Phase;

/// Configuration for game constants and tuning parameters.
/// Centralizes magic numbers to enable runtime configuration and difficulty modes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GameConfig {
    // Game lifecycle constants (from games.rs)
    /// Character count threshold for area constriction
    pub low_character_threshold: u32,
    /// Number of weapons spawned during feast
    pub feast_weapon_count: u32,
    /// Number of shields spawned during feast
    pub feast_shield_count: u32,
    /// Number of consumables spawned during feast
    pub feast_consumable_count: u32,
    /// Probability of day events occurring (1.0 = 100%, 0.25 = 25%)
    pub day_event_frequency: f64,
    /// Probability of night events occurring (1.0 = 100%, 0.125 = 12.5%)
    pub night_event_frequency: f64,
    /// Enable instant death outcomes for catastrophic events
    pub instant_death_enabled: bool,
    /// Enable the trauma producer pipeline (acquire/reinforce trauma afflictions)
    pub trauma_enabled: bool,
    /// Enable phobia afflictions (spawn-time acquisition, per-cycle scan)
    pub phobias_enabled: bool,
    /// Enable fixation afflictions (spawn-time acquisition, per-cycle processing)
    pub fixations_enabled: bool,
    /// Enable addiction processing (brain layer, decay, observer tracking)
    pub addiction_enabled: bool,
    /// Global multiplier for event severity (1.0 = normal, 2.0 = double damage)
    pub catastrophic_severity_multiplier: f64,

    // Character AI decision thresholds (from characters/brains.rs)
    /// Enemy count threshold for "few enemies" AI decisions
    pub low_enemy_limit: u32,
    /// Health threshold for low health AI decisions
    pub low_health_limit: u32,
    /// Health threshold for mid health AI decisions
    pub mid_health_limit: u32,
    /// Extreme low sanity threshold for desperate actions
    pub extreme_low_sanity_limit: u32,
    /// Low sanity threshold for impulsive actions
    pub low_sanity_limit: u32,
    /// Mid sanity threshold for cautious actions
    pub mid_sanity_limit: u32,
    /// Low movement threshold for exhaustion checks
    pub low_movement_limit: u32,
    /// High intelligence threshold for tactical decisions
    pub high_intelligence_limit: u32,
    /// Low intelligence threshold for reckless decisions
    pub low_intelligence_limit: u32,

    // Character lifecycle constants (from characters/mod.rs)
    /// Sanity level at which characters may attempt suicide
    pub sanity_break_level: u32,

    // Survival cadence (per-day schedules, independent of phase count)
    /// Even hours on which the survival clock ticks: hunger/thirst bars,
    /// starvation/dehydration HP drain, and the affliction cascade all
    /// advance only at these hours (6 per day, every 4 h).
    pub survival_tick_hours: [u8; 6],
    /// Starvation drain on the first starvation day, in damage units
    /// (blood cost = units × 10). ~10 units/day at the old cadence.
    pub starvation_hp_base_per_day: u32,
    /// Daily growth of the starvation drain; 125 = ×1.25 per starvation
    /// day, ceiling-applied (12, 15, 19, 24, 30, …).
    pub starvation_growth_pct: u32,
    /// Dehydration drain on the first dehydrated day (damage units).
    pub dehydration_hp_base_per_day: u32,
    /// Daily growth of the dehydration drain (ceiling ×1.25 by default).
    pub dehydration_growth_pct: u32,

    // Day/night boundaries (the phase redesign; both even hours 00–22)
    /// First phase hour of every game day (default 06).
    pub day_start_hour: u8,
    /// Hour night falls (default 20). Night runs nightfall → day_start
    /// (wrap-aware), so days start at 06 and nights last 20 → 06.
    pub nightfall_hour: u8,

    // Attribute maximums (from characters/mod.rs)
    pub max_health: u32,
    pub max_sanity: u32,
    pub max_movement: u32,
    pub max_strength: u32,
    pub max_defense: u32,
    pub max_bravery: u32,
    pub max_intelligence: u32,
    pub max_persuasion: u32,
    pub max_luck: u32,
    pub max_agility: u32,
}

impl Default for GameConfig {
    /// Returns default configuration matching the original hardcoded values.
    fn default() -> Self {
        Self {
            // Game lifecycle
            low_character_threshold: 8,
            feast_weapon_count: 2,
            feast_shield_count: 2,
            feast_consumable_count: 4,
            day_event_frequency: 1.0 / 4.0,
            night_event_frequency: 1.0 / 8.0,
            instant_death_enabled: true,
            trauma_enabled: true,
            phobias_enabled: true,
            fixations_enabled: true,
            addiction_enabled: true,
            catastrophic_severity_multiplier: 1.0,

            // Character AI
            low_enemy_limit: 6,
            low_health_limit: 20,
            mid_health_limit: 40,
            extreme_low_sanity_limit: 10,
            low_sanity_limit: 20,
            mid_sanity_limit: 35,
            low_movement_limit: 10,
            high_intelligence_limit: 35,
            low_intelligence_limit: 80,

            // Character lifecycle
            sanity_break_level: 9,

            // Survival cadence: tick every 4 h; day-scaled flat drains.
            survival_tick_hours: [0, 4, 8, 12, 16, 20],
            starvation_hp_base_per_day: 12,
            starvation_growth_pct: 125,
            dehydration_hp_base_per_day: 12,
            dehydration_growth_pct: 125,

            // Day/night boundaries.
            day_start_hour: 6,
            nightfall_hour: 20,

            // Attribute maximums
            max_health: 100,
            max_sanity: 100,
            max_movement: 100,
            max_strength: 50,
            max_defense: 50,
            max_bravery: 100,
            max_intelligence: 100,
            max_persuasion: 100,
            max_luck: 100,
            max_agility: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GameConfig::default();
        assert_eq!(config.low_character_threshold, 8);
        assert_eq!(config.feast_weapon_count, 2);
        assert_eq!(config.max_health, 100);
        assert_eq!(config.low_health_limit, 20);
    }

    #[test]
    fn test_config_serialization() {
        let config = GameConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: GameConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_event_config_defaults() {
        let config = GameConfig::default();
        assert_eq!(config.day_event_frequency, 0.25);
        assert_eq!(config.night_event_frequency, 0.125);
        assert!(config.instant_death_enabled);
        assert_eq!(config.catastrophic_severity_multiplier, 1.0);
    }

    #[test]
    fn test_easy_mode_config() {
        let config = GameConfig {
            instant_death_enabled: false,
            catastrophic_severity_multiplier: 0.5,
            ..GameConfig::default()
        };

        assert!(!config.instant_death_enabled);
        assert_eq!(config.catastrophic_severity_multiplier, 0.5);
    }

    #[test]
    fn test_hard_mode_config() {
        let config = GameConfig {
            instant_death_enabled: true,
            catastrophic_severity_multiplier: 2.0,
            day_event_frequency: 0.5,
            ..GameConfig::default()
        };

        assert_eq!(config.catastrophic_severity_multiplier, 2.0);
        assert_eq!(config.day_event_frequency, 0.5);
    }
}

/// Narrative slot of a phase, derived from the configured day/night
/// boundaries. One source of truth for lighting, incident chances,
/// event frequencies, brain bias, and display copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaySlot {
    /// First phase of the game day (`day_start_hour`).
    Dawn,
    /// Bright day hours between the dawn and dusk anchors.
    Day,
    /// Two hours before nightfall (18 by default).
    Dusk,
    /// From nightfall to the day start (wrap-aware).
    Night,
}

impl GameConfig {
    /// Night check against the configured boundaries (wrap-aware).
    pub fn is_night_phase(&self, phase: Phase) -> bool {
        phase.is_night(self.day_start_hour, self.nightfall_hour)
    }

    /// Dusk anchor: two hours before nightfall (18 with the defaults).
    pub fn dusk_hour(&self) -> u8 {
        (self.nightfall_hour + 24 - 2) % 24
    }

    /// Configured narrative slot for a phase.
    pub fn day_slot(&self, phase: Phase) -> DaySlot {
        if phase.hour() == self.day_start_hour {
            DaySlot::Dawn
        } else if phase.hour() == self.dusk_hour() {
            DaySlot::Dusk
        } else if self.is_night_phase(phase) {
            DaySlot::Night
        } else {
            DaySlot::Day
        }
    }

    /// Bright-daylight band: day hours excluding the dawn/dusk anchors.
    pub fn is_bright(&self, phase: Phase) -> bool {
        matches!(self.day_slot(phase), DaySlot::Day)
    }

    /// Even hours in `00..=24`, distinct, nightfall after day start is
    /// fine either way (wrap-aware) but the two hours must differ.
    pub fn day_night_hours_valid(&self) -> bool {
        self.day_start_hour < 24
            && self.nightfall_hour < 24
            && self.day_start_hour.is_multiple_of(2)
            && self.nightfall_hour.is_multiple_of(2)
            && self.day_start_hour != self.nightfall_hour
    }
}

#[cfg(test)]
mod day_night_tests {
    use super::*;

    #[test]
    fn defaults_produce_expected_slots() {
        let cfg = GameConfig::default();
        assert_eq!(cfg.day_start_hour, 6);
        assert_eq!(cfg.nightfall_hour, 20);
        assert_eq!(cfg.dusk_hour(), 18);
        for hour in [6u8, 8, 10, 12, 14, 16] {
            let p = Phase::from_hour(hour).unwrap();
            assert!(cfg.day_slot(p) != DaySlot::Night, "hour {hour} is day");
        }
        assert_eq!(cfg.day_slot(Phase::from_hour(6).unwrap()), DaySlot::Dawn);
        assert_eq!(cfg.day_slot(Phase::from_hour(18).unwrap()), DaySlot::Dusk);
        for hour in [20u8, 22, 0, 2, 4] {
            let p = Phase::from_hour(hour).unwrap();
            assert_eq!(cfg.day_slot(p), DaySlot::Night, "hour {hour} is night");
        }
        assert!(cfg.is_bright(Phase::from_hour(12).unwrap()));
        assert!(!cfg.is_bright(Phase::from_hour(6).unwrap()));
        assert!(cfg.day_night_hours_valid());
    }

    #[test]
    fn shifted_boundaries_move_slots() {
        let cfg = GameConfig {
            day_start_hour: 8,
            nightfall_hour: 18,
            ..Default::default()
        };
        assert_eq!(cfg.dusk_hour(), 16);
        assert_eq!(cfg.day_slot(Phase::from_hour(8).unwrap()), DaySlot::Dawn);
        assert_eq!(cfg.day_slot(Phase::from_hour(6).unwrap()), DaySlot::Night);
        assert_eq!(cfg.day_slot(Phase::from_hour(18).unwrap()), DaySlot::Night);
        assert_eq!(cfg.day_slot(Phase::from_hour(16).unwrap()), DaySlot::Dusk);
        assert!(cfg.day_night_hours_valid());
    }

    #[test]
    fn invalid_hours_rejected() {
        let odd = GameConfig {
            day_start_hour: 7, // odd
            ..Default::default()
        };
        assert!(!odd.day_night_hours_valid());
        let equal = GameConfig {
            nightfall_hour: 6, // same as the day start
            ..Default::default()
        };
        assert!(!equal.day_night_hours_valid());
        let out_of_range = GameConfig {
            day_start_hour: 24,
            ..Default::default()
        };
        assert!(!out_of_range.day_night_hours_valid());
    }
}
