//! Multi-round combat: the round-loop state machine (spec 2026-10-09 §3, §6).
//!
//! A fight is a sequence of rounds driven by the game cycle. Each round rolls
//! initiative, resolves one action per combatant in initiative order, and checks
//! for a termination condition after every beat (a kill or successful flee skips
//! the round's remaining beats). The loop is a state machine over the two
//! combatants; the game cycle owns the loop and the surrounding message
//! emission.
//!
//! Action *selection* is delegated to a [`CombatActionProvider`] so the combat
//! brain (a later change) can slot in without touching this loop. The default
//! provider always attacks, which is enough to exercise kill / stamina-draw /
//! round-cap termination before the brain lands.

use crate::Character;
use crate::combat_tuning::CombatTuning;
use crate::stamina_band::stamina_band;
use rand::RngExt;
use rand::prelude::*;
use shared::combat_beat::{BeatOutcome, CombatActionKind, CombatBeat, CombatResult, CombatRound};
use shared::messages::MessagePayload;
use uuid::Uuid;
use world::messages::TaggedEvent;

use super::resolve::tref;
use crate::traits::CharacterAttribute;

/// Chooses a combatant's action for a round against the opposing side.
///
/// The attack target rides along: `None` falls back to the first live
/// opposing member (pair behavior), an id no longer in the fight skips the
/// beat (design 2026-10-20 §8). Flee's defender is computed by the loop
/// (fastest living opponent), not the provider.
pub trait CombatActionProvider {
    fn action(
        &mut self,
        actor: &Character,
        opposing_side: &[&Character],
    ) -> (CombatActionKind, Option<Uuid>);

    /// Forced Spare/Execute verdict when `actor` faces an opponent who has
    /// cowered (spec §7). Defaults to `Execute` — hostile strangers get no
    /// mercy; the combat brain overrides with alliance/trait-aware scoring.
    fn verdict(&mut self, _actor: &Character, cowerer: &Character) -> CombatActionKind {
        let _ = cowerer;
        CombatActionKind::Execute
    }
}

/// Placeholder provider: every combatant attacks every round.
///
/// Enough to drive kill / stamina-draw / round-cap termination before the
/// combat brain lands. Flee can still occur via [`CombatRoundLoop::force_action`]
/// for termination tests.
pub struct AttackOnlyProvider;

impl CombatActionProvider for AttackOnlyProvider {
    fn action(
        &mut self,
        _actor: &Character,
        _side: &[&Character],
    ) -> (CombatActionKind, Option<Uuid>) {
        (CombatActionKind::Attack, None)
    }
}

/// d20 + speed/stamina/weapon modifiers (spec §3.3). Placeholder numbers live
/// in [`CombatTuning`].
fn initiative_roll(c: &Character, tuning: &CombatTuning, rng: &mut impl Rng) -> i32 {
    let d20: i32 = rng.random_range(1..=tuning.initiative_die as i32);
    // `agility` is the speed proxy (there is no dedicated `speed` attribute).
    let speed_mod = (c.effective_attribute(CharacterAttribute::Speed) as i32 - 10) / 5;
    let band_mod = match stamina_band(c.stamina, c.max_stamina, tuning) {
        shared::messages::StaminaBand::Fresh => tuning.initiative_fresh_bonus,
        shared::messages::StaminaBand::Winded => tuning.initiative_winded_bonus,
        shared::messages::StaminaBand::Exhausted => tuning.initiative_exhausted_bonus,
    };
    // Weapon speed: the equipped weapon's `effect`, halved (placeholder divisor).
    let weapon_speed = c
        .items
        .iter()
        .rfind(|i| i.is_weapon() && i.current_durability > 0)
        .map(|w| w.effect / tuning.initiative_weapon_speed_divisor)
        .unwrap_or(0);
    d20 + speed_mod + band_mod + weapon_speed
}

/// Two mutable borrows from the combatant vec, by index (indices differ).
///
/// Outer/inner lifetimes stay separate: the returned borrows only live as
/// long as this borrow of the vec, not as the elements' original lifetime.
fn two_refs<'o>(
    chars: &'o mut [&mut Character],
    i: usize,
    j: usize,
) -> (&'o mut Character, &'o mut Character) {
    if i < j {
        let (left, right) = chars.split_at_mut(j);
        (&mut *left[i], &mut *right[0])
    } else {
        let (left, right) = chars.split_at_mut(i);
        (&mut *right[0], &mut *left[j])
    }
}

/// State machine driving one fight among a set of borrowed combatants.
///
/// Melee v2 substrate (design 2026-10-20 §8): the loop owns the combatants
/// as a set with side membership instead of exactly two borrowed references.
/// A pair fight is the two-entry set `[[0], [1]]`; joins push more later.
/// The game cycle builds this over `split_at_mut` indices and calls
/// [`CombatRoundLoop::advance_round`] once per step until
/// [`CombatRoundLoop::is_over`] is true, emitting the returned beats.
pub struct CombatRoundLoop<'a> {
    /// Combatants, in join order. Taken out during each `advance_round` and
    /// put back before it returns.
    chars: Vec<&'a mut Character>,
    /// Side membership by index into `chars`, in join order. A pair starts
    /// `[[0], [1]]`. Flee and a spare verdict remove the index from its side.
    sides: [Vec<usize>; 2],
    combat_id: String,
    round: u32,
    result: Option<CombatResult>,
    /// Pending cowerers, FIFO by cower order: their beats are suppressed and
    /// the next opposing actor answers the head of the queue with the forced
    /// verdict (design §7).
    cowering: Vec<usize>,
    tuning: CombatTuning,
    provider: Box<dyn CombatActionProvider + 'a>,
}

impl<'a> CombatRoundLoop<'a> {
    /// Start a fight with the default attack-only provider.
    pub fn new(
        first: &'a mut Character,
        second: &'a mut Character,
        combat_id: String,
        tuning: CombatTuning,
    ) -> Self {
        Self::with_provider(
            first,
            second,
            combat_id,
            tuning,
            Box::new(AttackOnlyProvider),
        )
    }

    /// Start a fight with a custom action provider (the combat brain, or a
    /// scripted provider in tests).
    pub fn with_provider(
        first: &'a mut Character,
        second: &'a mut Character,
        combat_id: String,
        tuning: CombatTuning,
        provider: Box<dyn CombatActionProvider + 'a>,
    ) -> Self {
        Self::with_sides(
            vec![first, second],
            [vec![0], vec![1]],
            combat_id,
            tuning,
            provider,
        )
    }

