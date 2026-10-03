use super::*;
use crate::areas::{Area, AreaDetails};
use crate::characters::events::CharacterEvent;
use crate::characters::incidents::{SleepIncident, SleepShelter, apply_sleep_incident};
use crate::characters::statuses::CharacterStatus;
use crate::characters::{
    ActionSuggestion, Character, EncounterContext, EnvironmentContext, calculate_stamina_cost,
};
use crate::items::{Item, OwnsItems};
use crate::messages::{AreaRef, CharacterRef, ItemRef, MessagePayload};
use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use shared::messages::SleepIncidentKind;
use std::collections::HashMap;

impl Game {
    /// Pre-computed, immutable view of game state used by `execute_cycle`.
    ///
    /// `build_cycle_context` materialises this from `&self` so the executor
    /// half of the cycle (which holds `&mut self`) can read pre-snapshotted
    /// data without re-borrowing. This split also gives gamemaker overrides
    /// (and future cycle-modifier hooks) a typed seam between the
    /// "what the world looks like" phase and the "what each character does"
    /// phase.
    pub(super) fn build_cycle_context(
        &self,
        phase: crate::messages::Phase,
        closed_areas: Vec<Area>,
        living_characters: Vec<Character>,
        living_characters_count: usize,
    ) -> CycleContext {
        use crate::messages::Phase;
        let day = phase == Phase::Day;
        let action_suggestion = match (self.day, day) {
            (Some(1), true) => Some(ActionSuggestion {
                action: Action::Move(None),
                probability: Some(0.5),
            }),
            (Some(3), true) => Some(ActionSuggestion {
                action: Action::Move(Some(Area::Hub)),
                probability: Some(0.75),
            }),
            (_, _) => None,
        };

        let mut area_details_map = HashMap::with_capacity(self.areas.len());
        for (i, area_detail) in self.areas.iter().enumerate() {
            if let Some(area) = &area_detail.area {
                area_details_map.insert(*area, i);
            }
        }

        let mut characters_by_area: HashMap<Area, Vec<Character>> = HashMap::new();
        for character in living_characters {
            characters_by_area
                .entry(character.area)
                .or_default()
                .push(character);
        }

        // Per-area living-character density. Threaded into `EnvironmentContext`
        // so `Brain::choose_destination` can apply a per-enemy crowd penalty
        // and disperse crowded areas without a call-site escape hatch
        // (hangrier_games-4wnj).
        let enemy_density: HashMap<Area, u32> = characters_by_area
            .iter()
            .map(|(area, characters)| (*area, characters.len() as u32))
            .collect();

        CycleContext {
            is_day: day,
            phase,
            current_day: self.day.unwrap_or(1),
            action_suggestion,
            area_details_map,
            characters_by_area,
            enemy_density,
            combat_tuning_snapshot: self.combat_tuning.clone(),
            all_areas_snapshot: self.areas.clone(),
            closed_areas,
            living_characters_count,
        }
    }

