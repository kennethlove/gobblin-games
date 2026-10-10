//! Game-side narration for `shared::combat_beat::CombatBeat`.
//!
//! The data types live in `shared` so they can ride `MessagePayload`. The
//! narration depends on `world::output::GameOutput`, so it lives here as an
//! extension trait.

pub use shared::combat_beat::{
    BeatOutcome, CombatActionKind, CombatBeat, StressReport, SwingOutcome, WearOutcomeReport,
    WearReport,
};

use world::output::GameOutput;

/// Render a `CombatBeat` into the prose lines historically emitted by
/// `Character::attacks()`.
///
/// For `Attack` beats the order is: wear lines (weapon then shield, in `wear`
/// vec order), then outcome lines, then an optional trailing horrified line if
/// the swing's `stress.stress_damage > 0`. Non-swing actions render a single
/// descriptive line.
pub trait CombatBeatExt {
    fn to_log_lines(&self) -> Vec<String>;
}

impl CombatBeatExt for CombatBeat {
    fn to_log_lines(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(4);
        let a = &self.actor.name;
        let t = self
            .target
            .as_ref()
            .map(|c| c.name.as_str())
            .unwrap_or("");

        // Non-swing actions narrate a single line and stop.
        match &self.action {
            CombatActionKind::Flee => {
                return vec![match &self.outcome {
                    BeatOutcome::Fled => format!("{a} flees from {t}!"),
                    _ => format!("{a} tries to flee but {t} cuts them off!"),
                }];
            }
            CombatActionKind::Cower => {
                return vec![format!("{a} cowers and begs {t} for mercy!")];
            }
            CombatActionKind::Spare => {
                return vec![format!("{a} spares {t}.")];
            }
            CombatActionKind::Execute => {
                return vec![format!("{a} executes the helpless {t}.")];
            }
            CombatActionKind::UseItem { item } => {
                return vec![format!("{a} uses {} in the fight.", item.name)];
            }
            CombatActionKind::Attack => {}
        }

        // Attack: extract the swing outcome + stress from the beat outcome.
        let BeatOutcome::Swing { outcome, stress, .. } = &self.outcome else {
            return out;
        };

        // 1. Wear lines.
        for w in &self.wear {
            match w.outcome {
                WearOutcomeReport::Pristine => {}
                WearOutcomeReport::Worn => {
                    if w.owner.identifier == self.actor.identifier {
                        out.push(GameOutput::WeaponWear(&w.owner.name, &w.item.name).to_string());
                    } else {
                        out.push(GameOutput::ShieldWear(&w.owner.name, &w.item.name).to_string());
                    }
                }
                WearOutcomeReport::Broken => {
                    if w.owner.identifier == self.actor.identifier {
                        out.push(GameOutput::WeaponBreak(&w.owner.name, &w.item.name).to_string());
                        if let Some(penalty) = w.mid_action_penalty {
                            out.push(
                                GameOutput::WeaponShattersMidSwing(
                                    &w.owner.name,
                                    &w.item.name,
                                    penalty as u32,
                                )
                                .to_string(),
                            );
                        }
                    } else {
                        out.push(GameOutput::ShieldBreak(&w.owner.name, &w.item.name).to_string());
                        if let Some(penalty) = w.mid_action_penalty {
                            out.push(
                                GameOutput::ShieldShattersMidBlock(
                                    &w.owner.name,
                                    &w.item.name,
                                    penalty as u32,
                                )
                                .to_string(),
                            );
                        }
                    }
                }
            }
        }

        // 2. Outcome lines.
        match outcome {
            SwingOutcome::Miss => {
                out.push(GameOutput::CharacterAttackMiss(a, t).to_string());
            }
            SwingOutcome::Wound { .. } => {
                out.push(GameOutput::CharacterAttackWin(a, t).to_string());
                out.push(GameOutput::CharacterAttackWound(a, t).to_string());
            }
            SwingOutcome::CriticalHitWound { .. } => {
                out.push(GameOutput::CharacterCriticalHit(a, t).to_string());
                out.push(GameOutput::CharacterAttackWin(a, t).to_string());
                out.push(GameOutput::CharacterAttackWound(a, t).to_string());
            }
            SwingOutcome::BlockWound { .. } => {
                out.push(GameOutput::CharacterPerfectBlock(t, a).to_string());
                out.push(GameOutput::CharacterAttackLose(t, a).to_string());
                out.push(GameOutput::CharacterAttackWound(a, t).to_string());
            }
            SwingOutcome::Kill { .. } => {
                out.push(GameOutput::CharacterAttackWin(a, t).to_string());
                out.push(GameOutput::CharacterAttackSuccessKill(a, t).to_string());
            }
            SwingOutcome::AttackerDied { .. } => {
                out.push(GameOutput::CharacterAttackLose(t, a).to_string());
                out.push(GameOutput::CharacterAttackDied(a, t).to_string());
            }
            SwingOutcome::FumbleSurvive { .. } => {
                out.push(GameOutput::CharacterCriticalFumble(a).to_string());
            }
            SwingOutcome::FumbleDeath { .. } => {
                out.push(GameOutput::CharacterCriticalFumble(a).to_string());
                out.push(GameOutput::CharacterAttackDied(a, "themselves").to_string());
            }
            SwingOutcome::SelfAttackWound { .. } => {
                out.push(GameOutput::CharacterSelfHarm(a).to_string());
                out.push(GameOutput::CharacterAttackWin(a, a).to_string());
                out.push(GameOutput::CharacterAttackWound(a, a).to_string());
            }
            SwingOutcome::Suicide { .. } => {
                out.push(GameOutput::CharacterSelfHarm(a).to_string());
                out.push(GameOutput::CharacterAttackWin(a, a).to_string());
                out.push(GameOutput::CharacterSuicide(a).to_string());
            }
        }

        // 3. Optional trailing horrified line.
        if stress.stress_damage > 0 {
            let name = stress
                .stressed
                .as_ref()
                .map(|c| c.name.as_str())
                .unwrap_or(a);
            out.push(GameOutput::CharacterHorrified(name, stress.stress_damage).to_string());
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::messages::CharacterRef;

    fn t(name: &str) -> CharacterRef {
        CharacterRef {
            identifier: format!("id-{name}").into(),
            name: name.into(),
        }
    }

    fn sword() -> shared::messages::ItemRef {
        shared::messages::ItemRef {
            identifier: "sword-1".into(),
            name: "Iron Sword".into(),
        }
    }

    fn shield() -> shared::messages::ItemRef {
        shared::messages::ItemRef {
            identifier: "shield-1".into(),
            name: "Iron Buckler".into(),
        }
    }

    fn swing_beat(outcome: SwingOutcome) -> CombatBeat {
        CombatBeat {
            round: 1,
            order: 0,
            actor: t("Alice"),
            action: CombatActionKind::Attack,
            target: Some(t("Bob")),
            weapon: Some(sword()),
            shield: Some(shield()),
            wear: vec![],
            outcome: BeatOutcome::Swing {
                outcome,
                stress: StressReport::default(),
                actor_stamina_cost: 0,
                target_stamina_cost: 0,
            },
            significant: false,
        }
    }

    #[test]
    fn miss_renders_one_line() {
        let beat = swing_beat(SwingOutcome::Miss);
        let lines = beat.to_log_lines();
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].contains("missed") || lines[0].contains("miss"),
            "got: {}",
            lines[0]
        );
    }