    /// Build a fight over explicit side membership (indices into `chars`,
    /// join order). Tests use it for multi-member sides; joins need it too.
    fn with_sides(
        chars: Vec<&'a mut Character>,
        sides: [Vec<usize>; 2],
        combat_id: String,
        tuning: CombatTuning,
        provider: Box<dyn CombatActionProvider + 'a>,
    ) -> Self {
        Self {
            chars,
            sides,
            combat_id,
            round: 0,
            result: None,
            cowering: Vec::new(),
            tuning,
            provider,
        }
    }

    pub fn combat_id(&self) -> &str {
        &self.combat_id
    }

    pub fn is_over(&self) -> bool {
        self.result.is_some()
    }

    pub fn result(&self) -> Option<&CombatResult> {
        self.result.as_ref()
    }

    /// Advance one round. Returns the resolved [`CombatRound`] (possibly with
    /// fewer beats than combatants if combatants dropped out mid-round) and
    /// appends this round's message events to `events`.
    pub fn advance_round(
        &mut self,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
    ) -> CombatRound {
        if self.result.is_some() {
            return CombatRound {
                number: self.round,
                beats: vec![],
            };
        }
        // Take the combatants out for the round so `self` methods can use
        // tuning/provider/result alongside them; put back before returning.
        let mut chars = std::mem::take(&mut self.chars);
        let round = self.advance_round_with(&mut chars, rng, events);
        self.chars = chars;
        round
    }

    fn advance_round_with(
        &mut self,
        chars: &mut Vec<&'a mut Character>,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
    ) -> CombatRound {
        self.round += 1;
        let round_no = self.round;

        // Initiative decides who acts first; ties keep join order so
        // resolution stays deterministic (and for a pair reproduces the old
        // first-then-second order).
        let mut actives = self.active_indices(chars);
        let rolls: Vec<i32> = actives
            .iter()
            .map(|&i| initiative_roll(&*chars[i], &self.tuning, rng))
            .collect();
        let mut ordered: Vec<(usize, i32)> = actives.into_iter().zip(rolls).collect();
        ordered.sort_by_key(|&(_, roll)| std::cmp::Reverse(roll)); // stable: ties keep join order
        actives = ordered.into_iter().map(|(i, _)| i).collect();

        let mut beats: Vec<CombatBeat> = Vec::with_capacity(actives.len());

        // Resolve in initiative order. `order` is the beat position.
        for (order, &actor_idx) in actives.iter().enumerate() {
            if self.result.is_some() {
                break; // fight ended on the prior beat — skip the rest of the round
            }
            self.resolve_one_action(
                chars,
                round_no,
                order as u8,
                actor_idx,
                rng,
                events,
                &mut beats,
            );
        }

        // Round cap: a pathological stalemate terminates in a Draw (spec §6).
        if self.result.is_none() && self.round >= self.tuning.max_rounds {
            self.result = Some(CombatResult::Draw);
        }

        CombatRound {
            number: round_no,
            beats,
        }
    }

    /// Indices into `chars` still on a side and alive — the combatants whose
    /// beats remain.
    fn active_indices(&self, chars: &[&mut Character]) -> Vec<usize> {
        self.sides[0]
            .iter()
            .chain(self.sides[1].iter())
            .copied()
            .filter(|&i| chars[i].is_alive())
            .collect()
    }

    /// Live members of one side, by index into `chars`.
    fn active_on(&self, side: usize, chars: &[&mut Character]) -> Vec<usize> {
        self.sides[side]
            .iter()
            .copied()
            .filter(|&i| chars[i].is_alive())
            .collect()
    }

    fn side_of(&self, idx: usize) -> usize {
        if self.sides[0].contains(&idx) { 0 } else { 1 }
    }

