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
//!
//! # Boundary conversion odds (spec §5; design A consume)
//!
//! Every affliction on a dead goblin is consumed when it revives — the
//! affliction never crosses the game boundary, only a trait can (spec
//! §3.2). Eligible mental kinds roll their per-kind odds and, on success,
//! grant the permanent trait form; a failed roll and kinds with no
//! permanent form are gone with nothing (spec §5's 0% row).
//!
//! | Affliction            | Permanent trait | Odds  |
//! |-----------------------|-----------------|-------|
//! | `Trauma`              | `Fragile`       | 60%   |
//! | `Phobia(_)`           | `Paranoid`      | 50%   |
//! | `Fixation(_)`         | `Loyal`         | 40%   |
//! | `Addiction(_)`        | `Reckless`      | 40%   |
//!
//! All numbers are **placeholders**, tuned with observability like every
//! other odds table (spec §10.6). Trait pairings are placeholder flavor
//! mappings until dedicated permanent forms exist (e.g. a phobia-trait for
//! Trauma). Non-mental kinds never convert.
//!
//! # Event boon/curse odds (spec §3 path 3)
//!
//! Boons and curses roll **independently** at revival — one revival can
//! land both. A decided boon always sticks (no persistence roll); a
//! curse is the scarring memory and only sticks if it passes its
//! persistence roll.
//!
//! | Roll                   | Odds | Effect                                      |
//! |------------------------|------|---------------------------------------------|
//! | Boon trigger           | 30%  | Revival rolls a one-off boon. Always sticks.|
//! | Curse trigger          | 30%  | Revival rolls a one-off curse.              |
//! | Curse persistence      | 60%  | The curse sticks across games; a failed     |
//! |                        |      | roll leaves nothing behind.                 |
//!
//! All numbers are **placeholders** (spec §10.6). See
//! [`maybe_grant_event_traits`].
//!
//! # Event-grant durations (temporary traits)
//!
//! Event boons/curses are the only grants that carry a duration: each
//! granted trait gets `N` games from [`EVENT_DURATION_GAMES`], recorded
//! beside the trait in [`Character::temporary_traits`]. Pool rolls,
//! creation rolls, and affliction conversion ([`convert_afflictions`])
//! stay permanent. The countdown ticks once per played game at the finish
//! seam ([`Character::tick_temporary_traits`], run by
//! `Game::post_game_recovery` before this module's revival roll), so a
//! boon granted with `N = 2` covers the next two games and then vanishes
//! with no trace. Boons and curses share one table: both are the same
//! "one-off event" flavor (spec §3 path 3), and curse longevity already
//! has its own lever ([`EVENT_CURSE_PERSIST_ODDS`] gates whether a curse
//! sticks at all) — one shared placeholder table keeps the tuning
//! surface small until observability shows they need to diverge.

use crate::Character;
use crate::statuses::CharacterStatus;
use crate::traits::{self, Trait};
use rand::RngExt;
use rand::prelude::*;
use shared::afflictions::AfflictionKind;

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

/// Per-kind affliction → trait conversion odds at revival (spec §5's
/// table, one config struct of plain numbers). **Placeholder values** —
/// tuned with observability like every other odds table (spec §10.6).
/// Percent of a `0..100` draw: success when `draw < odds`.
#[derive(Debug, Clone, Copy)]
pub struct ConversionOdds {
    pub trauma: u8,
    pub phobia: u8,
    pub fixation: u8,
    pub addiction: u8,
}

/// The conversion odds table. Only the four mental kinds with metadata
/// structs + brain overrides are eligible (spec §5: kinds without a
/// permanent form convert at 0% and are not listed).
pub const CONVERSION_ODDS: ConversionOdds = ConversionOdds {
    // Spec §5 table: trauma hardens into a permanent scar-trait.
    trauma: 60,
    // Placeholder (spec §10.6).
    phobia: 50,
    // Placeholder (spec §10.6).
    fixation: 40,
    // Placeholder (spec §10.6).
    addiction: 40,
};

