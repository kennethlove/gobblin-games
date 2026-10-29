use super::kind::{AfflictionKind, BodyPart};
use serde::{Deserialize, Serialize};

/// Storage discriminator. Same kind on different parts is independent;
/// same kind on the same part collapses to one slot.
pub type AfflictionKey = (AfflictionKind, Option<BodyPart>);

/// Coarse beast taxonomy used by `DeathCause::Beast`. Trauma source matching
/// is at this granularity (no per-individual beast keying in v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeastKind {
    Tracker,
    Mutt,
    Wolf,
    Bear,
    Snake,
    Bird,
    Other,
}

impl std::fmt::Display for BeastKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BeastKind::Tracker => write!(f, "tracker"),
            BeastKind::Mutt => write!(f, "mutt"),
            BeastKind::Wolf => write!(f, "wolf"),
            BeastKind::Bear => write!(f, "bear"),
            BeastKind::Snake => write!(f, "snake"),
            BeastKind::Bird => write!(f, "bird"),
            BeastKind::Other => write!(f, "beast"),
        }
    }
}

/// Coarse hazard taxonomy used by `DeathCause::Hazard`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HazardKind {
    Trap,
    Pit,
    FallingDebris,
    ToxicGas,
    Quicksand,
    Other,
}

impl std::fmt::Display for HazardKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HazardKind::Trap => write!(f, "trap"),
            HazardKind::Pit => write!(f, "pit"),
            HazardKind::FallingDebris => write!(f, "falling debris"),
            HazardKind::ToxicGas => write!(f, "toxic gas"),
            HazardKind::Quicksand => write!(f, "quicksand"),
            HazardKind::Other => write!(f, "hazard"),
        }
    }
}

/// Coarse classification of how a character died, used as the keying source for
/// trauma acquisition (spec §4). Stored on `TraumaSource` variants and as
/// the `cause` field of `MessagePayload::CharacterKilled`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DeathCause {
    Character(String),
    Combat,
    Suicide,
    CriticalFumble,
    Fire,
    Drowning,
    Beast(BeastKind),
    Hazard(HazardKind),
    Starvation,
    Dehydration,
    Affliction(AfflictionKind),
    Gamemaker,
    Unknown,
}

impl std::fmt::Display for DeathCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeathCause::Character(name) => write!(f, "{name}"),
            DeathCause::Combat => write!(f, "combat"),
            DeathCause::Suicide => write!(f, "suicide"),
            DeathCause::CriticalFumble => write!(f, "critical_fumble"),
            DeathCause::Fire => write!(f, "fire"),
            DeathCause::Drowning => write!(f, "drowning"),
            DeathCause::Beast(kind) => write!(f, "{kind}"),
            DeathCause::Hazard(kind) => write!(f, "{kind}"),
            DeathCause::Starvation => write!(f, "starvation"),
            DeathCause::Dehydration => write!(f, "dehydration"),
            DeathCause::Affliction(kind) => write!(f, "{kind}"),
            DeathCause::Gamemaker => write!(f, "gamemaker"),
            DeathCause::Unknown => write!(f, "unknown"),
        }
    }
}

/// How a death reads to the crowd (ul6): a pure copy-tier signal computed
/// from what a death already carries — no stored state, no new feeds.
/// WHY: option A scope (copy tiers only); witness count is deliberately
/// skipped — no death site has it in scope and cause + record + round
/// carry the joke. Day-summary callouts (option B) can ride this fn later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathTier {
    Glorious,
    Embarrassing,
    Mundane,
}

/// Cause absurdity ranks highest; a decorated fighter falling in combat
/// earns praise; dying on the opening swing reads embarrassing.
pub fn death_tier(cause: &DeathCause, victim_wins: u32, round: Option<u32>) -> DeathTier {
    let absurd = matches!(
        cause,
        DeathCause::CriticalFumble
            | DeathCause::Suicide
            | DeathCause::Hazard(_)
            | DeathCause::Drowning
            | DeathCause::Fire
    );
    if absurd {
        return DeathTier::Embarrassing;
    }
    if matches!(cause, DeathCause::Combat | DeathCause::Character(_)) && victim_wins >= 3 {
        return DeathTier::Glorious;
    }
    if round == Some(1) {
        return DeathTier::Embarrassing;
    }
    DeathTier::Mundane
}

/// Coarse class of mass-casualty event source (spec §4, §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CauseClass {
    Combat,
    Environmental,
    Gamemaker,
    Mixed,
}

/// Source of a trauma affliction, capturing the triggering event.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraumaSource {
    WitnessedAllyDeath {
        ally: String,
        cause: Option<DeathCause>,
    },
    NearDeath {
        cause: DeathCause,
    },
    Betrayal {
        by: String,
    },
    MassCasualty {
        cause_class: CauseClass,
        deaths_this_cycle: u32,
    },
}

/// Origin of an affliction. `Patron` and `Gamemaker` variants are reserved
/// for future systems but ship in v1 to avoid enum churn (per spec §3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AfflictionSource {
    Spawn,
    Combat { attacker_id: String },
    Environmental,
    Cascade { from: AfflictionKey },
    Patron,
    Gamemaker,
}

#[cfg(test)]
mod death_tier_tests {
    use super::{DeathCause, DeathTier, HazardKind, death_tier};

    #[test]
    fn absurd_causes_always_read_embarrassing() {
        for cause in [
            DeathCause::CriticalFumble,
            DeathCause::Suicide,
            DeathCause::Hazard(HazardKind::Other),
            DeathCause::Drowning,
            DeathCause::Fire,
        ] {
            assert_eq!(
                death_tier(&cause, 99, Some(1)),
                DeathTier::Embarrassing,
                "{cause:?} reads embarrassing regardless of record"
            );
        }
    }

    #[test]
    fn decorated_fighter_in_combat_earns_glorious() {
        assert_eq!(
            death_tier(&DeathCause::Combat, 3, Some(4)),
            DeathTier::Glorious
        );
        assert_eq!(
            death_tier(&DeathCause::Character("Stinky".to_string()), 3, Some(4)),
            DeathTier::Glorious
        );
        assert_eq!(
            death_tier(&DeathCause::Combat, 2, Some(4)),
            DeathTier::Mundane
        );
    }

    #[test]
    fn dying_on_the_opening_swing_reads_embarrassing() {
        assert_eq!(
            death_tier(&DeathCause::Combat, 0, Some(1)),
            DeathTier::Embarrassing
        );
    }
}
