//! Combat resolution: attack contest, damage application, violence stress.
//!
//! This submodule owns the pure combat-logic functions:
//! - `attack_contest` — rolls, modifiers, wear, outcome determination
//! - `apply_combat_results` — damage application, stat bookkeeping
//! - `calculate_violence_stress` / `apply_violence_stress` — mental damage
//! - `AttackContestOutcome` — the value object returned by `attack_contest`

use crate::Character;
use crate::actions::{AttackOutcome, AttackResult};
use crate::combat::inflict_table::{
    HitSeverity, WeaponKind, lookup_break_mid_swing_inflict, lookup_inflicts,
};
use crate::stamina_band::stamina_band;
use crate::traits::CharacterAttribute;
use rand::RngExt;
use rand::prelude::*;
use shared::combat_beat::{CombatBeat, StressReport, SwingOutcome, WearOutcomeReport, WearReport};
use shared::conditions::{ConditionSeverity, MentalCondition};
use shared::messages::{CharacterRef, MessagePayload};
use shared::messages::{ItemRef, StaminaBand};
use shared::wounds::{BodyPart, WoundSeverity, WoundType};
use std::cmp::Ordering;
use world::items::{Item, OwnsItems};
use world::messages::TaggedEvent;
use world::output::GameOutput;

// ---------------------------------------------------------------------------
// Helpers shared between resolve.rs and mod.rs (via pub(super))
// ---------------------------------------------------------------------------

/// Build a `CharacterRef` from a character.
pub(super) fn tref(t: &Character) -> CharacterRef {
    CharacterRef {
        identifier: t.identifier.clone().into(),
        name: t.name.clone(),
    }
}

pub(super) fn iref(i: &Item) -> ItemRef {
    ItemRef {
        identifier: i.identifier.clone().into(),
        name: i.name.clone(),
    }
}

/// Build a baseline `CombatBeat` (no wear, no stress) for the current swing.
/// Callers fill in `outcome` and (eventually) `wear` / `stress` per branch.
pub(super) fn new_beat(
    attacker: &Character,
    target: &Character,
    outcome: SwingOutcome,
    tuning: &crate::combat_tuning::CombatTuning,
) -> CombatBeat {
    CombatBeat {
        round: 1,
        order: 0,
        actor: tref(attacker),
        action: shared::combat_beat::CombatActionKind::Attack,
        target: Some(tref(target)),
        weapon: attacker
            .items
            .iter()
            .rfind(|i| i.is_weapon() && i.current_durability > 0)
            .map(iref),
        shield: target
            .items
            .iter()
            .rfind(|i| i.is_shield() && i.current_durability > 0)
            .map(iref),
        wear: Vec::new(),
        outcome: shared::combat_beat::BeatOutcome::Swing {
            outcome,
            stress: StressReport::default(),
            actor_stamina_cost: tuning.stamina_cost_attacker,
            target_stamina_cost: tuning.stamina_cost_target,
        },
        significant: false,
    }
}

// ---------------------------------------------------------------------------
// AttackContestOutcome
// ---------------------------------------------------------------------------

/// Outcome of a single `attack_contest` invocation.
///
/// Carries the high-level `AttackResult` plus enough wear/penalty data for
/// the caller to assemble a `CombatBeat` without re-snapshotting equipment
/// state.
pub struct AttackContestOutcome {
    pub result: AttackResult,
    /// Pre-computed damage for this result (avoids recomputing in callers).
    pub damage: u32,
    /// Wear records emitted in attack-roll order: weapon (if equipped) then
    /// shield (if equipped). Items that were `Pristine` are omitted.
    pub wear: Vec<WearReport>,
    /// Affliction drafts to apply to the target (loser of the contest).
    pub inflicts: Vec<crate::AfflictionDraft>,
    /// Affliction drafts to apply to the attacker (e.g. BreakMidSwing recoil).
    pub attacker_inflicts: Vec<crate::AfflictionDraft>,
}

// ---------------------------------------------------------------------------
// attack_contest
// ---------------------------------------------------------------------------

