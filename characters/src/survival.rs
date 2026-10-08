use crate::Character;
use areas::weather::Weather;

// Wire-visible band enums live in the `shared` crate (they are serialised
// in `MessagePayload::{Hunger,Thirst}BandChanged`). Re-export them here so
// existing `characters::survival::{HungerBand, ThirstBand}` imports
// keep compiling.
pub use shared::messages::{HungerBand, ThirstBand};

const HIGH_ATTR_THRESHOLD: u32 = 75;
const LOW_ATTR_THRESHOLD: u32 = 25;

pub fn hunger_band(value: u8) -> HungerBand {
    match value {
        0 => HungerBand::Sated,
        1..=2 => HungerBand::Peckish,
        3..=4 => HungerBand::Hungry,
        _ => HungerBand::Starving,
    }
}

pub fn thirst_band(value: u8) -> ThirstBand {
    match value {
        0 => ThirstBand::Sated,
        1 => ThirstBand::Thirsty,
        2 => ThirstBand::Parched,
        _ => ThirstBand::Dehydrated,
    }
}

/// True if a band-change event into this band should be surfaced in the
/// public timeline (Action panel). Lower bands are private/Inspect-only.
pub fn hunger_band_is_public(band: HungerBand) -> bool {
    matches!(band, HungerBand::Hungry | HungerBand::Starving)
}

pub fn thirst_band_is_public(band: ThirstBand) -> bool {
    matches!(band, ThirstBand::Parched | ThirstBand::Dehydrated)
}

/// Mutates `character` in place to advance one phase of survival
/// (hunger + thirst).
///
/// Tick rules (per spec):
/// - Base +1 hunger and +1 thirst per phase.
/// - High strength (>= HIGH_ATTR_THRESHOLD) adds +1 hunger.
/// - Low strength (<= LOW_ATTR_THRESHOLD) ticks hunger every other phase.
/// - High stamina (relative to max_stamina) adds +1 thirst; low stamina
///   ticks thirst every other phase.
/// - If exposed (not sheltered) AND weather is Blizzard: +1 hunger.
/// - If exposed AND weather is Heatwave: +1 thirst.
///
/// HP loss for Starving/Dehydrated states is handled separately by the
/// drain helpers in this module.
/// Survival ticks per day — one per `GameConfig::survival_tick_hours`
/// entry (6: every 4 h). The survival clock (bars, HP drain, cascade)
/// advances only on those hours, independent of how many phases a day has.
pub const SURVIVAL_TICKS_PER_DAY: usize = 6;

// Per-tick hunger/thirst deltas across the 6-tick day. Patterns hold the
// old daily totals exactly: 4/day baseline, 8/day high tier, 2/day low tier.
const HUNGER_PATTERN_LOW: [u8; SURVIVAL_TICKS_PER_DAY] = [1, 0, 0, 1, 0, 0];
const HUNGER_PATTERN_MID: [u8; SURVIVAL_TICKS_PER_DAY] = [1, 1, 0, 1, 1, 0];
const HUNGER_PATTERN_HIGH: [u8; SURVIVAL_TICKS_PER_DAY] = [2, 2, 0, 2, 2, 0];
const THIRST_PATTERN_LOW: [u8; SURVIVAL_TICKS_PER_DAY] = [1, 0, 0, 1, 0, 0];
const THIRST_PATTERN_MID: [u8; SURVIVAL_TICKS_PER_DAY] = [1, 1, 0, 1, 1, 0];
const THIRST_PATTERN_HIGH: [u8; SURVIVAL_TICKS_PER_DAY] = [2, 2, 0, 2, 2, 0];

/// Advances hunger/thirst bars on a survival-tick hour. `tick_index` is
/// the position of the current hour in `survival_tick_hours` (0..6).
pub fn tick_survival(
    character: &mut Character,
    weather: &Weather,
    sheltered: bool,
    tick_index: usize,
) {
    let strength = character.attributes.strength;
    // Stamina lives on Character, not Attributes; project the current stamina
    // onto a 0..=100 scale relative to max_stamina so the same thresholds
    // can be compared.
    let stamina_scaled: u32 = character
        .stamina
        .saturating_mul(100)
        .checked_div(character.max_stamina)
        .unwrap_or(0);

    let tick = tick_index % SURVIVAL_TICKS_PER_DAY;

    let mut hunger_delta: u8 = if strength <= LOW_ATTR_THRESHOLD {
        HUNGER_PATTERN_LOW[tick]
    } else if strength >= HIGH_ATTR_THRESHOLD {
        HUNGER_PATTERN_HIGH[tick]
    } else {
        HUNGER_PATTERN_MID[tick]
    };

    let mut thirst_delta: u8 = if stamina_scaled <= LOW_ATTR_THRESHOLD {
        THIRST_PATTERN_LOW[tick]
    } else if stamina_scaled >= HIGH_ATTR_THRESHOLD {
        THIRST_PATTERN_HIGH[tick]
    } else {
        THIRST_PATTERN_MID[tick]
    };

    // Weather pressure lands only on active ticks, which keeps the daily
    // bonus totals at the old per-phase cadence (4/day baseline).
    if hunger_delta > 0 && !sheltered && matches!(weather, Weather::Blizzard) {
        hunger_delta += 1;
    }
    if thirst_delta > 0 && !sheltered && matches!(weather, Weather::Heatwave) {
        thirst_delta += 1;
    }

    character.hunger = character.hunger.saturating_add(hunger_delta);
    character.thirst = character.thirst.saturating_add(thirst_delta);
}

