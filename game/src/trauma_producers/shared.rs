//! Shared types and helpers for the trauma producer pipeline.
//!
//! Provides internal data structures (`TraumaEvent`, `TraumaMessage`),
//! the applying/forwarding helpers, and the phobia co-acquire stub.

use crate::characters::Character;
use crate::characters::afflictions::TraumaAcquisition;
use crate::games::Game;
use shared::afflictions::{DeathCause, Severity, TraumaSource};
use shared::messages::{CharacterRef, MessagePayload, MessageSource};

/// Collected trauma event before application (avoids borrow conflicts).
pub(super) struct TraumaEvent {
    pub(super) character_id: String,
    pub(super) character_name: String,
    pub(super) source: TraumaSource,
    pub(super) severity: Severity,
    /// Death cause for phobia co-acquire stub.
    pub(super) cause_hint: DeathCause,
}

/// Message data collected during trauma application, pushed afterwards.
struct TraumaMessage {
    character_id: String,
    character_name: String,
    acquisition: TraumaAcquisition,
}

/// Apply collected trauma events, emitting appropriate messages.
/// When `with_phobia_stub` is true, calls the phobia co-acquire stub after
/// each acquisition.
pub(super) fn apply_trauma_events(
    game: &mut Game,
    events: Vec<TraumaEvent>,
    with_phobia_stub: bool,
) {
    let mut messages: Vec<TraumaMessage> = Vec::new();

    for event in events {
        let Some(character) = game
            .characters
            .iter_mut()
            .find(|t| t.identifier == event.character_id)
        else {
            continue;
        };

        let acquisition = character.try_acquire_trauma(event.source, event.severity);
        messages.push(TraumaMessage {
            character_id: event.character_id.clone(),
            character_name: event.character_name.clone(),
            acquisition,
        });

        if with_phobia_stub {
            try_co_acquire_phobia(character, &event.cause_hint);
        }
    }

    // Push messages after mutable borrows end
    for msg in messages {
        push_trauma_message(
            game,
            &msg.character_name,
            &msg.character_id,
            &msg.acquisition,
        );
    }
}

/// Push a `TraumaAcquired` or `TraumaReinforced` message onto the game log.
fn push_trauma_message(
    game: &mut Game,
    character_name: &str,
    character_id: &str,
    acquisition: &TraumaAcquisition,
) {
    let payload = match acquisition {
        TraumaAcquisition::Acquired { severity, source } => MessagePayload::TraumaAcquired {
            character: character_id.to_string(),
            severity: severity.to_string(),
            source: format!("{source:?}"),
        },
        TraumaAcquisition::Reinforced {
            from_severity,
            to_severity,
            floor_bumped,
        } => MessagePayload::TraumaReinforced {
            character: character_id.to_string(),
            from_severity: from_severity.to_string(),
            to_severity: to_severity.to_string(),
            floor_bumped: *floor_bumped,
        },
    };

    let tick = game.tick_counter.next();
    game.push_message(
        MessageSource::Character(character_id.to_string()),
        format!("character:{character_id}"),
        format_trauma_content(character_name, acquisition),
        payload,
        tick,
    );
}

/// Format human-readable content for a trauma message.
fn format_trauma_content(name: &str, acquisition: &TraumaAcquisition) -> String {
    match acquisition {
        TraumaAcquisition::Acquired { severity, .. } => {
            format!("{name} acquires trauma ({severity}).")
        }
        TraumaAcquisition::Reinforced {
            from_severity,
            to_severity,
            floor_bumped,
        } => {
            if *floor_bumped {
                format!("{name}'s trauma reinforced: {from_severity} → {to_severity}.")
            } else {
                format!("{name}'s trauma reinforced ({to_severity}).")
            }
        }
    }
}

/// Map a killer reference and cause to a `DeathCause`. If a killer is present,
/// their character identity takes priority; otherwise the original cause is returned.
pub(super) fn map_cause_to_death_cause(
    killer: Option<&CharacterRef>,
    cause: &DeathCause,
) -> DeathCause {
    if let Some(k) = killer {
        return DeathCause::Character(k.identifier.to_string());
    }
    cause.clone()
}

/// Stub for phobia co-acquisition. No-op until phobia PR lands.
///
/// TODO(phobia-pr1): wire to `try_acquire_phobia` when the phobia affliction
/// system is implemented.
#[allow(dead_code)]
fn try_co_acquire_phobia(_character: &mut Character, _cause: &DeathCause) {
    // No-op stub. Will trigger phobia acquisition once the phobia system exists.
}
