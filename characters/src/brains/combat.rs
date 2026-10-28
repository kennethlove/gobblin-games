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
use crate::traits::{BehaviorAxis, CharacterAttribute, behavior_sum};
use shared::combat_beat::CombatActionKind;
use shared::messages::{ItemRef, StaminaBand};
use uuid::Uuid;
use world::items::Attribute;

/// Blood at or below this counts as wounded (blood scale is 0..=1000).
const WOUNDED_BLOOD: u32 = 500;
/// Hopeless = near-dead (this blood or less) AND in the Exhausted band.
const HOPELESS_BLOOD: u32 = 150;

/// Per-round combat decision-maker. Holds a copy of the fight's tuning so the
/// provider signature stays `(actor, opposing_side)` as the round loop expects.
pub struct CombatBrain {
    tuning: CombatTuning,
}

impl CombatBrain {
    pub fn new(tuning: CombatTuning) -> Self {
        Self { tuning }
    }

    /// Score and pick this round's action for `actor` against the opposing
    /// side (design §4.3; a pair passes its one member).
    pub fn choose_action(
        &self,
        actor: &Character,
        opposing_side: &[&Character],
    ) -> CombatActionKind {
        let t = &self.tuning;
        let wounded = actor.blood <= WOUNDED_BLOOD;
        let exhausted = stamina_band(actor.stamina, actor.max_stamina, t) == StaminaBand::Exhausted;
        let hopeless = actor.blood <= HOPELESS_BLOOD && exhausted;

        // Attack — the default posture.
        let mut attack = t.brain_attack_base;
        if behavior_sum(&actor.traits, BehaviorAxis::AttackPosture) > 0 {
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
        if behavior_sum(&actor.traits, BehaviorAxis::FleeCowardice) > 0 {
            flee += t.brain_flee_cautious_bonus;
        }
        // Outmatched against the strongest member — one weak ally must not
        // make a losing fight look winnable.
        let strongest = opposing_side
            .iter()
            .map(|m| m.effective_attribute(CharacterAttribute::Strength))
            .max()
            .unwrap_or(0);
        let outmatched = strongest > actor.effective_attribute(CharacterAttribute::Strength);
        if outmatched {
            flee += t.brain_flee_outmatched_bonus;
        }

        // Cower — only when hopeless.
        let mut cower = t.brain_cower_base;
        if hopeless {
            cower += t.brain_cower_hopeless_bonus;
        }

        // Call Ally — a recorded ally plus EITHER being outmatched or the
        // GangUp drive (gangers summon even from the winning side; the size
        // rule itself is enforced by the loop). No ally, no call.
        let mut call_ally = i32::MIN / 2;
        if !actor.allies.is_empty()
            && (outmatched || behavior_sum(&actor.traits, BehaviorAxis::GangUp) > 0)
        {
            call_ally = t.brain_call_ally_base;
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
            best_score = cower;
        }
        // Checked last: a hopeless actor's cower wins any tie with a call.
        if call_ally > best_score {
            best_kind = CombatActionKind::CallAlly;
        }
        best_kind
    }

    /// Pick this round's attack target among `opposing_side` (design §4.3):
    /// wounded members score up, aggressive actors favor the strongest,
    /// cowards the weakest. Ties keep slice order (join order). Returns
    /// `None` only for an empty side.
    pub fn choose_target(&self, actor: &Character, opposing_side: &[&Character]) -> Option<Uuid> {
        // cif: the fight's champion = highest lifetime wins among these
        // candidates (zero wins everywhere = nobody; ties share the bonus,
        // the existing stable first-wins rule still picks the winner).
        let champion = opposing_side
            .iter()
            .filter(|m| m.statistics.wins > 0)
            .max_by_key(|m| m.statistics.wins)
            .map(|m| m.id);
        let t = &self.tuning;
        let strength_pref = if behavior_sum(&actor.traits, BehaviorAxis::AttackPosture) > 0 {
            1
        } else if behavior_sum(&actor.traits, BehaviorAxis::FleeCowardice) > 0 {
            -1
        } else {
            0
        };

        let mut best: Option<(Uuid, i32)> = None;
        for m in opposing_side {
            let mut score = 0;
            if m.blood <= WOUNDED_BLOOD {
                score += t.brain_target_wounded_bonus;
            }
            // cif: the champion wears a target — modest bump that stays
            // under the wound bonus (hunted, not insta-focused).
            if champion == Some(m.id) {
                score += t.brain_target_champion_bonus;
            }
            // Attribute points scale into the weight's magnitude so one
            // strength point stays a fraction of the wounded bonus.
            score += strength_pref
                * (m.effective_attribute(CharacterAttribute::Strength) as i32 / 10)
                * t.brain_target_strong_bonus;
            // Strict `>`: ties keep the earlier (join-order) member.
            if best.is_none_or(|(_, best_score)| score > best_score) {
                best = Some((m.id, score));
            }
        }
        best.map(|(id, _)| id)
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
        if behavior_sum(&actor.traits, BehaviorAxis::SpareInclination) > 0 {
            spare += t.brain_spare_trait_bonus;
        }

        let mut execute = 0;
        execute += behavior_sum(&actor.traits, BehaviorAxis::ExecuteFromTreachery)
            * t.brain_execute_treacherous_bonus;
        execute += behavior_sum(&actor.traits, BehaviorAxis::ExecuteFromAggression)
            * t.brain_execute_aggressive_bonus;

        if spare > execute {
            CombatActionKind::Spare
        } else {
            CombatActionKind::Execute
        }
    }
}

impl CombatActionProvider for CombatBrain {
    fn action(
        &mut self,
        actor: &Character,
        opposing_side: &[&Character],
    ) -> (CombatActionKind, Option<Uuid>) {
        let kind = self.choose_action(actor, opposing_side);
        // Only an attack consumes a target; flee/UseItem/cower ignore it.
        let target = if matches!(kind, CombatActionKind::Attack) {
            self.choose_target(actor, opposing_side)
        } else {
            None
        };
        (kind, target)
    }

