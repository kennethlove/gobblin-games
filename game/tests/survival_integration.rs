//! Deterministic integration tests for the shelter + hunger/thirst
//! survival system on the 6-tick-per-day survival schedule.

use areas::weather::Weather;
use characters::Character;
use characters::survival::{
    ThirstBand, apply_dehydration_drain, apply_starvation_drain, drink_water, eat_food,
    thirst_band, tick_survival,
};

const BASE_UNITS: u32 = 12;
const GROWTH_PCT: u32 = 125;
const TICKS_PER_DAY: usize = 6;

fn mid_character(name: &str) -> Character {
    let mut t = Character::new(name.to_string(), None, None);
    t.blood = 1000;
    // Mid-range strength so the hunger tick lands the +1 base path.
    t.attributes.strength = 50;
    // Stamina at half-max so the thirst tick lands the baseline path.
    t.stamina = t.max_stamina / 2;
    t
}

#[test]
fn no_food_no_water_dies_of_dehydration_first() {
    let mut t = mid_character("Doomed");
    let mut ticks_to_dehydrated_band = 0u32;
    for day in 1..=20u16 {
        for tick in 0..TICKS_PER_DAY {
            tick_survival(&mut t, &Weather::Clear, false, tick);
            if thirst_band(t.thirst) == ThirstBand::Dehydrated && ticks_to_dehydrated_band == 0 {
                ticks_to_dehydrated_band = u32::from(day - 1) * 6 + tick as u32 + 1;
            }
            let _ = apply_dehydration_drain(&mut t, day, BASE_UNITS, GROWTH_PCT, tick);
            let _ = apply_starvation_drain(&mut t, day, BASE_UNITS, GROWTH_PCT, tick);
            if t.effective_health() == 0 {
                // Confirm thirst drove the death.
                assert_eq!(thirst_band(t.thirst), ThirstBand::Dehydrated);
                assert!(
                    ticks_to_dehydrated_band > 0
                        && ticks_to_dehydrated_band < u32::from(day - 1) * 6 + tick as u32 + 1,
                    "must reach Dehydrated before death"
                );
                return;
            }
        }
    }
    panic!("character did not die in 20 days");
}

#[test]
fn carrying_water_extends_survival() {
    // Drink on the fourth tick of day 1 to demonstrate that a water sip
    // clears the dehydration day stamp and pushes death further out.
    // The character eats every tick so dehydration is the only killer.
    fn run_to_death(drink_amount: u8) -> u32 {
        let mut t = mid_character("R");
        for day in 1..=30u16 {
            for tick in 0..TICKS_PER_DAY {
                tick_survival(&mut t, &Weather::Clear, false, tick);
                eat_food(&mut t, 30);
                if day == 1 && tick == 3 && drink_amount > 0 {
                    drink_water(&mut t, drink_amount);
                }
                let _ = apply_dehydration_drain(&mut t, day, BASE_UNITS, GROWTH_PCT, tick);
                let _ = apply_starvation_drain(&mut t, day, BASE_UNITS, GROWTH_PCT, tick);
                if t.effective_health() == 0 {
                    return u32::from(day - 1) * 6 + tick as u32 + 1;
                }
            }
        }
        30 * 6
    }

    let baseline = run_to_death(0);
    let with_water = run_to_death(4);
    assert!(
        with_water >= baseline + 2,
        "carrying water should extend life by at least 2 ticks (baseline={baseline}, with_water={with_water})"
    );
}

#[test]
fn sheltered_in_heatwave_does_not_accrue_weather_thirst() {
    let mut t_sheltered = mid_character("S");
    let mut t_exposed = mid_character("E");

    // Ticks 0-2 are active thirst ticks (pattern [1,1,0,1,1,0]).
    for tick in 0..3 {
        tick_survival(&mut t_sheltered, &Weather::Heatwave, true, tick);
        tick_survival(&mut t_exposed, &Weather::Heatwave, false, tick);
    }
    assert!(
        t_exposed.thirst > t_sheltered.thirst,
        "exposed character should accrue more thirst (sheltered={}, exposed={})",
        t_sheltered.thirst,
        t_exposed.thirst
    );
}