    #[test]
    fn weapon_break_with_penalty_renders_shatters_line() {
        let mut beat = swing_beat(SwingOutcome::Miss);
        beat.wear.push(WearReport {
            owner: t("Alice"),
            item: sword(),
            outcome: WearOutcomeReport::Broken,
            forfeited_effect: Some(5),
            mid_action_penalty: Some(3),
        });
        let lines = beat.to_log_lines();
        assert!(
            lines.iter().any(|l| l.contains("shatters mid-swing")),
            "expected shatters-mid-swing line; got {:?}",
            lines
        );
        assert!(
            lines.iter().any(|l| l.contains("-3 attack")),
            "expected '-3 attack' in shatters line; got {:?}",
            lines
        );
    }

    #[test]
    fn flee_fled_renders_one_line() {
        let beat = CombatBeat {
            round: 1,
            order: 1,
            actor: t("Bob"),
            action: CombatActionKind::Flee,
            target: None,
            weapon: None,
            shield: None,
            wear: vec![],
            outcome: BeatOutcome::Fled,
            significant: true,
        };
        let lines = beat.to_log_lines();
        assert_eq!(lines.len(), 1);
    }

    /// Snapshot tests covering every `SwingOutcome` variant plus
    /// representative wear/stress combinations. These lock the
    /// "single source of truth" property of `to_log_lines()`.
    mod snapshots {
        use super::*;
        use shared::messages::{CharacterRef, ItemRef};

        fn t(name: &str) -> CharacterRef {
            CharacterRef {
                identifier: format!("id-{name}").into(),
                name: name.into(),
            }
        }

        fn sword() -> ItemRef {
            ItemRef {
                identifier: "sword-1".into(),
                name: "Iron Sword".into(),
            }
        }

        fn shield() -> ItemRef {
            ItemRef {
                identifier: "shield-1".into(),
                name: "Iron Buckler".into(),
            }
        }