    fn verdict(&mut self, actor: &Character, cowerer: &Character) -> CombatActionKind {
        self.choose_verdict(actor, cowerer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Trait;
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
        assert_eq!(brain().choose_action(&a, &[&o]), CombatActionKind::Attack);
    }

    #[test]
    fn wounded_goblin_with_heal_item_uses_it() {
        let mut a = char_named("Alice");
        a.blood = 300;
        a.items.push(potion());
        let o = char_named("Bob");
        assert!(matches!(
            brain().choose_action(&a, &[&o]),
            CombatActionKind::UseItem { .. }
        ));
    }

    #[test]
    fn wounded_exhausted_goblin_without_item_flees() {
        let mut a = char_named("Alice");
        a.blood = 300;
        a.stamina = 10; // Exhausted band (default threshold 20% of max)
        let o = char_named("Bob");
        assert_eq!(brain().choose_action(&a, &[&o]), CombatActionKind::Flee);
    }

    #[test]
    fn hopeless_goblin_cowers() {
        let mut a = char_named("Alice");
        a.blood = 100; // near-dead
        a.stamina = 10; // Exhausted
        let o = char_named("Bob");
        assert_eq!(brain().choose_action(&a, &[&o]), CombatActionKind::Cower);
    }

    #[test]
    fn cautious_wounded_goblin_flees() {
        let mut a = char_named("Alice");
        a.blood = 400;
        a.traits.push(Trait::Cautious);
        let o = char_named("Bob");
        assert_eq!(brain().choose_action(&a, &[&o]), CombatActionKind::Flee);
    }

    #[test]
    fn wounded_member_is_preferred_target() {
        let mut actor = char_named("Alice");
        actor.traits.clear(); // no strength preference — the wound decides
        let mut fresh = char_named("Bob");
        fresh.blood = 1000;
        let mut hurt = char_named("Carol");
        hurt.blood = 200; // <= WOUNDED_BLOOD
        let side = [&fresh, &hurt];
        assert_eq!(brain().choose_target(&actor, &side), Some(hurt.id));
    }

    /// cif: highest lifetime wins wears the target.
    #[test]
    fn champion_is_preferred_target() {
        let mut actor = char_named("Alice");
        actor.traits.clear(); // no wound/strength tilts — the title decides
        let mut fresh = char_named("Bob");
        fresh.blood = 1000;
        fresh.statistics.wins = 5;
        let mut champ = char_named("Carol");
        champ.blood = 1000;
        champ.statistics.wins = 8;
        let side = [&fresh, &champ];
        assert_eq!(brain().choose_target(&actor, &side), Some(champ.id));
    }

    /// The wound bonus (15) outranks the champion bump (8) — hunted,
    /// not insta-focused.
    #[test]
    fn wound_outranks_champion_bonus() {
        let mut actor = char_named("Alice");
        actor.traits.clear();
        let mut champ = char_named("Bob");
        champ.blood = 1000;
        champ.statistics.wins = 12;
        let mut hurt = char_named("Carol");
        hurt.blood = 200; // wounded
        hurt.statistics.wins = 3;
        let side = [&champ, &hurt];
        assert_eq!(brain().choose_target(&actor, &side), Some(hurt.id));
    }

    /// Nobody has won a fight: no champion, bonus never engages.
    #[test]
    fn zero_wins_name_no_champion() {
        let mut actor = char_named("Alice");
        actor.traits.clear();
        let a = char_named("Bob");
        let b = char_named("Carol");
        let side = [&a, &b];
        // stable first-wins rule still picks the earlier member (as if
        // the champion bump did not exist — which it does not).
        assert_eq!(brain().choose_target(&actor, &side), Some(a.id));
    }

    #[test]
    fn target_tie_keeps_join_order() {
        let mut actor = char_named("Alice");
        actor.traits.clear();
        let mut first = char_named("Bob");
        first.traits.clear();
        first.attributes.strength = 10;
        let mut second = char_named("Carol");
        second.traits.clear();
        second.attributes.strength = 10;
        let side = [&first, &second];
        assert_eq!(brain().choose_target(&actor, &side), Some(first.id));
    }

    #[test]
    fn aggressive_actor_prefers_strongest() {
        let mut actor = char_named("Alice");
        actor.traits.clear();
        actor.traits.push(Trait::Aggressive);
        let mut weak = char_named("Bob");
        weak.attributes.strength = 1;
        weak.blood = 200;
        let mut strong = char_named("Carol");
        strong.attributes.strength = 50;
        strong.blood = 200; // equal wounds — strength alone decides
        let side = [&weak, &strong];
        assert_eq!(brain().choose_target(&actor, &side), Some(strong.id));
    }

    #[test]
    fn cowardly_actor_prefers_weakest() {
        let mut actor = char_named("Alice");
        actor.traits.clear();
        actor.traits.push(Trait::Cautious);
        let mut weak = char_named("Bob");
        weak.attributes.strength = 1;
        weak.blood = 200;
        let mut strong = char_named("Carol");
        strong.attributes.strength = 50;
        strong.blood = 200;
        let side = [&strong, &weak]; // strongest listed first
        assert_eq!(brain().choose_target(&actor, &side), Some(weak.id));
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

    #[test]
    fn outmatched_with_an_ally_calls_for_backup() {
        let mut a = char_named("Alice");
        a.attributes.strength = 1;
        a.allies.push(Uuid::new_v4()); // presence is all the brain checks
        let mut o = char_named("Bob");
        o.attributes.strength = 50;
        assert_eq!(brain().choose_action(&a, &[&o]), CombatActionKind::CallAlly);
    }

    #[test]
    fn outmatched_without_an_ally_keeps_fighting() {
        let mut a = char_named("Alice");
        a.attributes.strength = 1;
        let mut o = char_named("Bob");
        o.attributes.strength = 50;
        assert_eq!(brain().choose_action(&a, &[&o]), CombatActionKind::Attack);
    }

    #[test]
    fn not_outmatched_never_calls() {
        let mut a = char_named("Alice");
        a.attributes.strength = 10;
        a.allies.push(Uuid::new_v4());
        let mut o = char_named("Bob");
        o.attributes.strength = 10; // `>` is strict: equal strength is not outmatched
        assert_eq!(brain().choose_action(&a, &[&o]), CombatActionKind::Attack);
    }

    #[test]
    fn gang_up_actor_calls_even_when_strongest() {
        // Big team's strongest goblin: not outmatched, no pressure — but
        // the GangUp drive summons anyway. Legal only while ahead; the
        // loop enforces that side rule, the brain never sees side sizes.
        let mut a = char_named("Alice");
        a.attributes.strength = 50;
        a.traits.push(Trait::Aggressive);
        a.allies.push(Uuid::new_v4());
        let mut o = char_named("Bob");
        o.attributes.strength = 1;
        assert_eq!(brain().choose_action(&a, &[&o]), CombatActionKind::CallAlly);
    }
}
