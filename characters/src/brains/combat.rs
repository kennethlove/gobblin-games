//! Per-round combat brain (spec `docs/superpowers/specs/2026-10-09-multi-round-combat-design.md` §4, §5, §7).
//!
//! The outer brain commits `Attack` once; from then on every round this brain
//! scores the combat options for each fighter until the fight ends:
//! **Attack / Use Item / Flee / Cower**, plus the forced **Spare / Execute**
//! verdict after a cower.
//!
//! Scoring is weight-based with placeholder weights in [`CombatTuning`]
//! (`brain_*` fields) — tuned later alongside initiative/flee values.
//! Ties resolve in fixed priority Attack > UseItem > Flee > Cower.
//!
//! Flee reaches the loop through this brain's weight (§5): it is no longer
//! always-on — a fresh, confident goblin prefers to keep swinging.

use crate::Character;
use crate::combat::round_loop::CombatActionProvider;
use crate::combat_tuning::CombatTuning;
use crate::stamina_band::stamina_band;
use crate::traits::Trait;
use shared::combat_beat::CombatActionKind;
use shared::messages::{ItemRef, StaminaBand};
use world::items::Attribute;

/// Blood at or below this counts as wounded (blood scale is 0..=1000).
const WOUNDED_BLOOD: u32 = 500;
/// Hopeless = near-dead (this blood or less) AND in the Exhausted band.
const HOPELESS_BLOOD: u32 = 150;

/// Per-round combat decision-maker. Holds a copy of the fight's tuning so the
/// provider signature stays `(actor, opponent)` as the round loop expects.
pub struct CombatBrain {
    tuning: CombatTuning,
}

impl CombatBrain {
    pub fn new(tuning: CombatTuning) -> Self {
        Self { tuning }
    }

    /// Score and pick this round's action for `actor` against `opponent`.
    pub fn choose_action(&self, actor: &Character, opponent: &Character) -> CombatActionKind {
        let t = &self.tuning;
        let wounded = actor.blood <= WOUNDED_BLOOD;
        let exhausted = stamina_band(actor.stamina, actor.max_stamina, t) == StaminaBand::Exhausted;
        let hopeless = actor.blood <= HOPELESS_BLOOD && exhausted;

        // Attack — the default posture.
        let mut attack = t.brain_attack_base;
        if actor.traits.contains(&Trait::Aggressive) || actor.traits.contains(&Trait::Reckless) {
            attack += t.brain_attack_aggressive_bonus;
        }
        if wounded {
            attack += t.brain_attack_wounded_penalty;
        }

        // Use Item — only offered when wounded and carrying a health
        // consumable; otherwise the option doesn't score at all.
        let heal_item: Option<ItemRef> = if wounded {
            actor
                .consumables()
                .into_iter()
                .find(|i| i.attribute == Attribute::Health)
                .map(|i| ItemRef {
                    identifier: i.identifier.into(),
                    name: i.name,
                })
        } else {
            None
        };
        let use_item = if heal_item.is_some() {
            t.brain_heal_wounded_bonus
        } else {
            i32::MIN / 2 // ineligible; never outranks a real score
        };

        // Flee — cowardice, exhaustion, wounds, and being outmatched all push.
        let mut flee = t.brain_flee_base;
        if wounded {
            flee += t.brain_flee_wounded_bonus;
        }
        if exhausted {
            flee += t.brain_flee_exhausted_bonus;
        }
        if actor.traits.contains(&Trait::Cautious) {
            flee += t.brain_flee_cautious_bonus;
        }
        if opponent.attributes.strength > actor.attributes.strength {
            flee += t.brain_flee_outmatched_bonus;
        }

        // Cower — only when hopeless.
        let mut cower = t.brain_cower_base;
        if hopeless {
            cower += t.brain_cower_hopeless_bonus;
        }

        let mut best_kind = CombatActionKind::Attack;
        let mut best_score = attack;
        if let Some(item) = heal_item
            && use_item > best_score
        {
            best_kind = CombatActionKind::UseItem { item };
            best_score = use_item;
        }
        if flee > best_score {
            best_kind = CombatActionKind::Flee;
            best_score = flee;
        }
        if cower > best_score {
            best_kind = CombatActionKind::Cower;
            let _ = best_score;
        }
        best_kind
    }

