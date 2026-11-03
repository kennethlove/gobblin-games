use super::*;
use areas::AreaId;
use areas::events::AreaEvent;
use rand::rngs::SmallRng;

/// Blocks (time cycles) a hazard stays active after arrival; each
/// cycle re-runs the survival pass and ages by one, then it wipes.
/// (q7k: config knob comes later.)
const MAX_HAZARD_BLOCKS: u8 = 3;

impl Game {
    pub(super) fn run_trauma_producers(&mut self, _phase: shared::messages::Phase) {
        crate::trauma_producers::run_trauma_producers(self);
    }

    /// Announce the hazards active in an area.
    ///
    /// Emits one `MessageSource::Area` line per active hazard with a typed
    /// `MessagePayload::AreaEvent` (kind parsed from the event name,
    /// `description` = the rendered line). Hazards persist for
    /// `MAX_HAZARD_BLOCKS` cycles (q7k), so an active area re-announces
    /// each cycle; there is no close summary — nothing closes anymore.
    pub(super) fn announce_area_events(&mut self) -> Result<(), GameError> {
        // Snapshot to avoid borrow conflicts with self.messages below.
        let snapshots: Vec<(String, Vec<(String, String)>)> = self
            .areas
            .iter()
            .filter(|a| !a.is_open())
            .filter_map(|a| {
                a.area.map(|area| {
                    let lines = a
                        .cell_hazards
                        .iter()
                        .map(|h| {
                            (
                                format!("{} (triangle {})", h.event, h.sub.index() + 1),
                                h.event.to_string(),
                            )
                        })
                        .collect();
                    (area.to_string(), lines)
                })
            })
            .collect();

        for (area_name, event_names) in snapshots {
            let subject = format!("area:{}", area_name);
            for (display, kind_key) in &event_names {
                let content = crate::events::GameEvent::AreaEvent {
                    area_event: display.clone(),
                    area_name: area_name.clone(),
                }
                .to_string();
                let payload = shared::messages::MessagePayload::AreaEvent {
                    area: shared::messages::AreaRef {
                        identifier: area_name.clone().into(),
                        name: area_name.clone(),
                    },
                    kind: Self::area_event_kind(kind_key),
                    description: content.clone(),
                };
                self.push_message(
                    shared::messages::MessageSource::Area(area_name.clone()),
                    subject.clone(),
                    content,
                    payload,
                    self.tick_counter.boundary(),
                );
            }
        }
        Ok(())
    }

    /// Coarse kind carried on a hazard's typed payload — the bucket the
    /// feed groups by; the exact event name rides in `description`.
    fn area_event_kind(event_name: &str) -> shared::messages::AreaEventKind {
        use shared::messages::AreaEventKind;
        match event_name {
            "earthquake" => AreaEventKind::Earthquake,
            "flood" => AreaEventKind::Flood,
            "wildfire" => AreaEventKind::Fire,
            "blizzard" | "avalanche" | "sandstorm" => AreaEventKind::Storm,
            "landslide" | "heatwave" | "drought" | "rockslide" | "sinkhole" => {
                AreaEventKind::Hazard
            }
            _ => AreaEventKind::Other,
        }
    }

    /// Hazards: every live cell hazard re-runs its survival pass for
    /// this cycle, then ages one block; at 0 blocks it drops out of
    /// the cell (q7k TTL, q6k per cell). One roll per cell per cycle
    /// for the characters standing ON that triangle.
    pub(super) fn hazard_survival(&mut self, rng: &mut SmallRng) -> Result<(), GameError> {
        // Two-phase: gather first so the survival pass can take
        // `&mut self` without fighting the areas borrow (E0499 — same
        // trap as the cycle event closure).
        let mut active: Vec<(AreaId, areas::hex::SubTriangle, AreaEvent)> = Vec::new();
        for area_details in self.areas.iter_mut() {
            if area_details.cell_hazards.is_empty() {
                continue;
            }
            let area = area_details.area.unwrap();
            let mut survivors = Vec::with_capacity(area_details.cell_hazards.len());
            for mut hazard in area_details.cell_hazards.drain(..) {
                if hazard.blocks_left == 0 {
                    continue;
                }
                active.push((area, hazard.sub, hazard.event.clone()));
                hazard.blocks_left -= 1;
                survivors.push(hazard);
            }
            area_details.cell_hazards = survivors;
        }
        for (area, sub, event) in active {
            self.process_event_for_area(&area, sub, &event, rng)?;
        }
        Ok(())
    }