/// Ceiling growth for the day-scaled drain: day 1 = `base_units`, then
/// × `growth_pct` per starvation/dehydration day (12, 15, 19, 24, 30 …).
pub fn daily_drain_units(base_units: u32, growth_pct: u32, days: u16) -> u32 {
    if days == 0 {
        return 0;
    }
    let growth = f64::from(growth_pct) / 100.0;
    (f64::from(base_units) * growth.powi(i32::from(days) - 1)).ceil() as u32
}

/// This tick's exact slice of the daily budget via prefix division —
/// the six slices always sum to `daily_units`.
pub fn drain_units_for_tick(daily_units: u32, tick_index: usize) -> u32 {
    let n = u32::from(SURVIVAL_TICKS_PER_DAY as u16);
    let i = u32::try_from(tick_index % SURVIVAL_TICKS_PER_DAY).unwrap_or(0);
    (daily_units * (i + 1)) / n - (daily_units * i) / n
}

/// Applies the day-scaled starvation drain (damage units × 10 blood) on
/// survival-tick hours. Returns the HP lost this tick (0 if not starving).
/// Tracks the first starving day on the character; eating clears it.
pub fn apply_starvation_drain(
    character: &mut Character,
    game_day: u16,
    base_units: u32,
    growth_pct: u32,
    tick_index: usize,
) -> u32 {
    if hunger_band(character.hunger) != HungerBand::Starving {
        character.starving_since_day = None;
        return 0;
    }
    let since = *character.starving_since_day.get_or_insert(game_day);
    let days = game_day.saturating_sub(since).saturating_add(1);
    let daily = daily_drain_units(base_units, growth_pct, days);
    let hp = drain_units_for_tick(daily, tick_index) * 10;
    character.blood = character.blood.saturating_sub(hp);
    hp
}

/// Applies the day-scaled dehydration drain (damage units × 10 blood).
/// Independent of starvation (own day stamp).
pub fn apply_dehydration_drain(
    character: &mut Character,
    game_day: u16,
    base_units: u32,
    growth_pct: u32,
    tick_index: usize,
) -> u32 {
    if thirst_band(character.thirst) != ThirstBand::Dehydrated {
        character.dehydrated_since_day = None;
        return 0;
    }
    let since = *character.dehydrated_since_day.get_or_insert(game_day);
    let days = game_day.saturating_sub(since).saturating_add(1);
    let daily = daily_drain_units(base_units, growth_pct, days);
    let hp = drain_units_for_tick(daily, tick_index) * 10;
    character.blood = character.blood.saturating_sub(hp);
    hp
}

/// Reduces hunger by `amount`, clearing the starvation day stamp.
pub fn eat_food(character: &mut Character, amount: u8) {
    character.hunger = character.hunger.saturating_sub(amount);
    character.starving_since_day = None;
}

