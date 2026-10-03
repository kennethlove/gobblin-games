use serde::{Deserialize, Serialize};

use crate::messages::CharacterRef;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AudienceEventKind {
    KillMade,
    KillReceived,
    AttackTrapped,
    RescueAlly,
    AllianceFormed,
    BetrayalCommitted,
    AfflictionAcquired,
    SurvivedAreaEvent,
    UnderdogVictory,
    ClanLoyaltyAct,
    Cowardice,
    TrapSet,
    TrapTriggered,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AudienceEvent {
    KillMade {
        actor: CharacterRef,
        victim: CharacterRef,
        magnitude: u32,
        modifier: f32,
    },
    KillReceived {
        victim: CharacterRef,
        actor: Option<CharacterRef>,
        magnitude: u32,
        modifier: f32,
    },
    AttackTrapped {
        actor: CharacterRef,
        victim: CharacterRef,
    },
    RescueAlly {
        actor: CharacterRef,
        ally: CharacterRef,
    },
    AllianceFormed {
        characters: Vec<CharacterRef>,
    },
    BetrayalCommitted {
        actor: CharacterRef,
        victim: CharacterRef,
    },
    AfflictionAcquired {
        character: CharacterRef,
        kind: String,
    },
    SurvivedAreaEvent {
        character: CharacterRef,
    },
    UnderdogVictory {
        actor: CharacterRef,
        victim: CharacterRef,
    },
    ClanLoyaltyAct {
        actor: CharacterRef,
        clan: u8,
    },
    Cowardice {
        character: CharacterRef,
    },
    /// A character sets a trap in an area.
    TrapSet {
        character: CharacterRef,
    },
    /// A character triggers a trap.
    TrapTriggered {
        victim: CharacterRef,
    },
}

impl AudienceEvent {
    pub fn kind(&self) -> AudienceEventKind {
        match self {
            Self::KillMade { .. } => AudienceEventKind::KillMade,
            Self::KillReceived { .. } => AudienceEventKind::KillReceived,
            Self::AttackTrapped { .. } => AudienceEventKind::AttackTrapped,
            Self::RescueAlly { .. } => AudienceEventKind::RescueAlly,
            Self::AllianceFormed { .. } => AudienceEventKind::AllianceFormed,
            Self::BetrayalCommitted { .. } => AudienceEventKind::BetrayalCommitted,
            Self::AfflictionAcquired { .. } => AudienceEventKind::AfflictionAcquired,
            Self::SurvivedAreaEvent { .. } => AudienceEventKind::SurvivedAreaEvent,
            Self::UnderdogVictory { .. } => AudienceEventKind::UnderdogVictory,
            Self::ClanLoyaltyAct { .. } => AudienceEventKind::ClanLoyaltyAct,
            Self::Cowardice { .. } => AudienceEventKind::Cowardice,
            Self::TrapSet { .. } => AudienceEventKind::TrapSet,
            Self::TrapTriggered { .. } => AudienceEventKind::TrapTriggered,
        }
    }

    /// Base × modifier; floor at 1 to avoid 0-magnitude triggers.
    pub fn magnitude_score(&self) -> u32 {
        let (base, modifier) = match self {
            Self::KillMade {
                magnitude,
                modifier,
                ..
            }
            | Self::KillReceived {
                magnitude,
                modifier,
                ..
            } => (*magnitude, *modifier),
            Self::AttackTrapped { .. } => (6, 1.0),
            Self::RescueAlly { .. } => (5, 1.0),
            Self::AllianceFormed { .. } => (3, 1.0),
            Self::BetrayalCommitted { .. } => (7, 1.0),
            Self::AfflictionAcquired { .. } => (3, 1.0),
            Self::SurvivedAreaEvent { .. } => (4, 1.0),
            Self::UnderdogVictory { .. } => (10, 1.0),
            Self::ClanLoyaltyAct { .. } => (5, 1.0),
            Self::Cowardice { .. } => (2, 1.0),
            Self::TrapSet { .. } => (3, 1.0),
            Self::TrapTriggered { .. } => (4, 1.0),
        };
        ((base as f32 * modifier).max(1.0)) as u32
    }

    /// Characters whose affinity-with-patron is updated by this event.
    pub fn affected_characters(&self) -> Vec<&CharacterRef> {
        match self {
            Self::KillMade { actor, victim, .. }
            | Self::AttackTrapped { actor, victim }
            | Self::BetrayalCommitted { actor, victim }
            | Self::UnderdogVictory { actor, victim } => vec![actor, victim],
            Self::KillReceived { victim, actor, .. } => match actor {
                Some(a) => vec![victim, a],
                None => vec![victim],
            },
            Self::RescueAlly { actor, ally } => vec![actor, ally],
            Self::AllianceFormed { characters } => characters.iter().collect(),
            Self::AfflictionAcquired { character, .. }
            | Self::SurvivedAreaEvent { character }
            | Self::Cowardice { character } => vec![character],
            Self::ClanLoyaltyAct { actor, .. } => vec![actor],
            Self::TrapSet { character } => vec![character],
            Self::TrapTriggered { victim } => vec![victim],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str) -> CharacterRef {
        CharacterRef {
            identifier: name.into(),
            name: name.into(),
        }
    }

    #[test]
    fn kill_made_magnitude_uses_base_times_modifier() {
        let ev = AudienceEvent::KillMade {
            actor: t("a"),
            victim: t("b"),
            magnitude: 5,
            modifier: 2.0,
        };
        assert_eq!(ev.magnitude_score(), 10);
    }

    #[test]
    fn betrayal_kind_roundtrips() {
        let ev = AudienceEvent::BetrayalCommitted {
            actor: t("a"),
            victim: t("b"),
        };
        assert_eq!(ev.kind(), AudienceEventKind::BetrayalCommitted);
    }

    #[test]
    fn alliance_affects_all_members() {
        let ev = AudienceEvent::AllianceFormed {
            characters: vec![t("a"), t("b"), t("c")],
        };
        assert_eq!(ev.affected_characters().len(), 3);
    }

    #[test]
    fn magnitude_score_never_zero() {
        let ev = AudienceEvent::KillMade {
            actor: t("a"),
            victim: t("b"),
            magnitude: 0,
            modifier: 0.0,
        };
        assert!(ev.magnitude_score() >= 1);
    }
}
