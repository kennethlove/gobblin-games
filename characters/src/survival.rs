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
pub fn tick_survival(character: &mut Character, weather: &Weather, sheltered: bool) {
    let strength = character.attributes.strength;
    // Stamina lives on Character, not Attributes; project the current stamina
    // onto a 0..=100 scale relative to max_stamina so the same thresholds
    // can be compared.
    let stamina_scaled: u32 = character
        .stamina
        .saturating_mul(100)
        .checked_div(character.max_stamina)
        .unwrap_or(0);

    let hunger_delta: u8 = if strength <= LOW_ATTR_THRESHOLD {
        // Tick every other phase: use the parity of the existing counter as
        // a deterministic skip mechanism. First tick is skipped.
        if character.hunger.is_multiple_of(2) {
            0
        } else {
            1
        }
    } else if strength >= HIGH_ATTR_THRESHOLD {
        2
    } else {
        1
    };

    let thirst_delta: u8 = if stamina_scaled <= LOW_ATTR_THRESHOLD {
        if character.thirst.is_multiple_of(2) {
            0
        } else {
            1
        }
    } else if stamina_scaled >= HIGH_ATTR_THRESHOLD {
        2
    } else {
        1
    };

    let weather_hunger_bonus: u8 = if !sheltered && matches!(weather, Weather::Blizzard) {
        1
    } else {
        0
    };
    let weather_thirst_bonus: u8 = if !sheltered && matches!(weather, Weather::Heatwave) {
        1
    } else {
        0
    };

    character.hunger = character
        .hunger
        .saturating_add(hunger_delta + weather_hunger_bonus);
    character.thirst = character
        .thirst
        .saturating_add(thirst_delta + weather_thirst_bonus);
}

/// Applies escalating starvation HP drain. Returns HP lost this phase
/// (0 if the character is not in the Starving band).
pub fn apply_starvation_drain(character: &mut Character) -> u32 {
    if hunger_band(character.hunger) != HungerBand::Starving {
        return 0;
    }
    character.starvation_drain_step = character.starvation_drain_step.saturating_add(1);
    let lost = character.starvation_drain_step as u32;
    character.blood = character.blood.saturating_sub(lost * 10);
    lost
}

/// Applies escalating dehydration HP drain. Returns HP lost this phase.
pub fn apply_dehydration_drain(character: &mut Character) -> u32 {
    if thirst_band(character.thirst) != ThirstBand::Dehydrated {
        return 0;
    }
    character.dehydration_drain_step = character.dehydration_drain_step.saturating_add(1);
    let lost = character.dehydration_drain_step as u32;
    character.blood = character.blood.saturating_sub(lost * 10);
    lost
}

/// Reduces hunger by `amount`, resetting the starvation drain counter.
pub fn eat_food(character: &mut Character, amount: u8) {
    character.hunger = character.hunger.saturating_sub(amount);
    character.starvation_drain_step = 0;
}

/// Reduces thirst by `amount`, resetting the dehydration drain counter.
pub fn drink_water(character: &mut Character, amount: u8) {
    character.thirst = character.thirst.saturating_sub(amount);
    character.dehydration_drain_step = 0;
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
        tick_survival(&mut t, &Weather::Clear, false);
        assert_eq!(t.hunger, 1);
        assert_eq!(t.thirst, 1);
    }

    #[test]
    fn survival_tick_heatwave_exposed_adds_thirst() {
        let mut t = baseline_character();
        tick_survival(&mut t, &Weather::Heatwave, false);
        assert_eq!(t.hunger, 1);
        assert_eq!(t.thirst, 2, "heatwave + exposed adds +1 thirst");
    }

    #[test]
    fn survival_tick_blizzard_exposed_adds_hunger() {
        let mut t = baseline_character();
        tick_survival(&mut t, &Weather::Blizzard, false);
        assert_eq!(t.hunger, 2, "blizzard + exposed adds +1 hunger");
        assert_eq!(t.thirst, 1);
    }

    #[test]
    fn survival_tick_sheltered_suppresses_weather_modifier() {
        let mut t = baseline_character();
        tick_survival(&mut t, &Weather::Heatwave, true);
        assert_eq!(t.thirst, 1, "shelter suppresses heatwave bonus");
        let mut t2 = baseline_character();
        tick_survival(&mut t2, &Weather::Blizzard, true);
        assert_eq!(t2.hunger, 1, "shelter suppresses blizzard bonus");
    }

    #[test]
    fn survival_tick_high_strength_increases_hunger() {
        let mut t = baseline_character();
        t.attributes.strength = 80;
        tick_survival(&mut t, &Weather::Clear, false);
        assert_eq!(t.hunger, 2, "high-strength bodies burn more calories");
    }

    #[test]
    fn survival_tick_high_stamina_increases_thirst() {
        let mut t = baseline_character();
        t.stamina = 80; // 80% of max_stamina
        tick_survival(&mut t, &Weather::Clear, false);
        assert_eq!(t.thirst, 2);
    }

    #[test]
    fn survival_tick_low_strength_skips_hunger_every_other_phase() {
        let mut t = baseline_character();
        t.attributes.strength = 20; // low
        // Phase 1: skip (parity 0)
        tick_survival(&mut t, &Weather::Clear, false);
        assert_eq!(t.hunger, 0, "low-strength skips first phase");
        // Bump parity to odd manually so the next call ticks.
        t.hunger = 1;
        tick_survival(&mut t, &Weather::Clear, false);
        assert_eq!(t.hunger, 2, "low-strength ticks once parity is odd");
    }

    fn starving_character() -> Character {
        let mut t = baseline_character();
        t.hunger = 5;
        t.blood = 1000;
        t.starvation_drain_step = 0;
        t
    }

    #[test]
    fn starvation_drain_escalates_each_phase() {
        let mut t = starving_character();
        let lost1 = apply_starvation_drain(&mut t);
        assert_eq!(lost1, 1);
        assert_eq!(t.blood, 990);
        assert_eq!(t.starvation_drain_step, 1);

        let lost2 = apply_starvation_drain(&mut t);
        assert_eq!(lost2, 2);
        assert_eq!(t.blood, 970);

        let lost3 = apply_starvation_drain(&mut t);
        assert_eq!(lost3, 3);
        assert_eq!(t.blood, 940);
    }

    #[test]
    fn starvation_drain_no_op_when_not_starving() {
        let mut t = baseline_character();
        t.hunger = 3;
        let lost = apply_starvation_drain(&mut t);
        assert_eq!(lost, 0);
        assert_eq!(t.starvation_drain_step, 0);
    }

    #[test]
    fn eating_food_resets_drain_step_and_reduces_hunger() {
        let mut t = starving_character();
        apply_starvation_drain(&mut t);
        apply_starvation_drain(&mut t);
        eat_food(&mut t, 3);
        assert_eq!(t.hunger, 2);
        assert_eq!(t.starvation_drain_step, 0);
    }

    #[test]
    fn dehydration_drain_escalates_independently_of_starvation() {
        let mut t = baseline_character();
        t.thirst = 3;
        t.hunger = 5;
        t.blood = 1000;
        let h1 = apply_dehydration_drain(&mut t);
        let s1 = apply_starvation_drain(&mut t);
        assert_eq!(h1, 1);
        assert_eq!(s1, 1);
        assert_eq!(t.blood, 980);
    }

    #[test]
    fn drink_water_resets_dehydration_step() {
        let mut t = baseline_character();
        t.thirst = 3;
        apply_dehydration_drain(&mut t);
        drink_water(&mut t, 2);
        assert_eq!(t.thirst, 1);
        assert_eq!(t.dehydration_drain_step, 0);
    }
}
