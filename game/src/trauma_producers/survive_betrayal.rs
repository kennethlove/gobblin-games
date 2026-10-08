//! Producer (d): Survive betrayal — Moderate trauma + phobia co-acquire stub.

use crate::games::Game;
use shared::afflictions::{Severity, TraumaSource};
use shared::messages::{MessagePayload, Phase};

use super::shared::{TraumaEvent, apply_trauma_events};

pub(super) fn produce_survive_betrayal(game: &mut Game, phase: Phase) {
    let mut events: Vec<TraumaEvent> = Vec::new();

    for msg in &game.messages {
        if msg.phase != phase {
            continue;
        }
        let MessagePayload::BetrayalTriggered { betrayer, victim } = &msg.payload else {
            continue;
        };

        let Some(character) = game
            .characters
            .iter()
            .find(|t| victim.identifier == t.identifier)
        else {
            continue;
        };

        if !character.is_alive() {
            continue;
        }

        events.push(TraumaEvent {
            character_id: character.identifier.to_string(),
            character_name: character.name.clone(),
            source: TraumaSource::Betrayal {
                by: betrayer.identifier.to_string(),
            },
            severity: Severity::Moderate,
            cause_hint: shared::afflictions::DeathCause::Character(betrayer.name.clone()),
        });
    }

    apply_trauma_events(game, events, true);
}
