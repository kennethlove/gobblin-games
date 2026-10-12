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
use shared::combat_beat::{
    BeatOutcome, CombatActionKind, CombatBeat, CombatResult, CombatRound,
};
use shared::messages::{CharacterRef, MessagePayload};
use world::messages::TaggedEvent;

use super::resolve::tref;

/// Chooses a combatant's action for a round. The combat brain implements this
/// later; the loop only needs the chosen [`CombatActionKind`].
pub trait CombatActionProvider {
    fn action(&mut self, actor: &Character, opponent: &Character) -> CombatActionKind;
}

/// Placeholder provider: every combatant attacks every round.
///
/// Enough to drive kill / stamina-draw / round-cap termination before the
/// combat brain lands. Flee can still occur via [`CombatRoundLoop::force_action`]
/// for termination tests.
pub struct AttackOnlyProvider;

impl CombatActionProvider for AttackOnlyProvider {
    fn action(&mut self, _actor: &Character, _opponent: &Character) -> CombatActionKind {
        CombatActionKind::Attack
    }
}

/// d20 + speed/stamina/weapon modifiers (spec §3.3). Placeholder numbers live
/// in [`CombatTuning`].
fn initiative_roll(c: &Character, tuning: &CombatTuning, rng: &mut impl Rng) -> i32 {
    let d20: i32 = rng.random_range(1..=tuning.initiative_die as i32);
    // `agility` is the speed proxy (there is no dedicated `speed` attribute).
    let speed_mod = (c.attributes.agility as i32 - 10) / 5;
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

/// State machine driving one fight between two borrowed combatants.
///
/// The game cycle builds this over `split_at_mut` indices and calls
/// [`CombatRoundLoop::advance_round`] once per step until
/// [`CombatRoundLoop::is_over`] is true, emitting the returned beats.
pub struct CombatRoundLoop<'a> {
    first: &'a mut Character,
    second: &'a mut Character,
    combat_id: String,
    round: u32,
    result: Option<CombatResult>,
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
        Self::with_provider(first, second, combat_id, tuning, Box::new(AttackOnlyProvider))
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
        Self {
            first,
            second,
            combat_id,
            round: 0,
            result: None,
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
    /// fewer than two beats if the fight ended mid-round) and appends this
    /// round's message events to `events`.
    pub fn advance_round(&mut self, rng: &mut impl Rng, events: &mut Vec<TaggedEvent>) -> CombatRound {
        if self.result.is_some() {
            return CombatRound {
                number: self.round,
                beats: vec![],
            };
        }

        self.round += 1;
        let round_no = self.round;

        // Initiative decides who acts first; ties keep stable (first-then-second)
        // order so resolution stays deterministic.
        let first_init = initiative_roll(self.first, &self.tuning, rng);
        let second_init = initiative_roll(self.second, &self.tuning, rng);
        let first_goes_first = first_init >= second_init;

        let mut beats: Vec<CombatBeat> = Vec::with_capacity(2);

        // Resolve in initiative order. `order` is the beat position (0/1);
        // `actor_is_first` selects which borrowed character acts.
        for (order, actor_is_first) in [(0u8, first_goes_first), (1u8, !first_goes_first)] {
            if self.result.is_some() {
                break; // fight ended on the prior beat — skip the rest of the round
            }
            self.resolve_one_action(round_no, order, actor_is_first, rng, events, &mut beats);
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

    /// Resolve a single combatant's action for this round, appending its beat
    /// to `beats` and any message events to `events`.
    #[allow(clippy::too_many_arguments)]
    fn resolve_one_action(
        &mut self,
        round_no: u32,
        order: u8,
        actor_is_first: bool,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
        beats: &mut Vec<CombatBeat>,
    ) {
        // Pick the action against the opponent. The provider borrows the two
        // characters immutably; pick which pair based on who acts.
        let action = if actor_is_first {
            let actor = &*self.first;
            let opponent = &*self.second;
            self.provider.action(actor, opponent)
        } else {
            let actor = &*self.second;
            let opponent = &*self.first;
            self.provider.action(actor, opponent)
        };

        match action {
            CombatActionKind::Attack => {
                self.resolve_attack(round_no, order, actor_is_first, rng, events, beats);
            }
            CombatActionKind::Flee => {
                self.resolve_flee(round_no, order, actor_is_first, rng, events, beats);
            }
            // Cower / Spare / Execute and item use are combat-brain territory
            // (kam.2). The default provider never emits them; leave the beat
            // unhandled here so the brain can own the verdict flow.
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_attack(
        &mut self,
        round_no: u32,
        order: u8,
        actor_is_first: bool,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
        beats: &mut Vec<CombatBeat>,
    ) {
        // Scratch buffer so we can re-stamp round/order on the beat `attacks()`
        // emits before forwarding the rest of the swing's events.
        let mut scratch: Vec<TaggedEvent> = Vec::new();

        if actor_is_first {
            let _ = self.first.attacks(
                self.second,
                rng,
                &mut scratch,
                shared::messages::Phase::DAY,
                &self.tuning,
            );
        } else {
            let _ = self.second.attacks(
                self.first,
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

        self.check_termination_after_beat();
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_flee(
        &mut self,
        round_no: u32,
        order: u8,
        actor_is_first: bool,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
        beats: &mut Vec<CombatBeat>,
    ) {
        // Contested: fleer d20 + speed + band vs chaser d20 + speed + band.
        let (fleer, chaser, fleer_is_first) = if actor_is_first {
            (&*self.first, &*self.second, true)
        } else {
            (&*self.second, &*self.first, false)
        };

        let fleer_roll = initiative_roll(fleer, &self.tuning, rng) + self.tuning.flee_contest_modifier;
        let chaser_roll = initiative_roll(chaser, &self.tuning, rng);
        let fled = fleer_roll > chaser_roll;

        let actor_ref: CharacterRef = tref(fleer);
        let target_ref: CharacterRef = tref(chaser);

        let beat = CombatBeat {
            round: round_no,
            order,
            actor: actor_ref.clone(),
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
            let (fled_ref, remaining_ref) = if fleer_is_first {
                (actor_ref, target_ref)
            } else {
                (target_ref, actor_ref)
            };
            self.result = Some(CombatResult::Fled {
                fled: fled_ref,
                remaining: remaining_ref,
            });
        }
    }

    /// Check kill / stamina-draw after an attack beat and set `result` if the
    /// fight ended.
    fn check_termination_after_beat(&mut self) {
        if self.result.is_some() {
            return;
        }
        let first_dead = !self.first.is_alive();
        let second_dead = !self.second.is_alive();
        if first_dead && !second_dead {
            self.result = Some(CombatResult::Winner {
                character: tref(self.second),
            });
        } else if second_dead && !first_dead {
            self.result = Some(CombatResult::Winner {
                character: tref(self.first),
            });
        } else if first_dead && second_dead {
            // Mutual kill — treat as a draw (both down, no survivor).
            self.result = Some(CombatResult::Draw);
        } else if self.first.stamina == 0 && self.second.stamina == 0 {
            // Both stamina-empty with no kill possible: disengage alive (§6).
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

    fn char_named(name: &str) -> Character {
        Character::new(name.to_string(), None, None)
    }

    /// Strong attacker vs near-dead target: the first attack kills, so the fight
    /// must end after one beat and skip the round's second beat.
    #[test]
    fn kill_ends_fight_mid_round_and_skips_second_beat() {
        let mut a = char_named("Alice");
        a.attributes.strength = 100;
        a.attributes.agility = 100;

        let mut b = char_named("Bob");
        b.blood = 1;

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
            Some(CombatResult::Winner { character }) => {
                assert_eq!(character.name, "Alice");
            }
            other => panic!("expected Winner(Alice), got {other:?}"),
        }
    }

    /// Both combatants at zero stamina disengage as a Draw without a survivor.
    #[test]
    fn both_stamina_empty_is_a_draw() {
        let mut a = char_named("Alice");
        a.stamina = 0;
        a.max_stamina = 100;
        a.attributes.strength = 0; // no damage → no kill

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

    /// A successful flee ends the fight with a Fled result and a significant beat.
    #[test]
    fn successful_flee_ends_fight_with_fled_result() {
        let mut a = char_named("Alice");
        a.attributes.agility = 100; // wins the contested flee roll

        let mut b = char_named("Bob");
        b.attributes.agility = 0;

        struct FleeProvider;
        impl CombatActionProvider for FleeProvider {
            fn action(&mut self, _a: &Character, _o: &Character) -> CombatActionKind {
                CombatActionKind::Flee
            }
        }

        let mut rng = SmallRng::seed_from_u64(11);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_provider(
            &mut a,
            &mut b,
            "c3".into(),
            CombatTuning::default(),
            Box::new(FleeProvider),
        );

        let round = combat.advance_round(&mut rng, &mut events);
        assert!(combat.is_over());
        let fled = match combat.result() {
            Some(CombatResult::Fled { fled, remaining }) => {
                assert_eq!(fled.name, "Alice");
                assert_eq!(remaining.name, "Bob");
                true
            }
            _ => false,
        };
        assert!(fled, "expected a Fled result");
        // The fleeing beat is significant.
        assert!(round.beats.iter().any(|b| b.significant));
    }

    /// A failed flee leaves the fight running (no result after one round).
    #[test]
    fn failed_flee_does_not_end_fight() {
        let mut a = char_named("Alice");
        a.attributes.agility = 0; // loses the contested flee roll

        let mut b = char_named("Bob");
        b.attributes.agility = 100;

        // Only Alice flees (and loses); Bob keeps fighting so the fight runs on.
        struct AliceFlees;
        impl CombatActionProvider for AliceFlees {
            fn action(&mut self, a: &Character, _o: &Character) -> CombatActionKind {
                if a.name == "Alice" {
                    CombatActionKind::Flee
                } else {
                    CombatActionKind::Attack
                }
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
        assert_eq!(round.beats.len(), 2, "both actors still act after a failed flee");
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

        let mut tuning = CombatTuning::default();
        tuning.max_rounds = 3;

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
        a.attributes.agility = 40;

        let mut b = char_named("Bob");
        b.attributes.strength = 30;
        b.attributes.agility = 20;
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
}
