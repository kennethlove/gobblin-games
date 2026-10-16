//! Per-game advancement event log — the *delta* half of cross-game carry.
//!
//! [`crate::baseline::GameBaseline`] is the floor a character settles back
//! to at game entry; this log is the explicit delta stamped the moment a
//! durable change happens (`Character::gain_trait` / `Character::lose_trait`
//! are the only writers). Settle consumes the log instead of inferring
//! intent by diffing ending state against the snapshot, which is what lets
//! an intentional trait *loss* survive settle — a diff cannot tell "lost on
//! purpose" from "was never there" (trait redesign spec §7.3, §10.8).
//!
//! Lifecycle: events accumulate during one game and are consumed (taken) by
//! [`crate::baseline::GameBaseline::settle_into`] at the next entry, so the
//! log always describes only the game in progress. It is persisted on the
//! character record (`#[serde(default)]`) because reset runs at API join
//! time — a mid-game save/load must not lose pending events. Bounded by
//! [`MAX_ADVANCEMENT_EVENTS`]: on overflow the oldest event is evicted
//! (trait churn within one game is small; events are consumed every game).

use crate::traits::Trait;
use serde::{Deserialize, Serialize};

/// Bound on one game's pending advancement events. When full, the oldest
/// event is evicted to make room — the log is per-game and consumed at
/// settle, so this only ever bounds a single game's worth of stamps.
pub const MAX_ADVANCEMENT_EVENTS: usize = 64;

/// One durable change, recorded as it happens.
///
/// The log is consumed at settle: `TraitGained`/`TraitLost` are replayed
/// against the baseline floor (in order), `XpPurchase` is recorded and
/// consumed *without* replay (see its variant docs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdvancementEvent {
    /// `gain_trait` granted a trait — persists through settle.
    TraitGained {
        #[serde(rename = "trait")]
        trait_: Trait,
    },
    /// `lose_trait` removed a trait — stays removed through settle (the
    /// regression this log exists to fix; snapshot-diffing used to restore it).
    TraitLost {
        #[serde(rename = "trait")]
        trait_: Trait,
    },
    /// Reserved slot for the XP system: purchases are
    /// immediate and non-negative (spec §10.9), so their state effect
    /// applies at purchase time and settle consumes this without
    /// replaying it. Recorded for the durable-change record and so e1d
    /// can extend the shape without a schema change. No mechanics yet —
    /// shape only, deliberately not blocking XP work.
    XpPurchase {
        /// Freeform record of what was bought (e1d will replace this with
        /// a typed purchase once XP mechanics exist).
        detail: String,
        /// Non-negative spend; `u32` encodes the §10.9 invariant by type.
        amount: u32,
    },
}

/// Append `event` to `log`, evicting the oldest entry at the cap.
pub fn push_event(log: &mut Vec<AdvancementEvent>, event: AdvancementEvent) {
    if log.len() >= MAX_ADVANCEMENT_EVENTS {
        log.remove(0);
    }
    log.push(event);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gained(trait_: Trait) -> AdvancementEvent {
        AdvancementEvent::TraitGained { trait_ }
    }

    #[test]
    fn log_is_bounded_and_keeps_newest() {
        let mut log = Vec::new();
        for i in 0..(MAX_ADVANCEMENT_EVENTS + 5) {
            // Alternate traits so every event is distinguishable.
            let t = if i % 2 == 0 {
                Trait::Tough
            } else {
                Trait::Resilient
            };
            push_event(&mut log, gained(t));
        }
        assert_eq!(log.len(), MAX_ADVANCEMENT_EVENTS);
        // Oldest evicted, newest kept: last iteration was i = 68 (even),
        // which stamps the first trait.
        assert_eq!(log.last(), Some(&gained(Trait::Tough)));
    }

    #[test]
    fn advancement_events_serde_roundtrip() {
        for event in [
            AdvancementEvent::TraitGained {
                trait_: Trait::Friendly,
            },
            AdvancementEvent::TraitLost {
                trait_: Trait::Paranoid,
            },
            AdvancementEvent::XpPurchase {
                detail: "+10% max stamina".into(),
                amount: 25,
            },
        ] {
            let json = serde_json::to_string(&event).expect("serialize");
            let back: AdvancementEvent = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(event, back);
        }
    }
}