/// Percent chance a revival rolls a one-off event BOON, and separately
/// a one-off event CURSE (spec §3 path 3; the two rolls are
/// independent, so one revival can land both). Placeholder (spec §10.6).
pub const EVENT_ODDS: u8 = 30;
/// Percent chance a rolled event CURSE sticks to the persistent
/// character across games. Boons don't roll persistence — a decided
/// boon always sticks. Placeholder (spec §10.6).
pub const EVENT_CURSE_PERSIST_ODDS: u8 = 60;
/// Boon candidates for the event roll (spec §3's "healed lungs"-style
/// one-off boons; placeholder pool — existing effect-carrying variants
/// only, no new variants).
pub const EVENT_BOON_TRAITS: &[Trait] = &[Trait::Tough, Trait::Resilient, Trait::Cunning];
/// Curse candidates for the event roll (spec §3's "sprained ankle"-style
/// one-off curses; placeholder pool).
pub const EVENT_CURSE_TRAITS: &[Trait] = &[Trait::Fragile, Trait::Nearsighted, Trait::Asthmatic];
/// Games an event-granted boon/curse lasts (placeholder table, rolled
/// uniformly per grant — see the module docs for why boons and curses
/// share it). Every event grant is temporary; conversion
/// ([`convert_afflictions`]) and pool/creation traits stay permanent.
pub const EVENT_DURATION_GAMES: &[u32] = &[1, 2, 3];

/// Roll one grant's duration from [`EVENT_DURATION_GAMES`] (uniform,
/// injected `Rng` so tests stay deterministic).
pub fn event_duration_games<R: Rng + ?Sized>(rng: &mut R) -> u32 {
    EVENT_DURATION_GAMES[rng.random_range(0..EVENT_DURATION_GAMES.len())]
}

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
        1 => &mut character.attributes.toughness,
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
    // Same eligibility check as creation and Character::gain_trait (spec §3).
    let eligible: Vec<Trait> = pool
        .iter()
        .copied()
        .filter(|t| traits::can_gain(&character.traits, *t).is_ok())
        .collect();
    eligible.choose(rng).copied()
}

/// Permanent trait form + per-kind odds for an affliction kind; `None`
/// for kinds with no permanent form (they convert at 0%, spec §5 — only
/// the four mental kinds are eligible). The trait pairings are
/// placeholders until dedicated permanent forms land (see module docs).
pub fn conversion_for(kind: &AfflictionKind) -> Option<(Trait, u8)> {
    match kind {
        AfflictionKind::Trauma => Some((Trait::Fragile, CONVERSION_ODDS.trauma)),
        AfflictionKind::Phobia(_) => Some((Trait::Paranoid, CONVERSION_ODDS.phobia)),
        AfflictionKind::Fixation(_) => Some((Trait::Loyal, CONVERSION_ODDS.fixation)),
        AfflictionKind::Addiction(_) => Some((Trait::Reckless, CONVERSION_ODDS.addiction)),
        _ => None,
    }
}

/// Design A consume (spec §3.2): every affliction on `character` is
/// removed — the map is taken whole, so a consumed affliction leaves no
/// entry and no pending-conversion state. Eligible mental kinds roll
/// [`CONVERSION_ODDS`]; a success grants the permanent trait through
/// [`Character::gain_trait`] (stamped into the advancement log), a
/// failure and ineligible kinds are gone with nothing. Never the
/// affliction and the trait together: even when the grant is blocked
/// (duplicate/conflict), the affliction stays consumed. Returns the
/// conversion traits granted.
pub fn convert_afflictions<R: Rng + ?Sized>(character: &mut Character, rng: &mut R) -> Vec<Trait> {
    let afflictions = std::mem::take(&mut character.afflictions);
    let mut granted = Vec::new();
    for affliction in afflictions.into_values() {
        let Some((trait_, odds)) = conversion_for(&affliction.kind) else {
            // No permanent form: converts at 0% — gone with nothing.
            continue;
        };
        if rng.random_range(0..100u8) >= odds {
            continue;
        }
        if character.gain_trait(trait_).is_ok() {
            granted.push(trait_);
        }
    }
    granted
}

