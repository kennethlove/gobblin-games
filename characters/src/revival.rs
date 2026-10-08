//! Post-game revival roll: every dead goblin wakes up after the game
//! finishes — death is never permanent. The roll decides *how* they
//! come back, and the outcome lands as persistent state on the character
//! (traits / attribute deltas) so it feeds trait evolution across games.
//!
//! # V1 odds (percent of rolls, `roll()` draws 0..100)
//!
//! | Outcome  | Odds | Effect                                                    |
//! |----------|------|-----------------------------------------------------------|
//! | `Clean`  | 60%  | Wakes with no lasting change.                              |
//! | `Penalty`| 20%  | Lingering ache: −1 to one of strength / defense / luck     |
//! |          |      | (stat chosen by the roll, floor 1). Persistent.            |
//! | `Scar`   | 10%  | Gains an eligible scar trait (Fragile, Paranoid,          |
//! |          |      | Nearsighted); falls back to −1 luck when none fit.         |
//! | `Bonus`  | 10%  | Gains an eligible bonus trait (Tough, Resilient, Cunning); |
//! |          |      | falls back to +1 luck when none fit.                       |
//!
//! Trait picks skip traits the character already has and trait conflicts
//! (`traits::CONFLICTS`). All draws go through an injected `Rng`, so tests
//! are deterministic under a seed. The roll runs once per game at the
//! finish seam — see `Game::post_game_recovery` and the comments in
//! `api/src/games/{mod,handlers}.rs`.

use crate::Character;
use crate::statuses::CharacterStatus;
use crate::traits::{self, Trait};
use rand::RngExt;
use rand::prelude::*;

/// How the goblin came back from the dead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevivalOutcome {
    /// Wakes up untouched.
    Clean,
    /// Persistent stat downside (see [`apply_penalty`]).
    Penalty,
    /// Persistent downside trait (see [`apply_scar`]).
    Scar,
    /// Persistent upside trait (see [`apply_bonus`]).
    Bonus,
}

/// Odds table for [`roll`]: `(outcome, low end of its 0..100 band)`.
/// Bands are contiguous and cover the full range.
const ODDS: &[(RevivalOutcome, u8)] = &[
    (RevivalOutcome::Clean, 0),
    (RevivalOutcome::Penalty, 60),
    (RevivalOutcome::Scar, 80),
    (RevivalOutcome::Bonus, 90),
];

/// Downside traits a scar can grant.
const SCAR_TRAITS: &[Trait] = &[Trait::Fragile, Trait::Paranoid, Trait::Nearsighted];
/// Upside traits a bonus can grant.
const BONUS_TRAITS: &[Trait] = &[Trait::Tough, Trait::Resilient, Trait::Cunning];

/// Roll one revival outcome using `rng` (injected so tests can seed it).
pub fn roll<R: Rng + ?Sized>(rng: &mut R) -> RevivalOutcome {
    let draw = rng.random_range(0..100u8);
    let mut outcome = RevivalOutcome::Clean;
    for (candidate, low) in ODDS {
        if draw >= *low {
            outcome = *candidate;
        }
    }
    outcome
}

/// −1 to one of strength / defense / luck (drawn from `rng`), floor 1.
pub fn apply_penalty<R: Rng + ?Sized>(character: &mut Character, rng: &mut R) {
    let stat = match rng.random_range(0..3u8) {
        0 => &mut character.attributes.strength,
        1 => &mut character.attributes.defense,
        _ => &mut character.attributes.luck,
    };
    if *stat > 1 {
        *stat -= 1;
    }
}

/// Grant an eligible scar trait; falls back to −1 luck when every scar
/// trait is already held or blocked by a conflict.
pub fn apply_scar<R: Rng + ?Sized>(character: &mut Character, rng: &mut R) {
    match pick_eligible(character, SCAR_TRAITS, rng) {
        Some(trait_) => character.traits.push(trait_),
        None => {
            if character.attributes.luck > 1 {
                character.attributes.luck -= 1;
            }
        }
    }
}

/// Grant an eligible bonus trait; falls back to +1 luck when every bonus
/// trait is already held or blocked by a conflict.
pub fn apply_bonus<R: Rng + ?Sized>(character: &mut Character, rng: &mut R) {
    match pick_eligible(character, BONUS_TRAITS, rng) {
        Some(trait_) => character.traits.push(trait_),
        None => character.attributes.luck = character.attributes.luck.saturating_add(1),
    }
}

/// Pick a random trait from `pool` that `character` can actually take
/// (not already held, no conflict with an existing trait).
fn pick_eligible<R: Rng + ?Sized>(
    character: &Character,
    pool: &[Trait],
    rng: &mut R,
) -> Option<Trait> {
    let eligible: Vec<Trait> = pool
        .iter()
        .copied()
        .filter(|t| !character.traits.contains(t))
        .filter(|t| {
            !character
                .traits
                .iter()
                .any(|held| traits::conflicts_with(*t, *held))
        })
        .collect();
    eligible.choose(rng).copied()
}

/// Restore a character to full playable health: healthy status, default
/// blood, no wounds, no sleep. Applied to revived goblins after their roll
/// and to every survivor at the game's finish seam.
pub fn restore(character: &mut Character) {
    character.set_status(CharacterStatus::Healthy);
    character.blood = super::default_blood();
    character.wounds.clear();
    character.sleeping = false;
    character.sleep_remaining = 0;
}

