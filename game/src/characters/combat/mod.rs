//! Combat-related functionality for characters.
//!
//! This module handles all attack and combat mechanics including:
//! - Attack contests between characters
//! - Combat result application
//! - Violence stress calculations
//! - Statistics updates

pub mod inflict_table;
pub mod resolve;

#[cfg(test)]
mod tests;

// Re-exports for public API.
pub use resolve::{AttackContestOutcome, attack_contest, resolve};

// Re-export for tests (pub(crate) items from resolve).
pub(crate) use resolve::apply_combat_results;

use crate::characters::Character;
use crate::characters::actions::{AttackOutcome, AttackResult};
use crate::messages::{CharacterRef, CombatEngagement, CombatOutcome, MessagePayload, TaggedEvent};
use rand::prelude::*;
use resolve::tref;
use world::output::GameOutput;

// ---------------------------------------------------------------------------
// Character::attacks — the combat orchestrator
// ---------------------------------------------------------------------------

impl Character {
    /// Character attacks another character
    /// Potentially fatal to either character
    ///
    /// Emits exactly one `MessagePayload::Combat` `TaggedEvent` per call when
    /// a real two-character engagement occurs. Self-harm / suicide / critical
    /// fumble paths are not engagements; they emit a single standalone
    /// `CharacterKilled` or `CharacterWounded` `TaggedEvent` instead.
    pub(crate) fn attacks(
        &mut self,
        target: &mut Character,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
        phase: shared::messages::Phase,
        tuning: &crate::characters::combat_tuning::CombatTuning,
    ) -> AttackOutcome {
        // Check self-attack BEFORE deducting stamina (stamina mutation would
        // otherwise break the derived PartialEq equality check below).
        let is_self_attack = self == target;

        // Sleep ambush (PR2c.2). If the target is asleep we wake
        // them with `InterruptionKind::Ambush` BEFORE damage resolution so
        // the wake-event precedes any CharacterWounded / CharacterKilled
        // emission. The ambush still lands — sleeping targets still take
        // the swing — but at least the timeline reflects the rude awakening.
        if !is_self_attack && target.sleeping {
            target.was_ambushed = true; // Signal to attack_contest: 0 defense
            let attacker_ref = CharacterRef {
                identifier: self.identifier.clone().into(),
                name: self.name.clone(),
            };
            target.wake_interrupted(
                shared::messages::InterruptionKind::Ambush {
                    attacker: attacker_ref,
                },
                phase,
                events,
            );
        }

        // Per-swing stamina cost: deduct from both combatants up-front.
        // Saturating semantics ensure neither character goes negative.
        // Action-gating (refusing to swing while exhausted) lands in Task 10.
        self.stamina = self.stamina.saturating_sub(tuning.stamina_cost_attacker);
        if !is_self_attack {
            target.stamina = target.stamina.saturating_sub(tuning.stamina_cost_target);
        }

        // Is the character attempting suicide?
        if is_self_attack {
            return self.handle_self_attack(target, events, tuning);
        }

        let character_name = self.name.clone();
        let target_name = target.name.clone();

        let mut detail_lines: Vec<String> = Vec::new();
        let mut sub_events: Vec<TaggedEvent> = Vec::new();

        let (contest, beat) = resolve(self, target, rng, &mut sub_events, tuning);
        let result = contest.result;
        let damage = contest.damage;
        let target_inflicts = contest.inflicts;
        let attacker_inflicts = contest.attacker_inflicts;

        for ev in sub_events.drain(..) {
            detail_lines.push(ev.content);
        }

        match result {
            AttackResult::CriticalHit => {
                detail_lines.push(
                    GameOutput::CharacterCriticalHit(character_name.as_str(), target_name.as_str())
                        .to_string(),
                );
                let _ = apply_combat_results(
                    self,
                    target,
                    damage,
                    GameOutput::CharacterAttackWin(character_name.as_str(), target_name.as_str()),
                    &mut sub_events,
                    tuning,
                    rng,
                );
                for ev in sub_events.drain(..) {
                    detail_lines.push(ev.content);
                }
            }
            AttackResult::CriticalFumble => {
                let fumble_content =
                    GameOutput::CharacterCriticalFumble(character_name.as_str()).to_string();
                self.blood = self.blood.saturating_sub(50);
                self.statistics.defeats += 1;

                if self.blood == 0 {
                    self.statistics.killed_by = Some("themselves (fumble)".to_string());
                    self.status = crate::characters::statuses::CharacterStatus::RecentlyDead;
                    self.recently_killed_by = Some(self.id);
                    let died_content =
                        GameOutput::CharacterAttackDied(character_name.as_str(), "themselves")
                            .to_string();
                    let combined = format!("{fumble_content} {died_content}");
                    events.push(TaggedEvent::new(
                        combined,
                        MessagePayload::CharacterKilled {
                            victim: tref(self),
                            killer: None,
                            cause: shared::afflictions::DeathCause::CriticalFumble,
                        },
                    ));
                    events.push(TaggedEvent::new(
                        String::new(),
                        MessagePayload::CombatSwing(beat),
                    ));
                    return AttackOutcome::Kill(target.clone(), self.clone());
                }

                events.push(TaggedEvent::new(
                    fumble_content,
                    MessagePayload::CharacterWounded {
                        victim: tref(self),
                        attacker: None,
                        hp_lost: 5,
                    },
                ));
                events.push(TaggedEvent::new(
                    String::new(),
                    MessagePayload::CombatSwing(beat),
                ));
                return AttackOutcome::Wound(self.clone(), target.clone());
            }
            AttackResult::PerfectBlock => {
                detail_lines.push(
                    GameOutput::CharacterPerfectBlock(
                        target_name.as_str(),
                        character_name.as_str(),
                    )
                    .to_string(),
                );
                let _ = apply_combat_results(
                    target,
                    self,
                    damage,
                    GameOutput::CharacterAttackLose(character_name.as_str(), target_name.as_str()),
                    &mut sub_events,
                    tuning,
                    rng,
                );
                for ev in sub_events.drain(..) {
                    detail_lines.push(ev.content);
                }
            }
            AttackResult::AttackerWins => {
                let _ = apply_combat_results(
                    self,
                    target,
                    damage,
                    GameOutput::CharacterAttackWin(character_name.as_str(), target_name.as_str()),
                    &mut sub_events,
                    tuning,
                    rng,
                );
                for ev in sub_events.drain(..) {
                    detail_lines.push(ev.content);
                }
            }
            AttackResult::AttackerWinsDecisively => {
                let _ = apply_combat_results(
                    self,
                    target,
                    damage,
                    GameOutput::CharacterAttackWinExtra(
                        character_name.as_str(),
                        target_name.as_str(),
                    ),
                    &mut sub_events,
                    tuning,
                    rng,
                );
                for ev in sub_events.drain(..) {
                    detail_lines.push(ev.content);
                }
            }
            AttackResult::DefenderWins => {
                let _ = apply_combat_results(
                    target,
                    self,
                    damage,
                    GameOutput::CharacterAttackLose(character_name.as_str(), target_name.as_str()),
                    &mut sub_events,
                    tuning,
                    rng,
                );
                for ev in sub_events.drain(..) {
                    detail_lines.push(ev.content);
                }
            }
            AttackResult::DefenderWinsDecisively => {
                let _ = apply_combat_results(
                    target,
                    self,
                    damage,
                    GameOutput::CharacterAttackLoseExtra(
                        character_name.as_str(),
                        target_name.as_str(),
                    ),
                    &mut sub_events,
                    tuning,
                    rng,
                );
                for ev in sub_events.drain(..) {
                    detail_lines.push(ev.content);
                }
            }
            AttackResult::Miss => {
                self.statistics.draws += 1;
                target.statistics.draws += 1;

                detail_lines.push(
                    GameOutput::CharacterAttackMiss(character_name.as_str(), target_name.as_str())
                        .to_string(),
                );

                let outcome = CombatOutcome::Stalemate;
                let summary = format!("{} attacks {} ({:?})", self.name, target.name, outcome);
                events.push(TaggedEvent::new(
                    summary,
                    MessagePayload::Combat(CombatEngagement {
                        attacker: tref(self),
                        target: tref(target),
                        outcome,
                        detail_lines,
                    }),
                ));
                events.push(TaggedEvent::new(
                    String::new(),
                    MessagePayload::CombatSwing(beat),
                ));

                return AttackOutcome::Miss(self.clone(), target.clone());
            }
        };

        for draft in &target_inflicts {
            let resolution = target.try_acquire_affliction(draft.clone());
            if matches!(
                resolution,
                crate::characters::afflictions::AcquireResolution::Insert
                    | crate::characters::afflictions::AcquireResolution::Upgrade(_)
                    | crate::characters::afflictions::AcquireResolution::Supersede(_)
            ) {
                events.push(TaggedEvent::new(
                    String::new(),
                    MessagePayload::AfflictionAcquired {
                        character_id: target.identifier.to_string(),
                        affliction: draft.kind.to_string(),
                        severity: draft.severity.to_string(),
                    },
                ));
            }
        }
        for draft in &attacker_inflicts {
            let resolution = self.try_acquire_affliction(draft.clone());
            if matches!(
                resolution,
                crate::characters::afflictions::AcquireResolution::Insert
                    | crate::characters::afflictions::AcquireResolution::Upgrade(_)
                    | crate::characters::afflictions::AcquireResolution::Supersede(_)
            ) {
                events.push(TaggedEvent::new(
                    String::new(),
                    MessagePayload::AfflictionAcquired {
                        character_id: self.identifier.to_string(),
                        affliction: draft.kind.to_string(),
                        severity: draft.severity.to_string(),
                    },
                ));
            }
        }

        let (outcome, attack_outcome) = if self.blood == 0 {
            self.statistics.killed_by = Some(target_name.clone());
            self.status = crate::characters::statuses::CharacterStatus::RecentlyDead;
            self.recently_killed_by = Some(target.id);

            detail_lines.push(
                GameOutput::CharacterAttackDied(character_name.as_str(), target_name.as_str())
                    .to_string(),
            );

            (
                CombatOutcome::Killed,
                AttackOutcome::Kill(target.clone(), self.clone()),
            )
        } else if target.blood == 0 {
            target.statistics.killed_by = Some(character_name.clone());
            target.status = crate::characters::statuses::CharacterStatus::RecentlyDead;
            target.recently_killed_by = Some(self.id);

            detail_lines.push(
                GameOutput::CharacterAttackSuccessKill(
                    character_name.as_str(),
                    target_name.as_str(),
                )
                .to_string(),
            );

            (
                CombatOutcome::Killed,
                AttackOutcome::Kill(self.clone(), target.clone()),
            )
        } else {
            detail_lines.push(
                GameOutput::CharacterAttackWound(character_name.as_str(), target_name.as_str())
                    .to_string(),
            );
            (
                CombatOutcome::Wounded,
                AttackOutcome::Wound(self.clone(), target.clone()),
            )
        };

        let summary = format!("{} attacks {} ({:?})", self.name, target.name, outcome);
        events.push(TaggedEvent::new(
            summary,
            MessagePayload::Combat(CombatEngagement {
                attacker: tref(self),
                target: tref(target),
                outcome,
                detail_lines,
            }),
        ));

        events.push(TaggedEvent::new(
            String::new(),
            MessagePayload::CombatSwing(beat),
        ));

        attack_outcome
    }
}
