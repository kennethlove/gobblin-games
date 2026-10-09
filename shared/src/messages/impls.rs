use super::*;
use crate::ids::CharacterId;

/// Whether a payload is persisted to the message log or routed to server
/// logs only. Single source of truth for the persist gate (api), the log
/// and timeline read paths, and the debug feed — see the
/// "compile goblin log to important events" epic for the decision table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Importance {
    /// Dramatically changes a goblin's (or the arena's) state — persisted.
    Persist,
    /// State is derivable from data or the event is low drama — emitted as
    /// `tracing::debug!` server logs only.
    ServerLogOnly,
}

impl MessagePayload {
    pub fn kind(&self) -> MessageKind {
        MessageKind::from(self)
    }

    /// Persistence tier for this payload (see [`Importance`]).
    ///
    /// Rules of thumb:
    /// - deaths, injuries, traps, state-changing item use, alliances,
    ///   betrayals, affliction acquisition/escalation, roster and arena
    ///   state changes, day summaries → [`Importance::Persist`]
    /// - anything derivable from current character/area data (HP, sanity,
    ///   bands, position, plain rest/sleep bookkeeping) or pure observers
    ///   and decay → [`Importance::ServerLogOnly`]
    /// - combat tiers by outcome: landed swings/flees persist, misses and
    ///   stalemates do not
    /// - `CycleStart` persists only at the day-start and nightfall hours
    ///   (2/day); the other 10 phase starts are clock bookkeeping
    /// - interrupted wakes persist (something hit them); rested wakes do not
    pub fn importance(&self) -> Importance {
        use MessagePayload::*;
        match self {
            // --- Outcome-dependent combat tiers -------------------------
            Combat(e) if matches!(e.outcome, CombatOutcome::Stalemate) => {
                Importance::ServerLogOnly
            }
            Combat(_) => Importance::Persist, // Killed/Wounded/flees
            CombatSwing(b)
                if matches!(
                    b.outcome,
                    crate::combat_beat::SwingOutcome::Miss
                ) => Importance::ServerLogOnly,
            CombatSwing(_) => Importance::Persist,
            CycleStart { phase, .. }
                if phase.hour() == Phase::DAY_START_HOUR
                    || phase.hour() == Phase::NIGHTFALL_HOUR =>
            {
                Importance::Persist
            }
            CycleStart { .. } | CycleEnd { .. } | PhaseStarted { .. } | PhaseEnded { .. } => {
                Importance::ServerLogOnly
            }
            CharacterWoke {
                reason: WakeReason::Interrupted { .. },
                ..
            } => Importance::Persist,

            // --- Persist: deaths, injuries, traps -----------------------
            CharacterKilled { .. }
            | CharacterBledOut { .. }
            | CharacterWounded { .. }
            | WoundInflicted { .. }
            | WoundInfected { .. }
            | WoundHealed { .. }
            | WoundTreated { .. }
            | WoundAmputated { .. }
            | CharacterTrapped { .. }
            | TrappedEscaped { .. }
            | CharacterDiedWhileTrapped { .. }
            | TrapSet { .. }
            | TrapTriggered { .. }
            | RescueAttempted { .. }
            | SleepIncident { .. } => Importance::Persist,

            // --- Persist: alliances and betrayal ------------------------
            AllianceFormed { .. }
            | AllianceProposed { .. }
            | AllianceDissolved { .. }
            | BetrayalTriggered { .. }
            | TrustShockBreak { .. } => Importance::Persist,

            // --- Persist: items that change state -----------------------
            ItemFound { .. }
            | ItemUsed { .. }
            | SubstanceUsed { .. } // addictive consumable use
            | PatronGift { .. } => Importance::Persist,

            // --- Persist: mental/affliction acquisition + escalation ----
            ConditionAcquired { .. }
            | ConditionResolved { .. }
            | AfflictionAcquired { .. }
            | AfflictionProgressed { .. }
            | AfflictionCascaded { .. }
            | TraumaAcquired { .. }
            | TraumaReinforced { .. }
            | TraumaEscalated { .. }
            | TraumaFlashback { .. }
            | PhobiaAcquired { .. }
            | PhobiaTriggered { .. }
            | PhobiaEscalated { .. }
            | FixationAcquired { .. }
            | FixationEscalated { .. }
            | FixationFired { .. }
            | FixationConsummated { .. }
            | FixationThwarted { .. }
            | AddictionAcquired { .. }
            | AddictionReinforced { .. }
            | AddictionEscalated { .. }
            | AddictionRelapse { .. } => Importance::Persist,

            // --- Persist: arena + game lifecycle ------------------------
            AreaClosed { .. } | AreaEvent { .. } | GameEnded { .. } | DaySummary { .. } => {
                Importance::Persist
            }

            // --- ServerLogOnly: derivable state / low drama -------------
            Generic
            | CharacterAttacked { .. }
            | CharacterMoved { .. }
            | CharacterHidden { .. }
            | ItemDropped { .. }
            | CharacterRested { .. }
            | CharacterStarved { .. }
            | CharacterDehydrated { .. }
            | SanityBreak { .. }
            | CharacterDesperate { .. }
            | HungerBandChanged { .. }
            | ThirstBandChanged { .. }
            | StaminaBandChanged { .. }
            | ShelterSought { .. }
            | Foraged { .. }
            | Drank { .. }
            | Ate { .. }
            | CharacterSlept { .. }
            | CharacterWoke { .. } // unguarded: rested wakes (Interrupted persists above)
            | WoundBled { .. } // per-tick blood drain — derivable from `blood`
            | AfflictionHealed { .. }
            | PhobiaObserved { .. }
            | PhobiaForgotten { .. }
            | PhobiaHabituated { .. } // severity decay — derivable
            | TraumaObserved { .. }
            | TraumaForgotten { .. }
            | TraumaHabituated { .. } // severity decay — derivable
            | TraumaAvoidance { .. }
            | FixationFaded { .. }
            | AddictionResisted { .. }
            | AddictionCraving { .. }
            | AddictionObserved { .. }
            | AddictionForgotten { .. }
            | AddictionHabituated { .. }
            | Struggling { .. }
            | PartialRescueProgress { .. } => Importance::ServerLogOnly,
        }
    }