/// Reduces thirst by `amount`, clearing the dehydration day stamp.
pub fn drink_water(character: &mut Character, amount: u8) {
    character.thirst = character.thirst.saturating_sub(amount);
    character.dehydrated_since_day = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(0, HungerBand::Sated)]
    #[case(1, HungerBand::Peckish)]
    #[case(2, HungerBand::Peckish)]
    #[case(3, HungerBand::Hungry)]
    #[case(4, HungerBand::Hungry)]
    #[case(5, HungerBand::Starving)]
    #[case(99, HungerBand::Starving)]
    fn hunger_band_thresholds(#[case] value: u8, #[case] expected: HungerBand) {
        assert_eq!(hunger_band(value), expected);
    }

    #[rstest]
    #[case(0, ThirstBand::Sated)]
    #[case(1, ThirstBand::Thirsty)]
    #[case(2, ThirstBand::Parched)]
    #[case(3, ThirstBand::Dehydrated)]
    #[case(99, ThirstBand::Dehydrated)]
    fn thirst_band_thresholds(#[case] value: u8, #[case] expected: ThirstBand) {
        assert_eq!(thirst_band(value), expected);
    }

    #[test]
    fn hunger_starving_is_publicly_visible() {
        assert!(hunger_band_is_public(HungerBand::Starving));
        assert!(hunger_band_is_public(HungerBand::Hungry));
        assert!(!hunger_band_is_public(HungerBand::Peckish));
        assert!(!hunger_band_is_public(HungerBand::Sated));
    }

    #[test]
    fn thirst_dehydrated_is_publicly_visible() {
        assert!(thirst_band_is_public(ThirstBand::Dehydrated));
        assert!(thirst_band_is_public(ThirstBand::Parched));
        assert!(!thirst_band_is_public(ThirstBand::Thirsty));
        assert!(!thirst_band_is_public(ThirstBand::Sated));
    }

    fn baseline_character() -> Character {
        let mut t = Character::new("Test".to_string(), None, None);
        // Mid-range strength + stamina: baseline ticks (1/1).
        t.attributes.strength = 50;
        t.max_stamina = 100;
        t.stamina = 50;
        t
    }

    #[test]
    fn survival_tick_baseline_clear_exposed() {
        let mut t = baseline_character();
        tick_survival(&mut t, &Weather::Clear, false, 0);
        assert_eq!(t.hunger, 1);
        assert_eq!(t.thirst, 1);
    }

    #[test]
    fn survival_tick_heatwave_exposed_adds_thirst() {
        let mut t = baseline_character();
        tick_survival(&mut t, &Weather::Heatwave, false, 0);
        assert_eq!(t.hunger, 1);
        assert_eq!(t.thirst, 2, "heatwave + exposed adds +1 thirst");
    }

    #[test]
    fn survival_tick_blizzard_exposed_adds_hunger() {
        let mut t = baseline_character();
        tick_survival(&mut t, &Weather::Blizzard, false, 0);
        assert_eq!(t.hunger, 2, "blizzard + exposed adds +1 hunger");
        assert_eq!(t.thirst, 1);
    }

    #[test]
    fn survival_tick_sheltered_suppresses_weather_modifier() {
        let mut t = baseline_character();
        tick_survival(&mut t, &Weather::Heatwave, true, 0);
        assert_eq!(t.thirst, 1, "shelter suppresses heatwave bonus");
        let mut t2 = baseline_character();
        tick_survival(&mut t2, &Weather::Blizzard, true, 0);
        assert_eq!(t2.hunger, 1, "shelter suppresses blizzard bonus");
    }

    #[test]
    fn survival_tick_high_strength_increases_hunger() {
        let mut t = baseline_character();
        t.attributes.strength = 80;
        tick_survival(&mut t, &Weather::Clear, false, 0);
        assert_eq!(t.hunger, 2, "high-strength bodies burn more calories");
    }

    #[test]
    fn survival_tick_high_stamina_increases_thirst() {
        let mut t = baseline_character();
        t.stamina = 80; // 80% of max_stamina
        tick_survival(&mut t, &Weather::Clear, false, 0);
        assert_eq!(t.thirst, 2);
    }

    #[test]
    fn survival_tick_low_strength_follows_two_per_day_pattern() {
        let mut t = baseline_character();
        t.attributes.strength = 20; // low
        // Pattern [1,0,0,1,0,0] → 2 hunger/day (old parity behavior).
        for (tick, expected) in [(0, 1), (1, 1), (2, 1), (3, 2), (4, 2), (5, 2)] {
            tick_survival(&mut t, &Weather::Clear, false, tick);
            assert_eq!(t.hunger, expected, "after tick {tick}");
        }
    }

    #[test]
    fn survival_tick_daily_totals_hold_old_cadence() {
        // Baseline: 4 hunger + 4 thirst per day over the 6-tick schedule.
        let mut t = baseline_character();
        for tick in 0..6 {
            tick_survival(&mut t, &Weather::Clear, false, tick);
        }
        assert_eq!(t.hunger, 4, "baseline hunger holds 4/day");
        assert_eq!(t.thirst, 4, "baseline thirst holds 4/day");

        // High tier burns 8/day; low tier holds 2/day.
        let mut strong = baseline_character();
        strong.attributes.strength = 80;
        strong.stamina = 80;
        for tick in 0..6 {
            tick_survival(&mut strong, &Weather::Clear, false, tick);
        }
        assert_eq!(strong.hunger, 8, "high-strength hunger holds 8/day");
        assert_eq!(strong.thirst, 8, "high-stamina thirst holds 8/day");

        let mut weak = baseline_character();
        weak.attributes.strength = 20;
        weak.stamina = 20;
        for tick in 0..6 {
            tick_survival(&mut weak, &Weather::Clear, false, tick);
        }
        assert_eq!(weak.hunger, 2, "low-strength hunger holds 2/day");
        assert_eq!(weak.thirst, 2, "low-stamina thirst holds 2/day");
    }

    fn starving_character() -> Character {
        let mut t = baseline_character();
        t.hunger = 5;
        t.blood = 1000;
        t.starving_since_day = None;
        t
    }

    const BASE: u32 = 12;
    const GROWTH: u32 = 125;

    #[test]
    fn daily_drain_units_ceiling_growth() {
        let days: Vec<u32> = (1..=6)
            .map(|d| daily_drain_units(BASE, GROWTH, d))
            .collect();
        assert_eq!(days, vec![12, 15, 19, 24, 30, 37]);
    }

    #[test]
    fn drain_units_for_tick_slices_sum_exactly() {
        for daily in [12, 15, 19, 24, 30] {
            let total: u32 = (0..6).map(|i| drain_units_for_tick(daily, i)).sum();
            assert_eq!(total, daily, "slices of {daily} must sum exactly");
        }
        // Day 1 (12) spreads evenly: 2 units per tick.
        assert_eq!(
            (0..6)
                .map(|i| drain_units_for_tick(12, i))
                .collect::<Vec<_>>(),
            vec![2, 2, 2, 2, 2, 2]
        );
        // Day 2 (15): 2,3,2,3,2,3.
        assert_eq!(
            (0..6)
                .map(|i| drain_units_for_tick(15, i))
                .collect::<Vec<_>>(),
            vec![2, 3, 2, 3, 2, 3]
        );
    }

    #[test]
    fn starvation_day_one_drains_twelve_units() {
        let mut t = starving_character();
        let mut total_hp = 0;
        for tick in 0..6 {
            total_hp += apply_starvation_drain(&mut t, 1, BASE, GROWTH, tick);
        }
        assert_eq!(total_hp, 120, "day 1 = 12 damage units = 120 HP");
        assert_eq!(t.blood, 880);
        assert_eq!(t.starving_since_day, Some(1), "stamp records day 1");
    }

    #[test]
    fn starvation_day_two_drains_fifteen_units() {
        let mut t = starving_character();
        t.starving_since_day = Some(1);
        let mut total_hp = 0;
        for tick in 0..6 {
            total_hp += apply_starvation_drain(&mut t, 2, BASE, GROWTH, tick);
        }
        assert_eq!(total_hp, 150, "day 2 = ceil(12 x 1.25) = 15 units");
        assert_eq!(t.blood, 850);
    }

    #[test]
    fn starvation_drain_no_op_when_not_starving() {
        let mut t = baseline_character();
        t.hunger = 3;
        t.starving_since_day = Some(1);
        let lost = apply_starvation_drain(&mut t, 4, BASE, GROWTH, 0);
        assert_eq!(lost, 0);
        assert!(
            t.starving_since_day.is_none(),
            "leaving the band clears the stamp"
        );
    }

    #[test]
    fn eating_food_clears_starvation_stamp_and_reduces_hunger() {
        let mut t = starving_character();
        apply_starvation_drain(&mut t, 1, BASE, GROWTH, 0);
        assert_eq!(t.starving_since_day, Some(1));
        eat_food(&mut t, 3);
        assert_eq!(t.hunger, 2);
        assert!(
            t.starving_since_day.is_none(),
            "eat restarts the drain clock"
        );
    }

    #[test]
    fn dehydration_drain_is_independent_of_starvation() {
        let mut t = baseline_character();
        t.thirst = 3;
        t.hunger = 5;
        t.blood = 1000;
        let dehy = apply_dehydration_drain(&mut t, 1, BASE, GROWTH, 0);
        let starv = apply_starvation_drain(&mut t, 1, BASE, GROWTH, 0);
        assert_eq!(dehy, 20, "day-1 dehydration slice = 2 units");
        assert_eq!(starv, 20, "day-1 starvation slice = 2 units");
        assert_eq!(t.blood, 960);
        assert_eq!(t.dehydrated_since_day, Some(1));
        assert_eq!(t.starving_since_day, Some(1));
    }

    #[test]
    fn drink_water_clears_dehydration_stamp() {
        let mut t = baseline_character();
        t.thirst = 3;
        apply_dehydration_drain(&mut t, 1, BASE, GROWTH, 0);
        drink_water(&mut t, 2);
        assert_eq!(t.thirst, 1);
        assert!(t.dehydrated_since_day.is_none());
    }
}