    /// Resolve a single combatant's action for this round, appending its beat
    /// to `beats` and any message events to `events`.
    #[allow(clippy::too_many_arguments)]
    fn resolve_one_action(
        &mut self,
        chars: &mut Vec<&'a mut Character>,
        round_no: u32,
        order: u8,
        actor_idx: usize,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
        beats: &mut Vec<CombatBeat>,
    ) {
        // Pending cowerer: their own beat is suppressed, the next opposing
        // actor answers with the forced verdict (design §7). Their side-mates
        // keep fighting.
        if self.cowering.contains(&actor_idx) {
            return;
        }
        let actor_side = self.side_of(actor_idx);
        if let Some(&cowerer) = self
            .cowering
            .iter()
            .find(|&&i| self.side_of(i) != actor_side)
        {
            self.resolve_verdict(chars, round_no, order, actor_idx, cowerer, events, beats);
            return;
        }

        // The opposing side, as refs for the provider — never contains a
        // cowerer: the verdict interception above fires first (design §7).
        let opposing = self.active_on(1 - actor_side, chars);
        if opposing.is_empty() {
            // Opposing side already gone — the beat that emptied it set the
            // result; nothing left to do.
            return;
        }
        let (action, target) = {
            let side: Vec<&Character> = opposing.iter().map(|&i| &*chars[i]).collect();
            self.provider.action(&*chars[actor_idx], &side)
        };

        match action {
            CombatActionKind::Attack => {
                // Provider target: `None` falls back to the first live
                // member (pair behavior); an id no longer in the fight
                // skips the beat (design §8).
                let target_idx = match target {
                    Some(id) => match opposing.iter().find(|&&i| chars[i].id == id) {
                        Some(&t) => t,
                        None => return,
                    },
                    None => opposing[0],
                };
                self.resolve_attack(
                    chars, round_no, order, actor_idx, target_idx, rng, events, beats,
                );
            }
            CombatActionKind::Flee => {
                // Contested against the fastest living opponent (design §5);
                // a pair has exactly one.
                let Some(&defender) = opposing
                    .iter()
                    .max_by_key(|&&i| initiative_roll(&*chars[i], &self.tuning, rng))
                else {
                    return;
                };
                self.resolve_flee(
                    chars, round_no, order, actor_idx, defender, rng, events, beats,
                );
            }
            CombatActionKind::UseItem { item } => {
                self.resolve_use_item(chars, round_no, order, actor_idx, item, events, beats);
            }
            CombatActionKind::Cower => {
                // Record the cower and emit its beat; the fight continues until
                // an opposing actor answers with Spare/Execute.
                self.cowering.push(actor_idx);
                let beat = CombatBeat {
                    round: round_no,
                    order,
                    actor: tref(&*chars[actor_idx]),
                    action: CombatActionKind::Cower,
                    target: None,
                    weapon: None,
                    shield: None,
                    wear: vec![],
                    outcome: BeatOutcome::Cowered,
                    significant: true,
                };
                beats.push(beat.clone());
                events.push(TaggedEvent::new(
                    String::new(),
                    MessagePayload::CombatBeat(beat),
                ));
            }
            // Spare/Execute only ever arrive through the forced verdict path.
            CombatActionKind::Spare | CombatActionKind::Execute => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_attack(
        &mut self,
        chars: &mut Vec<&'a mut Character>,
        round_no: u32,
        order: u8,
        actor_idx: usize,
        target_idx: usize,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
        beats: &mut Vec<CombatBeat>,
    ) {
        // Scratch buffer so we can re-stamp round/order on the beat `attacks()`
        // emits before forwarding the rest of the swing's events.
        let mut scratch: Vec<TaggedEvent> = Vec::new();
        {
            let (actor, target) = two_refs(chars, actor_idx, target_idx);
            let _ = actor.attacks(
                target,
                rng,
                &mut scratch,
                shared::messages::Phase::DAY,
                &self.tuning,
            );
        }

        // Re-stamp the emitted beat with this round's number and beat order.
        for ev in scratch.iter_mut() {
            if let MessagePayload::CombatBeat(beat) = &mut ev.payload {
                beat.round = round_no;
                beat.order = order;
                beats.push(beat.clone());
            }
        }
        events.extend(scratch);

        self.check_termination_after_beat(chars);
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_flee(
        &mut self,
        chars: &mut Vec<&'a mut Character>,
        round_no: u32,
        order: u8,
        actor_idx: usize,
        opponent_idx: usize,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
        beats: &mut Vec<CombatBeat>,
    ) {
        // Contested: fleer d20 + speed + band vs the fastest living
        // opponent (a pair has exactly one; design §5).
        let (fleer_roll, chaser_roll) = {
            let (fleer, chaser) = two_refs(chars, actor_idx, opponent_idx);
            (
                initiative_roll(&*fleer, &self.tuning, rng) + self.tuning.flee_contest_modifier,
                initiative_roll(&*chaser, &self.tuning, rng),
            )
        };
        let fled = fleer_roll > chaser_roll;

        let beat = CombatBeat {
            round: round_no,
            order,
            actor: tref(&*chars[actor_idx]),
            action: CombatActionKind::Flee,
            target: None,
            weapon: None,
            shield: None,
            wear: vec![],
            outcome: if fled {
                BeatOutcome::Fled
            } else {
                BeatOutcome::FailedFlee
            },
            significant: fled,
        };
        beats.push(beat.clone());
        events.push(TaggedEvent::new(
            String::new(),
            MessagePayload::CombatBeat(beat),
        ));

        if fled {
            // The fleer leaves the fight — the fight itself only ends when a
            // whole side is gone (design §5/§6). A pair: the other side wins.
            let side = self.side_of(actor_idx);
            self.sides[side].retain(|&i| i != actor_idx);
        }
        self.check_termination_after_beat(chars);
    }

    /// The cowerer's opposing actor answers with the provider's forced
    /// verdict (spec §7, design §7): `Spare` frees the cowerer from the fight
    /// alive, `Execute` kills them.
    #[allow(clippy::too_many_arguments)]
    fn resolve_verdict(
        &mut self,
        chars: &mut Vec<&'a mut Character>,
        round_no: u32,
        order: u8,
        verdict_idx: usize,
        cowerer_idx: usize,
        events: &mut Vec<TaggedEvent>,
        beats: &mut Vec<CombatBeat>,
    ) {
        let verdict = {
            let (actor, cowerer) = two_refs(chars, verdict_idx, cowerer_idx);
            self.provider.verdict(&*actor, &*cowerer)
        };
        // Head of the queue resolved; side-mates that cowered later stay.
        self.cowering.retain(|&i| i != cowerer_idx);

        if matches!(verdict, CombatActionKind::Spare) {
            let (winner, spared) = (tref(&*chars[verdict_idx]), tref(&*chars[cowerer_idx]));
            let beat = CombatBeat {
                round: round_no,
                order,
                actor: winner,
                action: CombatActionKind::Spare,
                target: Some(spared),
                weapon: None,
                shield: None,
                wear: vec![],
                outcome: BeatOutcome::Spared,
                significant: true,
            };
            beats.push(beat.clone());
            events.push(TaggedEvent::new(
                String::new(),
                MessagePayload::CombatBeat(beat),
            ));
            // The spared cowerer leaves the fight alive — their fight, not
            // the whole one (design §7). A pair: the spare wins it.
            let side = self.side_of(cowerer_idx);
            self.sides[side].retain(|&i| i != cowerer_idx);
            self.check_termination_after_beat(chars);
            return;
        }

        // Execute: the verdict kills the cowerer outright. Drain their blood
        // (same observable death state an attack kill leaves behind for the
        // cycle's death processing) and report the kill.
        let (killer_ref, victim_ref, damage) = {
            let (killer, victim) = two_refs(chars, verdict_idx, cowerer_idx);
            let damage = victim.blood;
            victim.blood = 0;
            (tref(&*killer), tref(&*victim), damage)
        };

        events.push(TaggedEvent::new(
            String::new(),
            MessagePayload::CharacterKilled {
                victim: victim_ref.clone(),
                killer: Some(killer_ref.clone()),
                cause: shared::afflictions::DeathCause::Combat,
            },
        ));

        let beat = CombatBeat {
            round: round_no,
            order,
            actor: killer_ref,
            action: CombatActionKind::Execute,
            target: Some(victim_ref),
            weapon: None,
            shield: None,
            wear: vec![],
            outcome: BeatOutcome::Executed { damage },
            significant: true,
        };
        beats.push(beat.clone());
        events.push(TaggedEvent::new(
            String::new(),
            MessagePayload::CombatBeat(beat),
        ));
        self.check_termination_after_beat(chars);
    }

    /// Consume a health/relevant consumable (brain-chosen `UseItem`). A failed
    /// use produces no beat — the actor simply loses their action.
    #[allow(clippy::too_many_arguments)]
    fn resolve_use_item(
        &mut self,
        chars: &mut Vec<&'a mut Character>,
        round_no: u32,
        order: u8,
        actor_idx: usize,
        item: shared::messages::ItemRef,
        events: &mut Vec<TaggedEvent>,
        beats: &mut Vec<CombatBeat>,
    ) {
        let mut scratch: Vec<TaggedEvent> = Vec::new();
        let actor_ref = {
            let actor: &mut Character = &mut *chars[actor_idx];
            let Some(chosen) = actor
                .consumables()
                .into_iter()
                .find(|i| item.identifier == i.identifier)
            else {
                return;
            };
            if actor
                .try_use_consumable(&chosen, &mut scratch, None)
                .is_err()
            {
                return;
            }
            tref(actor)
        };
        events.extend(scratch);

        let beat = CombatBeat {
            round: round_no,
            order,
            actor: actor_ref,
            action: CombatActionKind::UseItem { item },
            target: None,
            weapon: None,
            shield: None,
            wear: vec![],
            outcome: BeatOutcome::ItemUsed,
            significant: false,
        };
        beats.push(beat.clone());
        events.push(TaggedEvent::new(
            String::new(),
            MessagePayload::CombatBeat(beat),
        ));
        self.check_termination_after_beat(chars);
    }

    /// Check side-empty / stamina-draw after a beat and set `result` if the
    /// fight ended (design §6): a side with no living members left loses;
    /// everyone stamina-empty on both sides draws.
    fn check_termination_after_beat(&mut self, chars: &[&mut Character]) {
        if self.result.is_some() {
            return;
        }
        let a = self.active_on(0, chars);
        let b = self.active_on(1, chars);
        if a.is_empty() && b.is_empty() {
            // Both sides gone (e.g. mutual kill) — no survivor.
            self.result = Some(CombatResult::Draw);
        } else if a.is_empty() {
            self.result = Some(CombatResult::Victory {
                victors: b.iter().map(|&i| tref(&*chars[i])).collect(),
            });
        } else if b.is_empty() {
            self.result = Some(CombatResult::Victory {
                victors: a.iter().map(|&i| tref(&*chars[i])).collect(),
            });
        } else if a.iter().all(|&i| chars[i].stamina == 0)
            && b.iter().all(|&i| chars[i].stamina == 0)
        {
            // Both sides stamina-empty with no kill possible: disengage (§6).
            self.result = Some(CombatResult::Draw);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Character;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    /// Deterministic character for combat tests. `Character::new` rolls
    /// traits/brain from the global entropy RNG, which made round-loop
    /// tests flaky under the full workspace run — seed from the name so
    /// every construction is reproducible.
    fn char_named(name: &str) -> Character {
        use std::hash::{Hash, Hasher};
        let seed = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            name.hash(&mut h);
            h.finish()
        };
        let mut rng = SmallRng::seed_from_u64(seed);
        Character::new_with_rng(name.to_string(), None, None, &mut rng)
    }

    /// Strong attacker vs near-dead target: the first attack kills, so the fight
    /// must end after one beat and skip the round's second beat.
    #[test]
    fn kill_ends_fight_mid_round_and_skips_second_beat() {
        let mut a = char_named("Alice");
        a.attributes.strength = 100;
        a.attributes.speed = 100;

        let mut b = char_named("Bob");
        b.blood = 1;
        // Pin initiative: Alice must act first so the kill skips beat 2.
        // (Default agility left this test flaky on d20 variance.)
        b.attributes.speed = 0;
        b.stamina = 1; // Exhausted band -> initiative -2, always behind Alice

        let mut rng = SmallRng::seed_from_u64(7);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::new(&mut a, &mut b, "c1".into(), CombatTuning::default());

        let round = combat.advance_round(&mut rng, &mut events);

        // At most one beat: the kill skipped the second actor.
        assert!(
            round.beats.len() <= 1,
            "expected <= 1 beat after a mid-round kill, got {}",
            round.beats.len()
        );
        assert!(combat.is_over());
        match combat.result() {
            Some(CombatResult::Victory { victors }) => {
                assert_eq!(victors.len(), 1);
                assert_eq!(victors[0].name, "Alice");
            }
            other => panic!("expected Victory(Alice), got {other:?}"),
        }
    }

    /// Both combatants at zero stamina disengage as a Draw without a survivor.
    #[test]
    fn both_stamina_empty_is_a_draw() {
        let mut a = char_named("Alice");
        a.stamina = 0;
        a.max_stamina = 100;
        a.attributes.strength = 0; // no damage -> no kill

        let mut b = char_named("Bob");
        b.stamina = 0;
        b.max_stamina = 100;
        b.attributes.strength = 0;

        let mut rng = SmallRng::seed_from_u64(3);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::new(&mut a, &mut b, "c2".into(), CombatTuning::default());

        let _ = combat.advance_round(&mut rng, &mut events);
        assert!(combat.is_over());
        assert_eq!(combat.result(), Some(&CombatResult::Draw));
    }

    /// A successful flee exits the fleer from the fight; in a pair that ends
    /// it with the opponent standing and a significant beat.
    #[test]
    fn successful_flee_exits_fleer_and_opponent_stands() {
        let mut a = char_named("Alice");
        a.attributes.speed = 100; // wins the contested flee roll

        let mut b = char_named("Bob");
        b.attributes.speed = 0;

        struct FleeProvider;
        impl CombatActionProvider for FleeProvider {
            fn action(
                &mut self,
                _a: &Character,
                _side: &[&Character],
            ) -> (CombatActionKind, Option<Uuid>) {
                (CombatActionKind::Flee, None)
            }
        }

        let mut rng = SmallRng::seed_from_u64(11);
        let mut events = Vec::new();
        let bob = tref(&b);
        let mut combat = CombatRoundLoop::with_provider(
            &mut a,
            &mut b,
            "c3".into(),
            CombatTuning::default(),
            Box::new(FleeProvider),
        );

        let round = combat.advance_round(&mut rng, &mut events);
        assert!(combat.is_over());
        let result = combat.result().cloned();
        drop(combat);
        assert_eq!(
            result,
            Some(CombatResult::Victory { victors: vec![bob] }),
            "fleer exits; Bob stands"
        );
        // The fleeing beat is significant.
        assert!(round.beats.iter().any(|b| b.significant));
    }

    /// A failed flee leaves the fight running (no result after one round).
    #[test]
    fn failed_flee_does_not_end_fight() {
        let mut a = char_named("Alice");
        a.attributes.speed = 0; // loses the contested flee roll

        let mut b = char_named("Bob");
        b.attributes.speed = 100;
        // `Attributes::new()` (inside `new_with_rng`) still rolls from the
        // global entropy RNG; pin strength so Bob can't randomly one-shot
        // Alice and end the fight before the flee assertion.
        b.attributes.strength = 0;

        // Only Alice flees (and loses); Bob keeps fighting so the fight runs on.
        struct AliceFlees;
        impl CombatActionProvider for AliceFlees {
            fn action(
                &mut self,
                a: &Character,
                _side: &[&Character],
            ) -> (CombatActionKind, Option<Uuid>) {
                let kind = if a.name == "Alice" {
                    CombatActionKind::Flee
                } else {
                    CombatActionKind::Attack
                };
                (kind, None)
            }
        }

        let mut rng = SmallRng::seed_from_u64(5);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_provider(
            &mut a,
            &mut b,
            "c4".into(),
            CombatTuning::default(),
            Box::new(AliceFlees),
        );

        let round = combat.advance_round(&mut rng, &mut events);
        assert!(!combat.is_over(), "failed flee must not end the fight");
        assert_eq!(
            round.beats.len(),
            2,
            "both actors still act after a failed flee"
        );
    }

    /// Reaching the round cap terminates a stalemate in a Draw.
    #[test]
    fn round_cap_terminates_stalemate_in_draw() {
        let mut a = char_named("Alice");
        a.attributes.strength = 0; // never kills
        a.stamina = 100;
        a.max_stamina = 1000; // never stamina-exhausted

        let mut b = char_named("Bob");
        b.attributes.strength = 0;
        b.stamina = 100;
        b.max_stamina = 1000;

        let tuning = CombatTuning {
            max_rounds: 3,
            ..CombatTuning::default()
        };

        let mut rng = SmallRng::seed_from_u64(9);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::new(&mut a, &mut b, "c5".into(), tuning);

        for _ in 0..3 {
            if combat.is_over() {
                break;
            }
            let _ = combat.advance_round(&mut rng, &mut events);
        }
        assert!(combat.is_over(), "round cap must end the fight");
        assert_eq!(combat.result(), Some(&CombatResult::Draw));
    }

    /// A seeded multi-round fight runs to completion with monotonically ordered
    /// rounds and no beats after the fight ended.
    #[test]
    fn seeded_fight_runs_to_completion_with_ordered_rounds() {
        let mut a = char_named("Alice");
        a.attributes.strength = 30;
        a.attributes.speed = 40;

        let mut b = char_named("Bob");
        b.attributes.strength = 30;
        b.attributes.speed = 20;
        b.blood = 60;

        let mut rng = SmallRng::seed_from_u64(4242);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::new(&mut a, &mut b, "c6".into(), CombatTuning::default());

        let mut last_round = 0u32;
        let mut total_beats = 0usize;
        while !combat.is_over() {
            let round = combat.advance_round(&mut rng, &mut events);
            assert!(
                round.number > last_round,
                "round numbers must increase: {} then {}",
                last_round,
                round.number
            );
            last_round = round.number;
            total_beats += round.beats.len();
            // Beats carry this round's number and valid order positions.
            for (i, beat) in round.beats.iter().enumerate() {
                assert_eq!(beat.round, round.number);
                assert!(beat.order <= 1);
                let _ = i;
            }
        }
        assert!(combat.is_over());
        assert!(total_beats > 0, "fight must produce at least one beat");
        // Emitted events must not reference a beat from a later round than the
        // final resolved round.
        for ev in &events {
            if let MessagePayload::CombatBeat(beat) = &ev.payload {
                assert!(beat.round <= last_round);
            }
        }
    }

    // --- Cower / verdict flow (spec §7) ---

    /// Alice (initiative-first) cowers; Bob's same-round verdict spares her.
    /// Both beats land in round 1 and the fight ends with both alive.
    #[test]
    fn cower_then_spare_produces_both_beats_and_spared_result() {
        struct AliceCowers;
        impl CombatActionProvider for AliceCowers {
            fn action(
                &mut self,
                a: &Character,
                _side: &[&Character],
            ) -> (CombatActionKind, Option<Uuid>) {
                let kind = if a.name == "Alice" {
                    CombatActionKind::Cower
                } else {
                    CombatActionKind::Attack
                };
                (kind, None)
            }
            fn verdict(&mut self, _a: &Character, _c: &Character) -> CombatActionKind {
                CombatActionKind::Spare
            }
        }

        let mut a = char_named("Alice");
        a.attributes.speed = 100;
        let mut b = char_named("Bob");
        b.attributes.speed = 0;
        let bob = tref(&b);

        let mut rng = SmallRng::seed_from_u64(13);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_provider(
            &mut a,
            &mut b,
            "c7".into(),
            CombatTuning::default(),
            Box::new(AliceCowers),
        );

        let round = combat.advance_round(&mut rng, &mut events);
        assert!(combat.is_over());
        let result = combat.result().cloned();
        drop(combat);
        assert_eq!(
            result,
            Some(CombatResult::Victory { victors: vec![bob] }),
            "spare ends the cowerer's fight; Bob stands"
        );
        assert_eq!(round.beats.len(), 2);
        assert_eq!(round.beats[0].action, CombatActionKind::Cower);
        assert_eq!(round.beats[0].outcome, BeatOutcome::Cowered);
        assert!(round.beats[0].significant);
        assert_eq!(round.beats[1].action, CombatActionKind::Spare);
        assert_eq!(round.beats[1].outcome, BeatOutcome::Spared);
        assert!(a.is_alive(), "spared means alive");
    }

    /// Default verdict (no brain override) executes the cowerer: kill beat,
    /// victim down to zero blood, Winner result.
    #[test]
    fn cower_then_execute_kills_and_ends_fight() {
        struct AliceCowers;
        impl CombatActionProvider for AliceCowers {
            fn action(
                &mut self,
                a: &Character,
                _side: &[&Character],
            ) -> (CombatActionKind, Option<Uuid>) {
                let kind = if a.name == "Alice" {
                    CombatActionKind::Cower
                } else {
                    CombatActionKind::Attack
                };
                (kind, None)
            }
        }

        let mut a = char_named("Alice");
        a.attributes.speed = 100;
        let mut b = char_named("Bob");
        b.attributes.speed = 0;
        let bob = tref(&b);

        let mut rng = SmallRng::seed_from_u64(17);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_provider(
            &mut a,
            &mut b,
            "c8".into(),
            CombatTuning::default(),
            Box::new(AliceCowers),
        );

        let round = combat.advance_round(&mut rng, &mut events);
        assert!(combat.is_over());
        let result = combat.result().cloned();
        drop(combat);
        assert_eq!(result, Some(CombatResult::Victory { victors: vec![bob] }));
        assert_eq!(round.beats.len(), 2);
        assert_eq!(round.beats[1].action, CombatActionKind::Execute);
        assert_eq!(
            round.beats[1].outcome,
            BeatOutcome::Executed { damage: 1000 }
        );
        assert_eq!(a.blood, 0, "executed means dead");
        assert!(
            events.iter().any(|ev| matches!(
                ev.payload,
                MessagePayload::CharacterKilled { ref killer, .. }
                    if killer.as_ref().is_some_and(|k| k.name == "Bob")
            )),
            "execution must report the kill"
        );
    }

    /// The verdict comes on the opponent's next action — even when that's in
    /// the following round — and the cowerer never acts (or re-cowers) between.
    #[test]
    fn verdict_arrives_on_opponents_next_action_even_across_rounds() {
        struct AliceCowersBobAttacks;
        impl CombatActionProvider for AliceCowersBobAttacks {
            fn action(
                &mut self,
                a: &Character,
                _side: &[&Character],
            ) -> (CombatActionKind, Option<Uuid>) {
                let kind = if a.name == "Alice" {
                    CombatActionKind::Cower
                } else {
                    CombatActionKind::Attack
                };
                (kind, None)
            }
            fn verdict(&mut self, _a: &Character, _c: &Character) -> CombatActionKind {
                CombatActionKind::Spare
            }
        }

        let mut a = char_named("Alice");
        a.attributes.speed = 0;
        a.attributes.strength = 0;
        let mut b = char_named("Bob");
        b.attributes.speed = 100;
        b.attributes.strength = 0;

        let mut rng = SmallRng::seed_from_u64(23);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_provider(
            &mut a,
            &mut b,
            "c9".into(),
            CombatTuning::default(),
            Box::new(AliceCowersBobAttacks),
        );

        let r1 = combat.advance_round(&mut rng, &mut events);
        assert!(!combat.is_over(), "cower is not terminal");
        assert_eq!(
            r1.beats.last().map(|b| &b.action),
            Some(&CombatActionKind::Cower),
            "round 1 ends with Alice's cower"
        );
        let r2 = combat.advance_round(&mut rng, &mut events);
        assert!(combat.is_over());
        assert_eq!(r2.beats.len(), 1, "only Bob's verdict beat in round 2");
        assert_eq!(r2.beats[0].action, CombatActionKind::Spare);
        let cower_beats = events
            .iter()
            .filter(|ev| {
                matches!(&ev.payload, MessagePayload::CombatBeat(b)
                    if b.action == CombatActionKind::Cower)
            })
            .count();
        assert_eq!(cower_beats, 1, "never a repeat cower beat");
    }

    /// Across many seeds (initiative order can flip between rounds), a cowerer
    /// never emits a second Cower beat before the verdict resolves.
    #[test]
    fn cowerer_never_recowers_across_seeds() {
        struct AliceCowers;
        impl CombatActionProvider for AliceCowers {
            fn action(
                &mut self,
                a: &Character,
                _side: &[&Character],
            ) -> (CombatActionKind, Option<Uuid>) {
                let kind = if a.name == "Alice" {
                    CombatActionKind::Cower
                } else {
                    CombatActionKind::Attack
                };
                (kind, None)
            }
            fn verdict(&mut self, _a: &Character, _c: &Character) -> CombatActionKind {
                CombatActionKind::Spare
            }
        }

        for seed in 0..64 {
            let mut a = char_named("Alice");
            a.attributes.speed = 5;
            a.attributes.strength = 0;
            let mut b = char_named("Bob");
            b.attributes.speed = 5;
            b.attributes.strength = 0;

            let mut rng = SmallRng::seed_from_u64(seed);
            let mut events = Vec::new();
            let mut combat = CombatRoundLoop::with_provider(
                &mut a,
                &mut b,
                format!("c10-{seed}"),
                CombatTuning::default(),
                Box::new(AliceCowers),
            );

            let mut beats = 0;
            while !combat.is_over() {
                beats += combat.advance_round(&mut rng, &mut events).beats.len();
            }
            assert!(
                matches!(combat.result(), Some(CombatResult::Victory { .. })),
                "seed {seed}: spared"
            );
            assert!(
                beats <= 3,
                "seed {seed}: attack? + cower + verdict, got {beats}"
            );
            let cowers = events
                .iter()
                .filter(|ev| {
                    matches!(&ev.payload, MessagePayload::CombatBeat(b)
                        if b.action == CombatActionKind::Cower)
                })
                .count();
            let verdicts = events
                .iter()
                .filter(|ev| {
                    matches!(&ev.payload, MessagePayload::CombatBeat(b)
                        if b.action == CombatActionKind::Spare)
                })
                .count();
            assert_eq!(cowers, 1, "seed {seed}: exactly one cower beat");
            assert_eq!(verdicts, 1, "seed {seed}: exactly one verdict beat");
        }
    }

    // --- Use Item / brain-driven flee ---

    /// Wounded Alice with a health potion heals via the brain on her first
    /// beat; the ItemUsed beat lands and her blood rises.
    #[test]
    fn use_item_beat_heals_via_brain() {
        use crate::brains::combat::CombatBrain;
        use world::items::{Item, ItemRarity, ItemType};

        let mut a = char_named("Alice");
        a.blood = 300;
        a.attributes.speed = 100;
        a.items.push(Item::new(
            "potion",
            ItemType::Consumable,
            ItemRarity::Common,
            10,
            world::items::Attribute::Health,
            200,
        ));
        let mut b = char_named("Bob");
        b.attributes.speed = 0;
        b.attributes.strength = 0; // no damage while Alice heals

        let mut rng = SmallRng::seed_from_u64(29);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_provider(
            &mut a,
            &mut b,
            "c11".into(),
            CombatTuning::default(),
            Box::new(CombatBrain::new(CombatTuning::default())),
        );

        let round = combat.advance_round(&mut rng, &mut events);
        drop(combat);
        assert!(
            matches!(round.beats[0].action, CombatActionKind::UseItem { .. }),
            "expected UseItem beat, got {:?}",
            round.beats[0].action
        );
        assert_eq!(round.beats[0].outcome, BeatOutcome::ItemUsed);
        assert!(a.blood > 300, "healed: {}", a.blood);
    }

    /// A wounded, exhausted goblin flees through the brain (not a scripted
    /// provider) and, with the agility advantage, escapes.
    #[test]
    fn wounded_exhausted_brain_flees_and_escapes() {
        use crate::brains::combat::CombatBrain;

        let mut a = char_named("Alice");
        a.blood = 300;
        a.stamina = 10; // Exhausted
        a.attributes.speed = 100;
        let mut b = char_named("Bob");
        b.attributes.speed = 0;
        b.attributes.strength = 0; // can't land damage; Alice's flee decides
        let bob = tref(&b);

        let mut rng = SmallRng::seed_from_u64(31);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_provider(
            &mut a,
            &mut b,
            "c12".into(),
            CombatTuning::default(),
            Box::new(CombatBrain::new(CombatTuning::default())),
        );

        // Flee is now a real contest (tuned negative modifier); Alice's agility
        // edge makes her re-attempt each round until she escapes. Bounded so a
        // pathological RNG can't hang the test.
        for _ in 0..20 {
            if combat.is_over() {
                break;
            }
            combat.advance_round(&mut rng, &mut events);
        }
        let result = combat.result().cloned();
        drop(combat);
        assert_eq!(
            result,
            Some(CombatResult::Victory { victors: vec![bob] }),
            "fleer exits; Bob stands"
        );
    }

    /// Tuning-pass distribution sanity (seeded, deliberately loose bands —
    /// these guard the *shape* of the tuned defaults, not exact numbers).
    /// Reference distributions measured with `cargo run --example
    /// tuning_harness -- 4000`: winner 76.5% / fled 20.7% / draw 2.8%,
    /// rounds 2-6 66.8%, flee success 25.9%, initiative near-even.
    #[test]
    fn tuned_defaults_produce_sane_fight_distributions() {
        use crate::brains::combat::CombatBrain;
        use shared::combat_beat::CombatActionKind;
        use shared::messages::MessagePayload;

        let tuning = CombatTuning::default();
        const N: usize = 400;
        let mut draws = 0usize;
        let mut fled_fights = 0usize;
        let mut rounds_2_6 = 0usize;
        let mut flee_attempts = 0usize;
        let mut flee_successes = 0usize;
        let mut alice_initiative = 0usize;

        for seed in 0..N as u64 {
            let mut rng = SmallRng::seed_from_u64(seed);
            let mut a = char_named("Alice");
            let mut b = char_named("Bob");
            a.traits.clear();
            a.items.clear();
            b.traits.clear();
            b.items.clear();
            // Roll per-fight attributes from this seed so the distribution
            // samples a population of fighters, not one fixed name-hash
            // matchup (char_named seeds are deterministic per name).
            use crate::Attributes;
            a.attributes = Attributes::from_rng(&mut rng);
            b.attributes = Attributes::from_rng(&mut rng);

            let mut events = Vec::new();
            let mut combat = CombatRoundLoop::with_provider(
                &mut a,
                &mut b,
                format!("c{seed}"),
                tuning.clone(),
                Box::new(CombatBrain::new(tuning.clone())),
            );

            let mut rounds = 0u32;
            while !combat.is_over() {
                rounds += 1;
                combat.advance_round(&mut rng, &mut events);
            }
            if (2..=6).contains(&rounds) {
                rounds_2_6 += 1;
            }
            let fight_fled = events.iter().any(|ev| {
                matches!(&ev.payload, MessagePayload::CombatBeat(b)
                    if matches!(b.outcome, BeatOutcome::Fled))
            });
            if fight_fled {
                fled_fights += 1;
            }
            if let Some(CombatResult::Draw) = combat.result() {
                draws += 1;
            }
            for ev in &events {
                if let MessagePayload::CombatBeat(beat) = &ev.payload {
                    if matches!(beat.action, CombatActionKind::Flee) {
                        flee_attempts += 1;
                        if matches!(beat.outcome, shared::combat_beat::BeatOutcome::Fled) {
                            flee_successes += 1;
                        }
                    }
                    if beat.round == 1 && beat.order == 0 && beat.actor.name == "Alice" {
                        alice_initiative += 1;
                    }
                }
            }
        }

        let draws_pct = draws * 100 / N;
        assert!(draws_pct < 10, "draws must stay rare: {draws_pct}%");

        let rounds_pct = rounds_2_6 * 100 / N;
        assert!(
            rounds_pct >= 50,
            "most fights should run 2-6 rounds: {rounds_pct}%"
        );

        // Flee must be meaningful but not dominant: some fights end in flee,
        // and roughly a quarter of attempts actually escape (loose 10-40% band).
        assert!(
            fled_fights > 0,
            "flee should end a meaningful share of fights"
        );
        assert!(
            fled_fights * 100 / N <= 40,
            "flee must not dominate: {}%",
            fled_fights * 100 / N
        );
        if flee_attempts >= 20 {
            let success_pct = flee_successes * 100 / flee_attempts;
            assert!(
                (10..=40).contains(&success_pct),
                "flee success band: {success_pct}%"
            );
        }

        // Initiative should stay near-even for equal fighters (loose 40-60% band).
        let init_pct = alice_initiative * 100 / N;
        assert!(
            (40..=60).contains(&init_pct),
            "initiative should be near-even: {init_pct}%"
        );
    }

    // --- Melee v2: side targeting & the cower queue (design §7-§8) ---

    /// A scripted explicit target is honored: Bob's fallback would hit the
    /// first-listed member (Alice), but he hits Carol.
    #[test]
    fn explicit_target_is_honored() {
        struct TargetByName;
        impl CombatActionProvider for TargetByName {
            fn action(
                &mut self,
                a: &Character,
                side: &[&Character],
            ) -> (CombatActionKind, Option<Uuid>) {
                if a.name == "Bob" {
                    let carol = side
                        .iter()
                        .find(|c| c.name == "Carol")
                        .expect("Carol on the opposing side");
                    (CombatActionKind::Attack, Some(carol.id))
                } else {
                    (CombatActionKind::Attack, None)
                }
            }
        }

        let mut a = char_named("Alice");
        a.attributes.strength = 0;
        let mut b = char_named("Bob");
        b.attributes.strength = 0;
        let mut c = char_named("Carol");
        c.attributes.strength = 0;

        let mut rng = SmallRng::seed_from_u64(41);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            vec![&mut a, &mut b, &mut c],
            // Bob alone vs Alice+Carol: his fallback would be Alice.
            [vec![1], vec![0, 2]],
            "t1".into(),
            CombatTuning {
                max_rounds: 1,
                ..CombatTuning::default()
            },
            Box::new(TargetByName),
        );

        let round = combat.advance_round(&mut rng, &mut events);
        assert!(
            round
                .beats
                .iter()
                .any(|beat| beat.actor.name == "Bob"
                    && matches!(beat.action, CombatActionKind::Attack)),
            "Bob must land an attack"
        );
        for beat in round.beats.iter().filter(|beat| beat.actor.name == "Bob") {
            if matches!(beat.action, CombatActionKind::Attack) {
                assert_eq!(
                    beat.target.as_ref().map(|t| t.name.as_str()),
                    Some("Carol"),
                    "Bob's chosen target must be honored"
                );
            }
        }
    }

    /// An id nobody in the fight owns skips the beat; the fight runs on.
    #[test]
    fn stale_target_skips_the_beat() {
        struct AliceHitsNobody;
        impl CombatActionProvider for AliceHitsNobody {
            fn action(
                &mut self,
                a: &Character,
                _side: &[&Character],
            ) -> (CombatActionKind, Option<Uuid>) {
                if a.name == "Alice" {
                    (CombatActionKind::Attack, Some(Uuid::nil()))
                } else {
                    (CombatActionKind::Attack, None)
                }
            }
        }

        let mut a = char_named("Alice");
        a.attributes.strength = 0;
        let mut b = char_named("Bob");
        b.attributes.strength = 0;

        let mut rng = SmallRng::seed_from_u64(43);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_provider(
            &mut a,
            &mut b,
            "t2".into(),
            CombatTuning::default(),
            Box::new(AliceHitsNobody),
        );

        let round = combat.advance_round(&mut rng, &mut events);
        assert!(
            !round.beats.iter().any(|beat| beat.actor.name == "Alice"),
            "stale target skips Alice's beat"
        );
        assert!(
            round.beats.iter().any(|beat| beat.actor.name == "Bob"),
            "Bob still acts"
        );
        assert!(!combat.is_over(), "a skipped beat is not terminal");
    }

    /// A cower only stops the cowerer: their side-mate keeps attacking and
    /// the opposing actor answers with the verdict (design §7).
    #[test]
    fn side_mates_keep_fighting_while_mate_cowers() {
        struct AliceCowersOthersFight;
        impl CombatActionProvider for AliceCowersOthersFight {
            fn action(
                &mut self,
                a: &Character,
                _side: &[&Character],
            ) -> (CombatActionKind, Option<Uuid>) {
                if a.name == "Alice" {
                    (CombatActionKind::Cower, None)
                } else {
                    (CombatActionKind::Attack, None)
                }
            }
            fn verdict(&mut self, _a: &Character, _c: &Character) -> CombatActionKind {
                CombatActionKind::Spare
            }
        }

        let mut a = char_named("Alice");
        a.attributes.speed = 100;
        a.attributes.strength = 0;
        let mut d = char_named("Dave");
        d.attributes.speed = 100;
        d.attributes.strength = 0;
        let mut b = char_named("Bob");
        b.attributes.speed = 0;
        b.attributes.strength = 0;

        let mut rng = SmallRng::seed_from_u64(47);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            vec![&mut a, &mut d, &mut b],
            [vec![0, 1], vec![2]],
            "t3".into(),
            CombatTuning::default(),
            Box::new(AliceCowersOthersFight),
        );

        let round = combat.advance_round(&mut rng, &mut events);
        assert!(
            round
                .beats
                .iter()
                .any(|beat| beat.actor.name == "Alice" && beat.action == CombatActionKind::Cower),
            "Alice cowers"
        );
        assert!(
            round
                .beats
                .iter()
                .any(|beat| beat.actor.name == "Dave" && beat.action == CombatActionKind::Attack),
            "Dave (side-mate of the cowerer) keeps fighting"
        );
        assert!(
            round.beats.iter().any(|beat| beat.actor.name == "Bob"
                && beat.action == CombatActionKind::Spare
                && beat.target.as_ref().is_some_and(|t| t.name == "Alice")),
            "Bob's verdict spares Alice"
        );
    }

    /// Two cowerers are resolved FIFO by cower order — one verdict per turn,
    /// queue order preserved across rounds.
    #[test]
    fn multiple_cowerers_get_verdicts_in_cower_order() {
        struct AliceDaveCower;
        impl CombatActionProvider for AliceDaveCower {
            fn action(
                &mut self,
                a: &Character,
                _side: &[&Character],
            ) -> (CombatActionKind, Option<Uuid>) {
                if a.name == "Bob" {
                    (CombatActionKind::Attack, None)
                } else {
                    (CombatActionKind::Cower, None)
                }
            }
            fn verdict(&mut self, _a: &Character, _c: &Character) -> CombatActionKind {
                CombatActionKind::Spare
            }
        }

        let mut a = char_named("Alice");
        a.attributes.speed = 100;
        a.attributes.strength = 0;
        let mut d = char_named("Dave");
        d.attributes.speed = 0; // Alice (speed 100) always cowers first
        d.attributes.strength = 0;
        let mut b = char_named("Bob");
        b.attributes.speed = 0;
        b.attributes.strength = 0;
        let bob = tref(&b);

        let mut rng = SmallRng::seed_from_u64(53);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            vec![&mut a, &mut d, &mut b],
            [vec![0, 1], vec![2]],
            "t4".into(),
            CombatTuning::default(),
            Box::new(AliceDaveCower),
        );

        let r1 = combat.advance_round(&mut rng, &mut events);
        assert!(!combat.is_over(), "first verdict only frees Alice");
        let r2 = combat.advance_round(&mut rng, &mut events);
        assert!(combat.is_over());
        let spare_targets: Vec<&str> = r1
            .beats
            .iter()
            .chain(r2.beats.iter())
            .filter(|beat| beat.action == CombatActionKind::Spare)
            .filter_map(|beat| beat.target.as_ref().map(|t| t.name.as_str()))
            .collect();
        assert_eq!(spare_targets, ["Alice", "Dave"], "FIFO cower order");
        let result = combat.result().cloned();
        drop(combat);
        assert_eq!(result, Some(CombatResult::Victory { victors: vec![bob] }));
    }
}
