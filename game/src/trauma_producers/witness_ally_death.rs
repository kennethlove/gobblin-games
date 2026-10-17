//! Producer (a)/(b): Witness ally death — Mild trauma + phobia co-acquire stub.

use crate::games::Game;
use areas::location::Location;
use shared::afflictions::{Severity, TraumaSource};
use shared::messages::{CharacterRef, MessagePayload, Phase};

use super::shared::{TraumaEvent, apply_trauma_events, map_cause_to_death_cause};

pub(super) fn produce_witness_ally_death(game: &mut Game, phase: Phase) {
    // Phase 1: collect killed characters with location info
    let killed: Vec<(
        String,
        Location,
        uuid::Uuid,
        Option<CharacterRef>,
        shared::afflictions::DeathCause,
    )> = game
        .messages
        .iter()
        .filter(|m| m.phase == phase)
        .filter_map(|m| match &m.payload {
            MessagePayload::CharacterKilled {
                victim,
                killer,
                cause,
            } => game
                .characters
                .iter()
                .find(|t| victim.identifier == t.identifier)
                .map(|vt| {
                    (
                        victim.identifier.to_string(),
                        vt.location,
                        vt.id,
                        killer.clone(),
                        cause.clone(),
                    )
                }),
            _ => None,
        })
        .collect();

    // Phase 2: find living allies witnessing the death — same area,
    // or same sub-tile when witnessing.trauma_producers is on.
    let co_presence = game.config.witnessing.trauma_producers;
    let mut events: Vec<TraumaEvent> = Vec::new();
    for (victim_id, victim_location, victim_uuid, killer, cause) in &killed {
        for t in &game.characters {
            let witnessed = t.location.area == victim_location.area
                && (!co_presence || t.location == *victim_location);
            if t.is_alive() && t.allies.contains(victim_uuid) && witnessed {
                let death_cause = map_cause_to_death_cause(killer.as_ref(), cause);
                events.push(TraumaEvent {
                    character_id: t.identifier.to_string(),
                    character_name: t.name.clone(),
                    source: TraumaSource::WitnessedAllyDeath {
                        ally: victim_id.clone(),
                        cause: Some(death_cause),
                    },
                    severity: Severity::Mild,
                    cause_hint: cause.clone(),
                });
            }
        }
    }

    // Phase 3: apply trauma and emit messages
    apply_trauma_events(game, events, true);
}
