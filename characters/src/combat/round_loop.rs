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

/// Symmetric-ally check (design §7): alliances are stored one way per
/// character (cap `MAX_ALLIES`), so a missing
/// record on one side must not read as "not allied".
fn is_ally(a: &Character, b: &Character) -> bool {
    a.allies.contains(&b.id) || b.allies.contains(&a.id)
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
    /// Indices that left the fight (fled or spared): never active, and the
    /// round-boundary scan will not re-admit them (design §5/§7).
    exited: Vec<usize>,
    tuning: CombatTuning,
    provider: Box<dyn CombatActionProvider + 'a>,
    /// Arena layout for the one-hop neighborhood (ring + door).
    layout: Vec<(areas::AreaId, areas::hex::Axial)>,
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
            areas::hex::default_layout().to_vec(),
            vec![first, second],
            [vec![0], vec![1]],
            combat_id,
            tuning,
            provider,
        )
    }

    /// Build a fight over explicit side membership (indices into `chars`,
    /// join order). Tests use it for multi-member sides; the cycle passes
    /// the whole world so the round-boundary scan can see join candidates.
    pub fn with_sides(
        layout: Vec<(areas::AreaId, areas::hex::Axial)>,
        chars: Vec<&'a mut Character>,
        sides: [Vec<usize>; 2],
        combat_id: String,
        tuning: CombatTuning,
        provider: Box<dyn CombatActionProvider + 'a>,
    ) -> Self {
        Self {
            layout,
            chars,
            sides,
            combat_id,
            round: 0,
            result: None,
            cowering: Vec::new(),
            exited: Vec::new(),
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

    /// §14 (7si): the ambush opener — the entry swing resolves before round 1
    /// while the target still sleeps; round 1 then rolls initiative normally.
    /// The beat carries round 0 (it rides `events`, same as any round's beats).
    /// `resolve_attack` runs the termination check, so an opener kill settles
    /// the fight here and [`Self::advance_round`] never starts. No-op when the
    /// target is awake — an awake fight opens with round 1 as before.
    pub fn resolve_opener(&mut self, rng: &mut impl Rng, events: &mut Vec<TaggedEvent>) {
        // Entry order is fixed by the cycle: sides[0] = attacker, sides[1] = target.
        let Some(&target) = self.sides[1].first() else {
            return;
        };
        let Some(&attacker) = self.sides[0].first() else {
            return;
        };
        if !self.chars[target].sleeping {
            return;
        }
        let mut chars = std::mem::take(&mut self.chars);
        let mut beats = Vec::new();
        self.resolve_attack(&mut chars, 0, 0, attacker, target, rng, events, &mut beats);
        self.chars = chars;
    }

    fn advance_round_with(
        &mut self,
        chars: &mut Vec<&'a mut Character>,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
    ) -> CombatRound {
        // Round boundary first (design §8): joiners are admitted before
        // initiative so they beat in the round starting right now — the
        // "next round" from the round they joined at.
        self.scan_joins(chars, events);
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

    /// Round-boundary join scan (design §8): a living character outside the
    /// fight, in the fight's sub-tile, allied to exactly one side joins that
    /// side; an ally of both sides sits out. Capacity blocks joins, never
    /// kicks members. A stranger may still throw in uninvited via
    /// [`Self::self_invite_side`] (ck7); anyone else waits for the public
    /// [`CombatRoundLoop::join_member`] hook.
    fn scan_joins(&mut self, chars: &[&mut Character], events: &mut Vec<TaggedEvent>) {
        let total = self.sides[0].len() + self.sides[1].len();
        if total >= self.tuning.max_combatants {
            return;
        }
        let Some(&home) = self.sides[0].first().or(self.sides[1].first()) else {
            return; // nobody in the fight — nothing to anchor a sub-tile to
        };
        let home_at = chars[home].location;
        for idx in 0..chars.len() {
            if self.sides[0].contains(&idx)
                || self.sides[1].contains(&idx)
                || self.exited.contains(&idx)
            {
                continue;
            }
            let cand = &*chars[idx];
            if !cand.is_alive() || cand.location != home_at {
                continue;
            }
            let on0 = self.sides[0].iter().any(|&m| is_ally(&*chars[m], cand));
            let on1 = self.sides[1].iter().any(|&m| is_ally(&*chars[m], cand));
            let side = match (on0, on1) {
                (true, true) => continue, // ambiguous ally sits out (design §4.1)
                (true, false) => 0,
                (false, true) => 1,
                // Uninvited bystander (ck7): the side falls out of the
                // fighter they pick on; `None` means they sit this round out.
                (false, false) => match self.self_invite_side(cand, chars) {
                    Some(side) => side,
                    None => continue,
                },
            };
            self.add_join(idx, side, chars, events);
            if self.sides[0].len() + self.sides[1].len() >= self.tuning.max_combatants {
                break;
            }
        }
    }

    /// Which side an uninvited bystander throws in with (ck7): nobody
    /// vouches for them, so the side falls out of the fighter they pick a
    /// fight with — the strongest living fighter, the way an aggressive
    /// actor scores targets (design §3 "side opposite their chosen target",
    /// §4.3). `None` when they don't join:
    ///
    /// - only bullies invite themselves (`AttackPosture`); calm bystanders
    ///   watch,
    /// - a strictly ahead side never grows from a self-invite: the GangUp
    ///   size rule (PR #91) lets the *drive* summon allies onto a winning
    ///   side, but a volunteer with no sponsor is exactly the opportunistic
    ///   pile-on that rule keeps rare — the drive cannot reach it,
    /// - nobody living left to pick on means no target, hence no side.
    fn self_invite_side(&self, cand: &Character, chars: &[&mut Character]) -> Option<usize> {
        if crate::traits::behavior_sum(&cand.traits, crate::traits::BehaviorAxis::AttackPosture)
            <= 0
        {
            return None;
        }
        // Chosen target: the strongest living fighter; strength ties keep
        // join order (strict `>` walking both sides in join order).
        let members = self.sides[0].iter().chain(self.sides[1].iter()).copied();
        let mut target: Option<usize> = None;
        let mut best: u32 = 0;
        for i in members {
            if !chars[i].is_alive() {
                continue;
            }
            let strength = chars[i].effective_attribute(CharacterAttribute::Strength);
            if target.is_none() || strength > best {
                target = Some(i);
                best = strength;
            }
        }
        let target = target?;
        let side = 1 - self.side_of(target);
        if self.sides[side].len() > self.sides[1 - side].len() {
            return None;
        }
        Some(side)
    }

    /// Public join hook (design §4.1/§12.3): put pool index `idx` on
    /// `side` for the round starting next — a joined ally goes on their
    /// sponsor's side, an uninvited fighter on the side opposite their
    /// target. Callers own that choice; the hook owns capacity and
    /// idempotence. Call between rounds (`advance_round` not running):
    /// `idx` indexes the pool the loop was built with — the cycle's
    /// `self.characters` order.
    pub fn join_member(&mut self, idx: usize, side: usize, events: &mut Vec<TaggedEvent>) {
        let Some(character) = self.chars.get(idx).map(|c| tref(c)) else {
            return; // not in the pool — nothing to admit
        };
        if self.sides[0].contains(&idx)
            || self.sides[1].contains(&idx)
            || self.exited.contains(&idx)
            || self.sides[0].len() + self.sides[1].len() >= self.tuning.max_combatants
        {
            return;
        }
        self.sides[side].push(idx);
        events.push(TaggedEvent::new(
            format!("{} joins the fight!", character.name),
            MessagePayload::CombatJoined {
                combat_id: self.combat_id.clone(),
                character,
                side: format!("side-{}", side + 1),
                // Boundary semantics match the scan's.
                round: self.round + 1,
            },
        ));
    }

    fn add_join(
        &mut self,
        idx: usize,
        side: usize,
        chars: &[&mut Character],
        events: &mut Vec<TaggedEvent>,
    ) {
        if self.sides[0].contains(&idx)
            || self.sides[1].contains(&idx)
            || self.exited.contains(&idx)
            || self.sides[0].len() + self.sides[1].len() >= self.tuning.max_combatants
        {
            return;
        }
        self.sides[side].push(idx);
        let joined = tref(&*chars[idx]);
        let line = format!("{} joins the fight!", joined.name);
        events.push(TaggedEvent::new(
            line,
            MessagePayload::CombatJoined {
                combat_id: self.combat_id.clone(),
                character: joined,
                side: format!("side-{}", side + 1),
                // Scan runs before the round increments; an external caller
                // sits at the boundary. Both mean "round starting next".
                round: self.round + 1,
            },
        ));
    }

    /// Who a `CallAlly` can actually bring in: an alive ally of the caller
    /// within one sub-hop — `sub_neighbors` of the caller's own triangle
    /// (same-area ring neighbors plus the cross-area door). The round scan
    /// covers the caller's own triangle at the round top, so only the
    /// neighborhood is the call's job; anything further needs a walk, not
    /// a shout (wpe).
    fn find_call_target(&self, actor_idx: usize, chars: &[&'a mut Character]) -> Option<usize> {
        if self.sides[0].len() + self.sides[1].len() >= self.tuning.max_combatants {
            return None;
        }
        let actor = &*chars[actor_idx];
        let near: Vec<areas::location::Location> =
            areas::location::sub_neighbors(&self.layout, actor.location)
                .into_iter()
                .flatten()
                .collect();
        (0..chars.len()).find(|&i| {
            i != actor_idx
                && !self.sides[0].contains(&i)
                && !self.sides[1].contains(&i)
                && !self.exited.contains(&i)
                && chars[i].is_alive()
                && near.contains(&chars[i].location)
                && is_ally(actor, &*chars[i])
        })
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
        let (mut action, mut target) = {
            let side: Vec<&Character> = opposing.iter().map(|&i| &*chars[i]).collect();
            self.provider.action(&*chars[actor_idx], &side)
        };
        // A call only works when an ally can arrive; otherwise it is
        // satisfied (allies already fighting) and the beat carries on.
        let mut called: Option<usize> = None;
        if matches!(action, CombatActionKind::CallAlly) {
            // Gangs are legal but uncommon: when ahead, only the GangUp
            // drive may summon (behind or tied, anyone may call).
            let side = self.side_of(actor_idx);
            let ahead = self.sides[side].len() > self.sides[1 - side].len();
            if ahead
                && crate::traits::behavior_sum(
                    &chars[actor_idx].traits,
                    crate::traits::BehaviorAxis::GangUp,
                ) <= 0
            {
                action = CombatActionKind::Attack;
                target = None;
            } else if let Some(idx) = self.find_call_target(actor_idx, chars) {
                // The ally runs over: relocate them onto the caller's own
                // triangle — members share the fight's location (pair
                // members start together; called allies land here), so
                // the world state after the fight reads true.
                let at = chars[actor_idx].location;
                chars[idx].location = at;
                self.add_join(idx, side, chars, events);
                called = Some(idx);
            } else {
                action = CombatActionKind::Attack;
                target = None;
            }
        }

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
            CombatActionKind::CallAlly => {
                // The join is already recorded (CombatJoined above); emit the
                // beat naming who is coming — they beat from the next round
                // (the initiative list for this one is fixed).
                let Some(called) = called else {
                    return;
                };
                let beat = CombatBeat {
                    round: round_no,
                    order,
                    actor: tref(&*chars[actor_idx]),
                    action: CombatActionKind::CallAlly,
                    target: Some(tref(&*chars[called])),
                    weapon: None,
                    shield: None,
                    wear: vec![],
                    outcome: BeatOutcome::CalledAlly,
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
            self.exited.push(actor_idx); // stays out — cycle relocates them only post-fight
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
            self.exited.push(cowerer_idx);
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
    use crate::traits::Trait;
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
            areas::hex::default_layout().to_vec(),
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
            areas::hex::default_layout().to_vec(),
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
            areas::hex::default_layout().to_vec(),
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

    // --- Melee v2: round-boundary joins (design §8) ---

    /// `(name, side, round)` for every join in this run.
    fn joined(events: &[TaggedEvent]) -> Vec<(String, String, u32)> {
        events
            .iter()
            .filter_map(|ev| match &ev.payload {
                MessagePayload::CombatJoined {
                    character,
                    side,
                    round,
                    ..
                } => Some((character.name.clone(), side.clone(), *round)),
                _ => None,
            })
            .collect()
    }

    /// An ally of a fighter, in the same sub-tile, joins at the boundary and
    /// beats in the round that starts there (design §8).
    #[test]
    fn ally_same_subtile_joins_and_gets_a_beat() {
        let mut a = char_named("Alice");
        a.attributes.strength = 0;
        let mut b = char_named("Bob");
        b.attributes.strength = 0;
        let mut c = char_named("Carol");
        c.attributes.strength = 0;
        a.allies = vec![c.id];

        let mut rng = SmallRng::seed_from_u64(7);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b, &mut c],
            [vec![0], vec![1]],
            "t3".into(),
            CombatTuning {
                max_rounds: 1,
                ..CombatTuning::default()
            },
            Box::new(AttackOnlyProvider),
        );

        let round = combat.advance_round(&mut rng, &mut events);
        assert_eq!(
            joined(&events),
            vec![("Carol".into(), "side-1".into(), 1)],
            "the joiner lands on their ally's side for round 1"
        );
        assert!(
            round.beats.iter().any(|beat| beat.actor.name == "Carol"),
            "the joiner must fight in the round that follows the boundary"
        );
    }

    /// Capacity is a join-time cap: once the sides hold `max_combatants`,
    /// the scan admits nobody (design §8c/§10).
    #[test]
    fn capacity_blocks_the_join_scan() {
        let mut a = char_named("Alice");
        let mut b = char_named("Bob");
        let mut c = char_named("Carol");
        c.attributes.strength = 0;
        a.allies = vec![c.id];

        let mut rng = SmallRng::seed_from_u64(8);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b, &mut c],
            [vec![0], vec![1]],
            "t4".into(),
            CombatTuning {
                max_combatants: 2,
                ..CombatTuning::default()
            },
            Box::new(AttackOnlyProvider),
        );

        let round = combat.advance_round(&mut rng, &mut events);
        assert!(joined(&events).is_empty(), "the cap holds: nobody joins");
        assert!(
            !round.beats.iter().any(|beat| beat.actor.name == "Carol"),
            "a blocked joiner never beats"
        );
    }

    /// An ally of BOTH sides sits out (design §4.1); a stranger to both
    /// waits for the public hook instead of self-inviting.
    #[test]
    fn ambiguous_and_unrelated_neighbours_sit_out() {
        // (ambiguous: allied to Alice and Bob)
        let mut a = char_named("Alice");
        let mut b = char_named("Bob");
        let mut c = char_named("Carol");
        c.attributes.strength = 0;
        a.allies = vec![c.id];
        b.allies = vec![c.id];

        let mut rng = SmallRng::seed_from_u64(11);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b, &mut c],
            [vec![0], vec![1]],
            "t5".into(),
            CombatTuning {
                max_rounds: 1,
                ..CombatTuning::default()
            },
            Box::new(AttackOnlyProvider),
        );
        combat.advance_round(&mut rng, &mut events);
        assert!(
            joined(&events).is_empty(),
            "an ambiguous ally must not pick a side"
        );

        // (calm stranger: allied to neither fighter, no bully streak)
        let mut a = char_named("Alice");
        let mut b = char_named("Bob");
        let mut c = char_named("Carol");
        c.attributes.strength = 0;
        c.traits.clear(); // no AttackPosture: bystander, not intervener
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b, &mut c],
            [vec![0], vec![1]],
            "t5b".into(),
            CombatTuning {
                max_rounds: 1,
                ..CombatTuning::default()
            },
            Box::new(AttackOnlyProvider),
        );
        let mut rng = SmallRng::seed_from_u64(12);
        combat.advance_round(&mut rng, &mut events);
        assert!(
            joined(&events).is_empty(),
            "a calm stranger sits out; only bullies self-invite (ck7)"
        );
    }

    /// An ally in a different sub-tile is out of reach of the scan (§8).
    #[test]
    fn ally_in_another_subtile_does_not_join() {
        let mut a = char_named("Alice");
        let mut b = char_named("Bob");
        let mut c = char_named("Carol");
        c.location = areas::location::Location::center(areas::AreaId::sector1());
        c.attributes.strength = 0;
        a.allies = vec![c.id];

        let mut rng = SmallRng::seed_from_u64(13);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b, &mut c],
            [vec![0], vec![1]],
            "t6".into(),
            CombatTuning {
                max_rounds: 1,
                ..CombatTuning::default()
            },
            Box::new(AttackOnlyProvider),
        );
        combat.advance_round(&mut rng, &mut events);
        assert!(joined(&events).is_empty(), "distance blocks the scan");
    }

    /// The public hook admits a pool member between rounds (design §12.3);
    /// they beat in the round starting next.
    #[test]
    fn join_member_hook_admits_between_rounds() {
        let mut a = char_named("Alice");
        a.attributes.strength = 0;
        let mut b = char_named("Bob");
        b.attributes.strength = 0;
        let mut c = char_named("Carol");
        c.attributes.strength = 0;

        let mut rng = SmallRng::seed_from_u64(17);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b, &mut c],
            [vec![0], vec![1]],
            "t7".into(),
            CombatTuning::default(),
            Box::new(AttackOnlyProvider),
        );

        combat.advance_round(&mut rng, &mut events);
        assert!(joined(&events).is_empty(), "no ally to auto-join yet");

        combat.join_member(2, 0, &mut events);
        assert_eq!(
            joined(&events),
            vec![("Carol".into(), "side-1".into(), 2)],
            "the hook admits Carol for round 2"
        );

        let round2 = combat.advance_round(&mut rng, &mut events);
        assert!(
            round2.beats.iter().any(|beat| beat.actor.name == "Carol"),
            "the hooked-in member fights in round 2"
        );
    }

    /// Scripted: Alice calls for backup, everyone else attacks.
    struct AliceCalls;
    impl CombatActionProvider for AliceCalls {
        fn action(
            &mut self,
            a: &Character,
            _side: &[&Character],
        ) -> (CombatActionKind, Option<Uuid>) {
            if a.name == "Alice" {
                (CombatActionKind::CallAlly, None)
            } else {
                (CombatActionKind::Attack, None)
            }
        }
    }

    #[test]
    fn call_ally_brings_a_neighbor_in() {
        let mut a = char_named("Alice");
        a.location = areas::location::Location::center(areas::AreaId::hub());
        a.attributes.strength = 0;
        a.attributes.speed = 100;
        let mut b = char_named("Bob");
        b.attributes.strength = 0;
        b.attributes.speed = 0;
        let mut c = char_named("Carol");
        c.attributes.strength = 0;
        c.attributes.speed = 0;
        // One sub-hop from Alice: her first occupied neighbor triangle (the
        // round scan only covers her own triangle, so the call must).
        c.location = areas::location::sub_neighbors(&areas::hex::default_layout(), a.location)
            .into_iter()
            .flatten()
            .next()
            .expect("Alice's triangle has neighbors");
        a.allies = vec![c.id];

        let mut rng = SmallRng::seed_from_u64(19);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b, &mut c],
            [vec![0], vec![1]],
            "t8".into(),
            CombatTuning::default(),
            Box::new(AliceCalls),
        );

        let round1 = combat.advance_round(&mut rng, &mut events);
        assert!(
            round1.beats.iter().any(|beat| {
                beat.action == CombatActionKind::CallAlly
                    && beat.target.as_ref().is_some_and(|t| t.name == "Carol")
            }),
            "Alice's call beat names Carol: {:?}",
            round1.beats.iter().map(|b| &b.action).collect::<Vec<_>>()
        );
        assert_eq!(
            joined(&events),
            vec![("Carol".into(), "side-1".into(), 2)],
            "mid-round join: round 1's initiative is fixed, she fights from round 2"
        );

        let round2 = combat.advance_round(&mut rng, &mut events);
        assert!(
            round2.beats.iter().any(|beat| beat.actor.name == "Carol"),
            "the called ally beats from round 2"
        );
        drop(combat);
        assert_eq!(c.location, a.location, "the called ally ran to the fight");
    }

    #[test]
    fn call_without_an_arriving_ally_falls_back_to_attack() {
        let mut a = char_named("Alice");
        a.attributes.strength = 0;
        a.attributes.speed = 100;
        let mut b = char_named("Bob");
        b.attributes.strength = 0;
        b.attributes.speed = 0;

        let mut rng = SmallRng::seed_from_u64(21);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b],
            [vec![0], vec![1]],
            "t9".into(),
            CombatTuning::default(),
            Box::new(AliceCalls),
        );

        let round1 = combat.advance_round(&mut rng, &mut events);
        assert!(
            round1
                .beats
                .iter()
                .any(|beat| beat.action == CombatActionKind::Attack && beat.actor.name == "Alice"),
            "nobody to call: Alice swings instead"
        );
        assert!(
            joined(&events).is_empty(),
            "no CombatJoined without an arriving ally"
        );
    }

    #[test]
    fn call_ignores_allies_beyond_one_hop() {
        let mut a = char_named("Alice");
        a.location = areas::location::Location::center(areas::AreaId::hub());
        a.attributes.strength = 0;
        a.attributes.speed = 100;
        let mut b = char_named("Bob");
        b.attributes.strength = 0;
        b.attributes.speed = 0;
        let mut c = char_named("Carol");
        c.location = areas::location::Location::center(areas::AreaId::sector3());
        c.attributes.strength = 0;
        c.attributes.speed = 0;
        a.allies = vec![c.id]; // an ally, but past the one-hop reach

        let mut rng = SmallRng::seed_from_u64(23);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b, &mut c],
            [vec![0], vec![1]],
            "t10".into(),
            CombatTuning::default(),
            Box::new(AliceCalls),
        );

        let round1 = combat.advance_round(&mut rng, &mut events);
        assert!(
            round1
                .beats
                .iter()
                .all(|beat| beat.action != CombatActionKind::CallAlly),
            "Carol is out of shouting range: no call beat"
        );
        assert!(
            round1
                .beats
                .iter()
                .any(|beat| beat.action == CombatActionKind::Attack && beat.actor.name == "Alice"),
            "Alice swings instead"
        );
        assert!(joined(&events).is_empty(), "Carol never joins");
    }

    #[test]
    fn bigger_side_needs_gang_up_to_summon() {
        // 2v1 and ahead: plain goblin's call falls back to a swing.
        let mut a = char_named("Alice");
        a.attributes.strength = 0;
        a.attributes.speed = 100;
        a.allies.push(Uuid::new_v4()); // a third ally exists somewhere
        let mut d = char_named("Dave");
        d.attributes.strength = 0;
        d.attributes.speed = 90;
        let mut b = char_named("Bob");
        b.attributes.strength = 0;
        b.attributes.speed = 0;

        let mut rng = SmallRng::seed_from_u64(29);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut d, &mut b],
            [vec![0, 1], vec![2]],
            "t11".into(),
            CombatTuning::default(),
            Box::new(AliceCalls),
        );

        let round1 = combat.advance_round(&mut rng, &mut events);
        assert!(
            round1.beats.iter().any(|beat| {
                beat.action == CombatActionKind::Attack && beat.actor.name == "Alice"
            }),
            "ahead and unganged: Alice swings instead of summoning"
        );
        assert!(
            joined(&events).is_empty(),
            "no join while ahead without the GangUp drive"
        );
    }

    #[test]
    fn ganger_on_the_bigger_side_summons() {
        // Same 2v1, Alice carries the drive (Aggressive's GangUp row): legal.
        let mut a = char_named("Alice");
        a.attributes.strength = 0;
        a.attributes.speed = 100;
        a.traits.push(Trait::Aggressive);
        // The ally must sit OUTSIDE the fight — members are never summon
        // candidates — and in this fight's pool (find scans the pool).
        let mut c = char_named("Carol");
        c.attributes.strength = 0;
        // One sub-hop away: the round scan covers only Alice's own triangle,
        // so an in-triangle ally would already be a member by round top.
        c.location = areas::location::sub_neighbors(&areas::hex::default_layout(), a.location)
            .into_iter()
            .flatten()
            .next()
            .expect("Alice's triangle has neighbors");
        a.allies = vec![c.id];
        let mut d = char_named("Dave");
        d.attributes.strength = 0;
        d.attributes.speed = 90;
        let mut b = char_named("Bob");
        b.attributes.strength = 0;
        b.attributes.speed = 0;

        let mut rng = SmallRng::seed_from_u64(31);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut d, &mut b, &mut c],
            [vec![0, 1], vec![2]],
            "t12".into(),
            CombatTuning::default(),
            Box::new(AliceCalls),
        );

        let round1 = combat.advance_round(&mut rng, &mut events);
        assert!(
            round1.beats.iter().any(|beat| {
                beat.action == CombatActionKind::CallAlly
                    && beat.target.as_ref().is_some_and(|t| t.name == "Carol")
            }),
            "ahead but ganged: the summon stands: {:?}",
            round1.beats.iter().map(|b| &b.action).collect::<Vec<_>>()
        );
        assert_eq!(
            joined(&events),
            vec![("Carol".into(), "side-1".into(), 2)],
            "mid-round join: round 1's initiative is fixed, Carol fights from round 2"
        );
    }

    // --- Uninvited third parties (ck7): bully bystanders self-invite ---

    /// A bully stranger in the fight's sub-tile picks the strongest fighter
    /// as their target and joins the side opposing them, fighting from the
    /// round they join (design §3 "side opposite their chosen target").
    #[test]
    fn bully_stranger_self_invites_against_the_strongest() {
        let mut a = char_named("Alice");
        a.attributes.strength = 50; // the strongest fighter in the room
        let mut b = char_named("Bob");
        b.attributes.strength = 0;
        let mut c = char_named("Carol");
        c.attributes.strength = 0;
        c.traits.clear();
        c.traits.push(Trait::Aggressive); // AttackPosture: the bully gate

        let mut rng = SmallRng::seed_from_u64(37);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b, &mut c],
            [vec![0], vec![1]],
            "t13".into(),
            CombatTuning {
                max_rounds: 1,
                ..CombatTuning::default()
            },
            Box::new(AttackOnlyProvider),
        );

        let round = combat.advance_round(&mut rng, &mut events);
        assert_eq!(
            joined(&events),
            vec![("Carol".into(), "side-2".into(), 1)],
            "uninvited: no sponsor, so Carol lands opposite her target Alice"
        );
        assert!(
            round.beats.iter().any(|beat| beat.actor.name == "Carol"),
            "the intervener fights in the round that follows the boundary"
        );
    }

    /// The GangUp size rule reaches self-invites too: a bully whose chosen
    /// side is strictly ahead sits out — a volunteer with no sponsor never
    /// piles onto the winning side (PR #91's invariant, minus the drive).
    #[test]
    fn size_rule_blocks_a_self_invite_onto_the_ahead_side() {
        let mut a = char_named("Alice");
        a.attributes.strength = 50; // strongest: Carol would pick her
        let mut b = char_named("Bob");
        b.attributes.strength = 0;
        let mut carl = char_named("Carl");
        carl.attributes.strength = 0;
        let mut e = char_named("Eve");
        e.attributes.strength = 0;
        e.traits.clear();
        e.traits.push(Trait::Aggressive);

        let mut rng = SmallRng::seed_from_u64(41);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b, &mut carl, &mut e],
            [vec![0], vec![1, 2]],
            "t14".into(),
            CombatTuning {
                max_rounds: 1,
                ..CombatTuning::default()
            },
            Box::new(AttackOnlyProvider),
        );

        combat.advance_round(&mut rng, &mut events);
        assert!(
            joined(&events).is_empty(),
            "Bob+Carl already outnumber Alice: the pile-on stays illegal"
        );
    }

    /// An aggressive ALLY is still an ally first: the sponsor's side wins
    /// over the self-invite's target rule.
    #[test]
    fn bully_ally_joins_their_sponsors_side_not_the_self_invite_side() {
        let mut a = char_named("Alice");
        a.attributes.strength = 50; // strongest: self-invite would say side-2
        let mut b = char_named("Bob");
        b.attributes.strength = 0;
        let mut c = char_named("Carol");
        c.attributes.strength = 0;
        c.traits.clear();
        c.traits.push(Trait::Aggressive);
        a.allies = vec![c.id]; // but Alice sponsored her first

        let mut rng = SmallRng::seed_from_u64(43);
        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b, &mut c],
            [vec![0], vec![1]],
            "t15".into(),
            CombatTuning {
                max_rounds: 1,
                ..CombatTuning::default()
            },
            Box::new(AttackOnlyProvider),
        );

        combat.advance_round(&mut rng, &mut events);
        assert_eq!(
            joined(&events),
            vec![("Carol".into(), "side-1".into(), 1)],
            "alliance outranks the bully rule: Carol lands with her ally"
        );
    }

    /// §14 (7si): the entry swing resolves before round 1 while the target
    /// sleeps — beat at round 0, wake, damage — then initiative rolls.
    #[test]
    fn ambush_opener_swings_before_round_one() {
        let mut a = char_named("Alice");
        let mut b = char_named("Bob");
        a.attributes.strength = 50;
        b.sleeping = true;
        b.blood = 900;

        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b],
            [vec![0], vec![1]],
            "amb".into(),
            CombatTuning::default(),
            Box::new(AttackOnlyProvider),
        );
        let mut rng = SmallRng::seed_from_u64(7);
        let mut events = Vec::new();

        combat.resolve_opener(&mut rng, &mut events);

        assert!(
            events.iter().any(|ev| matches!(
                &ev.payload,
                MessagePayload::CombatBeat(beat) if beat.round == 0
            )),
            "the opener beat carries round 0"
        );
        assert!(!combat.is_over(), "900 blood survives one swing");
        drop(combat);
        assert!(!b.sleeping, "the wake fired");
        assert!(b.blood < 900, "the swing landed");
    }

    #[test]
    fn opener_is_a_no_op_when_the_target_is_awake() {
        let mut a = char_named("Alice");
        let mut b = char_named("Bob");
        b.blood = 900;

        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b],
            [vec![0], vec![1]],
            "amb2".into(),
            CombatTuning::default(),
            Box::new(AttackOnlyProvider),
        );
        let mut rng = SmallRng::seed_from_u64(7);
        let mut events = Vec::new();

        combat.resolve_opener(&mut rng, &mut events);

        assert!(events.is_empty(), "no opener against an awake target");
        assert!(!combat.is_over(), "the fight still needs its rounds");
        drop(combat);
        assert_eq!(b.blood, 900, "nobody touched the awake fighter");
    }

    /// An opener kill settles the fight before round 1: the loop never starts,
    /// the victim never acts.
    #[test]
    fn opener_kill_ends_the_fight_before_round_one() {
        let mut a = char_named("Alice");
        let mut b = char_named("Bob");
        a.attributes.strength = 50;
        b.sleeping = true;
        b.blood = 1;

        let mut combat = CombatRoundLoop::with_sides(
            areas::hex::default_layout().to_vec(),
            vec![&mut a, &mut b],
            [vec![0], vec![1]],
            "amb3".into(),
            CombatTuning::default(),
            Box::new(AttackOnlyProvider),
        );
        let mut rng = SmallRng::seed_from_u64(7);
        let mut events = Vec::new();

        combat.resolve_opener(&mut rng, &mut events);

        assert!(combat.is_over(), "the opener decided the fight");
        let result = combat.result().cloned();
        drop(combat);
        assert_eq!(b.blood, 0, "the victim died before round 1");
        assert!(
            matches!(&result, Some(CombatResult::Victory { victors })
                if victors.iter().any(|v| v.identifier == a.identifier)),
            "the ambusher wins: {result:?}"
        );
    }
}