    /// Triggers events for the current cycle.
    pub(super) fn trigger_cycle_events(
        &mut self,
        phase: shared::messages::Phase,
        rng: &mut SmallRng,
    ) -> Result<(), GameError> {
        use world::config::DaySlot;
        // Substrate-only: the dawn/dusk anchors stay silent. Day-band
        // hours fire day events, night-band hours fire night events.
        if matches!(self.config.day_slot(phase), DaySlot::Dawn | DaySlot::Dusk) {
            return Ok(());
        }
        // Per-area per-cycle probability from the game config (persisted
        // per game; old rows without the fields hydrate to the defaults).
        let frequency = if self.config.is_night_phase(phase) {
            self.config.night_event_frequency
        } else {
            self.config.day_event_frequency
        };
        let day = !self.config.is_night_phase(phase);

        // Collect events to trigger (avoid borrow conflicts)
        // If it's nighttime, trigger an event
        // If it is daytime and not day #1 or day #3, trigger an event
        if !day || ![1, 3].contains(&self.day.unwrap_or(1)) {
            for area_details in self.areas.iter_mut() {
                if rng.random_bool(frequency) {
                    // Generate terrain-appropriate event
                    let area_event = AreaEvent::random_for_terrain(&area_details.terrain.base, rng);

                    // q6k: one roll per cycle picks WHICH of the hex's
                    // six triangles the hazard strikes.
                    let cell = areas::hex::SubTriangle::new(
                        rng.random_range(0..areas::hex::SubTriangle::COUNT as u8),
                    );
                    if let Some(existing) = area_details
                        .cell_hazards
                        .iter_mut()
                        .find(|h| h.sub == cell && h.event == area_event)
                    {
                        // Same cell, same event: extend the TTL (q7k's
                        // refresh rule, now per cell).
                        existing.blocks_left = MAX_HAZARD_BLOCKS;
                    } else {
                        area_details.cell_hazards.push(areas::CellHazard {
                            sub: cell,
                            event: area_event,
                            blocks_left: MAX_HAZARD_BLOCKS,
                        });
                    }
                }
            }
        }

        // Hazards (q7k): survival re-runs for every active hazard
        // this cycle, each ages one block, wipes at 0.
        self.hazard_survival(rng)?;

        Ok(())
    }

    /// If the character count is low, constrain them by forcing
    /// hazards into open areas (q7k valve — same rule, no movement
    /// block; flagged for tuning).
    pub(super) fn constrain_areas(&mut self, rng: &mut SmallRng) -> Result<(), GameError> {
        let character_count = self.living_characters_count() as u32;
        let odds = character_count as f64 / 24.0;

        // WHY: read the config field (was a dead duplicate of this
        // const); tests with tiny casts disable the random closing rolls.
        if !(1..self.config.low_character_threshold).contains(&character_count) {
            return Ok(());
        }

        // Forced hazards land on a random cell of a still-open area —
        // same terrain-honest picker as the trigger (q6k policy fix:
        // the valve used to roll terrain-agnostic events, Sinkhole
        // included). Kept for tuning per q7k.
        let mut forced: Vec<(AreaId, AreaEvent)> = Vec::new();
        if let Some(area_details) = self.random_open_area() {
            forced.push((
                area_details.area.unwrap(),
                AreaEvent::random_for_terrain(&area_details.terrain.base, rng),
            ));
        }
        if rng.random_bool(odds)
            && let Some(area_details) = self.random_open_area()
        {
            forced.push((
                area_details.area.unwrap(),
                AreaEvent::random_for_terrain(&area_details.terrain.base, rng),
            ));
        }

        for (area_id, event) in forced {
            let cell = areas::hex::SubTriangle::new(
                rng.random_range(0..areas::hex::SubTriangle::COUNT as u8),
            );
            if let Some(details) = self.areas.iter_mut().find(|a| a.area == Some(area_id)) {
                details.cell_hazards.push(areas::CellHazard {
                    sub: cell,
                    event,
                    blocks_left: MAX_HAZARD_BLOCKS,
                });
            }
        }
        Ok(())
    }
}