    /// Iterate over `self.characters`, applying survival ticks, brain decisions,
    /// and combat using the pre-built `CycleContext`. After the iteration
    /// ends the collected per-character events are drained into `self.messages`
    /// via `flush_character_events`, and any alliance events emitted during the
    /// cycle are processed.
    pub(super) fn execute_cycle(
        &mut self,
        ctx: CycleContext,
        rng: &mut SmallRng,
    ) -> Result<(), GameError> {
        let CycleContext {
            is_day: day,
            phase,
            current_day,
            action_suggestion,
            area_details_map,
            characters_by_area,
            enemy_density,
            combat_tuning_snapshot,
            all_areas_snapshot,
            closed_areas,
            living_characters_count,
        } = ctx;

        let mut collected_events: Vec<CollectedEvent> = Vec::new();
        let mut drained_alliance_events: Vec<crate::characters::alliances::AllianceEvent> =
            Vec::new();

        // Two-phase resolution (tm6a): collect indices of characters that
        // survive survival/sleep ticks first, then execute actions in a
        // second pass with liveness checks so characters killed by earlier
        // actions in the same phase cannot act.
        let mut characters_to_act: Vec<usize> = Vec::new();

        for (idx, character) in self.characters.iter_mut().enumerate() {
            if !character.is_alive() {
                // Newly-dead characters (status=RecentlyDead going into this
                // cycle) trigger a DeathRecorded event so allies process the
                // ally-death cascade. Killer attribution is read from the
                // character's transient `recently_killed_by` field, which combat
                // sites set when the death was caused by another character.
                // Environmental/status deaths leave it `None`. Promote to Dead
                // after enqueueing so the same character does not re-emit on
                // subsequent cycles.
                if character.status == CharacterStatus::RecentlyDead {
                    let killer = character.recently_killed_by.take();
                    drained_alliance_events.push(
                        crate::characters::alliances::AllianceEvent::DeathRecorded {
                            deceased: character.id,
                            killer,
                        },
                    );
                }
                character.status = CharacterStatus::Dead;
                continue;
            }

            if !rng.random_bool(character.attributes.luck as f64 / 100.0) {
                character.events.push(CharacterEvent::random());
            }

            // Survival tick (spec §6, §7). Each living character, once per
            // phase: tick hunger/thirst, apply escalating drain, emit any
            // band-change events, and route 0-HP starvation/dehydration
            // deaths through CharacterKilled with the appropriate cause.
            // Loot drop is handled centrally by clean_up_recent_deaths
            // after the cycle ends.
            {
                use crate::areas::weather::current_weather;
                use crate::characters::survival::{
                    apply_dehydration_drain, apply_starvation_drain, hunger_band, thirst_band,
                    tick_survival,
                };
                use crate::messages::{CharacterRef, MessagePayload};
                use shared::afflictions::DeathCause;

                let weather = current_weather();
                let phase_index: u32 = self.day.unwrap_or(1) * 2 + u32::from(!day);
                let sheltered = character
                    .sheltered_until
                    .is_some_and(|until| until > phase_index);

                // Affliction cascade tick (spec §5). Runs once per phase per
                // living character. Sheltered characters may recover; exposed may
                // worsen. Severe + exposed can spawn successors or kill.
                {
                    use crate::characters::afflictions::tuning::AfflictionTuning;
                    use crate::characters::afflictions::{
                        CascadeOutcome, apply_cascade, tick_cascade,
                    };
                    use shared::afflictions::Severity;

                    let affliction_list: Vec<_> = character.afflictions.values().cloned().collect();
                    if !affliction_list.is_empty() {
                        let tuning = AfflictionTuning::default();
                        let cascade_result =
                            tick_cascade(&affliction_list, sheltered, &tuning, rng);
                        let successors = apply_cascade(&mut character.afflictions, &cascade_result);

                        for succ in &successors {
                            character.afflictions.insert(succ.key(), succ.clone());
                        }

                        let tref = CharacterRef {
                            identifier: character.identifier.clone().into(),
                            name: character.name.clone(),
                        };

                        for (kind, outcome) in &cascade_result.outcomes {
                            match outcome {
                                CascadeOutcome::SteppedDown { from, to } => {
                                    if matches!(to, Severity::Mild)
                                        && matches!(from, Severity::Mild)
                                    {
                                        let line = format!("{}'s {} healed.", character.name, kind);
                                        collected_events.push((
                                            character.identifier.to_string(),
                                            character.name.clone(),
                                            line,
                                            Some(MessagePayload::AfflictionHealed {
                                                character_id: tref.identifier.to_string(),
                                                affliction: kind.to_string(),
                                            }),
                                            None,
                                        ));
                                    } else {
                                        let line = format!(
                                            "{}'s {} improved: {} → {}.",
                                            character.name, kind, from, to
                                        );
                                        collected_events.push((
                                            character.identifier.to_string(),
                                            character.name.clone(),
                                            line,
                                            Some(MessagePayload::AfflictionProgressed {
                                                character_id: tref.identifier.to_string(),
                                                affliction: kind.to_string(),
                                                from_severity: from.to_string(),
                                                to_severity: to.to_string(),
                                            }),
                                            None,
                                        ));
                                    }
                                }
                                CascadeOutcome::SteppedUp { from, to } => {
                                    let line = format!(
                                        "{}'s {} worsened: {} → {}.",
                                        character.name, kind, from, to
                                    );
                                    collected_events.push((
                                        character.identifier.to_string(),
                                        character.name.clone(),
                                        line,
                                        Some(MessagePayload::AfflictionProgressed {
                                            character_id: tref.identifier.to_string(),
                                            affliction: kind.to_string(),
                                            from_severity: from.to_string(),
                                            to_severity: to.to_string(),
                                        }),
                                        None,
                                    ));
                                }
                                CascadeOutcome::SpawnedSuccessor { from, to } => {
                                    let line = format!(
                                        "{}'s {} cascaded into {}.",
                                        character.name, from, to
                                    );
                                    collected_events.push((
                                        character.identifier.to_string(),
                                        character.name.clone(),
                                        line,
                                        Some(MessagePayload::AfflictionCascaded {
                                            character_id: tref.identifier.to_string(),
                                            from_affliction: from.to_string(),
                                            to_affliction: to.to_string(),
                                        }),
                                        None,
                                    ));
                                }
                                CascadeOutcome::DeathRoll { survived: false } => {
                                    let line = format!("{} succumbs to {}.", character.name, kind);
                                    collected_events.push((
                                        character.identifier.to_string(),
                                        character.name.clone(),
                                        line,
                                        Some(MessagePayload::CharacterKilled {
                                            victim: tref.clone(),
                                            killer: None,
                                            cause: shared::afflictions::DeathCause::Affliction(
                                                kind.clone(),
                                            ),
                                        }),
                                        None,
                                    ));
                                    character.status = CharacterStatus::RecentlyDead;
                                }
                                _ => {}
                            }
                        }

                        if cascade_result.character_died {
                            continue;
                        }
                    }
                }

                let prior_hunger = hunger_band(character.hunger);
                let prior_thirst = thirst_band(character.thirst);

                // Sleep substrate (bd-s0je): once per phase, every living
                // character that did NOT spend the phase asleep ages by one
                // cycle. The brain doesn't yet score `Action::Sleep`, so this
                // simply tracks accumulated wakefulness for downstream PRs.
                if !character.sleeping {
                    character.cycles_awake = character.cycles_awake.saturating_add(1);
                }

                tick_survival(character, &weather, sheltered);
                let hp_lost_starv = apply_starvation_drain(character);
                let hp_lost_dehy = apply_dehydration_drain(character);

                let new_hunger = hunger_band(character.hunger);
                let new_thirst = thirst_band(character.thirst);
                let tref = CharacterRef {
                    identifier: character.identifier.clone().into(),
                    name: character.name.clone(),
                };

                if new_hunger != prior_hunger {
                    collected_events.push((
                        character.identifier.to_string(),
                        character.name.clone(),
                        String::new(),
                        Some(MessagePayload::HungerBandChanged {
                            character: tref.clone(),
                            from: prior_hunger,
                            to: new_hunger,
                        }),
                        None,
                    ));
                }
                if new_thirst != prior_thirst {
                    collected_events.push((
                        character.identifier.to_string(),
                        character.name.clone(),
                        String::new(),
                        Some(MessagePayload::ThirstBandChanged {
                            character: tref.clone(),
                            from: prior_thirst,
                            to: new_thirst,
                        }),
                        None,
                    ));
                }

                // Stamina recovery + band-cross detection. Runs once per
                // phase per living character. For v1 we use Action::None (idle
                // recovery 5/phase); proper Rest/sheltered scaling lands when
                // the action chosen by `process_turn_phase` is plumbed back
                // here. `sheltered` reuses the value computed above for the
                // hunger/thirst tick.
                if character.blood > 0 {
                    use crate::characters::stamina_band::stamina_band;

                    let prior_band = stamina_band(
                        character.stamina,
                        character.max_stamina,
                        &combat_tuning_snapshot,
                    );
                    character.recover_stamina(
                        &crate::characters::actions::Action::None,
                        sheltered,
                        new_hunger,
                        new_thirst,
                        &combat_tuning_snapshot,
                    );
                    let new_band = stamina_band(
                        character.stamina,
                        character.max_stamina,
                        &combat_tuning_snapshot,
                    );
                    if new_band != prior_band {
                        let line = format!(
                            "{} stamina: {:?} -> {:?}",
                            character.name, prior_band, new_band
                        );
                        collected_events.push((
                            character.identifier.to_string(),
                            character.name.clone(),
                            line,
                            Some(MessagePayload::StaminaBandChanged {
                                character: tref.clone(),
                                from: prior_band,
                                to: new_band,
                            }),
                            None,
                        ));
                    }
                }

                // Death routing for survival-induced 0 HP. Dehydration takes
                // precedence over starvation when both landed in the same
                // tick.
                if character.blood == 0 && (hp_lost_starv > 0 || hp_lost_dehy > 0) {
                    let cause = if hp_lost_dehy > 0 {
                        DeathCause::Dehydration
                    } else {
                        DeathCause::Starvation
                    };
                    let line = format!("{} succumbs to {}.", character.name, cause);
                    collected_events.push((
                        character.identifier.to_string(),
                        character.name.clone(),
                        line,
                        Some(MessagePayload::CharacterKilled {
                            victim: tref,
                            killer: None,
                            cause,
                        }),
                        None,
                    ));
                    character.status = CharacterStatus::RecentlyDead;
                    continue;
                }
            }

            // Sleep tick (PR2c.1, bd-9sjj). Sleeping characters skip the
            // brain pipeline entirely: regen stamina (always) and HP
            // (gated on absence of Wounded / Infected / Sick per spec
            // §6.4), then decrement `sleep_remaining`. When the
            // countdown drains to zero, flip `sleeping = false`, reset
            // `cycles_awake`, and emit `CharacterWoke { Rested }`.
            // Interruption handling lives in PR2c.2 (bd-1zju); this PR
            // ships the natural-wake path only.
            if character.sleeping {
                use crate::messages::{CharacterRef, MessagePayload};
                use shared::messages::WakeReason;

                // Spec §6.4 PR2c.2 (bd-1zju). Before regenerating, check
                // whether an area event in the sleeper's current area is
                // active. If so, wake the character with the appropriate
                // `InterruptionKind::AreaEvent` and skip regen this phase
                // — they didn't actually rest, they were jolted awake.
                let area_event_kind = area_details_map
                    .get(&character.area)
                    .and_then(|&idx| self.areas.get(idx))
                    .and_then(|a| a.events.first())
                    .map(area_event_to_kind);
                if let Some(kind) = area_event_kind {
                    let mut wake_events: Vec<crate::messages::TaggedEvent> = Vec::new();
                    let woke = character.wake_interrupted(
                        shared::messages::InterruptionKind::AreaEvent { kind },
                        phase,
                        &mut wake_events,
                    );
                    if woke {
                        for ev in wake_events.drain(..) {
                            collected_events.push((
                                character.identifier.to_string(),
                                character.name.clone(),
                                ev.content,
                                Some(ev.payload),
                                None,
                            ));
                        }
                        character.sleep_shelter = None;
                        continue;
                    }
                }

                // ── Sleep incident roll (we6l) ──
                // While unconscious, the character is vulnerable. Roll for a
                // random sleep incident each sleeping phase. Wake-causing
                // incidents (theft, relocation, animal, ally abandonment,
                // limb injury) interrupt sleep immediately. Flavor-only
                // incidents (annoying) are remembered for the natural wake.
                let biome = area_details_map
                    .get(&character.area)
                    .and_then(|&idx| self.areas.get(idx))
                    .map(|a| a.terrain.base)
                    .unwrap_or(crate::terrain::types::BaseTerrain::Clearing);
                let phase_index: u32 = self.day.unwrap_or(1) * 4 + phase.ord() as u32;
                let is_sheltered = character
                    .sheltered_until
                    .is_some_and(|until| until > phase_index);
                if let Some(incident) = SleepIncident::roll(
                    rng,
                    phase,
                    biome,
                    is_sheltered,
                    character
                        .sleep_shelter
                        .as_ref()
                        .unwrap_or(&SleepShelter::None),
                    current_day,
                ) {
                    let description = apply_sleep_incident(character, &incident, rng);
                    let incident_kind: shared::messages::SleepIncidentKind = (&incident).into();

                    if incident.wakes_character() {
                        // Wake-causing incident: emit flavor event, then
                        // wake the character with the incident as the reason.
                        let flavor_line = crate::output::GameOutput::CharacterSleepFlavor(
                            character.name.as_str(),
                            &description,
                        )
                        .to_string();
                        collected_events.push((
                            character.identifier.to_string(),
                            character.name.clone(),
                            flavor_line,
                            Some(MessagePayload::SleepIncident {
                                character: CharacterRef {
                                    identifier: character.identifier.clone().into(),
                                    name: character.name.clone(),
                                },
                                kind: incident_kind.clone(),
                                description: description.clone(),
                            }),
                            None,
                        ));

                        let incident_msg = crate::output::GameOutput::CharacterWakesFromIncident(
                            character.name.as_str(),
                            &description,
                        )
                        .to_string();
                        collected_events.push((
                            character.identifier.to_string(),
                            character.name.clone(),
                            incident_msg,
                            Some(MessagePayload::CharacterWoke {
                                character: CharacterRef {
                                    identifier: character.identifier.clone().into(),
                                    name: character.name.clone(),
                                },
                                phase,
                                reason: shared::messages::WakeReason::Interrupted {
                                    event: shared::messages::InterruptionKind::Incident {
                                        kind: incident_kind,
                                    },
                                },
                            }),
                            None,
                        ));
                        character.sleeping = false;
                        character.sleep_remaining = 0;
                        character.cycles_awake = 0;
                        character.pending_sleep_incident = None;
                        character.sleep_shelter = None;
                        continue;
                    } else {
                        // Flavor-only incident: emit flavor, remember it,
                        // then continue with regen as normal.
                        let flavor_line = crate::output::GameOutput::CharacterSleepFlavor(
                            character.name.as_str(),
                            &description,
                        )
                        .to_string();
                        collected_events.push((
                            character.identifier.to_string(),
                            character.name.clone(),
                            flavor_line,
                            Some(MessagePayload::SleepIncident {
                                character: CharacterRef {
                                    identifier: character.identifier.clone().into(),
                                    name: character.name.clone(),
                                },
                                kind: incident_kind.clone(),
                                description: description.clone(),
                            }),
                            None,
                        ));
                        character.pending_sleep_incident = Some(incident_kind);
                    }
                }

                let blocked = character.afflictions.keys().any(|(kind, _)| {
                    matches!(
                        kind,
                        shared::afflictions::AfflictionKind::Wounded
                            | shared::afflictions::AfflictionKind::Infected
                            | shared::afflictions::AfflictionKind::Sick
                    )
                });
                let prior_stamina = character.stamina;
                let prior_hp = character.blood;
                character.stamina = character
                    .stamina
                    .saturating_add(SLEEP_STAMINA_PER_PHASE)
                    .min(character.max_stamina);
                if !blocked {
                    character.blood = character
                        .blood
                        .saturating_add(SLEEP_HP_PER_PHASE)
                        .min(crate::characters::wounds::MAX_BLOOD);
                }
                let restored_stamina = character.stamina.saturating_sub(prior_stamina);
                let restored_hp = character.blood.saturating_sub(prior_hp);

                // The CharacterSlept emission for the *entry* phase happens
                // in process_turn_phase. Each subsequent phase the sleeper
                // is silent except for the final CharacterWoke; we do not
                // re-emit per-phase regen events to avoid log spam. The
                // restored_* totals are consumed by the consolidated
                // CharacterWoke payload.
                character.sleep_remaining = character.sleep_remaining.saturating_sub(1);
                if character.sleep_remaining == 0 {
                    character.sleeping = false;
                    character.cycles_awake = 0;
                    character.sleep_shelter = None;
                    let tref = CharacterRef {
                        identifier: character.identifier.clone().into(),
                        name: character.name.clone(),
                    };
                    let incident_suffix = character
                        .pending_sleep_incident
                        .take()
                        .map(|kind| match kind {
                            SleepIncidentKind::Hallucination => {
                                " — still shaken by strange dreams".to_string()
                            }
                            _ => " — though their sleep was restless".to_string(),
                        })
                        .unwrap_or_default();
                    let line = format!(
                        "{} {}",
                        crate::output::GameOutput::CharacterWakesRested(character.name.as_str()),
                        incident_suffix,
                    );
                    collected_events.push((
                        character.identifier.to_string(),
                        character.name.clone(),
                        line,
                        Some(MessagePayload::CharacterWoke {
                            character: tref,
                            phase,
                            reason: WakeReason::Rested,
                        }),
                        None,
                    ));
                }
                let _ = (restored_stamina, restored_hp);
                continue;
            }

            // Character survived survival + sleep ticks — queue for action phase.
            // Two-phase resolution (tm6a): actions execute in a second pass
            // with liveness checks so characters killed by earlier actions
            // cannot act.
            characters_to_act.push(idx);
        }

        // ── Phobia scan ────────────────────────────────────────────
        // Run after survival/sleep ticks, before action execution.
        // Detects firing phobias, handles reinforcement/decay,
        // tracks observer state, emits escalation/habituation/observation
        // messages so severity changes take effect before brain decisions.
        if self.config.phobias_enabled && !characters_to_act.is_empty() {
            use crate::characters::afflictions::phobia::scan_character;
            use crate::characters::afflictions::phobia::triggers::PhobiaContext;

            // Monotonically increasing cycle number (Day 1 Day = 1, etc.).
            let phobia_cycle = (current_day.saturating_sub(1)) * 4 + phase.ord() as u32;

            for &idx in &characters_to_act {
                let area = self.characters[idx].area;
                let Some(area_details) = all_areas_snapshot.iter().find(|ad| ad.area == Some(area))
                else {
                    continue;
                };

                // Use the pre-built character snapshot for observer tracking.
                let other_characters: &[Character] = characters_by_area
                    .get(&area)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);

                let phobia_ctx = PhobiaContext {
                    area: area_details,
                    is_night: !day,
                    other_characters_in_area: other_characters,
                    cycle_messages: &[],
                    cycle: phobia_cycle,
                };

                let scan_result =
                    scan_character(&mut self.characters[idx], &phobia_ctx, phobia_cycle, rng);

                for msg in scan_result.messages {
                    let line = phobia_message_line(&msg, &self.characters[idx].name);
                    collected_events.push((
                        self.characters[idx].identifier.to_string(),
                        self.characters[idx].name.clone(),
                        line,
                        Some(msg),
                        None,
                    ));
                }
            }
        }