/// Generate attack data for each character.
/// Each rolls a d20 to decide a basic attack / defense value.
/// Strength and any weapon are added to the attack roll.
/// Defense and any shield are added to the defense roll.
/// If either roll is more than 1.5x the other, that triggers a "decisive" victory.
/// Natural 1 on attack = critical fumble (attacker takes damage).
/// Natural 20 on attack = critical hit (triple damage).
/// Natural 20 on defense = perfect block (defender counters).
pub fn attack_contest(
    attacker: &mut Character,
    target: &mut Character,
    rng: &mut impl Rng,
    events: &mut Vec<TaggedEvent>,
    tuning: &crate::combat_tuning::CombatTuning,
) -> AttackContestOutcome {
    // Get attack roll and strength modifier
    let base_attack_roll: i32 = rng.random_range(1..=20); // Base roll
    let mut attack_roll = base_attack_roll;
    attack_roll += attacker.effective_attribute(CharacterAttribute::Strength) as i32; // Add strength
    {
        let band = stamina_band(attacker.stamina, attacker.max_stamina, tuning);
        let penalty = match band {
            StaminaBand::Fresh => 0,
            StaminaBand::Winded => tuning.winded_roll_penalty,
            StaminaBand::Exhausted => tuning.exhausted_roll_penalty,
        };
        attack_roll += penalty;
    }

    let mut wear: Vec<WearReport> = Vec::new();

    // Track weapon kind and whether it broke for inflict table lookup.
    let mut weapon_kind = WeaponKind::Unarmed;
    let mut weapon_broken = false;

    // If the attacker has a weapon, use it. The effect bonus always applies;
    // wear is conditional (cg9) and runs in the wear phase below, gated on
    // the pre-wear result. classify_weapon runs here because the emission
    // path only exists when wear actually fires.
    if let Some(weapon) = attacker.equipped_weapon_mut() {
        weapon_kind = classify_weapon(weapon);
        attack_roll += weapon.effect; // Add weapon damage
    }

    // Get defense roll and defense modifier
    let base_defense_roll: i32 = rng.random_range(1..=20); // Base roll
    let mut defense_roll = base_defense_roll;
    defense_roll += target.effective_attribute(CharacterAttribute::Toughness) as i32; // Add defense
    {
        let band = stamina_band(target.stamina, target.max_stamina, tuning);
        let penalty = match band {
            StaminaBand::Fresh => 0,
            StaminaBand::Winded => tuning.winded_roll_penalty,
            StaminaBand::Exhausted => tuning.exhausted_roll_penalty,
        };
        defense_roll += penalty;
    }

    // Trapped characters cannot dodge effectively — halve defense
    if target
        .afflictions
        .values()
        .any(|a| matches!(a.kind, shared::afflictions::AfflictionKind::Trapped(_)))
    {
        defense_roll = (defense_roll as f32 / 2.0).ceil() as i32;
    }

    // Sleep ambush bonus (40t7): sleeping targets can't defend at all.
    // They just woke up (was_ambushed flag set by attacks()) but haven't
    // had time to react — treat as 0 defense.
    if target.was_ambushed {
        defense_roll = 0;
    }

    // TODO(dvd): apply patron_affinity_penalty(attacker, PATRON_PENALTY_ATTACK_TRAPPED)
    //            when attacking a character with any AfflictionKind::Trapped(_)

    // If the defender has a shield, use it — effect only; wear is conditional
    // (cg9) and runs in the wear phase below. shield_effect is remembered so
    // the wear gate can test the shield's decisive margin.
    let mut shield_effect = 0;
    if let Some(shield) = target.equipped_shield_mut() {
        shield_effect = shield.effect;
        defense_roll += shield.effect; // Add shield defense
    }

    // Pre-wear decision: gates the wear tiers. The final result is decided
    // again below on break-modified rolls, so a shattering shield can still
    // flip a block exactly as before — only wear is gated now.
    let provisional_result = decide_result(
        base_attack_roll,
        base_defense_roll,
        attack_roll,
        defense_roll,
        tuning,
    );

    // Wear phase (cg9): every amount can be zero, and zero means the item
    // takes no wear this contest. Clean hits and plain misses wear nothing.
    let weapon_wear_amount = weapon_wear_amount(
        &provisional_result,
        attack_roll,
        defense_roll,
        shield_effect,
        target,
        tuning,
    );
    let shield_wear_amount = if shield_block_was_decisive(
        &provisional_result,
        attack_roll,
        defense_roll,
        shield_effect,
    ) {
        tuning.shield_wear_on_decisive_block
    } else {
        0
    };

    let weapon_outcome = if weapon_wear_amount > 0
        && let Some(weapon) = attacker.equipped_weapon_mut()
    {
        let outcome = weapon.wear(weapon_wear_amount);
        // Defer clone: only Worn/Broken paths need the snapshot.
        match outcome {
            world::items::WearOutcome::Pristine => None,
            _ => Some((weapon.clone(), outcome)),
        }
    } else {
        None
    };
    if let Some((weapon, outcome)) = weapon_outcome {
        let attacker_ref = tref(attacker);
        let item_ref = ItemRef {
            identifier: weapon.identifier.clone().into(),
            name: weapon.name.clone(),
        };
        match outcome {
            world::items::WearOutcome::Pristine => {}
            world::items::WearOutcome::Worn => {
                let content = GameOutput::WeaponWear(attacker.name.as_str(), weapon.name.as_str())
                    .to_string();
                events.push(TaggedEvent::new(
                    content,
                    MessagePayload::ItemUsed {
                        character: attacker_ref.clone(),
                        item: item_ref.clone(),
                    },
                ));
                wear.push(WearReport {
                    owner: attacker_ref,
                    item: item_ref,
                    outcome: WearOutcomeReport::Worn,
                    forfeited_effect: None,
                    mid_action_penalty: None,
                });
            }
            world::items::WearOutcome::Broken => {
                weapon_broken = true;
                let content = GameOutput::WeaponBreak(attacker.name.as_str(), weapon.name.as_str())
                    .to_string();
                events.push(TaggedEvent::new(
                    content,
                    MessagePayload::ItemUsed {
                        character: attacker_ref.clone(),
                        item: item_ref.clone(),
                    },
                ));
                if let Err(err) = attacker.remove_item(&weapon) {
                    eprintln!("Failed to remove weapon: {}", err);
                }
                // D1 + D2: forfeit the just-applied effect bonus and apply 1d4 penalty.
                attack_roll -= weapon.effect;
                let penalty = rng.random_range(1..=4);
                attack_roll -= penalty;
                let narration = GameOutput::WeaponShattersMidSwing(
                    attacker.name.as_str(),
                    weapon.name.as_str(),
                    penalty as u32,
                )
                .to_string();
                events.push(TaggedEvent::new(
                    narration,
                    MessagePayload::ItemUsed {
                        character: attacker_ref.clone(),
                        item: item_ref.clone(),
                    },
                ));
                wear.push(WearReport {
                    owner: attacker_ref,
                    item: item_ref,
                    outcome: WearOutcomeReport::Broken,
                    forfeited_effect: Some(weapon.effect),
                    mid_action_penalty: Some(penalty),
                });
            }
        }
    }

    let shield_outcome = if shield_wear_amount > 0
        && let Some(shield) = target.equipped_shield_mut()
    {
        let outcome = shield.wear(shield_wear_amount);
        match outcome {
            world::items::WearOutcome::Pristine => None,
            _ => Some((shield.clone(), outcome)),
        }
    } else {
        None
    };
    if let Some((shield, outcome)) = shield_outcome {
        let target_ref = tref(target);
        let item_ref = ItemRef {
            identifier: shield.identifier.clone().into(),
            name: shield.name.clone(),
        };
        match outcome {
            world::items::WearOutcome::Pristine => {}
            world::items::WearOutcome::Worn => {
                let content =
                    GameOutput::ShieldWear(target.name.as_str(), shield.name.as_str()).to_string();
                events.push(TaggedEvent::new(
                    content,
                    MessagePayload::ItemUsed {
                        character: target_ref.clone(),
                        item: item_ref.clone(),
                    },
                ));
                wear.push(WearReport {
                    owner: target_ref,
                    item: item_ref,
                    outcome: WearOutcomeReport::Worn,
                    forfeited_effect: None,
                    mid_action_penalty: None,
                });
            }
            world::items::WearOutcome::Broken => {
                let content =
                    GameOutput::ShieldBreak(target.name.as_str(), shield.name.as_str()).to_string();
                events.push(TaggedEvent::new(
                    content,
                    MessagePayload::ItemUsed {
                        character: target_ref.clone(),
                        item: item_ref.clone(),
                    },
                ));
                if let Err(err) = target.remove_item(&shield) {
                    eprintln!("Failed to remove shield: {}", err);
                }
                // D3 mirror: forfeit shield effect + apply 1d4 defense penalty.
                defense_roll -= shield.effect;
                let penalty = rng.random_range(1..=4);
                defense_roll -= penalty;
                let narration = GameOutput::ShieldShattersMidBlock(
                    target.name.as_str(),
                    shield.name.as_str(),
                    penalty as u32,
                )
                .to_string();
                events.push(TaggedEvent::new(
                    narration,
                    MessagePayload::ItemUsed {
                        character: target_ref.clone(),
                        item: item_ref.clone(),
                    },
                ));
                wear.push(WearReport {
                    owner: target_ref,
                    item: item_ref,
                    outcome: WearOutcomeReport::Broken,
                    forfeited_effect: Some(shield.effect),
                    mid_action_penalty: Some(penalty),
                });
            }
        }
    }

    // Check for critical outcomes based on natural rolls (before modifiers)
    let result = decide_result(
        base_attack_roll,
        base_defense_roll,
        attack_roll,
        defense_roll,
        tuning,
    );

    // D5: CriticalFumble clears attacker-side break-penalty fields on the
    // recorded wear report. The fumble path's catastrophic self-damage is the
    // story; piling the snapped-weapon penalty on top is double jeopardy.
    if matches!(result, AttackResult::CriticalFumble)
        && let Some(report) = wear.iter_mut().find(|w| {
            w.outcome == WearOutcomeReport::Broken
                && w.owner.identifier == tref(attacker).identifier
        })
    {
        report.forfeited_effect = None;
        report.mid_action_penalty = None;
    }

    // Phase 3: affliction inflict table lookup.
    let hit_severity = result_to_hit_severity(&result);
    let attacker_id = attacker.identifier.as_str();
    let (inflicts, attacker_inflicts) =
        if matches!(result, AttackResult::Miss | AttackResult::CriticalFumble) {
            (Vec::new(), Vec::new())
        } else {
            let target_inflicts = lookup_inflicts(weapon_kind, hit_severity, attacker_id, rng);
            // BreakMidSwing follow-through: when weapon shatters, attacker suffers
            // a recoil injury in addition to any target inflicts.
            let attacker_inflicts = if weapon_broken {
                lookup_break_mid_swing_inflict(weapon_kind, attacker_id, rng)
                    .into_iter()
                    .collect()
            } else {
                Vec::new()
            };
            (target_inflicts, attacker_inflicts)
        };

    // TODO(dvd): emit PatronEvent::AttackOnTrapped when attacker wins against
    //            a trapped target, so the patronship system can apply affinity
    //            penalties and generate audience-disapproval narration.

    // Reset transient ambush flag for next combat
    target.was_ambushed = false;

    AttackContestOutcome {
        result,
        damage: 0, // placeholder — resolved by resolve() after contest
        wear,
        inflicts,
        attacker_inflicts,
    }
}

