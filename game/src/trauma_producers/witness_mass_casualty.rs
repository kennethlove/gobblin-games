//! Producer (f): Witness mass casualty — Moderate/Severe based on death count.

use crate::games::Game;
use areas::location::Location;
use shared::afflictions::{CauseClass, Severity, TraumaSource};
use shared::messages::{MessagePayload, Phase};

use super::shared::{TraumaEvent, apply_trauma_events};

pub(super) fn produce_witness_mass_casualty(game: &mut Game, phase: Phase) {
    // Phase 1: collect the locations where this phase's deaths happened
    let mut deaths: Vec<Location> = Vec::new();

    for msg in &game.messages {
        if msg.phase != phase {
            continue;
        }
        let MessagePayload::CharacterKilled { victim, .. } = &msg.payload else {
            continue;
        };

        let Some(vt) = game
            .characters
            .iter()
            .find(|t| victim.identifier == t.identifier)
        else {
            continue;
        };

        deaths.push(vt.location);
    }

    // Phase 2: living characters who witnessed >= 3 deaths — same area,
    // or same sub-tile when witnessing.trauma_producers is on.
    let co_presence = game.config.witnessing.trauma_producers;
    let mut events: Vec<TraumaEvent> = Vec::new();

    for t in &game.characters {
        if !t.is_alive() {
            continue;
        }
        let death_count = deaths
            .iter()
            .filter(|d| d.area == t.location.area && (!co_presence || **d == t.location))
            .count() as u32;
        if death_count < 3 {
            continue;
        }

        let severity = if death_count >= 5 {
            Severity::Severe
        } else {
            Severity::Moderate
        };

        events.push(TraumaEvent {
            character_id: t.identifier.to_string(),
            character_name: t.name.clone(),
            source: TraumaSource::MassCasualty {
                cause_class: CauseClass::Mixed,
                deaths_this_cycle: death_count,
            },
            severity,
            cause_hint: shared::afflictions::DeathCause::Unknown,
        });
    }

    apply_trauma_events(game, events, false);
}