        // ── Trauma cycle processing ──────────────────────────────────
        // Run after phobia scan, before action execution.
        // Handles flashback rolls, observer tracking, and decay.
        if self.config.trauma_enabled && !characters_to_act.is_empty() {
            let trauma_cycle = (current_day.saturating_sub(1)) * 4 + phase.ord() as u32;

            for &idx in &characters_to_act {
                let area = self.characters[idx].area;
                let other_characters: &[Character] = characters_by_area
                    .get(&area)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);

                let t_result = crate::characters::afflictions::trauma::process_traumas(
                    &mut self.characters[idx],
                    other_characters,
                    trauma_cycle,
                    rng,
                );

                for msg in t_result.messages {
                    let line = format_trauma_message(&msg, &self.characters[idx].name);
                    collected_events.push((
                        self.characters[idx].identifier.to_string(),
                        self.characters[idx].name.clone(),
                        line,
                        Some(msg),
                        None,
                    ));
                }
            }
        }

        // ── Addiction cycle processing ────────────────────────────────
        // Run after trauma processing, before action execution.
        // Handles High/Withdrawal tick, decay, observer tracking.
        if self.config.addiction_enabled && !characters_to_act.is_empty() {
            let addiction_cycle = (current_day.saturating_sub(1)) * 4 + phase.ord() as u32;

            for &idx in &characters_to_act {
                let area = self.characters[idx].area;
                let other_characters: &[Character] = characters_by_area
                    .get(&area)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);

                let msgs = crate::characters::afflictions::addiction::process_addictions(
                    &mut self.characters[idx],
                    other_characters,
                    addiction_cycle,
                    rng,
                );

                for msg in msgs {
                    let line = format_addiction_message(&msg, &self.characters[idx].name);
                    collected_events.push((
                        self.characters[idx].identifier.to_string(),
                        self.characters[idx].name.clone(),
                        line,
                        Some(msg),
                        None,
                    ));
                }
            }
        }

        // ── Hangover tick-down ──────────────────────────────────────
        for &idx in &characters_to_act {
            let t = &mut self.characters[idx];
            if t.hangover_cycles_remaining > 0 {
                t.hangover_cycles_remaining -= 1;
                if t.hangover_cycles_remaining == 0 {
                    let line = format!("{}'s hangover fades — rough night, clear head", t.name);
                    collected_events.push((
                        t.identifier.to_string(),
                        t.name.clone(),
                        line,
                        None,
                        None,
                    ));
                }
            }
        }

        // Sort by initiative so faster characters act first (tm6a).
        characters_to_act.sort_by_cached_key(|&idx| {
            let agility = self.characters[idx].attributes.agility;
            std::cmp::Reverse(initiative_score(agility, rng))
        });

        // --- Phase 2: Execute actions with liveness checks ---
        let mut pending_thefts: Vec<(usize, Uuid)> = Vec::new();
        for idx in characters_to_act {
            // Build sleeping_nearby BEFORE the mutable character borrow so
            // the self.characters.iter() doesn't conflict (ls5a).
            // Wasted work on dead characters (continue'd below) but harmless.
            let character_area = self.characters[idx].area;
            let sleeping_nearby: Vec<(Uuid, String)> = self
                .characters
                .iter()
                .filter(|t| {
                    t.is_alive() && t.sleeping && t.area == character_area && !t.items.is_empty()
                })
                .map(|t| (t.id, t.name.clone()))
                .collect();

            let character = &mut self.characters[idx];

            // Liveness gate: character may have been killed by an earlier
            // action in this same phase (tm6a).
            if !character.is_alive() {
                if character.status == CharacterStatus::RecentlyDead {
                    let killer = character.recently_killed_by.take();
                    drained_alliance_events.push(
                        crate::characters::alliances::AllianceEvent::DeathRecorded {
                            deceased: character.id,
                            killer,
                        },
                    );
                }
                character.status = CharacterStatus::Dead;
                continue;
            }

            let area_index = match area_details_map.get(&character.area) {
                Some(&old_area_idx) => old_area_idx,
                None => continue,
            };

            // Build available destinations BEFORE taking mutable borrow of area_details
            let available_destinations = character
                .area
                .neighbors()
                .into_iter()
                .filter_map(|neighbor_area| {
                    // Find the AreaDetails for this neighbor
                    self.areas
                        .iter()
                        .find(|ad| ad.area == Some(neighbor_area))
                        .map(|ad| {
                            // Calculate stamina cost to move to this area
                            let move_action = Action::Move(Some(neighbor_area));
                            let stamina_cost =
                                calculate_stamina_cost(&move_action, &ad.terrain, character);

                            crate::areas::DestinationInfo {
                                area: neighbor_area,
                                terrain: ad.terrain.clone(),
                                active_events: ad.events.clone(),
                                stamina_cost,
                            }
                        })
                })
                .collect();

            let area_details = &mut self.areas[area_index];

            let mut environment_details = EnvironmentContext {
                is_day: day,
                phase,
                area_details,
                closed_areas: &closed_areas,
                available_destinations,
                all_areas: &all_areas_snapshot,
                enemy_density: &enemy_density,
                current_day,
                combat_tuning: &combat_tuning_snapshot,
                sleeping_nearby,
            };

            // Get nearby characters using the pre-computed map
            let ev = Vec::new();
            let nearby_characters = {
                match characters_by_area.get(&character.area) {
                    Some(characters) => characters,
                    None => &ev,
                }
            };
            let nearby_characters_count = nearby_characters.len() as u32;

            let targets: Vec<Character> = nearby_characters
                .iter()
                .filter(|t| t.is_visible() && t.identifier != character.identifier)
                .cloned()
                .collect();

            let encounter_context = EncounterContext {
                nearby_characters_count,
                potential_targets: targets,
                total_living_characters: living_characters_count as u32,
            };

            // ── Brain rescue priority override ──
            // Before executing, check if this character should rescue a
            // co-located trapped character instead of their chosen action.
            let mut override_suggestion = action_suggestion.clone();
            if character.is_alive()
                && let Some(target_id) = crate::characters::rescue::evaluate_rescue_opportunity(
                    character,
                    environment_details.area_details,
                    nearby_characters,
                    rng,
                )
            {
                override_suggestion = Some(ActionSuggestion {
                    action: Action::Rescue { target: target_id },
                    probability: Some(1.0),
                });
            }
            let mut character_events: Vec<crate::messages::TaggedEvent> = Vec::new();
            character.process_turn_phase(
                override_suggestion,
                &mut environment_details,
                encounter_context,
                rng,
                &mut character_events,
            );
            for tagged in character_events {
                collected_events.push((
                    character.identifier.to_string(),
                    character.name.clone(),
                    tagged.content,
                    Some(tagged.payload),
                    None,
                ));
            }
            drained_alliance_events.append(&mut character.drain_alliance_events());

            // Collect pending theft from sleeping character (ls5a).
            // The pending_theft_target was set by act_take_item during
            // process_turn_phase. Actual item transfer happens after the
            // character borrow is released (below) to avoid conflicting
            // &mut self.characters borrows.
            if let Some(sleeper_uuid) = character.pending_theft_target.take() {
                pending_thefts.push((idx, sleeper_uuid));
            }
        }

        // ── Process pending sleep theft (ls5a) ──
        // Iterate thefts collected during Phase 2. The character borrow is
        // released by now so we can split_at_mut on self.characters freely.
        for (thief_idx, sleeper_uuid) in &pending_thefts {
            let sleeper_idx = match self.characters.iter().position(|t| t.id == *sleeper_uuid) {
                Some(idx) => idx,
                None => continue, // sleeper died or vanished
            };
            if sleeper_idx == *thief_idx {
                continue; // shouldn't happen
            }

            // split_at_mut for simultaneous mutable access to thief + sleeper
            let (thief, sleeper) = if *thief_idx < sleeper_idx {
                let (left, right) = self.characters.split_at_mut(sleeper_idx);
                (&mut left[*thief_idx], &mut right[0])
            } else {
                let (left, right) = self.characters.split_at_mut(*thief_idx);
                (&mut right[0], &mut left[sleeper_idx])
            };

            if sleeper.items.is_empty() {
                continue;
            }
            let item_idx = rng.random_range(0..sleeper.items.len());
            let stolen = sleeper.items.remove(item_idx);
            let item_name = stolen.name.clone();
            let thief_name = thief.name.clone();
            let sleeper_name = sleeper.name.clone();
            thief.add_item(stolen.clone());

            let line = format!(
                "{} steals {} from sleeping {}!",
                thief_name, item_name, sleeper_name
            );
            collected_events.push((
                thief.identifier.to_string(),
                thief.name.clone(),
                line,
                Some(MessagePayload::ItemFound {
                    character: CharacterRef {
                        identifier: thief.identifier.clone().into(),
                        name: thief.name.clone(),
                    },
                    item: ItemRef {
                        identifier: stolen.identifier.clone().into(),
                        name: item_name,
                    },
                    area: AreaRef {
                        identifier: thief.area.to_string().into(),
                        name: thief.area.to_string(),
                    },
                }),
                None,
            ));
        }

        // ── Fixation processing ──
        // Run after Phase 2 actions so drained_alliance_events contains
        // DeathRecorded events (with killer attributions) from this cycle.
        if self.config.fixations_enabled {
            let fixation_indices: Vec<usize> = self
                .characters
                .iter()
                .enumerate()
                .filter(|(_, t)| {
                    t.is_alive()
                        && crate::characters::afflictions::fixation::count_fixations(&t.afflictions)
                            > 0
                })
                .map(|(i, _)| i)
                .collect();

            if !fixation_indices.is_empty() {
                use crate::characters::afflictions::fixation::{
                    FixationContext, process_character_fixations,
                };
                use std::collections::HashMap;
                use uuid::Uuid;

                // Build identifier → UUID lookup.
                let id_to_uuid: HashMap<String, Uuid> = self
                    .characters
                    .iter()
                    .map(|t| (t.identifier.to_string(), t.id))
                    .collect();

                // Build dead-character → killer lookup from drained alliance events.
                let mut dead_character_killers: HashMap<Uuid, Option<Uuid>> = HashMap::new();
                for event in &drained_alliance_events {
                    if let crate::characters::alliances::AllianceEvent::DeathRecorded {
                        deceased,
                        killer,
                    } = event
                    {
                        dead_character_killers.insert(*deceased, *killer);
                    }
                }

                // Closed areas (lowercased for matching).
                let closed_areas: std::collections::BTreeSet<String> = self
                    .areas
                    .iter()
                    .filter(|a| !a.is_open())
                    .filter_map(|a| a.area.map(|area| area.to_string().to_lowercase()))
                    .collect();

                // All item IDs still present in the game.
                let all_item_ids: std::collections::BTreeSet<String> = self
                    .characters
                    .iter()
                    .flat_map(|t| t.items.iter().map(|i| i.identifier.to_string()))
                    .chain(
                        self.areas
                            .iter()
                            .flat_map(|a| a.items.iter().map(|i| i.identifier.to_string())),
                    )
                    .collect();

                // Character identifier → area name mapping for same-area contact checks.
                let character_areas: HashMap<String, String> = self
                    .characters
                    .iter()
                    .map(|t| (t.identifier.to_string(), t.area.to_string()))
                    .collect();

                let fix_ctx = FixationContext {
                    cycle: (current_day.saturating_sub(1)) * 4 + phase.ord() as u32,
                    dead_character_killers: &dead_character_killers,
                    id_to_uuid: &id_to_uuid,
                    character_areas: &character_areas,
                    closed_areas: &closed_areas,
                    all_item_ids: &all_item_ids,
                };

                for idx in &fixation_indices {
                    let msgs = process_character_fixations(&mut self.characters[*idx], &fix_ctx);
                    for msg in msgs {
                        let line = fixation_message_line(&msg, &self.characters[*idx].name);
                        collected_events.push((
                            self.characters[*idx].identifier.to_string(),
                            self.characters[*idx].name.clone(),
                            line,
                            Some(msg),
                            None,
                        ));
                    }
                }
            }
        }

        self.flush_character_events(collected_events);

        // Promote drained alliance events into the game queue and process them
        // so betrayal/death cascades take effect before the next cycle.
        if !drained_alliance_events.is_empty() {
            self.alliance_events.append(&mut drained_alliance_events);
            self.process_alliance_events(rng);
        }
        Ok(())
    }

    /// Drain collected per-character events into `self.messages`.
    ///
    /// Each contiguous run of events sharing the same `identifier` is one
    /// character action and gets a single fresh tick from `self.tick_counter`.
    /// Per-event `emit_index` (advanced inside `push_message`) preserves
    /// intra-character ordering. Sites carrying a typed `MessagePayload` push
    /// that payload directly; legacy stringly sites synthesise a fallback.
    ///
    /// Message coalescing (spec §11.5): repeated `MovedTo` events for the
    /// same area within a phase collapse into a single emission — only the
    /// first is kept, duplicates are silently dropped.
    pub(super) fn flush_character_events(&mut self, collected_events: Vec<CollectedEvent>) {
        use crate::messages::MessagePayload;

        let mut last_identifier: Option<String> = None;
        let mut current_tick: u32 = self.tick_counter.boundary();
        // Track last MovedTo area per character for coalescing.
        let mut last_move_area: HashMap<String, String> = HashMap::new();

        for (identifier, _name, content, payload, _event) in collected_events {
            // Coalesce: skip CharacterMoved if same destination as last move for this character.
            if let Some(MessagePayload::CharacterMoved { to: dest_area, .. }) = &payload
                && let Some(last_area) = last_move_area.get(&identifier)
                && last_area == &dest_area.name
            {
                continue;
            }

            // Record the destination area for future coalescing.
            if let Some(MessagePayload::CharacterMoved { to: dest_area, .. }) = &payload {
                last_move_area.insert(identifier.clone(), dest_area.name.clone());
            }

            if last_identifier.as_ref() != Some(&identifier) {
                current_tick = self.tick_counter.next();
                last_identifier = Some(identifier.clone());
            }
            let source = crate::messages::MessageSource::Character(identifier.clone());
            let payload = payload.unwrap_or_else(|| Self::fallback_payload(&source));
            self.push_message(source, identifier, content, payload, current_tick);
        }
    }

    /// Runs the characters' logic for the current cycle.
    ///
    /// Thin wrapper that builds the immutable `CycleContext` from `&self`
    /// then runs `execute_cycle` with `&mut self`. The split makes the
    /// "snapshot" and "mutate" halves separately testable and gives
    /// gamemaker overrides a typed seam to inject suggestions.
    pub(super) fn run_character_cycle(
        &mut self,
        phase: crate::messages::Phase,
        rng: &mut SmallRng,
        closed_areas: Vec<Area>,
        living_characters: Vec<Character>,
        living_characters_count: usize,
    ) -> Result<(), GameError> {
        // Lazy-spawn patrons for in-progress games created before patronship landed.
        if self.patrons.is_empty() {
            self.spawn_patrons(rng);
        }

        let ctx = self.build_cycle_context(
            phase,
            closed_areas,
            living_characters,
            living_characters_count,
        );

        // Snapshot message count so we can isolate this cycle's payloads for
        // the patronship translator.
        let pre_cycle_msg_len = self.messages.len();

        self.execute_cycle(ctx, rng)?;

        // Patronship PR1: translate cycle messages → AudienceEvents and update affinities.
        // PR2: resolve gifts and deliver them.
        let cycle_payloads: Vec<shared::messages::MessagePayload> = self
            .messages
            .iter()
            .skip(pre_cycle_msg_len)
            .map(|m| m.payload.clone())
            .collect();
        {
            let ctx = crate::patrons::PatronContext::new(self);
            let mut all_events = Vec::new();
            for p in &cycle_payloads {
                all_events.extend(crate::patrons::translate(p, &ctx));
            }
            crate::patrons::update_affinities(self, &all_events);

            // Gift resolution: patrons with high affinity and remaining budget
            // may deliver items to affected characters.
            let gifts = crate::patrons::resolve_gifts(self, &all_events, rng);
            for gift in gifts {
                let recipient_id = match &gift.payload {
                    shared::messages::MessagePayload::PatronGift { recipient, .. } => {
                        recipient.identifier.clone()
                    }
                    _ => unreachable!(),
                };
                let line = crate::output::GameOutput::PatronGift(recipient_id.as_ref(), &gift.item)
                    .to_string();
                let source = crate::messages::MessageSource::Game(self.identifier.clone());
                let subject = format!("patron_gift:{recipient_id}");
                let tick = self.tick_counter.next();
                let payload = gift.payload;
                if let Some(character) = self
                    .characters
                    .iter_mut()
                    .find(|t| recipient_id == t.identifier)
                {
                    character.add_item(gift.item);
                }
                self.push_message(source, subject, line, payload, tick);
            }
        }

        Ok(())
    }

    /// Runs a cycle of the game, either day or night.
    /// 1. Announce area events.
    /// 2. Open an area if there are no open areas.
    /// 3. Trigger any events for this cycle if we're past the first three days.
    /// 4. Trigger Feast Day events.
    /// 5. Close more areas by spawning more events if the characters are getting low.
    /// 6. Run the character cycle.
    /// 7. Update the characters in the game.
    pub(super) fn do_a_cycle(&mut self, phase: crate::messages::Phase) -> Result<(), GameError> {
        let mut rng = SmallRng::from_rng(&mut rand::rng());

        // Announce area events
        self.announce_area_events()?;

        // If there are no open areas, we need to open one.
        self.ensure_open_area();

        // Trigger any events for this cycle
        self.trigger_cycle_events(phase, &mut rng)?;

        // If the character count is low, constrain them by closing areas.
        self.constrain_areas(&mut rng)?;

        self.characters.shuffle(&mut rng);
        let closed_areas: Vec<Area> = self
            .closed_areas()
            .iter()
            .filter_map(|ad| ad.area)
            .clone()
            .collect();
        let living_characters = self.living_characters();
        let living_characters_count: usize = living_characters.len();

        self.run_character_cycle(
            phase,
            &mut rng,
            closed_areas,
            living_characters,
            living_characters_count,
        )?;
        Ok(())
    }

    /// Any characters who have died in the current cycle will be moved to the "dead" list,
    /// and their items will be added to the area they died in.
    pub(super) fn clean_up_recent_deaths(&mut self) {
        let character_count = self.characters.len();

        for i in 0..character_count {
            // Using a for loop to avoid mutable borrow issues
            if self.characters[i].is_alive() {
                continue;
            }
            let character_items: Vec<Item> = self.characters[i].items.clone();

            if self.characters[i].status == CharacterStatus::RecentlyDead {
                self.characters[i].statistics.day_killed = self.day;
                let character_area = self.characters[i].area;

                if let Some(area) = self.get_area_details_mut(character_area) {
                    for item in character_items {
                        area.add_item(item.clone());
                    }
                }
            }

            self.characters[i].dies();
        }
    }

    /// Get a mutable reference to the area details for a given area.
    pub(super) fn get_area_details_mut(&mut self, area: Area) -> Option<&mut AreaDetails> {
        self.areas.iter_mut().find(|ad| ad.area == Some(area))
    }
}