        /// Build an Attack beat with the given swing outcome and no wear/stress.
        fn base_beat(outcome: SwingOutcome) -> CombatBeat {
            CombatBeat {
                round: 1,
                order: 0,
                actor: t("Alice"),
                action: CombatActionKind::Attack,
                target: Some(t("Bob")),
                weapon: Some(sword()),
                shield: Some(shield()),
                wear: vec![],
                outcome: BeatOutcome::Swing {
                    outcome,
                    stress: StressReport::default(),
                    actor_stamina_cost: 0,
                    target_stamina_cost: 0,
                },
                significant: false,
            }
        }

        /// Set the stress on a swing beat (helper for snapshot construction).
        fn set_stress(b: &mut CombatBeat, stress: StressReport) {
            if let BeatOutcome::Swing { stress: s, .. } = &mut b.outcome {
                *s = stress;
            }
        }

        macro_rules! snap {
            ($name:ident, $beat:expr) => {
                #[test]
                fn $name() {
                    let beat: CombatBeat = $beat;
                    let lines = beat.to_log_lines();
                    insta::assert_yaml_snapshot!(stringify!($name), lines);
                }
            };
        }

        // Plain outcomes (no wear, no stress).
        snap!(miss, base_beat(SwingOutcome::Miss));
        snap!(wound, base_beat(SwingOutcome::Wound { damage: 7 }));
        snap!(
            critical_hit_wound,
            base_beat(SwingOutcome::CriticalHitWound { damage: 21 })
        );
        snap!(
            block_wound,
            base_beat(SwingOutcome::BlockWound { damage: 8 })
        );
        snap!(kill, base_beat(SwingOutcome::Kill { damage: 12 }));
        snap!(
            attacker_died,
            base_beat(SwingOutcome::AttackerDied { damage: 9 })
        );
        snap!(
            fumble_survive,
            base_beat(SwingOutcome::FumbleSurvive { self_damage: 5 })
        );
        snap!(
            fumble_death,
            base_beat(SwingOutcome::FumbleDeath { self_damage: 5 })
        );
        snap!(
            self_attack_wound,
            base_beat(SwingOutcome::SelfAttackWound { damage: 4 })
        );
        snap!(suicide, base_beat(SwingOutcome::Suicide { damage: 4 }));

        // Wear-only combinations.
        snap!(wound_with_weapon_worn, {
            let mut b = base_beat(SwingOutcome::Wound { damage: 7 });
            b.wear.push(WearReport {
                owner: t("Alice"),
                item: sword(),
                outcome: WearOutcomeReport::Worn,
                forfeited_effect: None,
                mid_action_penalty: None,
            });
            b
        });

        snap!(wound_with_shield_worn, {
            let mut b = base_beat(SwingOutcome::Wound { damage: 7 });
            b.wear.push(WearReport {
                owner: t("Bob"),
                item: shield(),
                outcome: WearOutcomeReport::Worn,
                forfeited_effect: None,
                mid_action_penalty: None,
            });
            b
        });

        snap!(miss_with_weapon_break_no_penalty, {
            let mut b = base_beat(SwingOutcome::Miss);
            b.wear.push(WearReport {
                owner: t("Alice"),
                item: sword(),
                outcome: WearOutcomeReport::Broken,
                forfeited_effect: None,
                mid_action_penalty: None,
            });
            b
        });

        snap!(wound_with_weapon_break_and_penalty, {
            let mut b = base_beat(SwingOutcome::Wound { damage: 4 });
            b.wear.push(WearReport {
                owner: t("Alice"),
                item: sword(),
                outcome: WearOutcomeReport::Broken,
                forfeited_effect: Some(5),
                mid_action_penalty: Some(3),
            });
            b
        });

        snap!(wound_with_shield_break_and_penalty, {
            let mut b = base_beat(SwingOutcome::Wound { damage: 9 });
            b.wear.push(WearReport {
                owner: t("Bob"),
                item: shield(),
                outcome: WearOutcomeReport::Broken,
                forfeited_effect: Some(4),
                mid_action_penalty: Some(2),
            });
            b
        });

        snap!(wound_with_both_break_and_penalty, {
            let mut b = base_beat(SwingOutcome::Wound { damage: 6 });
            b.wear.push(WearReport {
                owner: t("Alice"),
                item: sword(),
                outcome: WearOutcomeReport::Broken,
                forfeited_effect: Some(5),
                mid_action_penalty: Some(2),
            });
            b.wear.push(WearReport {
                owner: t("Bob"),
                item: shield(),
                outcome: WearOutcomeReport::Broken,
                forfeited_effect: Some(4),
                mid_action_penalty: Some(1),
            });
            b
        });

