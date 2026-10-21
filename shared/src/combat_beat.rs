//! Typed payloads describing one multi-round combat.
//!
//! A fight is a [`Combat`] — a sequence of [`CombatRound`]s, each holding one
//! [`CombatBeat`] per active combatant. A beat records a single combatant's
//! chosen action and its outcome. The data lives in `shared` so it can ride
//! `MessagePayload::CombatRoundStart` / `CombatBeat` / `CombatEnded`.
//! The narration `to_log_lines()` lives in the `game` crate, since it depends
//! on `GameOutput` rendering.
//!
//! These types supersede the single-swing `CombatBeat` that rode
//! `MessagePayload::CombatSwing` (now removed). `SwingOutcome`, `WearReport`,
//! and `StressReport` are reused verbatim inside the per-action [`BeatOutcome`].

use crate::messages::{CharacterRef, ItemRef};
use serde::{Deserialize, Serialize};

/// What happened to a piece of equipment during the swing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WearOutcomeReport {
    Pristine,
    Worn,
    Broken,
}

/// Wear/break record for one piece of equipment used in the swing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WearReport {
    /// Owner of the item (attacker for weapon, target for shield).
    pub owner: CharacterRef,
    pub item: ItemRef,
    pub outcome: WearOutcomeReport,
    /// Bonus this item *would* have contributed if it hadn't broken
    /// during this contest. `None` when the item didn't break.
    pub forfeited_effect: Option<i32>,
    /// Random penalty applied because the item snapped mid-action.
    /// `Some(1..=4)` when the break penalty fired, `None` otherwise.
    pub mid_action_penalty: Option<i32>,
}

/// High-level outcome of one swing (the `Attack` action's resolution).
///
/// Mirrors the post-resolution branches in the legacy `attacks()`. `FumbleDeath`
/// covers the previously implicit "fumble killed the attacker" path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwingOutcome {
    /// Attack missed entirely.
    Miss,
    /// Attacker landed a hit; target survived.
    Wound { damage: u32 },
    /// Attacker scored a critical hit; target survived.
    CriticalHitWound { damage: u32 },
    /// Defender countered (PerfectBlock); attacker took damage; attacker survived.
    BlockWound { damage: u32 },
    /// Target was killed by the attacker.
    Kill { damage: u32 },
    /// Attacker was killed by the target's counter.
    AttackerDied { damage: u32 },
    /// Attacker fumbled (nat-1) and survived self-damage.
    FumbleSurvive { self_damage: u32 },
    /// Attacker fumbled (nat-1) and killed themselves.
    FumbleDeath { self_damage: u32 },
    /// Attacker == target. Self-attack that wounded.
    SelfAttackWound { damage: u32 },
    /// Attacker == target. Self-attack that killed.
    Suicide { damage: u32 },
}

/// Stress damage applied to one of the swing's combatants after resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StressReport {
    /// Mental damage applied via `apply_violence_stress`. 0 means no horrified line.
    pub stress_damage: u32,
    /// Who took the stress (the swing's winner). `None` when `stress_damage`
    /// is 0; `Some` whenever a horrified line should render.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stressed: Option<CharacterRef>,
}

/// One character's chosen action within a round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatActionKind {
    /// Swing at the opponent. Resolves via `attack_contest`.
    Attack,
    /// Consume/use an item on self.
    UseItem { item: ItemRef },
    /// Attempt to flee the fight (contested speed roll).
    Flee,
    /// Stop fighting and beg for mercy.
    Cower,
    /// Winner's verdict after a cower: let them live.
    Spare,
    /// Winner's verdict after a cower: kill them.
    Execute,
}

/// Per-action outcome of one beat.
///
/// `Attack` reuses [`SwingOutcome`]; the other actions carry their own results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeatOutcome {
    /// An `Attack` beat — full swing resolution.
    Swing {
        outcome: SwingOutcome,
        stress: StressReport,
        /// Stamina deducted from the actor for this swing.
        #[serde(default)]
        actor_stamina_cost: u32,
        /// Stamina deducted from the target for this swing.
        #[serde(default)]
        target_stamina_cost: u32,
    },
    /// A `UseItem` beat — the item was consumed.
    ItemUsed,
    /// A `Flee` beat — escaped the fight.
    Fled,
    /// A `Flee` beat — the chaser caught up; stay in the fight.
    FailedFlee,
    /// A `Cower` beat — the character surrendered and awaits a verdict.
    Cowered,
    /// A `Spare` beat — the cowerer lives.
    Spared,
    /// An `Execute` beat — the cowerer was killed.
    Executed { damage: u32 },
}

