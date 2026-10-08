//! Producer (f): Witness mass casualty — Moderate/Severe based on death count.

use crate::areas::Area;
use crate::games::Game;
use shared::afflictions::{CauseClass, Severity, TraumaSource};
use shared::messages::{MessagePayload, Phase};

use super::shared::{TraumaEvent, apply_trauma_events};

pub(super) fn produce_witness_mass_casualty(game: &mut Game, phase: Phase) {
    // Phase 1: count deaths per area
    let mut deaths_by_area: std::collections::HashMap<Area, u32> = std::collections::HashMap::new();

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

        *deaths_by_area.entry(vt.area).or_insert(0) += 1;
    }

    // Phase 2: find living characters in areas with >= 3 deaths
    let mut events: Vec<TraumaEvent> = Vec::new();

    for (area, death_count) in &deaths_by_area {
        if *death_count < 3 {
            continue;
        }

        let severity = if *death_count >= 5 {
            Severity::Severe
        } else {
            Severity::Moderate
        };

        for t in &game.characters {
            if t.is_alive() && t.area == *area {
                events.push(TraumaEvent {
                    character_id: t.identifier.to_string(),
                    character_name: t.name.clone(),
                    source: TraumaSource::MassCasualty {
                        cause_class: CauseClass::Mixed,
                        deaths_this_cycle: *death_count,
                    },
                    severity,
                    cause_hint: shared::afflictions::DeathCause::Unknown,
                });
            }
        }
    }

    apply_trauma_events(game, events, false);
}