/// Natural-roll priority plus decisive-margin comparison — extracted so the
/// wear phase can see the outcome BEFORE wear/break mutations, while
/// `attack_contest` decides again on the final break-modified rolls.
fn decide_result(
    base_attack_roll: i32,
    base_defense_roll: i32,
    attack_roll: i32,
    defense_roll: i32,
    tuning: &crate::combat_tuning::CombatTuning,
) -> AttackResult {
    match (base_attack_roll, base_defense_roll) {
        (1, _) => AttackResult::CriticalFumble, // Natural 1 on attack - fumble!
        (20, _) => AttackResult::CriticalHit,   // Natural 20 on attack - crit!
        (_, 20) => AttackResult::PerfectBlock,  // Natural 20 on defense - perfect block!
        _ => {
            // Normal combat resolution - compare attack vs. defense
            match attack_roll.cmp(&defense_roll) {
                Ordering::Less => {
                    // If the defender wins
                    let difference =
                        defense_roll as f64 - (attack_roll as f64 * tuning.decisive_win_multiplier);
                    if difference > 0.0 {
                        // Defender wins significantly
                        AttackResult::DefenderWinsDecisively
                    } else {
                        AttackResult::DefenderWins
                    }
                }
                Ordering::Equal => AttackResult::Miss, // If they tie
                Ordering::Greater => {
                    // If the attacker wins
                    let difference =
                        attack_roll as f64 - (defense_roll as f64 * tuning.decisive_win_multiplier);

                    if difference > 0.0 {
                        // Attacker wins significantly
                        AttackResult::AttackerWinsDecisively
                    } else {
                        AttackResult::AttackerWins
                    }
                }
            }
        }
    }
}