/// Full record of one combatant's action within a round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatBeat {
    /// Round number this beat belongs to (1-based).
    pub round: u32,
    /// Position within the round (0 = initiative winner, 1 = second actor).
    pub order: u8,
    /// Who acted.
    pub actor: CharacterRef,
    /// What they chose to do.
    pub action: CombatActionKind,
    /// Who they acted against. `None` for actions with no target (e.g. Flee).
    pub target: Option<CharacterRef>,
    /// Weapon equipped by the actor at swing start (None if unarmed).
    pub weapon: Option<ItemRef>,
    /// Shield equipped by the target at swing start (None if unshielded).
    pub shield: Option<ItemRef>,
    /// Wear/break records emitted in attack-roll order: weapon first, then shield.
    pub wear: Vec<WearReport>,
    /// The action's outcome.
    pub outcome: BeatOutcome,
    /// Whether this beat is "significant" for persistence/UI down-ranking
    /// (kills, cower, spare/execute, successful flee, equipment breaks).
    #[serde(default)]
    pub significant: bool,
}

/// One round of a fight: one beat per active combatant, in initiative order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatRound {
    /// 1-based round number.
    pub number: u32,
    /// Beats in the order they resolved. Shorter than the combatant count when
    /// the fight terminated mid-round or combatants dropped out (death,
    /// flee, verdict) before their beat.
    pub beats: Vec<CombatBeat>,
}

/// The terminal result of a whole fight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum CombatResult {
    /// One side stands when the opposing side is gone (killed, fled, or
    /// spared out). A 1v1 kill, a fight-ending flee, and a spare all land
    /// here — the fight's beats carry which.
    Victory { victors: Vec<CharacterRef> },
    /// Neither side can continue (e.g. everyone stamina-empty); both
    /// disengage alive.
    Draw,
}

/// v1 result shapes, accepted on deserialize so persisted history keeps
/// decoding. Mapped per the v2 design doc §9: the standing side becomes
/// `Victory.victors`.
#[derive(Deserialize)]
// Decoding-only compat shape: the dropped names mirror v1 JSON exactly
// (v1 derived with default external tagging — PascalCase variant names).
#[allow(dead_code)]
enum CombatResultCompat {
    Winner { character: CharacterRef },
    Fled {
        fled: CharacterRef,
        remaining: CharacterRef,
    },
    Spared {
        winner: CharacterRef,
        spared: CharacterRef,
    },
    Victory { victors: Vec<CharacterRef> },
    Draw,
}

impl<'de> Deserialize<'de> for CombatResult {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match CombatResultCompat::deserialize(deserializer)? {
            CombatResultCompat::Winner { character } => {
                CombatResult::Victory { victors: vec![character] }
            }
            CombatResultCompat::Fled { remaining, .. } => {
                CombatResult::Victory { victors: vec![remaining] }
            }
            CombatResultCompat::Spared { winner, .. } => {
                CombatResult::Victory { victors: vec![winner] }
            }
            CombatResultCompat::Victory { victors } => CombatResult::Victory { victors },
            CombatResultCompat::Draw => CombatResult::Draw,
        })
    }
}