/// Roll one revival for a dead goblin: apply the persistent outcome, then
/// [`restore`] them so they are playable in future games. Returns the
/// outcome that was applied.
pub fn revive<R: Rng + ?Sized>(character: &mut Character, rng: &mut R) -> RevivalOutcome {
    let outcome = roll(rng);
    match outcome {
        RevivalOutcome::Clean => {}
        RevivalOutcome::Penalty => apply_penalty(character, rng),
        RevivalOutcome::Scar => apply_scar(character, rng),
        RevivalOutcome::Bonus => apply_bonus(character, rng),
    }
    restore(character);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Trait;
    use rand::rngs::SmallRng;
    use rstest::*;

    #[fixture]
    fn character() -> Character {
        Character::new("Snaggletooth".to_string(), None, None)
    }

    #[test]
    fn each_outcome_reachable_and_odds_within_tolerance() {
        let mut rng = SmallRng::seed_from_u64(42);
        let mut counts = [0u32; 4]; // indexed by outcome
        const SAMPLES: u32 = 10_000;
        for _ in 0..SAMPLES {
            let outcome = roll(&mut rng);
            counts[outcome as usize] += 1;
        }
        // Every outcome must show up.
        assert!(
            counts.iter().all(|&c| c > 0),
            "outcome unreachable: {counts:?}"
        );
        // Expected bands: Clean 60%, Penalty 20%, Scar 10%, Bonus 10%.
        let expected = [60, 20, 10, 10];
        for (i, &want) in expected.iter().enumerate() {
            let got = counts[i] as f64 / SAMPLES as f64 * 100.0;
            assert!(
                (got - want as f64).abs() <= 5.0,
                "outcome {i}: got {got}%, want {want}% ±5"
            );
        }
    }

    #[test]
    fn deterministic_under_the_same_seed() {
        let mut a = SmallRng::seed_from_u64(7);
        let mut b = SmallRng::seed_from_u64(7);
        for _ in 0..200 {
            assert_eq!(roll(&mut a), roll(&mut b));
        }
    }

    #[rstest]
    fn penalty_reduces_one_stat_but_never_below_one(mut character: Character) {
        character.traits.clear();
        let before = character.attributes.strength
            + character.attributes.defense
            + character.attributes.luck;
        let mut rng = SmallRng::seed_from_u64(3);
        apply_penalty(&mut character, &mut rng);
        let after = character.attributes.strength
            + character.attributes.defense
            + character.attributes.luck;
        assert!(
            after < before || before <= 3,
            "penalty should bite: {before} -> {after}"
        );
        assert!(character.attributes.strength >= 1);
        assert!(character.attributes.defense >= 1);
        assert!(character.attributes.luck >= 1);
    }

    #[rstest]
    fn scar_adds_a_scar_trait(mut character: Character) {
        character.traits.clear();
        let mut rng = SmallRng::seed_from_u64(11);
        apply_scar(&mut character, &mut rng);
        assert!(
            character.traits.iter().any(|t| SCAR_TRAITS.contains(t)),
            "expected a scar trait, got {:?}",
            character.traits
        );
    }

    #[rstest]
    fn bonus_adds_a_bonus_trait(mut character: Character) {
        character.traits.clear();
        let mut rng = SmallRng::seed_from_u64(13);
        apply_bonus(&mut character, &mut rng);
        assert!(
            character.traits.iter().any(|t| BONUS_TRAITS.contains(t)),
            "expected a bonus trait, got {:?}",
            character.traits
        );
    }

    #[rstest]
    fn scar_never_adds_conflicting_or_duplicate_traits(mut character: Character) {
        // Pin the trait set (Character::new rolls traits randomly).
        character.traits = vec![Trait::Friendly]; // conflicts with Paranoid
        let mut rng = SmallRng::seed_from_u64(17);
        for _ in 0..50 {
            apply_scar(&mut character, &mut rng);
        }
        // No duplicates.
        let unique: std::collections::HashSet<_> = character.traits.iter().collect();
        assert_eq!(
            unique.len(),
            character.traits.len(),
            "duplicate traits: {:?}",
            character.traits
        );
        // No Friendly + Paranoid pair.
        assert!(
            !(character.traits.contains(&Trait::Friendly)
                && character.traits.contains(&Trait::Paranoid)),
            "conflicting traits granted: {:?}",
            character.traits
        );
    }

    #[rstest]
    fn revive_wakes_a_dead_goblin(mut character: Character) {
        character.traits.clear();
        character.dies();
        assert!(!character.is_alive());
        let mut rng = SmallRng::seed_from_u64(5);
        let outcome = revive(&mut character, &mut rng);
        assert!(
            character.is_alive(),
            "revived goblin should be in play: {outcome:?}"
        );
        assert_eq!(character.status, CharacterStatus::Healthy);
        assert!(character.blood > 0);
        assert!(character.wounds.is_empty());
    }

    #[rstest]
    fn revive_applies_the_rolled_outcome(mut character: Character) {
        character.traits.clear();
        character.dies();
        // Find a seed whose first roll is Penalty (stat change), then check
        // the character's stat total moved while they woke up.
        let mut seeded = None;
        for seed in 0..500u64 {
            let mut probe = SmallRng::seed_from_u64(seed);
            if roll(&mut probe) == RevivalOutcome::Penalty {
                seeded = Some(seed);
                break;
            }
        }
        let seed = seeded.expect("Penalty is in the odds table, a seed must exist");
        let mut rng = SmallRng::seed_from_u64(seed);
        let before = character.attributes.strength
            + character.attributes.defense
            + character.attributes.luck;
        let outcome = revive(&mut character, &mut rng);
        assert_eq!(outcome, RevivalOutcome::Penalty);
        let after = character.attributes.strength
            + character.attributes.defense
            + character.attributes.luck;
        assert!(
            after < before || before <= 3,
            "penalty should mutate stats: {before} -> {after}"
        );
        assert!(character.is_alive());
    }
}