/// Whether this revival rolls a one-off event boon, and separately an
/// event curse ([`EVENT_ODDS`] each, independent — one revival can land
/// both). Placeholder.
pub fn should_roll_event<R: Rng + ?Sized>(rng: &mut R) -> bool {
    rng.random_range(0..100u8) < EVENT_ODDS
}

/// Whether a rolled event CURSE sticks to the persistent character
/// across games ([`EVENT_CURSE_PERSIST_ODDS`], placeholder). Boons are
/// not rolled here — a decided boon always sticks.
pub fn curse_persists<R: Rng + ?Sized>(rng: &mut R) -> bool {
    rng.random_range(0..100u8) < EVENT_CURSE_PERSIST_ODDS
}

/// Draw an eligible trait from the boon pool.
fn event_boon_candidate<R: Rng + ?Sized>(character: &Character, rng: &mut R) -> Option<Trait> {
    pick_eligible(character, EVENT_BOON_TRAITS, rng)
}

/// Draw an eligible trait from the curse pool.
fn event_curse_candidate<R: Rng + ?Sized>(character: &Character, rng: &mut R) -> Option<Trait> {
    pick_eligible(character, EVENT_CURSE_TRAITS, rng)
}

/// Roll the one-off event boon/curse at revival (spec §3 path 3).
/// Independent rolls: the boon trigger ([`EVENT_ODDS`]) and the curse
/// trigger ([`EVENT_ODDS`]) each fire, so one revival can land both. A
/// boon that triggers always sticks — decided means granted. A curse
/// only sticks if it passes [`curse_persists`]. Grants go through
/// [`Character::gain_trait`], so they are stamped as advancements and
/// cross the game boundary; each landed grant then draws a duration
/// from [`EVENT_DURATION_GAMES`] into [`Character::temporary_traits`],
/// so it expires after that many games (boons and curses alike). A trait
/// the character already holds permanently is rejected by
/// `gain_trait` (duplicate) and gains no duration entry. Returns
/// `(boon, curse)`, each `None` when untriggered, ineligible, or
/// (curse) failed persistence — a failed roll leaves no trait and no
/// state behind.
pub fn maybe_grant_event_traits<R: Rng + ?Sized>(
    character: &mut Character,
    rng: &mut R,
) -> (Option<Trait>, Option<Trait>) {
    let boon = if should_roll_event(rng) {
        event_boon_candidate(character, rng).and_then(|trait_| {
            character.gain_trait(trait_).ok().map(|()| {
                character
                    .temporary_traits
                    .insert(trait_, event_duration_games(rng));
                trait_
            })
        })
    } else {
        None
    };
    let curse = if should_roll_event(rng) {
        match event_curse_candidate(character, rng) {
            Some(trait_) if curse_persists(rng) => character.gain_trait(trait_).ok().map(|()| {
                character
                    .temporary_traits
                    .insert(trait_, event_duration_games(rng));
                trait_
            }),
            _ => None,
        }
    } else {
        None
    };
    (boon, curse)
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

/// Roll one revival for a dead goblin: apply the persistent outcome,
/// consume the character's afflictions at the boundary (design A —
/// [`convert_afflictions`]), roll the independent event boon/curse
/// ([`maybe_grant_event_traits`], each landed grant temporary —
/// [`EVENT_DURATION_GAMES`]), then [`restore`] them so they are playable
/// in future games. Returns the outcome that was applied. Expiry ticks
/// happen in `Game::post_game_recovery` *before* this runs, so a fresh
/// grant is never decremented in the boundary that granted it.
pub fn revive<R: Rng + ?Sized>(character: &mut Character, rng: &mut R) -> RevivalOutcome {
    // Lifetime counter (dww.10.1): every call here is one revival — the
    // function has no non-revival exit — so the seam increments up front.
    character.statistics.times_revived = character.statistics.times_revived.saturating_add(1);
    let outcome = roll(rng);
    match outcome {
        RevivalOutcome::Clean => {}
        RevivalOutcome::Penalty => apply_penalty(character, rng),
        RevivalOutcome::Scar => apply_scar(character, rng),
        RevivalOutcome::Bonus => apply_bonus(character, rng),
    }
    // Boundary-only acquisition (spec §10.5): conversion and event
    // boons/curses roll here, at the post-game boundary, nowhere else.
    convert_afflictions(character, rng);
    let _ = maybe_grant_event_traits(character, rng);
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
        // Stats are RNG-rolled by the fixture; pin them so the seeded penalty
        // always bites (previously flaky when the picked stat was already 1).
        character.attributes.strength = 10;
        character.attributes.toughness = 10;
        character.attributes.luck = 10;
        character.traits.clear();
        let before = character.attributes.strength
            + character.attributes.toughness
            + character.attributes.luck;
        let mut rng = SmallRng::seed_from_u64(3);
        apply_penalty(&mut character, &mut rng);
        let after = character.attributes.strength
            + character.attributes.toughness
            + character.attributes.luck;
        assert!(
            after < before || before <= 3,
            "penalty should bite: {before} -> {after}"
        );
        assert!(character.attributes.strength >= 1);
        assert!(character.attributes.toughness >= 1);
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
    fn revive_increments_the_lifetime_counter(mut character: Character) {
        character.traits.clear();
        character.dies();
        let mut rng = SmallRng::seed_from_u64(5);
        assert_eq!(character.statistics.times_revived, 0);
        revive(&mut character, &mut rng);
        assert_eq!(character.statistics.times_revived, 1);
        character.dies();
        revive(&mut character, &mut rng);
        assert_eq!(character.statistics.times_revived, 2);
    }

    #[rstest]
    fn revive_applies_the_rolled_outcome(mut character: Character) {
        // Pin RNG-rolled fixture stats so the seeded penalty always bites
        // (same pre-existing flake class as penalty_reduces_one_stat...).
        character.attributes.strength = 10;
        character.attributes.toughness = 10;
        character.attributes.luck = 10;
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
            + character.attributes.toughness
            + character.attributes.luck;
        let outcome = revive(&mut character, &mut rng);
        assert_eq!(outcome, RevivalOutcome::Penalty);
        let after = character.attributes.strength
            + character.attributes.toughness
            + character.attributes.luck;
        assert!(
            after < before || before <= 3,
            "penalty should mutate stats: {before} -> {after}"
        );
        assert!(character.is_alive());
    }

    // ---- Affliction → trait conversion at revival (spec §3.2 design A
    // consume, §5 odds table, §10.5 boundary-only) ----

    use shared::afflictions::{
        Affliction, AfflictionKind, AfflictionSource, FixationTarget, PhobiaTrigger, Severity,
        Substance, TrapKind,
    };
    use shared::conditions::{ConditionSeverity, MentalCondition};

    fn affliction_of(kind: AfflictionKind) -> Affliction {
        Affliction {
            kind,
            body_part: None,
            severity: Severity::Moderate,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".to_string(),
            },
            acquired_cycle: 1,
            last_progressed_cycle: 1,
            trauma_metadata: None,
            phobia_metadata: None,
            fixation_metadata: None,
            addiction_metadata: None,
            trapped_metadata: None,
        }
    }

    fn insert_affliction(character: &mut Character, kind: AfflictionKind) {
        let affliction = affliction_of(kind);
        character.afflictions.insert(affliction.key(), affliction);
    }

    /// Every mental affliction kind has a permanent form and nonzero odds;
    /// every other kind has no permanent form (converts at 0%, spec §5).
    #[test]
    fn conversion_table_covers_exactly_the_four_mental_kinds() {
        let mental = [
            AfflictionKind::Trauma,
            AfflictionKind::Phobia(PhobiaTrigger::Fire),
            AfflictionKind::Fixation(FixationTarget::AreaId("hub".to_string())),
            AfflictionKind::Addiction(Substance::Alcohol),
        ];
        for kind in mental {
            let conversion = conversion_for(&kind);
            assert!(
                conversion.is_some(),
                "{kind:?} is one of the four mental kinds and must convert"
            );
            let (_, odds) = conversion.unwrap();
            assert!(odds > 0, "{kind:?} conversion odds must be nonzero");
        }
        let physical = [
            AfflictionKind::Wounded,
            AfflictionKind::Infected,
            AfflictionKind::MissingArm,
            AfflictionKind::MissingLeg,
            AfflictionKind::Blind,
            AfflictionKind::Deaf,
            AfflictionKind::BrokenBone,
            AfflictionKind::Poisoned,
            AfflictionKind::Starving,
            AfflictionKind::Dehydrated,
            AfflictionKind::Frozen,
            AfflictionKind::Overheated,
            AfflictionKind::Burned,
            AfflictionKind::Sick,
            AfflictionKind::Electrocuted,
            AfflictionKind::Trapped(TrapKind::Drowning),
        ];
        for kind in physical {
            assert_eq!(
                conversion_for(&kind),
                None,
                "{kind:?} has no permanent form and must never convert"
            );
        }
    }

    /// Seeded revival conversion rolls per eligible kind: the grant rate
    /// conforms to the odds table (statistical sanity band, ±5 points over
    /// 10k deterministic seeds), and every grant consumes its affliction.
    #[test]
    fn conversion_frequency_conforms_to_the_odds_table() {
        let eligible = [
            (
                AfflictionKind::Trauma,
                CONVERSION_ODDS.trauma,
                Trait::Fragile,
            ),
            (
                AfflictionKind::Phobia(PhobiaTrigger::Blood),
                CONVERSION_ODDS.phobia,
                Trait::Paranoid,
            ),
            (
                AfflictionKind::Fixation(FixationTarget::Item("torch".to_string())),
                CONVERSION_ODDS.fixation,
                Trait::Loyal,
            ),
            (
                AfflictionKind::Addiction(Substance::Stimulant),
                CONVERSION_ODDS.addiction,
                Trait::Reckless,
            ),
        ];
        const SAMPLES: u64 = 10_000;
        for (kind, odds, expected_trait) in eligible {
            let (table_trait, table_odds) =
                conversion_for(&kind).expect("eligible kind must have a conversion");
            assert_eq!(table_odds, odds, "{kind:?} odds must come from the table");
            assert_eq!(table_trait, expected_trait);

            let mut converted = 0u32;
            let mut character = Character::new("Snaggletooth".to_string(), None, None);
            for seed in 0..SAMPLES {
                // Pin the trait set so every success can land (fresh
                // character per seed: no cross-seed trait buildup).
                character.traits.clear();
                character.advancement_log.clear();
                character.afflictions.clear();
                insert_affliction(&mut character, kind.clone());

                let mut rng = SmallRng::seed_from_u64(seed);
                let granted = convert_afflictions(&mut character, &mut rng);
                assert!(
                    character.afflictions.is_empty(),
                    "consumed afflictions must leave no entry (seed {seed})"
                );
                if granted.is_empty() {
                    // Design A: failure consumes with nothing.
                    assert!(
                        character.traits.is_empty(),
                        "seed {seed}: no trait on failure"
                    );
                } else {
                    converted += 1;
                    assert_eq!(granted, vec![expected_trait], "seed {seed}");
                    assert_eq!(character.traits, vec![expected_trait], "seed {seed}");
                }
            }
            let got = converted as f64 / SAMPLES as f64 * 100.0;
            assert!(
                (got - odds as f64).abs() <= 5.0,
                "{kind:?}: conversion rate {got}%, want {odds}% ±5"
            );
        }
    }

    /// Ineligible kinds never convert: over seeded revival rolls they never
    /// grant a trait (they are consumed with nothing, spec §5's 0% row).
    #[test]
    fn ineligible_kinds_never_convert() {
        let ineligible = [
            AfflictionKind::Wounded,
            AfflictionKind::Infected,
            AfflictionKind::MissingArm,
            AfflictionKind::MissingLeg,
            AfflictionKind::Blind,
            AfflictionKind::Deaf,
            AfflictionKind::BrokenBone,
            AfflictionKind::Poisoned,
            AfflictionKind::Starving,
            AfflictionKind::Dehydrated,
            AfflictionKind::Frozen,
            AfflictionKind::Overheated,
            AfflictionKind::Burned,
            AfflictionKind::Sick,
            AfflictionKind::Electrocuted,
            AfflictionKind::Trapped(TrapKind::Drowning),
        ];
        for kind in ineligible {
            let mut character = Character::new("Stinky".to_string(), None, None);
            for seed in 0..200u64 {
                character.traits.clear();
                character.advancement_log.clear();
                character.afflictions.clear();
                insert_affliction(&mut character, kind.clone());

                let mut rng = SmallRng::seed_from_u64(seed);
                let granted = convert_afflictions(&mut character, &mut rng);
                assert!(
                    granted.is_empty(),
                    "{kind:?} must never convert (seed {seed})"
                );
                assert!(character.traits.is_empty(), "{kind:?} granted a trait");
            }
        }
    }

    /// Design A, serialized: a consumed affliction leaves no affliction
    /// entry and no pending-conversion state, on both conversion paths
    /// (success grants the trait, failure grants nothing — never both the
    /// affliction and the trait), and `mental_conditions` stays untouched.
    #[test]
    fn consumed_afflictions_leave_no_trace() {
        let mut rng = SmallRng::seed_from_u64(9);
        let mut character = Character::new("Billy Slick".to_string(), None, None);
        character.traits.clear();
        character.mental_conditions = vec![MentalCondition::Pain {
            severity: ConditionSeverity::Moderate,
        }];
        insert_affliction(&mut character, AfflictionKind::Trauma);
        let before = serde_json::to_value(&character).unwrap();
        assert!(
            before.get("afflictions").is_some(),
            "fixture must serialize its affliction before conversion"
        );

        let mut saw_grant = false;
        let mut saw_no_grant = false;
        for _ in 0..100 {
            character.traits.clear();
            character.advancement_log.clear();
            character.afflictions.clear();
            insert_affliction(&mut character, AfflictionKind::Trauma);

            let granted = convert_afflictions(&mut character, &mut rng);
            let after = serde_json::to_value(&character).unwrap();
            // No affliction entry: the map serializes only when non-empty,
            // so a clean conversion drops the key entirely. There is no
            // pending-conversion state to serialize either (the design
            // carries no such field).
            assert!(
                after.get("afflictions").is_none(),
                "consumed affliction must not serialize: {after}"
            );
            // mental_conditions is per-game state the boundary does not own.
            assert_eq!(
                after["mental_conditions"], before["mental_conditions"],
                "conversion must not touch mental_conditions"
            );
            // Never affliction + trait together: grant ⟺ trait present.
            assert_eq!(
                granted.is_empty(),
                character.traits.is_empty(),
                "design A: exactly one of {granted:?} / {:?}",
                character.traits
            );
            saw_grant |= !granted.is_empty();
            saw_no_grant |= granted.is_empty();
        }
        assert!(
            saw_grant && saw_no_grant,
            "seed range must exercise both conversion paths"
        );
    }

    /// Conversion results stay permanent: design-A grants never get a
    /// duration entry, so boundary ticks never expire them.
    #[test]
    fn conversion_grants_stay_permanent() {
        let mut character = Character::new("Stinky".to_string(), None, None);
        for seed in 0..200u64 {
            character.traits.clear();
            character.advancement_log.clear();
            character.temporary_traits.clear();
            character.afflictions.clear();
            insert_affliction(&mut character, AfflictionKind::Trauma);

            let mut rng = SmallRng::seed_from_u64(seed);
            let granted = convert_afflictions(&mut character, &mut rng);
            assert!(
                character.temporary_traits.is_empty(),
                "seed {seed}: conversion must never set a duration"
            );
            for _ in 0..5 {
                character.tick_temporary_traits();
            }
            for trait_ in &granted {
                assert!(
                    character.traits.contains(trait_),
                    "seed {seed}: conversion trait {trait_:?} must survive boundary ticks"
                );
            }
        }
    }

    /// End-to-end through `revive()`: a dead goblin's afflictions are gone
    /// from the persisted character when it wakes.
    #[rstest]
    fn revive_consumes_afflictions_at_the_boundary(mut character: Character) {
        character.traits.clear();
        insert_affliction(
            &mut character,
            AfflictionKind::Addiction(Substance::Alcohol),
        );
        character.dies();
        assert!(!character.is_alive());

        let mut rng = SmallRng::seed_from_u64(4);
        let _ = revive(&mut character, &mut rng);

        assert!(character.is_alive());
        assert!(character.afflictions.is_empty());
        let serialized = serde_json::to_value(&character).unwrap();
        assert!(serialized.get("afflictions").is_none());
    }

    // ---- Event boons/curses at revival (spec §3 path 3) ----

    /// Independent boon/curse rolls honored across a simulated second revival:
    /// (a) `curse_persists` draws conform to `EVENT_CURSE_PERSIST_ODDS`,
    /// (b) boon grant rate = `EVENT_ODDS` (a decided boon always sticks) and
    ///     curse grant rate = `EVENT_ODDS × EVENT_CURSE_PERSIST_ODDS`; failed
    ///     curse persistence leaves the curse off,
    /// (c) landed traits stick across a second revival; presence after two
    ///     revivals matches the combined-rate formula.
    #[test]
    fn event_boon_curse_persistence_honored_across_a_second_revival() {
        const SAMPLES: u64 = 10_000;

        // (a) Curse persistence odds alone.
        let mut persisted = 0u32;
        for seed in 0..SAMPLES {
            let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(1_000));
            if curse_persists(&mut rng) {
                persisted += 1;
            }
        }
        let persisted_pct = persisted as f64 / SAMPLES as f64 * 100.0;
        assert!(
            (persisted_pct - EVENT_CURSE_PERSIST_ODDS as f64).abs() <= 5.0,
            "curse persistence rate {persisted_pct}%, want {EVENT_CURSE_PERSIST_ODDS}% ±5"
        );

        // (b) Boon rate = trigger; curse rate = trigger × persistence.
        let mut boon_count = 0u32;
        let mut curse_count = 0u32;
        let mut both_count = 0u32;
        let mut character = Character::new("Snaggletooth".to_string(), None, None);
        for seed in 0..SAMPLES {
            character.traits.clear();
            character.advancement_log.clear();
            character.temporary_traits.clear();
            let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(2_000));
            let (boon, curse) = maybe_grant_event_traits(&mut character, &mut rng);
            let mut expected = 0u32;
            if let Some(granted) = boon {
                boon_count += 1;
                expected += 1;
                assert!(
                    EVENT_BOON_TRAITS.contains(&granted),
                    "{granted:?} is not a boon trait"
                );
                assert!(
                    character.traits.contains(&granted),
                    "seed {seed}: boon must stick"
                );
                let duration = character.temporary_traits.get(&granted).copied();
                assert!(
                    matches!(duration, Some(d) if EVENT_DURATION_GAMES.contains(&d)),
                    "seed {seed}: boon {granted:?} must carry a duration from the table, got {duration:?}"
                );
            }
            if let Some(granted) = curse {
                curse_count += 1;
                expected += 1;
                assert!(
                    EVENT_CURSE_TRAITS.contains(&granted),
                    "{granted:?} is not a curse trait"
                );
                assert!(
                    character.traits.contains(&granted),
                    "seed {seed}: curse must stick"
                );
                let duration = character.temporary_traits.get(&granted).copied();
                assert!(
                    matches!(duration, Some(d) if EVENT_DURATION_GAMES.contains(&d)),
                    "seed {seed}: curse {granted:?} must carry a duration from the table, got {duration:?}"
                );
            }
            if boon.is_some() && curse.is_some() {
                both_count += 1;
            }
            assert_eq!(
                character.traits.len() as u32,
                expected,
                "seed {seed}: exactly the granted traits, nothing else"
            );
        }
        assert!(both_count > 0, "independent rolls must land both sometimes");
        let boon_pct = boon_count as f64 / SAMPLES as f64 * 100.0;
        assert!(
            (boon_pct - EVENT_ODDS as f64).abs() <= 5.0,
            "boon grant rate {boon_pct}%, want {EVENT_ODDS}% ±5 (boons always stick)"
        );
        let curse_pct = curse_count as f64 / SAMPLES as f64 * 100.0;
        let want_curse_pct = EVENT_ODDS as f64 * EVENT_CURSE_PERSIST_ODDS as f64 / 100.0;
        assert!(
            (curse_pct - want_curse_pct).abs() <= 5.0,
            "curse grant rate {curse_pct}%, want {want_curse_pct}% ±5"
        );

        // (c) Simulated second revival. Filter to outcome pairs that add no
        // scar/bonus traits, so every trait observed comes from the event path.
        let mut character = Character::new("Billy Slick".to_string(), None, None);
        let mut filtered = 0u32;
        let mut landed_after_first = 0u32;
        let mut landed_after_second = 0u32;
        for seed in 0..SAMPLES {
            character.traits.clear();
            character.advancement_log.clear();
            character.afflictions.clear();
            character.temporary_traits.clear();
            character.attributes.strength = 10;
            character.attributes.toughness = 10;
            character.attributes.luck = 10;

            let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(3_000));
            character.dies();
            let first_outcome = revive(&mut character, &mut rng);
            let after_first: Vec<Trait> = character.traits.clone();

            character.dies();
            let second_outcome = revive(&mut character, &mut rng);

            let adds_no_scar_or_bonus = matches!(
                first_outcome,
                RevivalOutcome::Clean | RevivalOutcome::Penalty
            ) && matches!(
                second_outcome,
                RevivalOutcome::Clean | RevivalOutcome::Penalty
            );
            if !adds_no_scar_or_bonus {
                continue;
            }
            filtered += 1;
            if !after_first.is_empty() {
                landed_after_first += 1;
            }
            if !character.traits.is_empty() {
                landed_after_second += 1;
            }
            // Persistence: whatever landed at the first revival still sticks.
            for trait_ in &after_first {
                assert!(
                    character.traits.contains(trait_),
                    "seed {seed}: {trait_:?} lost across the second revival"
                );
            }
        }
        assert!(filtered > 0, "filtered sample must not be empty");
        // Per-revival chance any event trait lands:
        // 1 − (1 − boon)·(1 - boon·curse-persist).
        let e = EVENT_ODDS as f64 / 100.0;
        let ep = e * EVENT_CURSE_PERSIST_ODDS as f64 / 100.0;
        let q = 1.0 - (1.0 - e) * (1.0 - ep);
        let first_pct = landed_after_first as f64 / filtered as f64 * 100.0;
        assert!(
            (first_pct - q * 100.0).abs() <= 5.0,
            "first-revival event rate {first_pct}%, want {}% ±5",
            q * 100.0
        );
        let second_pct = landed_after_second as f64 / filtered as f64 * 100.0;
        let want_second_pct = (1.0 - (1.0 - q) * (1.0 - q)) * 100.0;
        assert!(
            (second_pct - want_second_pct).abs() <= 5.0,
            "two-revival event rate {second_pct}%, want {want_second_pct}% ±5"
        );
    }
}