/// The whole fight: combatants, every round, and the terminal result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Combat {
    /// Stable identifier so the UI can group this fight's beats into one card.
    pub combat_id: String,
    /// All combatants, in stable (deterministic) join order.
    pub combatants: Vec<CharacterRef>,
    /// Side membership. `None` for v1 history (implicit first-vs-second).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sides: Option<[Vec<CharacterRef>; 2]>,
    /// Every round that resolved, in order.
    pub rounds: Vec<CombatRound>,
    /// The terminal result. `None` while the fight is still running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<CombatResult>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::ItemRef;

    fn t(name: &str) -> CharacterRef {
        t_id("id", name)
    }

    fn t_id(identifier: &str, name: &str) -> CharacterRef {
        CharacterRef {
            identifier: identifier.into(),
            name: name.into(),
        }
    }

    fn item(name: &str) -> ItemRef {
        ItemRef {
            identifier: "sword-1".into(),
            name: name.into(),
        }
    }

    #[test]
    fn wear_report_roundtrips_with_break_penalty_fields() {
        let report = WearReport {
            owner: t("A"),
            item: item("Iron Sword"),
            outcome: WearOutcomeReport::Broken,
            forfeited_effect: Some(3),
            mid_action_penalty: Some(2),
        };
        let json = serde_json::to_string(&report).unwrap();
        let back: WearReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
        assert_eq!(back.forfeited_effect, Some(3));
        assert_eq!(back.mid_action_penalty, Some(2));
    }

    #[test]
    fn combat_beat_attack_roundtrips_via_serde() {
        let beat = CombatBeat {
            round: 1,
            order: 0,
            actor: t("A"),
            action: CombatActionKind::Attack,
            target: Some(t("B")),
            weapon: Some(item("Iron Sword")),
            shield: None,
            wear: vec![],
            outcome: BeatOutcome::Swing {
                outcome: SwingOutcome::Miss,
                stress: StressReport::default(),
                actor_stamina_cost: 25,
                target_stamina_cost: 10,
            },
            significant: false,
        };
        let json = serde_json::to_string(&beat).unwrap();
        let back: CombatBeat = serde_json::from_str(&json).unwrap();
        assert_eq!(beat, back);
    }

    #[test]
    fn combat_beat_flee_roundtrips_with_no_target() {
        let beat = CombatBeat {
            round: 2,
            order: 1,
            actor: t("B"),
            action: CombatActionKind::Flee,
            target: None,
            weapon: None,
            shield: None,
            wear: vec![],
            outcome: BeatOutcome::Fled,
            significant: true,
        };
        let json = serde_json::to_string(&beat).unwrap();
        let back: CombatBeat = serde_json::from_str(&json).unwrap();
        assert_eq!(beat, back);
        assert!(back.target.is_none());
    }

    #[test]
    fn combat_round_and_result_roundtrip() {
        let round = CombatRound {
            number: 1,
            beats: vec![],
        };
        let combat = Combat {
            combat_id: "combat-1".into(),
            combatants: vec![t("A"), t("B")],
            sides: Some([vec![t("A")], vec![t("B")]]),
            rounds: vec![round.clone()],
            result: Some(CombatResult::Victory { victors: vec![t("A")] }),
        };
        let json = serde_json::to_string(&combat).unwrap();
        let back: Combat = serde_json::from_str(&json).unwrap();
        assert_eq!(combat, back);
        assert_eq!(back.rounds[0], round);
    }

    #[test]
    fn combat_result_roundtrips() {
        let results = [
            CombatResult::Victory { victors: vec![t("A")] },
            CombatResult::Victory {
                victors: vec![t("A"), t("B")],
            },
            CombatResult::Draw,
        ];
        for r in results {
            let json = serde_json::to_string(&r).unwrap();
            let back: CombatResult = serde_json::from_str(&json).unwrap();
            assert_eq!(r, back);
        }
    }

    #[test]
    fn legacy_v1_results_decode_into_victory() {
        let cases = [
            (
                r#"{"Winner":{"character":{"identifier":"a","name":"A"}}}"#,
                vec![t_id("a", "A")],
            ),
            (
                r#"{"Fled":{"fled":{"identifier":"a","name":"A"},"remaining":{"identifier":"b","name":"B"}}}"#,
                vec![t_id("b", "B")],
            ),
            (
                r#"{"Spared":{"winner":{"identifier":"a","name":"A"},"spared":{"identifier":"b","name":"B"}}}"#,
                vec![t_id("a", "A")],
            ),
        ];
        for (json, victors) in cases {
            let back: CombatResult = serde_json::from_str(json).unwrap();
            assert_eq!(back, CombatResult::Victory { victors });
        }
        let draw: CombatResult = serde_json::from_str(r#""Draw""#).unwrap();
        assert_eq!(draw, CombatResult::Draw);
    }

    #[test]
    fn legacy_v1_combat_decodes_with_no_sides() {
        let json = r#"{
            "combat_id": "combat-old",
            "combatants": [
                {"identifier":"a","name":"A"},
                {"identifier":"b","name":"B"}
            ],
            "rounds": [],
            "result": {"Winner":{"character":{"identifier":"a","name":"A"}}}
        }"#;
        let combat: Combat = serde_json::from_str(json).unwrap();
        assert_eq!(combat.combatants.len(), 2);
        assert_eq!(combat.sides, None);
        assert_eq!(
            combat.result,
            Some(CombatResult::Victory {
                victors: vec![t_id("a", "A")]
            })
        );
    }

    #[test]
    fn beat_outcome_cower_and_verdict_variants_roundtrip() {
        let outcomes = [
            BeatOutcome::Cowered,
            BeatOutcome::Spared,
            BeatOutcome::Executed { damage: 42 },
            BeatOutcome::ItemUsed,
            BeatOutcome::Fled,
            BeatOutcome::FailedFlee,
        ];
        for outcome in outcomes {
            let json = serde_json::to_string(&outcome).unwrap();
            let back: BeatOutcome = serde_json::from_str(&json).unwrap();
            assert_eq!(outcome, back);
        }
    }
}