    pub fn character_refs(&self) -> Vec<&CharacterRef> {
        use MessagePayload::*;
        let mut refs = Vec::new();
        match self {
            CharacterKilled { victim, killer, .. } => {
                refs.push(victim);
                if let Some(k) = killer {
                    refs.push(k);
                }
            }
            CharacterWounded {
                victim, attacker, ..
            }
            | CharacterAttacked { victim, attacker } => {
                refs.push(victim);
                if let Some(a) = attacker {
                    refs.push(a);
                }
            }
            Combat(e) => {
                refs.push(&e.attacker);
                refs.push(&e.target);
            }
            CombatSwing(b) => {
                refs.push(&b.attacker);
                refs.push(&b.target);
            }
            AllianceFormed { members } | AllianceDissolved { members, .. } => refs.extend(members),
            AllianceProposed { proposer, target } => {
                refs.push(proposer);
                refs.push(target);
            }
            BetrayalTriggered { betrayer, victim } => {
                refs.push(betrayer);
                refs.push(victim);
            }
            TrustShockBreak { character, partner } => {
                refs.push(character);
                refs.push(partner);
            }
            CharacterMoved { character, .. }
            | CharacterHidden { character, .. }
            | ItemFound { character, .. }
            | ItemUsed { character, .. }
            | ItemDropped { character, .. }
            | CharacterRested { character, .. }
            | CharacterStarved { character, .. }
            | CharacterDehydrated { character, .. }
            | SanityBreak { character }
            | CharacterBledOut { character }
            | WoundInfected { character, .. }
            | WoundHealed { character, .. }
            | HungerBandChanged { character, .. }
            | ThirstBandChanged { character, .. }
            | StaminaBandChanged { character, .. }
            | ShelterSought { character, .. }
            | Foraged { character, .. }
            | Drank { character, .. }
            | Ate { character, .. }
            | CharacterSlept { character, .. }
            | CharacterWoke { character, .. }
            | SleepIncident { character, .. }
            | TrapSet { character, .. } => refs.push(character),
            TrapTriggered { victim, .. } => refs.push(victim),
            PatronGift { recipient, .. } => refs.push(recipient),
            GameEnded { winner, .. } => {
                if let Some(w) = winner {
                    refs.push(w);
                }
            }
            AfflictionAcquired { .. }
            | AfflictionProgressed { .. }
            | AfflictionHealed { .. }
            | AfflictionCascaded { .. }
            | TraumaAcquired { .. }
            | TraumaReinforced { .. }
            | TraumaEscalated { .. }
            | TraumaFlashback { .. }
            | TraumaAvoidance { .. }
            | TraumaHabituated { .. }
            | PhobiaAcquired { .. }
            | PhobiaTriggered { .. }
            | PhobiaEscalated { .. }
            | PhobiaHabituated { .. }
            | FixationAcquired { .. }
            | FixationEscalated { .. }
            | FixationFired { .. }
            | FixationConsummated { .. }
            | FixationThwarted { .. }
            | FixationFaded { .. }
            | SubstanceUsed { .. }
            | AddictionAcquired { .. }
            | AddictionReinforced { .. }
            | AddictionEscalated { .. }
            | AddictionResisted { .. }
            | AddictionRelapse { .. }
            | AddictionCraving { .. }
            | AddictionHabituated { .. }
            | CharacterTrapped { .. }
            | Struggling { .. }
            | TrappedEscaped { .. }
            | CharacterDiedWhileTrapped { .. }
            | RescueAttempted { .. }
            | PartialRescueProgress { .. }
            | PhobiaObserved { .. }
            | PhobiaForgotten { .. }
            | TraumaObserved { .. }
            | TraumaForgotten { .. }
            | AddictionObserved { .. }
            | AddictionForgotten { .. }
            | WoundInflicted { .. }
            | WoundBled { .. }
            | WoundTreated { .. }
            | WoundAmputated { .. }
            | ConditionAcquired { .. }
            | ConditionResolved { .. }
            | CharacterDesperate { .. } => {}
            DaySummary {
                rollup, goblins, ..
            } => {
                refs.extend(rollup.kill_leaders.iter().map(|l| &l.character));
                refs.extend(goblins.iter().map(|g| &g.character));
            }
            Generic
            | AreaClosed { .. }
            | AreaEvent { .. }
            | CycleStart { .. }
            | CycleEnd { .. }
            | PhaseStarted { .. }
            | PhaseEnded { .. } => {}
        }
        refs
    }

