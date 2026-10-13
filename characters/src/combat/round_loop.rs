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
use shared::messages::{CharacterRef, MessagePayload};
use world::messages::TaggedEvent;

use super::resolve::tref;
use crate::traits::CharacterAttribute;

/// Chooses a combatant's action for a round. The combat brain implements this
/// later; the loop only needs the chosen [`CombatActionKind`].
pub trait CombatActionProvider {
    fn action(&mut self, actor: &Character, opponent: &Character) -> CombatActionKind;

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
    fn action(&mut self, _actor: &Character, _opponent: &Character) -> CombatActionKind {
        CombatActionKind::Attack
    }
}

/// d20 + speed/stamina/weapon modifiers (spec §3.3). Placeholder numbers live
/// in [`CombatTuning`].
fn initiative_roll(c: &Character, tuning: &CombatTuning, rng: &mut impl Rng) -> i32 {
    let d20: i32 = rng.random_range(1..=tuning.initiative_die as i32);
    // `agility` is the speed proxy (there is no dedicated `speed` attribute).
    let speed_mod = (c.effective_attribute(CharacterAttribute::Agility) as i32 - 10) / 5;
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
    /// Set once a combatant cowers: `Some(true)` when the first (initiative-
    /// side) character is the cowerer. The cowerer's later beats are
    /// suppressed; the opponent's next action resolves as a forced verdict.
    cowering: Option<bool>,
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
        Self {
            first,
            second,
            combat_id,
            round: 0,
            result: None,
            cowering: None,
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
        // Once someone has cowered: suppress their beats and turn the
        // opponent's next action into the forced verdict (spec §7).
        if let Some(cowerer_is_first) = self.cowering {
            if actor_is_first == cowerer_is_first {
                return; // already begging — no repeat cower beat
            }
            self.resolve_verdict(round_no, order, actor_is_first, events, beats);
            return;
        }

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
            CombatActionKind::UseItem { item } => {
                self.resolve_use_item(round_no, order, actor_is_first, item, events, beats);
            }
            CombatActionKind::Cower => {
                // Record the cower and emit its beat; the fight continues until
                // the opponent answers with Spare/Execute.
                self.cowering = Some(actor_is_first);
                let actor_ref = if actor_is_first {
                    tref(self.first)
                } else {
                    tref(self.second)
                };
                let beat = CombatBeat {
                    round: round_no,
                    order,
                    actor: actor_ref,
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

        let fleer_roll =
            initiative_roll(fleer, &self.tuning, rng) + self.tuning.flee_contest_modifier;
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

    /// The cowerer's opponent answers with the provider's forced verdict
    /// (spec §7): `Spare` ends the fight with both alive, `Execute` kills the
    /// cowerer and ends it with a winner.
    fn resolve_verdict(
        &mut self,
        round_no: u32,
        order: u8,
        actor_is_first: bool,
        events: &mut Vec<TaggedEvent>,
        beats: &mut Vec<CombatBeat>,
    ) {
        let verdict = if actor_is_first {
            self.provider.verdict(&*self.first, &*self.second)
        } else {
            self.provider.verdict(&*self.second, &*self.first)
        };

        if matches!(verdict, CombatActionKind::Spare) {
            let (winner, spared) = if actor_is_first {
                (tref(self.first), tref(self.second))
            } else {
                (tref(self.second), tref(self.first))
            };
            let beat = CombatBeat {
                round: round_no,
                order,
                actor: winner.clone(),
                action: CombatActionKind::Spare,
                target: Some(spared.clone()),
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
            self.result = Some(CombatResult::Spared { winner, spared });
            return;
        }

        // Execute: the verdict kills the cowerer outright. Drain their blood
        // (same observable death state an attack kill leaves behind for the
        // cycle's death processing) and report the kill.
        let (killer, victim) = if actor_is_first {
            (&*self.first, &mut *self.second)
        } else {
            (&*self.second, &mut *self.first)
        };
        let (killer_ref, victim_ref) = (tref(killer), tref(victim));
        let damage = victim.blood;
        victim.blood = 0;

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
            actor: killer_ref.clone(),
            action: CombatActionKind::Execute,
            target: Some(victim_ref.clone()),
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
        self.result = Some(CombatResult::Winner {
            character: killer_ref,
        });
    }

    /// Consume a health/relevant consumable (brain-chosen `UseItem`). A failed
    /// use produces no beat — the actor simply loses their action.
    fn resolve_use_item(
        &mut self,
        round_no: u32,
        order: u8,
        actor_is_first: bool,
        item: shared::messages::ItemRef,
        events: &mut Vec<TaggedEvent>,
        beats: &mut Vec<CombatBeat>,
    ) {
        let actor: &mut Character = if actor_is_first {
            &mut *self.first
        } else {
            &mut *self.second
        };
        let Some(chosen) = actor
            .consumables()
            .into_iter()
            .find(|i| item.identifier == i.identifier)
        else {
            return;
        };

        let mut scratch: Vec<TaggedEvent> = Vec::new();
        if actor
            .try_use_consumable(&chosen, &mut scratch, None)
            .is_err()
        {
            return;
        }
        events.extend(scratch);

        let beat = CombatBeat {
            round: round_no,
            order,
            actor: tref(actor),
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
        self.check_termination_after_beat();
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
        a.attributes.agility = 100;

        let mut b = char_named("Bob");
        b.blood = 1;
        // Pin initiative: Alice must act first so the kill skips beat 2.
        // (Default agility left this test flaky on d20 variance.)
        b.attributes.agility = 0;
        b.stamina = 1; // Exhausted band → initiative −2, always behind Alice

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
        // `Attributes::new()` (inside `new_with_rng`) still rolls from the
        // global entropy RNG; pin strength so Bob can't randomly one-shot
        // Alice and end the fight before the flee assertion.
        b.attributes.strength = 0;

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

    // --- Cower / verdict flow (spec §7) ---

    /// Alice (initiative-first) cowers; Bob's same-round verdict spares her.
    /// Both beats land in round 1 and the fight ends with both alive.
    #[test]
    fn cower_then_spare_produces_both_beats_and_spared_result() {
        struct AliceCowers;
        impl CombatActionProvider for AliceCowers {
            fn action(&mut self, a: &Character, _o: &Character) -> CombatActionKind {
                if a.name == "Alice" {
                    CombatActionKind::Cower
                } else {
                    CombatActionKind::Attack
                }
            }
            fn verdict(&mut self, _a: &Character, _c: &Character) -> CombatActionKind {
                CombatActionKind::Spare
            }
        }

        let mut a = char_named("Alice");
        a.attributes.agility = 100;
        let mut b = char_named("Bob");
        b.attributes.agility = 0;
        let (alice, bob) = (tref(&a), tref(&b));

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
            Some(CombatResult::Spared {
                winner: bob,
                spared: alice,
            })
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
            fn action(&mut self, a: &Character, _o: &Character) -> CombatActionKind {
                if a.name == "Alice" {
                    CombatActionKind::Cower
                } else {
                    CombatActionKind::Attack
                }
            }
        }

        let mut a = char_named("Alice");
        a.attributes.agility = 100;
        let mut b = char_named("Bob");
        b.attributes.agility = 0;
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
        assert_eq!(result, Some(CombatResult::Winner { character: bob }));
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
            fn action(&mut self, a: &Character, _o: &Character) -> CombatActionKind {
                if a.name == "Alice" {
                    CombatActionKind::Cower
                } else {
                    CombatActionKind::Attack
                }
            }
            fn verdict(&mut self, _a: &Character, _c: &Character) -> CombatActionKind {
                CombatActionKind::Spare
            }
        }

        let mut a = char_named("Alice");
        a.attributes.agility = 0;
        a.attributes.strength = 0;
        let mut b = char_named("Bob");
        b.attributes.agility = 100;
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
            fn action(&mut self, a: &Character, _o: &Character) -> CombatActionKind {
                if a.name == "Alice" {
                    CombatActionKind::Cower
                } else {
                    CombatActionKind::Attack
                }
            }
            fn verdict(&mut self, _a: &Character, _c: &Character) -> CombatActionKind {
                CombatActionKind::Spare
            }
        }

        for seed in 0..64 {
            let mut a = char_named("Alice");
            a.attributes.agility = 5;
            a.attributes.strength = 0;
            let mut b = char_named("Bob");
            b.attributes.agility = 5;
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
                matches!(combat.result(), Some(CombatResult::Spared { .. })),
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
        a.attributes.agility = 100;
        a.items.push(Item::new(
            "potion",
            ItemType::Consumable,
            ItemRarity::Common,
            10,
            world::items::Attribute::Health,
            200,
        ));
        let mut b = char_named("Bob");
        b.attributes.agility = 0;
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
        a.attributes.agility = 100;
        let mut b = char_named("Bob");
        b.attributes.agility = 0;
        b.attributes.strength = 0; // can't land damage; Alice's flee decides
        let (alice, bob) = (tref(&a), tref(&b));

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
            Some(CombatResult::Fled {
                fled: alice,
                remaining: bob,
            })
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

            struct Brain(CombatBrain);
            impl CombatActionProvider for Brain {
                fn action(&mut self, actor: &Character, opponent: &Character) -> CombatActionKind {
                    self.0.action(actor, opponent)
                }
                fn verdict(&mut self, actor: &Character, cowerer: &Character) -> CombatActionKind {
                    self.0.verdict(actor, cowerer)
                }
            }

            let mut events = Vec::new();
            let mut combat = CombatRoundLoop::with_provider(
                &mut a,
                &mut b,
                format!("c{seed}"),
                tuning.clone(),
                Box::new(Brain(CombatBrain::new(tuning.clone()))),
            );

            let mut rounds = 0u32;
            while !combat.is_over() {
                rounds += 1;
                combat.advance_round(&mut rng, &mut events);
            }
            if (2..=6).contains(&rounds) {
                rounds_2_6 += 1;
            }
            match combat.result() {
                Some(CombatResult::Draw) => draws += 1,
                Some(CombatResult::Fled { .. }) => fled_fights += 1,
                _ => {}
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
}