    /// Forced Spare/Execute verdict after `cowerer` begged (spec §7).
    /// Mercy must be earned: allies, shared team, and loyal/friendly
    /// temperaments spare; treacherous and aggressive ones execute; a hostile
    /// stranger scores 0v0 and dies (Execute wins ties).
    pub fn choose_verdict(&self, actor: &Character, cowerer: &Character) -> CombatActionKind {
        let t = &self.tuning;

        let mut spare = 0;
        if actor.allies.contains(&cowerer.id) {
            spare += t.brain_spare_ally_bonus;
        }
        if actor.team > 0 && actor.team == cowerer.team {
            spare += t.brain_spare_team_bonus;
        }
        if actor.traits.contains(&Trait::Loyal) || actor.traits.contains(&Trait::Friendly) {
            spare += t.brain_spare_trait_bonus;
        }

        let mut execute = 0;
        if actor.traits.contains(&Trait::Treacherous) {
            execute += t.brain_execute_treacherous_bonus;
        }
        if actor.traits.contains(&Trait::Aggressive) {
            execute += t.brain_execute_aggressive_bonus;
        }

        if spare > execute {
            CombatActionKind::Spare
        } else {
            CombatActionKind::Execute
        }
    }
}

impl CombatActionProvider for CombatBrain {
    fn action(&mut self, actor: &Character, opponent: &Character) -> CombatActionKind {
        self.choose_action(actor, opponent)
    }

    fn verdict(&mut self, actor: &Character, cowerer: &Character) -> CombatActionKind {
        self.choose_verdict(actor, cowerer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::items::{Item, ItemRarity, ItemType};

    /// RNG-rolled traits/items would make scoring assertions non-deterministic;
    /// clear them and set exactly what each test needs.
    fn char_named(name: &str) -> Character {
        let mut c = Character::new(name.to_string(), None, None);
        c.traits.clear();
        c.items.clear();
        c
    }

    fn brain() -> CombatBrain {
        CombatBrain::new(CombatTuning::default())
    }

    fn potion() -> Item {
        Item::new(
            "potion",
            ItemType::Consumable,
            ItemRarity::Common,
            10,
            Attribute::Health,
            200,
        )
    }

    #[test]
    fn healthy_goblin_attacks() {
        let a = char_named("Alice");
        let o = char_named("Bob");
        assert_eq!(brain().choose_action(&a, &o), CombatActionKind::Attack);
    }

    #[test]
    fn wounded_goblin_with_heal_item_uses_it() {
        let mut a = char_named("Alice");
        a.blood = 300;
        a.items.push(potion());
        let o = char_named("Bob");
        assert!(matches!(
            brain().choose_action(&a, &o),
            CombatActionKind::UseItem { .. }
        ));
    }

    #[test]
    fn wounded_exhausted_goblin_without_item_flees() {
        let mut a = char_named("Alice");
        a.blood = 300;
        a.stamina = 10; // Exhausted band (default threshold 20% of max)
        let o = char_named("Bob");
        assert_eq!(brain().choose_action(&a, &o), CombatActionKind::Flee);
    }

    #[test]
    fn hopeless_goblin_cowers() {
        let mut a = char_named("Alice");
        a.blood = 100; // near-dead
        a.stamina = 10; // Exhausted
        let o = char_named("Bob");
        assert_eq!(brain().choose_action(&a, &o), CombatActionKind::Cower);
    }

    #[test]
    fn cautious_wounded_goblin_flees() {
        let mut a = char_named("Alice");
        a.blood = 400;
        a.traits.push(Trait::Cautious);
        let o = char_named("Bob");
        assert_eq!(brain().choose_action(&a, &o), CombatActionKind::Flee);
    }

    #[test]
    fn hostile_stranger_is_executed() {
        let a = char_named("Alice");
        let o = char_named("Bob");
        assert_eq!(brain().choose_verdict(&a, &o), CombatActionKind::Execute);
    }

    #[test]
    fn recorded_ally_is_spared() {
        let mut a = char_named("Alice");
        let o = char_named("Bob");
        a.allies.push(o.id);
        assert_eq!(brain().choose_verdict(&a, &o), CombatActionKind::Spare);
    }

    #[test]
    fn teammate_is_spared() {
        let mut a = char_named("Alice");
        a.team = 3;
        let mut o = char_named("Bob");
        o.team = 3;
        assert_eq!(brain().choose_verdict(&a, &o), CombatActionKind::Spare);
    }

    #[test]
    fn treacherous_teammate_is_executed() {
        let mut a = char_named("Alice");
        a.team = 3;
        a.traits.push(Trait::Treacherous);
        let mut o = char_named("Bob");
        o.team = 3;
        // Spare 40 (team) < Execute 50 (treacherous) — betrayal wins.
        assert_eq!(brain().choose_verdict(&a, &o), CombatActionKind::Execute);
    }

    #[test]
    fn loyal_ally_spares() {
        let mut a = char_named("Alice");
        a.traits.push(Trait::Loyal);
        let o = char_named("Bob");
        a.allies.push(o.id);
        assert_eq!(brain().choose_verdict(&a, &o), CombatActionKind::Spare);
    }
}