    pub fn involves(&self, character_identifier: &str) -> bool {
        let id = character_identifier.parse::<CharacterId>().unwrap();
        if self.character_refs().iter().any(|t| t.identifier == id) {
            return true;
        }
        use MessagePayload::*;
        match self {
            AfflictionAcquired { character_id, .. }
            | AfflictionProgressed { character_id, .. }
            | AfflictionHealed { character_id, .. }
            | AfflictionCascaded { character_id, .. }
            | TraumaAcquired {
                character: character_id,
                ..
            }
            | TraumaReinforced {
                character: character_id,
                ..
            }
            | TraumaEscalated {
                character: character_id,
                ..
            }
            | TraumaFlashback {
                character: character_id,
                ..
            }
            | TraumaAvoidance {
                character: character_id,
                ..
            }
            | TraumaHabituated {
                character: character_id,
                ..
            }
            | PhobiaAcquired {
                character: character_id,
                ..
            }
            | PhobiaTriggered {
                character: character_id,
                ..
            }
            | PhobiaEscalated {
                character: character_id,
                ..
            }
            | PhobiaHabituated {
                character: character_id,
                ..
            }
            | FixationAcquired { character_id, .. }
            | FixationEscalated { character_id, .. }
            | FixationFired { character_id, .. }
            | FixationConsummated { character_id, .. }
            | FixationThwarted { character_id, .. }
            | FixationFaded { character_id, .. }
            | SubstanceUsed {
                character: character_id,
                ..
            }
            | AddictionAcquired {
                character: character_id,
                ..
            }
            | AddictionReinforced {
                character: character_id,
                ..
            }
            | AddictionEscalated {
                character: character_id,
                ..
            }
            | AddictionResisted {
                character: character_id,
                ..
            }
            | AddictionRelapse {
                character: character_id,
                ..
            }
            | AddictionCraving {
                character: character_id,
                ..
            }
            | AddictionHabituated {
                character: character_id,
                ..
            }
            | CharacterTrapped {
                character: character_id,
                ..
            }
            | Struggling {
                character: character_id,
                ..
            }
            | TrappedEscaped {
                character: character_id,
                ..
            }
            | CharacterDiedWhileTrapped {
                character: character_id,
                ..
            } => character_id.parse::<CharacterId>().ok() == Some(id),
            PhobiaObserved {
                observer, subject, ..
            }
            | PhobiaForgotten {
                observer, subject, ..
            }
            | TraumaObserved {
                observer, subject, ..
            }
            | TraumaForgotten {
                observer, subject, ..
            }
            | AddictionObserved {
                observer, subject, ..
            }
            | AddictionForgotten {
                observer, subject, ..
            } => {
                observer.parse::<CharacterId>().ok() == Some(id.clone())
                    || subject.parse::<CharacterId>().ok() == Some(id)
            }
            RescueAttempted {
                rescuer, target, ..
            }
            | PartialRescueProgress {
                rescuer, target, ..
            } => {
                rescuer.parse::<CharacterId>().ok() == Some(id.clone())
                    || target.parse::<CharacterId>().ok() == Some(id)
            }
            _ => false,
        }
    }
}