/// The block held because of the shield: the attack landed inside the
/// shield's contribution — `defense_roll - shield_effect < attack_roll`,
/// with DefenderWins already implying `attack_roll < defense_roll`.
fn shield_block_was_decisive(
    result: &AttackResult,
    attack_roll: i32,
    defense_roll: i32,
    shield_effect: i32,
) -> bool {
    matches!(
        result,
        AttackResult::DefenderWins | AttackResult::DefenderWinsDecisively
    ) && shield_effect > 0
        && attack_roll > defense_roll - shield_effect
}

/// Weapon wear tier for this contest (cg9). Zero means no wear: a clean hit
/// on bare flesh never dulls steel, and neither does a plain miss. Armor in
/// the target's inventory means the blow was absorbed by plate (middle tier);
/// DefenderWins outcomes consult the shield margin instead.
fn weapon_wear_amount(
    result: &AttackResult,
    attack_roll: i32,
    defense_roll: i32,
    shield_effect: i32,
    target: &Character,
    tuning: &crate::combat_tuning::CombatTuning,
) -> u32 {
    use AttackResult::*;
    match result {
        PerfectBlock => tuning.weapon_wear_on_perfect_block,
        CriticalFumble => tuning.weapon_wear_on_fumble,
        AttackerWins | AttackerWinsDecisively => {
            if target
                .items
                .iter()
                .any(|i| i.is_armor() && i.current_durability > 0)
            {
                tuning.weapon_wear_on_armor_block
            } else {
                0
            }
        }
        DefenderWins | DefenderWinsDecisively => {
            if shield_block_was_decisive(result, attack_roll, defense_roll, shield_effect) {
                tuning.weapon_wear_on_shield_block
            } else {
                0
            }
        }
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// resolve — pure combat resolution
// ---------------------------------------------------------------------------

/// Cap on how much of an incoming hit effective toughness can absorb:
/// toughness N mitigates N% of the damage, at most 50%.
pub(crate) const TOUGHNESS_MITIGATION_CAP: i32 = 50;

/// A hit that lands always costs at least this much blood, so mitigation
/// can never turn a damaging swing into a scratch that cannot kill.
pub(crate) const MIN_MITIGATED_DAMAGE: u32 = 1;

/// Toughness damage mitigation: the character taking the hit absorbs part
/// of it through effective toughness (base minus wound penalties). Wounded
/// characters mitigate less, so the same swing draws strictly more blood.
/// Zero damage (miss/fumble, or an attacker whose effective strength is 0)
/// passes through unchanged, matching the pre-mitigation zero-damage edge.
///
/// Exposed so the formula can be tested without a full combat roll.
pub(crate) fn mitigate_damage(raw_damage: u32, effective_toughness: i32) -> u32 {
    if raw_damage == 0 {
        return 0;
    }
    let pct = effective_toughness.clamp(0, TOUGHNESS_MITIGATION_CAP) as u32;
    (raw_damage * (100 - pct) / 100).max(MIN_MITIGATED_DAMAGE)
}

/// Pure combat resolution. Calls `attack_contest()` and builds the
/// `CombatBeat` from the result. Does NOT apply damage, stress, events,
/// or afflictions — the caller handles those side effects.
///
/// Returns the contest outcome (result, wear, inflicts) and the fully
/// constructed beat (including swing outcome and stress report).
pub fn resolve(
    attacker: &mut Character,
    defender: &mut Character,
    rng: &mut impl Rng,
    sub_events: &mut Vec<TaggedEvent>,
    tuning: &crate::combat_tuning::CombatTuning,
) -> (AttackContestOutcome, CombatBeat) {
    // Snapshot equipment before attack_contest mutates durability / removes items.
    let weapon = attacker
        .items
        .iter()
        .rfind(|i| i.is_weapon() && i.current_durability > 0)
        .map(iref);
    let shield = defender
        .items
        .iter()
        .rfind(|i| i.is_shield() && i.current_durability > 0)
        .map(iref);

    // Run the attack contest (dice rolling, equipment wear, inflict lookup).
    let mut contest = attack_contest(attacker, defender, rng, sub_events, tuning);
    let result = contest.result;

    // Compute damage based on result (strength scale 0-50, result multipliers up to 3x).
    // Scale to blood system: multiply by 10 so damage is in the 0-1000 blood range.
    let attack_strength = attacker.effective_attribute(CharacterAttribute::Strength);
    let defend_strength = defender.effective_attribute(CharacterAttribute::Strength);
    let raw_damage: u32 = match result {
        AttackResult::CriticalHit => attack_strength * 3,
        AttackResult::AttackerWins => attack_strength,
        AttackResult::AttackerWinsDecisively => attack_strength * 2,
        AttackResult::PerfectBlock => defend_strength * 2,
        AttackResult::DefenderWins => defend_strength,
        AttackResult::DefenderWinsDecisively => defend_strength * 2,
        AttackResult::CriticalFumble | AttackResult::Miss => 0,
    } * 10;

    // Toughness mitigation: the character taking the hit absorbs part of it
    // through effective toughness (wound penalties included). The dodge roll
    // above already rewards high toughness; the double-dip is accepted.
    let victim_toughness = match result {
        AttackResult::CriticalHit
        | AttackResult::AttackerWins
        | AttackResult::AttackerWinsDecisively => defender.effective_toughness(),
        AttackResult::PerfectBlock
        | AttackResult::DefenderWins
        | AttackResult::DefenderWinsDecisively => attacker.effective_toughness(),
        AttackResult::CriticalFumble | AttackResult::Miss => 0,
    };
    // Armor soak + wear (wzk): armor feeds toughness-style mitigation at
    // the damage step only — it never touches the success roll. A critical
    // finds the gap: no soak, no wear. Pristine armor soaks its full effect,
    // worn armor half; a break this hit forfeits the soak, like shield break
    // forfeits its effect.
    let is_counter = matches!(
        result,
        AttackResult::PerfectBlock
            | AttackResult::DefenderWins
            | AttackResult::DefenderWinsDecisively
    );
    let (armor_soak, armor_wear) = if raw_damage > 0 && !matches!(result, AttackResult::CriticalHit)
    {
        let taker = if is_counter {
            &mut *attacker
        } else {
            &mut *defender
        };
        if let Some(armor) = taker.equipped_armor_mut() {
            let effect = armor.effect;
            let pristine = armor.current_durability * 2 > armor.max_durability;
            let outcome = armor.wear(1);
            let soak = match outcome {
                world::items::WearOutcome::Broken => 0,
                _ if pristine => effect,
                _ => effect / 2,
            };
            match outcome {
                world::items::WearOutcome::Pristine => (soak, None),
                _ => (soak, Some((armor.clone(), outcome))),
            }
        } else {
            (0, None)
        }
    } else {
        (0, None)
    };
    let damage = mitigate_damage(raw_damage, victim_toughness + armor_soak);

    if let Some((armor, outcome)) = armor_wear {
        let taker = if is_counter {
            &mut *attacker
        } else {
            &mut *defender
        };
        let owner = tref(taker);
        let item_ref = ItemRef {
            identifier: armor.identifier.clone().into(),
            name: armor.name.clone(),
        };
        let item = MessagePayload::ItemUsed {
            character: owner.clone(),
            item: item_ref.clone(),
        };
        match outcome {
            world::items::WearOutcome::Worn => {
                sub_events.push(TaggedEvent::new(
                    GameOutput::ArmorWear(taker.name.as_str(), armor.name.as_str()).to_string(),
                    item,
                ));
                contest.wear.push(WearReport {
                    owner,
                    item: item_ref,
                    outcome: WearOutcomeReport::Worn,
                    forfeited_effect: None,
                    mid_action_penalty: None,
                });
            }
            world::items::WearOutcome::Broken => {
                sub_events.push(TaggedEvent::new(
                    GameOutput::ArmorBreak(taker.name.as_str(), armor.name.as_str()).to_string(),
                    item,
                ));
                if let Err(err) = taker.remove_item(&armor) {
                    eprintln!("Failed to remove armor: {}", err);
                }
                contest.wear.push(WearReport {
                    owner,
                    item: item_ref,
                    outcome: WearOutcomeReport::Broken,
                    forfeited_effect: Some(armor.effect),
                    mid_action_penalty: None,
                });
            }
            world::items::WearOutcome::Pristine => {}
        }
    }

    // Determine SwingOutcome from result and pre-damage health.
    let swing_outcome = match result {
        AttackResult::Miss => SwingOutcome::Miss,
        AttackResult::CriticalFumble => {
            if attacker.blood <= 50 {
                SwingOutcome::FumbleDeath { self_damage: 5 }
            } else {
                SwingOutcome::FumbleSurvive { self_damage: 5 }
            }
        }
        _ => {
            let victim_health = if is_counter {
                attacker.blood
            } else {
                defender.blood
            };

            if victim_health <= damage {
                if is_counter {
                    SwingOutcome::AttackerDied { damage }
                } else {
                    SwingOutcome::Kill { damage }
                }
            } else {
                match result {
                    AttackResult::CriticalHit => SwingOutcome::CriticalHitWound { damage },
                    AttackResult::PerfectBlock => SwingOutcome::BlockWound { damage },
                    _ => SwingOutcome::Wound { damage },
                }
            }
        }
    };

    // Compute stress damage (pure calculation matching apply_violence_stress).
    let stress_damage = if matches!(result, AttackResult::Miss | AttackResult::CriticalFumble) {
        0
    } else {
        let (winner_kills, winner_wins, winner_sanity) = match result {
            AttackResult::CriticalHit
            | AttackResult::AttackerWins
            | AttackResult::AttackerWinsDecisively => (
                attacker.statistics.kills,
                attacker.statistics.wins + 1,
                attacker.effective_sanity(),
            ),
            _ => (
                defender.statistics.kills,
                defender.statistics.wins + 1,
                defender.effective_sanity(),
            ),
        };
        calculate_violence_stress(winner_kills, winner_wins, winner_sanity, tuning)
    };
    let stressed = if stress_damage > 0 {
        Some(match result {
            AttackResult::CriticalHit
            | AttackResult::AttackerWins
            | AttackResult::AttackerWinsDecisively => tref(attacker),
            _ => tref(defender),
        })
    } else {
        None
    };

    let beat = CombatBeat {
        round: 1,
        order: 0,
        actor: tref(attacker),
        action: shared::combat_beat::CombatActionKind::Attack,
        target: Some(tref(defender)),
        weapon,
        shield,
        wear: contest.wear.clone(),
        outcome: shared::combat_beat::BeatOutcome::Swing {
            outcome: swing_outcome,
            stress: StressReport {
                stress_damage,
                stressed,
            },
            actor_stamina_cost: tuning.stamina_cost_attacker,
            target_stamina_cost: tuning.stamina_cost_target,
        },
        significant: false,
    };

    contest.damage = damage;

    (contest, beat)
}

/// Map an AttackResult to the inflict table's severity band.
fn result_to_hit_severity(result: &AttackResult) -> HitSeverity {
    match result {
        AttackResult::CriticalHit => HitSeverity::Critical,
        AttackResult::AttackerWinsDecisively
        | AttackResult::DefenderWinsDecisively
        | AttackResult::PerfectBlock => HitSeverity::Heavy,
        AttackResult::AttackerWins | AttackResult::DefenderWins => HitSeverity::Normal,
        AttackResult::CriticalFumble | AttackResult::Miss => HitSeverity::Normal,
    }
}

/// Classify an Item into a WeaponKind for the inflict table.
fn classify_weapon(item: &Item) -> WeaponKind {
    let name_lower = item.name.to_lowercase();
    if name_lower.contains("bow") || name_lower.contains("arrow") || name_lower.contains("spear") {
        WeaponKind::Ranged
    } else if name_lower.contains("knife")
        || name_lower.contains("sword")
        || name_lower.contains("blade")
        || name_lower.contains("dagger")
        || name_lower.contains("axe")
    {
        WeaponKind::Bladed
    } else if name_lower.contains("club")
        || name_lower.contains("hammer")
        || name_lower.contains("mace")
        || name_lower.contains("bat")
    {
        WeaponKind::Blunt
    } else {
        WeaponKind::Bladed // Default for unknown weapons
    }
}

// ---------------------------------------------------------------------------
// apply_combat_results
// ---------------------------------------------------------------------------

/// Maps raw damage to a wound severity.
fn damage_to_severity(damage: u32) -> WoundSeverity {
    if damage <= 10 {
        WoundSeverity::Minor
    } else if damage <= 25 {
        WoundSeverity::Moderate
    } else if damage <= 50 {
        WoundSeverity::Severe
    } else {
        WoundSeverity::Critical
    }
}

/// Picks a random body part for a combat wound.
/// Determines body part based on attack context.
/// Weapons target torso/limbs; unarmed favors head/torso.
/// Severe attacks are more likely to hit vital areas.
fn determine_body_part(
    attacker: &Character,
    severity: WoundSeverity,
    rng: &mut impl Rng,
) -> BodyPart {
    let has_weapon = attacker
        .items
        .iter()
        .any(|i| i.is_weapon() && i.current_durability > 0);

    // Severe+ attacks have elevated head-hit chance
    let head_chance = if matches!(severity, WoundSeverity::Severe | WoundSeverity::Critical) {
        0.20
    } else if has_weapon {
        0.05
    } else {
        0.15
    };

    let roll: f64 = rng.random();
    if roll < head_chance {
        return BodyPart::Head;
    }

    if has_weapon {
        // Weapons: mostly torso and limbs
        match rng.random_range(0..4) {
            0 => BodyPart::Torso,
            1 => BodyPart::LeftArm,
            2 => BodyPart::RightArm,
            _ => {
                if rng.random_bool(0.5) {
                    BodyPart::LeftLeg
                } else {
                    BodyPart::RightLeg
                }
            }
        }
    } else {
        // Unarmed: head/torso biased
        match rng.random_range(0..3) {
            0 => BodyPart::Torso,
            1 => BodyPart::LeftArm,
            _ => BodyPart::RightArm,
        }
    }
}

/// Checks if the defender's shield can prevent or mitigate a wound.
/// Returns the adjusted severity. None = wound prevented entirely.
fn shield_injury_prevention(
    loser: &mut Character,
    severity: WoundSeverity,
    rng: &mut impl Rng,
) -> Option<WoundSeverity> {
    let shield_idx = match loser
        .items
        .iter()
        .position(|i| i.is_shield() && i.current_durability > 0)
    {
        Some(idx) => idx,
        None => return Some(severity), // No shield -> wound passes through
    };

    // Shield blocks: 40% chance to prevent wound entirely
    if rng.random_bool(0.40) {
        let shield = &mut loser.items[shield_idx];
        let _outcome = shield.wear(1);
        return None; // Wound prevented
    }

    // Shield mitigates: 30% chance to downgrade severity
    if rng.random_bool(0.30) {
        let shield = &mut loser.items[shield_idx];
        let _outcome = shield.wear(1);
        let mitigated = match severity {
            WoundSeverity::Critical => WoundSeverity::Severe,
            WoundSeverity::Severe => WoundSeverity::Moderate,
            WoundSeverity::Moderate => WoundSeverity::Minor,
            WoundSeverity::Minor => return None, // Minor -> prevented
        };
        return Some(mitigated);
    }

    Some(severity)
}

/// Picks a wound type based on whether the attacker had a weapon.
fn wound_type_for_attack(attacker: &Character, rng: &mut impl Rng) -> WoundType {
    let has_weapon = attacker
        .items
        .iter()
        .any(|i| i.is_weapon() && i.current_durability > 0);
    if has_weapon {
        match rng.random_range(0..4) {
            0 => WoundType::Cut,
            1 => WoundType::Stab,
            2 => WoundType::Pierce,
            _ => WoundType::Tear,
        }
    } else {
        match rng.random_range(0..3) {
            0 => WoundType::Crush,
            1 => WoundType::Tear,
            _ => WoundType::Cut,
        }
    }
}

/// Apply the results of a combat encounter.
/// Creates a wound on the loser based on damage dealt, and updates statistics.
///
/// Returns the stress damage applied to the winner via
/// `apply_violence_stress` so the caller can populate
/// `CombatBeat.stress.stress_damage` for the swing payload.
pub(crate) fn apply_combat_results(
    winner: &mut Character,
    loser: &mut Character,
    damage_to_loser: u32,
    log_event: GameOutput,
    events: &mut Vec<TaggedEvent>,
    tuning: &crate::combat_tuning::CombatTuning,
    rng: &mut impl Rng,
) -> u32 {
    let severity = damage_to_severity(damage_to_loser);
    let body_part = determine_body_part(winner, severity, rng);
    let wound_type = wound_type_for_attack(winner, rng);

    // Shield may prevent or mitigate the wound
    let final_severity = match shield_injury_prevention(loser, severity, rng) {
        None => {
            // Shield prevented the wound entirely — still reduce blood for
            // backward compat, but no wound is created.
            loser.blood = loser.blood.saturating_sub(damage_to_loser);
            loser.statistics.defeats += 1;
            winner.statistics.wins += 1;
            let stress_damage = winner.apply_violence_stress(events, tuning);
            let content = log_event.to_string();
            events.push(TaggedEvent::new(
                content,
                MessagePayload::CharacterWounded {
                    victim: tref(loser),
                    attacker: Some(tref(winner)),
                    hp_lost: damage_to_loser,
                },
            ));
            return stress_damage;
        }
        Some(s) => s,
    };

    loser.create_wound(wound_type, final_severity, body_part);
    loser.drain_blood_from_wounds();
    loser.blood = loser.blood.saturating_sub(damage_to_loser);

    loser.statistics.defeats += 1;
    winner.statistics.wins += 1;
    let stress_damage = winner.apply_violence_stress(events, tuning);
    let content = log_event.to_string();
    events.push(TaggedEvent::new(
        content,
        MessagePayload::CharacterWounded {
            victim: tref(loser),
            attacker: Some(tref(winner)),
            hp_lost: damage_to_loser,
        },
    ));
    stress_damage
}

// ---------------------------------------------------------------------------
// Violence stress
// ---------------------------------------------------------------------------

/// Calculate stress from violent encounters
fn calculate_violence_stress(
    kills: u32,
    wins: u32,
    current_sanity: u32,
    tuning: &crate::combat_tuning::CombatTuning,
) -> u32 {
    let non_kill_wins = wins.saturating_sub(kills);

    let calculated_stress_f64 = if wins > 0 {
        // Calculate the stress potential based on kills and non-kill wins
        let raw_stress_potential = (kills as f64 * tuning.kill_stress_contribution)
            + (non_kill_wins as f64 * tuning.non_kill_win_stress_contribution);

        // Desensitize: the more total wins (violent encounters), the more this raw potential
        // is "spread out" or reduced. This gives an average stressfulness per encounter.
        let desensitized_stress_per_encounter = raw_stress_potential / wins as f64;

        // Scale by the character's current sanity percentage and apply a final divisor.
        // Lower sanity means less new stress from these types of events.
        desensitized_stress_per_encounter
            * (current_sanity as f64 / tuning.stress_sanity_normalization)
            / tuning.stress_final_divisor
    } else {
        // No wins (and therefore no kills), apply a base stress.
        tuning.base_stress_no_engagements
    };

    let rounded_stress = calculated_stress_f64.round();

    // Only apply stress if it's at least 1
    if rounded_stress >= 1.0 {
        rounded_stress as u32
    } else {
        0
    }
}

impl Character {
    /// Apply violence stress to character based on their combat history.
    ///
    /// Returns the amount of mental damage applied (0 if below threshold).
    /// Callers thread this value into `CombatBeat.stress.stress_damage` so
    /// the typed swing payload can reproduce the trailing horrified line
    /// rendered in `CombatEngagement.detail_lines`.
    pub(crate) fn apply_violence_stress(
        &mut self,
        events: &mut Vec<TaggedEvent>,
        tuning: &crate::combat_tuning::CombatTuning,
    ) -> u32 {
        let stress_damage = calculate_violence_stress(
            self.statistics.kills,
            self.statistics.wins,
            self.effective_sanity(),
            tuning,
        );

        if stress_damage > 0 {
            let severity = if stress_damage >= 10 {
                ConditionSeverity::Critical
            } else if stress_damage >= 5 {
                ConditionSeverity::Severe
            } else if stress_damage >= 2 {
                ConditionSeverity::Moderate
            } else {
                ConditionSeverity::Mild
            };

            let content =
                GameOutput::CharacterHorrified(self.name.as_str(), stress_damage).to_string();
            events.push(TaggedEvent::new(
                content,
                MessagePayload::SanityBreak {
                    character: tref(self),
                },
            ));
            // Apply immediate sanity reduction via mental condition
            self.mental_conditions.push(MentalCondition::Horrified {
                severity,
                remaining_periods: 3,
            });
        }
        stress_damage
    }

    /// Handle self-attack (suicide/self-harm) path.
    ///
    /// Deducts `strength` as physical damage, applies violence stress, and
    /// emits the appropriate events. Returns the outcome immediately.
    pub(crate) fn handle_self_attack(
        &mut self,
        target: &mut Character,
        events: &mut Vec<TaggedEvent>,
        tuning: &crate::combat_tuning::CombatTuning,
    ) -> AttackOutcome {
        let damage = self.effective_attribute(CharacterAttribute::Strength) * 10;
        self.blood = self.blood.saturating_sub(damage);
        let mut stress_events: Vec<TaggedEvent> = Vec::new();
        let stress_damage = self.apply_violence_stress(&mut stress_events, tuning);
        events.extend(stress_events);

        if self.blood > 0 {
            let content = GameOutput::CharacterSelfHarm(self.name.as_str()).to_string();
            events.push(TaggedEvent::new(
                content,
                MessagePayload::CharacterWounded {
                    victim: tref(self),
                    attacker: None,
                    hp_lost: damage,
                },
            ));
            let mut beat = new_beat(
                self,
                target,
                SwingOutcome::SelfAttackWound { damage },
                tuning,
            );
            if let shared::combat_beat::BeatOutcome::Swing {
                ref mut target_stamina_cost,
                ref mut stress,
                ..
            } = beat.outcome
            {
                *target_stamina_cost = 0;
                stress.stress_damage = stress_damage;
                if stress_damage > 0 {
                    stress.stressed = Some(tref(self));
                }
            }
            events.push(TaggedEvent::new(
                String::new(),
                MessagePayload::CombatBeat(beat),
            ));
            AttackOutcome::Wound(self.clone(), target.clone())
        } else {
            let content = GameOutput::CharacterSuicide(self.name.as_str()).to_string();
            events.push(TaggedEvent::new(
                content,
                MessagePayload::CharacterKilled {
                    victim: tref(self),
                    killer: None,
                    cause: shared::afflictions::DeathCause::Suicide,
                },
            ));
            let mut beat = new_beat(self, target, SwingOutcome::Suicide { damage }, tuning);
            if let shared::combat_beat::BeatOutcome::Swing {
                ref mut target_stamina_cost,
                ref mut stress,
                ..
            } = beat.outcome
            {
                *target_stamina_cost = 0;
                stress.stress_damage = stress_damage;
                if stress_damage > 0 {
                    stress.stressed = Some(tref(self));
                }
            }
            events.push(TaggedEvent::new(
                String::new(),
                MessagePayload::CombatBeat(beat),
            ));
            AttackOutcome::Kill(self.clone(), target.clone())
        }
    }
}