        // Stress combinations.
        snap!(wound_with_attacker_stress, {
            let mut b = base_beat(SwingOutcome::Wound { damage: 6 });
            set_stress(
                &mut b,
                StressReport {
                    stress_damage: 8,
                    stressed: Some(t("Alice")),
                },
            );
            b
        });

        snap!(block_wound_with_defender_stress, {
            let mut b = base_beat(SwingOutcome::BlockWound { damage: 8 });
            set_stress(
                &mut b,
                StressReport {
                    stress_damage: 5,
                    stressed: Some(t("Bob")),
                },
            );
            b
        });

        snap!(kill_with_attacker_stress_and_weapon_break, {
            let mut b = base_beat(SwingOutcome::Kill { damage: 14 });
            b.wear.push(WearReport {
                owner: t("Alice"),
                item: sword(),
                outcome: WearOutcomeReport::Broken,
                forfeited_effect: Some(5),
                mid_action_penalty: Some(2),
            });
            set_stress(
                &mut b,
                StressReport {
                    stress_damage: 12,
                    stressed: Some(t("Alice")),
                },
            );
            b
        });

        // --- Gap-coverage snapshots ---

        snap!(wound_with_pristine_weapon_and_shield, {
            let mut b = base_beat(SwingOutcome::Wound { damage: 5 });
            b.wear.push(WearReport {
                owner: t("Alice"),
                item: sword(),
                outcome: WearOutcomeReport::Pristine,
                forfeited_effect: None,
                mid_action_penalty: None,
            });
            b.wear.push(WearReport {
                owner: t("Bob"),
                item: shield(),
                outcome: WearOutcomeReport::Pristine,
                forfeited_effect: None,
                mid_action_penalty: None,
            });
            b
        });

        snap!(wound_with_both_worn, {
            let mut b = base_beat(SwingOutcome::Wound { damage: 6 });
            b.wear.push(WearReport {
                owner: t("Alice"),
                item: sword(),
                outcome: WearOutcomeReport::Worn,
                forfeited_effect: None,
                mid_action_penalty: None,
            });
            b.wear.push(WearReport {
                owner: t("Bob"),
                item: shield(),
                outcome: WearOutcomeReport::Worn,
                forfeited_effect: None,
                mid_action_penalty: None,
            });
            b
        });

        snap!(wound_with_stress_fallback_to_attacker, {
            let mut b = base_beat(SwingOutcome::Wound { damage: 4 });
            set_stress(
                &mut b,
                StressReport {
                    stress_damage: 6,
                    stressed: None,
                },
            );
            b
        });

        snap!(critical_hit_with_weapon_break_and_stress, {
            let mut b = base_beat(SwingOutcome::CriticalHitWound { damage: 18 });
            b.wear.push(WearReport {
                owner: t("Alice"),
                item: sword(),
                outcome: WearOutcomeReport::Broken,
                forfeited_effect: Some(6),
                mid_action_penalty: Some(3),
            });
            set_stress(
                &mut b,
                StressReport {
                    stress_damage: 4,
                    stressed: Some(t("Alice")),
                },
            );
            b
        });

        snap!(block_wound_with_shield_break_and_stress, {
            let mut b = base_beat(SwingOutcome::BlockWound { damage: 7 });
            b.wear.push(WearReport {
                owner: t("Bob"),
                item: shield(),
                outcome: WearOutcomeReport::Broken,
                forfeited_effect: Some(4),
                mid_action_penalty: Some(2),
            });
            set_stress(
                &mut b,
                StressReport {
                    stress_damage: 5,
                    stressed: Some(t("Bob")),
                },
            );
            b
        });

        snap!(attacker_died_with_defender_stress, {
            let mut b = base_beat(SwingOutcome::AttackerDied { damage: 10 });
            set_stress(
                &mut b,
                StressReport {
                    stress_damage: 9,
                    stressed: Some(t("Bob")),
                },
            );
            b
        });

        snap!(fumble_death_with_weapon_break, {
            let mut b = base_beat(SwingOutcome::FumbleDeath { self_damage: 8 });
            b.wear.push(WearReport {
                owner: t("Alice"),
                item: sword(),
                outcome: WearOutcomeReport::Broken,
                forfeited_effect: Some(5),
                mid_action_penalty: Some(2),
            });
            b
        });

        snap!(self_attack_wound_with_weapon_worn, {
            let mut b = base_beat(SwingOutcome::SelfAttackWound { damage: 3 });
            b.wear.push(WearReport {
                owner: t("Alice"),
                item: sword(),
                outcome: WearOutcomeReport::Worn,
                forfeited_effect: None,
                mid_action_penalty: None,
            });
            b
        });
    }
}
