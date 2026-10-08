use crate::areas::events::AreaEvent;
use crate::areas::{Area, AreaDetails};
use crate::characters::actions::Action;
use crate::characters::statuses::CharacterStatus;
use crate::characters::{ActionSuggestion, Character};
use rand::Rng;
use rand::RngExt;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use shared::GameStatus;
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::fmt::Display;
use uuid::Uuid;
use world::items::Item;
use world::items::OwnsItems;

pub mod alliances;
pub mod cycle_helpers;
pub mod messages;
pub mod patrons;

/// Stamina restored per phase to a sleeping character (PR2c.1).
/// Placeholder pending observability tuning per spec
/// `2026-05-03-four-phase-day-design.md` §6.4.
const SLEEP_STAMINA_PER_PHASE: u32 = 25;
/// HP restored per phase to a sleeping character, gated on absence of
/// Wounded / Infected / Sick.
const SLEEP_HP_PER_PHASE: u32 = 5;

/// Project a game-side `AreaEvent` onto the cross-cutting
/// `shared::messages::AreaEventKind` taxonomy used by sleep-interruption
/// payloads (PR2c.2). The mapping collapses several seasonal /
/// terrain variants into the same broad "kind" so downstream consumers
/// (UI badges, announcer prompts) can treat them uniformly.
fn area_event_to_kind(ev: &AreaEvent) -> shared::messages::AreaEventKind {
    use shared::messages::AreaEventKind as K;
    match ev {
        AreaEvent::Wildfire | AreaEvent::Sandstorm => K::Fire,
        AreaEvent::Flood | AreaEvent::Drought => K::Flood,
        AreaEvent::Earthquake
        | AreaEvent::Avalanche
        | AreaEvent::Landslide
        | AreaEvent::Rockslide
        | AreaEvent::Sinkhole => K::Earthquake,
        AreaEvent::Blizzard | AreaEvent::Heatwave => K::Storm,
    }
}

/// Generate a human-readable line for trauma-related messages.
fn format_trauma_message(
    payload: &shared::messages::MessagePayload,
    character_name: &str,
) -> String {
    use shared::messages::MessagePayload;
    match payload {
        MessagePayload::TraumaFlashback { source, .. } => {
            format!("{character_name} is haunted by {source}.")
        }
        MessagePayload::TraumaObserved {
            observer, subject, ..
        } => {
            format!("{observer} witnesses {subject}'s distress.")
        }
        MessagePayload::TraumaHabituated {
            from_severity,
            to_severity: Some(to),
            ..
        } => {
            format!("{character_name}'s trauma response weakens from {from_severity} to {to}.")
        }
        MessagePayload::TraumaHabituated {
            from_severity,
            to_severity: None,
            ..
        } => {
            format!("{character_name} begins to heal from {from_severity} trauma.")
        }
        _ => String::new(),
    }
}

/// Generate a human-readable line for fixation-related messages.
fn fixation_message_line(
    payload: &shared::messages::MessagePayload,
    character_name: &str,
) -> String {
    use shared::messages::MessagePayload;
    match payload {
        MessagePayload::FixationConsummated { target, .. } => {
            format!("{character_name}'s fixation on {target} is consummated!")
        }
        MessagePayload::FixationThwarted { target, reason, .. } => {
            format!("{character_name}'s fixation on {target} is thwarted ({reason}).")
        }
        MessagePayload::FixationFaded { target, .. } => {
            format!("{character_name}'s fixation on {target} fades away.")
        }
        _ => String::new(),
    }
}

/// Generate a human-readable line for phobia-related messages.
fn phobia_message_line(payload: &shared::messages::MessagePayload, character_name: &str) -> String {
    use shared::messages::MessagePayload;
    match payload {
        MessagePayload::PhobiaEscalated {
            trigger,
            from_severity,
            to_severity,
            ..
        } => {
            format!(
                "{character_name}'s fear of {trigger} intensifies from {from_severity} to {to_severity}."
            )
        }
        MessagePayload::PhobiaHabituated {
            trigger,
            from_severity,
            to_severity,
            ..
        } => {
            if let Some(to) = to_severity {
                format!("{character_name}'s fear of {trigger} fades from {from_severity} to {to}.")
            } else {
                format!("{character_name} has overcome their fear of {trigger}.")
            }
        }
        MessagePayload::PhobiaObserved {
            observer,
            subject,
            trigger,
            ..
        } => {
            format!("{observer} sees {subject}'s fear of {trigger}.")
        }
        MessagePayload::PhobiaForgotten {
            observer,
            subject,
            trigger,
            ..
        } => {
            format!("{observer} no longer remembers {subject}'s fear of {trigger}.")
        }
        _ => String::new(),
    }
}

/// Generate a human-readable line for addiction-related messages.
fn format_addiction_message(
    payload: &shared::messages::MessagePayload,
    character_name: &str,
) -> String {
    use shared::messages::MessagePayload;
    match payload {
        MessagePayload::AddictionObserved {
            observer, subject, ..
        } => {
            format!("{observer} notices {subject}'s addiction behavior.")
        }
        MessagePayload::AddictionForgotten {
            observer, subject, ..
        } => {
            format!("{observer} no longer remembers {subject}'s addiction.")
        }
        MessagePayload::AddictionHabituated {
            from_severity,
            to_severity: Some(to),
            ..
        } => {
            format!("{character_name}'s addiction weakens from {from_severity} to {to}.")
        }
        MessagePayload::AddictionHabituated {
            from_severity,
            to_severity: None,
            ..
        } => {
            format!("{character_name} overcomes their {from_severity} addiction.")
        }
        _ => String::new(),
    }
}

/// Errors that can occur during game operations.
#[derive(Debug, Clone, PartialEq)]
pub enum GameError {
    /// Error related to message operations
    MessageError(String),
    /// Area not found in game
    AreaNotFound(String),
    /// Character not found in game
    CharacterNotFound(String),
}

impl Display for GameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameError::MessageError(msg) => write!(f, "Message error: {}", msg),
            GameError::AreaNotFound(msg) => write!(f, "Area not found: {}", msg),
            GameError::CharacterNotFound(msg) => write!(f, "Character not found: {}", msg),
        }
    }
}

impl std::error::Error for GameError {}

impl From<String> for GameError {
    fn from(error: String) -> Self {
        GameError::MessageError(error)
    }
}

const LOW_CHARACTER_THRESHOLD: u32 = 8;
const FEAST_WEAPON_COUNT: u32 = 2;
const FEAST_SHIELD_COUNT: u32 = 2;
const FEAST_CONSUMABLE_COUNT: u32 = 4;
const DAY_EVENT_FREQUENCY: f64 = 1.0 / 4.0;
const NIGHT_EVENT_FREQUENCY: f64 = 1.0 / 8.0;

/// Per-period tick counter. Resets to 0 at every phase boundary.
/// Phase-boundary side-effect messages get tick=0.
/// First action in a phase gets tick=1.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TickCounter {
    current: u32,
}

impl TickCounter {
    pub fn reset(&mut self) {
        self.current = 0;
    }
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        self.current += 1;
        self.current
    }
    pub fn boundary(&self) -> u32 {
        0
    }
}

/// Represents the current state of the game.
///
/// `PartialEq` is implemented manually (identity-only via `identifier`) because
/// the transient `messages` buffer holds `GameMessage`s carrying
/// `MessagePayload`, which is not `PartialEq` (and adding it would require
/// deriving across the entire payload graph). Cache consumers requiring
/// `PartialEq` can rely on identity equality, which is sufficient for cache
/// dedup since a game is uniquely keyed by its identifier.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Game {
    pub identifier: String,
    pub name: String,
    pub status: GameStatus,
    pub day: Option<u32>,
    #[serde(default)]
    pub areas: Vec<AreaDetails>,
    #[serde(default)]
    pub characters: Vec<Character>,
    pub private: bool,
    #[serde(default)]
    pub config: world::config::GameConfig,
    /// Transient buffer of events emitted during the current cycle.
    /// Drained and persisted by the API layer after each `run_day_night_cycle`.
    /// Skipped during serialization since events live in their own table.
    #[serde(default, skip_serializing)]
    pub messages: Vec<shared::messages::GameMessage>,
    /// Transient queue of alliance lifecycle events drained between character
    /// turns inside `run_day_night_cycle`. Lives only for the duration of a
    /// single cycle; never persisted. See spec §7.5.
    #[serde(default, skip)]
    pub alliance_events: Vec<crate::characters::alliances::AllianceEvent>,
    /// Per-period tick counter; transient, never persisted.
    #[serde(skip, default)]
    pub tick_counter: TickCounter,
    /// Current phase (Day/Night) for the in-flight cycle. Transient, used
    /// by helper logging methods to stamp `GameMessage.phase`. Defaults to
    /// `Day` outside of an active cycle.
    #[serde(skip, default = "default_phase")]
    pub current_phase: shared::messages::Phase,
    /// Per-period emit index for the in-flight cycle. Transient. Reset at
    /// every phase boundary alongside `tick_counter`.
    #[serde(skip, default)]
    pub emit_index: u32,

    /// Tunable combat & stamina knobs. See spec
    /// `2026-05-03-stamina-combat-resource-design.md`.
    #[serde(default)]
    pub combat_tuning: crate::characters::combat_tuning::CombatTuning,

    /// NPC patrons that observe events and build per-character affinity.
    /// Lazily spawned on first cycle for backward-compat with pre-patronship games.
    #[serde(default)]
    pub patrons: Vec<shared::patrons::Patron>,
}

fn default_phase() -> shared::messages::Phase {
    shared::messages::Phase::Day
}

impl PartialEq for Game {
    /// Identity equality: two `Game`s are considered equal iff they share an
    /// `identifier`. See struct docs for rationale.
    fn eq(&self, other: &Self) -> bool {
        self.identifier == other.identifier
    }
}

impl Default for Game {
    /// Creates a new game with a random name.
    fn default() -> Game {
        let wp_gen = crate::witty_phrase_generator::WPGen::new();
        let mut name = String::new();
        if let Some(words) = wp_gen.with_words(3) {
            name = words.join("-").to_string();
        };

        Game {
            identifier: Uuid::new_v4().to_string(),
            name,
            status: Default::default(),
            day: None,
            areas: vec![],
            characters: vec![],
            private: true,
            config: Default::default(),
            messages: vec![],
            alliance_events: vec![],
            tick_counter: TickCounter::default(),
            current_phase: shared::messages::Phase::Day,
            emit_index: 0,
            combat_tuning: crate::characters::combat_tuning::CombatTuning::default(),
            patrons: vec![],
        }
    }
}

impl Display for Game {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// One per-character event collected during `execute_cycle` and drained
/// into `Game::messages` by `flush_character_events`.
///
/// Tuple shape: `(actor_identifier, actor_name, content, payload, optional GameEvent)`.
/// Sites carrying a typed `MessagePayload` (Some) push that payload directly;
/// legacy stringly sites carry `None` and synthesise a fallback in the drain.
type CollectedEvent = (
    String,
    String,
    String,
    Option<shared::messages::MessagePayload>,
    Option<crate::events::GameEvent>,
);

/// Pre-computed, immutable view of cycle inputs produced by
/// `Game::build_cycle_context` and consumed by `Game::execute_cycle`.
///
/// Splitting these out gives gamemaker overrides (and future cycle
/// modifiers) a typed seam between the "snapshot the world" phase and the
/// "iterate `&mut self.characters`" phase.
struct CycleContext {
    /// True for the day half of the cycle, false for night.
    is_day: bool,
    /// Full four-phase value (Dawn / Day / Dusk / Night) for this cycle.
    /// Threaded through to `EnvironmentContext` and the sleep tick handler
    /// so brain scoring and `CharacterSlept` / `CharacterWoke` payloads see the
    /// real phase rather than reconstructing it from `is_day`.
    phase: shared::messages::Phase,
    /// Current game day (1-indexed). Mirrors `Game::day.unwrap_or(1)` and
    /// is forwarded into `EnvironmentContext::current_day`.
    current_day: u32,
    /// Optional global action suggestion (e.g. day-1 spread, day-3
    /// Hub push).
    action_suggestion: Option<ActionSuggestion>,
    /// `Area -> index into self.areas` for O(1) area lookup during the
    /// `&mut self.characters` iteration.
    area_details_map: HashMap<Area, usize>,
    /// Owned per-area character snapshots used to build `EncounterContext`
    /// without re-borrowing `self.characters` during the executor loop.
    characters_by_area: HashMap<Area, Vec<Character>>,
    /// Per-area living-character density (4wnj). Read by
    /// `Brain::choose_destination` as a crowd penalty.
    enemy_density: HashMap<Area, u32>,
    /// Cached combat tuning so the executor never has to re-borrow `self`.
    combat_tuning_snapshot: crate::characters::combat_tuning::CombatTuning,
    /// Read-only snapshot of every area for multi-hop pathfinding.
    all_areas_snapshot: Vec<AreaDetails>,
    /// Areas closed for this cycle, propagated into `EnvironmentContext`.
    closed_areas: Vec<Area>,
    /// Total living character count (used by `EncounterContext`).
    living_characters_count: usize,
}

/// Calculate initiative score for a character: agility + random fuzz (0-20).
/// Higher score = acts earlier in the phase (tm6a).
fn initiative_score(agility: u32, rng: &mut impl Rng) -> u32 {
    agility + rng.random_range(0..=20)
}

impl Game {
    /// Create a new game with a given name.
    pub fn new(name: &str) -> Self {
        Game {
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// Runs at the end of the game.
    ///
    /// NOTE: post-game recovery does NOT fire here — `end()` runs at
    /// the start of a phase, and remaining phases of the day must still see
    /// dead goblins skip their turns. Recovery runs once per game at the
    /// API finish seam (`api::games::run_game_cycles`), right before
    /// characters are written back. Winner determination happens *before*
    /// `end()` in [`Self::check_for_winner`], so recovery never poisons
    /// the team victory.
    pub fn end(&mut self) {
        if self.status == GameStatus::Finished {
            return;
        }
        self.status = GameStatus::Finished;
    }

    /// Run the post-game recovery ([`crate::characters::revival`]) before
    /// characters are written back: every dead goblin runs the revival
    /// roll (Clean / Penalty / Scar / Bonus) and the outcome lands as
    /// persistent state (traits / attribute deltas); every survivor is
    /// restored to full health.
    pub fn post_game_recovery(&mut self) {
        let mut rng = rand::rng();
        for character in &mut self.characters {
            if character.is_alive() {
                crate::characters::revival::restore(character);
            } else {
                crate::characters::revival::revive(character, &mut rng);
            }
        }
    }

    /// Runs at the start of the game.
    pub fn start(&mut self) -> Result<(), GameError> {
        self.status = GameStatus::InProgress;
        Ok(())
    }

    /// Returns the characters that are alive.
    pub fn living_characters(&self) -> Vec<Character> {
        self.characters
            .iter()
            .filter(|t| t.is_alive())
            .cloned()
            .collect()
    }

    /// Returns the count of living characters without cloning.
    pub fn living_characters_count(&self) -> usize {
        self.characters.iter().filter(|t| t.is_alive()).count()
    }

    /// Returns the characters that are recently dead, i.e., died in the current round.
    #[cfg_attr(not(test), allow(dead_code))]
    fn recently_dead_characters(&self) -> Vec<Character> {
        self.characters
            .iter()
            .filter(|t| t.status == CharacterStatus::RecentlyDead)
            .cloned()
            .collect()
    }

    /// Returns the team that has won the game: the only team with living
    /// members. `None` while two or more teams stand or when nobody does —
    /// teammates never have to be the last goblin alive.
    pub fn winning_team(&self) -> Option<u32> {
        let mut teams: std::collections::BTreeSet<u32> = self
            .characters
            .iter()
            .filter(|t| t.is_alive())
            .map(|t| t.team)
            .collect();
        match teams.len() {
            1 => teams.pop_first(),
            _ => None,
        }
    }

    /// Returns a random area from the game.
    fn random_area(&mut self) -> Option<&mut AreaDetails> {
        self.areas.choose_mut(&mut rand::rng())
    }

    /// Returns a random open area from the game.
    fn random_open_area(&self) -> Option<AreaDetails> {
        self.open_areas().choose(&mut rand::rng()).cloned()
    }

    /// Returns a vec of open areas.
    fn open_areas(&self) -> Vec<AreaDetails> {
        self.areas.iter().filter(|a| a.is_open()).cloned().collect()
    }

    /// Returns a vec of closed areas.
    fn closed_areas(&self) -> Vec<AreaDetails> {
        self.areas
            .iter()
            .filter(|a| !a.is_open())
            .cloned()
            .collect()
    }

    fn check_for_winner(&mut self) -> Result<(), GameError> {
        // Already finished: never re-emit GameEnded (revival happens later
        // at the API finish seam — see `Game::post_game_recovery`).
        if self.status == GameStatus::Finished {
            return Ok(());
        }
        if let Some(team) = self.winning_team() {
            let game_id = self.identifier.clone();
            let label = world::clans::team_label(team);
            let payload = shared::messages::MessagePayload::GameEnded {
                winner: None,
                winning_team: Some(shared::messages::TeamRef {
                    team,
                    label: label.clone(),
                }),
            };
            let tick = self.tick_counter.boundary();
            self.push_message(
                shared::messages::MessageSource::Game(game_id.clone()),
                format!("game:{}", game_id),
                format!("{label} win the game!"),
                payload,
                tick,
            );
            self.end();
        } else if self.living_characters_count() == 0 {
            let game_id = self.identifier.clone();
            let payload = shared::messages::MessagePayload::GameEnded {
                winner: None,
                winning_team: None,
            };
            let tick = self.tick_counter.boundary();
            self.push_message(
                shared::messages::MessageSource::Game(game_id.clone()),
                format!("game:{}", game_id),
                "The game has ended with no survivors.".to_string(),
                payload,
                tick,
            );
            self.end();
        }
        Ok(())
    }

    /// Prepares the game state for a new cycle.
    /// Clears old messages and area events.
    /// Increments day count by 1 if this is the first phase of a new day.
    /// (Day 1 starts at `Phase::Day`; Day 2+ starts at `Phase::Dawn`.)
    fn prepare_cycle(&mut self, phase: shared::messages::Phase) -> Result<(), GameError> {
        if self.is_new_day_boundary(phase) {
            self.day = Some(self.day.unwrap_or(0) + 1);
        }

        // Clear all events from the previous cycle
        for area in self.areas.iter_mut() {
            area.events.clear();
        }
        Ok(())
    }

    /// True when this phase begins a new game-day. Day 1 begins at
    /// `Phase::Day` (no Dawn1 per spec §3); Day 2+ begins at `Phase::Dawn`.
    fn is_new_day_boundary(&self, phase: shared::messages::Phase) -> bool {
        matches!(
            (self.day, phase),
            (None | Some(0), shared::messages::Phase::Day)
                | (Some(_), shared::messages::Phase::Dawn)
        )
    }

    /// Announces the start of the cycle.
    fn announce_cycle_start(&mut self, phase: shared::messages::Phase) -> Result<(), GameError> {
        let current_day = self.day.unwrap_or(1);
        let game_id = self.identifier.clone();
        let subject = format!("game:{}", game_id);

        let content = match phase {
            shared::messages::Phase::Dawn => {
                format!("Dawn {} breaks pale over the arena.", current_day)
            }
            shared::messages::Phase::Day => match current_day {
                1 => format!("Day {}: The games have begun!", current_day),
                3 => format!(
                    "Day {}: Patrons take note of the remaining goblins.",
                    current_day
                ),
                _ => format!("Day {} dawns over the arena.", current_day),
            },
            shared::messages::Phase::Dusk => {
                format!("Dusk {} settles in long shadows.", current_day)
            }
            shared::messages::Phase::Night => {
                format!("Night {} falls. The arena grows dark.", current_day)
            }
        };

        let payload = shared::messages::MessagePayload::CycleStart {
            day: current_day,
            phase,
        };
        let tick = self.tick_counter.boundary();
        self.push_message(
            shared::messages::MessageSource::Game(game_id.clone()),
            subject.clone(),
            content.clone(),
            payload,
            tick,
        );

        // PhaseStarted: four-phase day substrate (spec §4 step 4).
        // Reuse the same message — no duplicate content string.
        let phase_payload = shared::messages::MessagePayload::PhaseStarted {
            day: current_day,
            phase,
            weather_summary: None,
        };
        self.push_message(
            shared::messages::MessageSource::Game(game_id),
            subject,
            String::new(),
            phase_payload,
            tick,
        );

        Ok(())
    }

    /// Announces the end of a cycle
    fn announce_cycle_end(&mut self, phase: shared::messages::Phase) -> Result<(), GameError> {
        let game_id = self.identifier.clone();
        let current_day = self.day.unwrap_or(1);

        // Death announcements are now emitted at the kill site as typed
        // `MessagePayload::CharacterKilled` (combat: `Combat(Killed)`; env /
        // status: `CharacterKilled`). Re-announcing "X has fallen" here
        // duplicated those events with a `SanityBreak` fallback payload
        // which mis-classified them in the timeline.

        let payload = shared::messages::MessagePayload::CycleEnd {
            day: current_day,
            phase,
        };
        let tick = self.tick_counter.boundary();
        self.push_message(
            shared::messages::MessageSource::Game(game_id.clone()),
            format!("game:{}", game_id),
            format!("End of {} {}.", phase, current_day),
            payload,
            tick,
        );

        // PhaseEnded: four-phase day substrate (spec §4 step 4).
        // Reuse the same message — no duplicate content string.
        let phase_payload = shared::messages::MessagePayload::PhaseEnded {
            day: current_day,
            phase,
        };
        self.push_message(
            shared::messages::MessageSource::Game(game_id.clone()),
            format!("game:{}", game_id),
            String::new(),
            phase_payload,
            tick,
        );
        Ok(())
    }

    /// Process survival checks for all characters in an area when an event occurs
    pub fn process_event_for_area(
        &mut self,
        area: &Area,
        event: &AreaEvent,
        rng: &mut impl Rng,
    ) -> Result<(), GameError> {
        // Get area terrain and events
        let (terrain, area_events) = {
            let area_idx = self
                .areas
                .iter()
                .position(|a| a.area.as_ref() == Some(area));

            match area_idx {
                Some(idx) => (self.areas[idx].terrain.base, self.areas[idx].events.clone()),
                None => return Ok(()), // Area not found
            }
        };

        // Find all alive characters in this area
        let character_indices: Vec<usize> = self
            .characters
            .iter()
            .enumerate()
            .filter(|(_, t)| t.is_alive() && t.area == area)
            .map(|(idx, _)| idx)
            .collect();

        if character_indices.is_empty() {
            return Ok(()); // No characters in area, nothing to do
        }

        let most_severe_event = if area_events.len() > 1 {
            // Find event with highest severity
            area_events
                .iter()
                .max_by_key(|e| e.severity_in_terrain(&terrain))
                .cloned()
                .unwrap_or_else(|| event.clone())
        } else {
            event.clone()
        };

        // Announce the event itself in the area channel so the broader narrative
        // captures *what happened* even when no characters are present to react.
        let area_name = area.to_string();
        let area_subject = format!("area:{}", area_name);
        self.log_event(
            shared::messages::MessageSource::Area(area_name.clone()),
            area_subject.clone(),
            crate::events::GameEvent::AreaEvent {
                area_event: most_severe_event.to_string(),
                area_name: area_name.clone(),
            },
        );

        // Process each character's survival check
        for character_idx in character_indices {
            // Sinkhole — instant death, no survival roll
            if matches!(most_severe_event, AreaEvent::Sinkhole) {
                let (name, id, cause) = {
                    let character = &mut self.characters[character_idx];
                    let name = character.name.clone();
                    let id = character.identifier.clone();
                    character.blood = 0;
                    let cause = match most_severe_event {
                        crate::areas::events::AreaEvent::Wildfire => {
                            shared::afflictions::DeathCause::Fire
                        }
                        crate::areas::events::AreaEvent::Flood
                        | crate::areas::events::AreaEvent::Sinkhole => {
                            shared::afflictions::DeathCause::Drowning
                        }
                        crate::areas::events::AreaEvent::Avalanche
                        | crate::areas::events::AreaEvent::Rockslide => {
                            shared::afflictions::DeathCause::Hazard(
                                shared::afflictions::HazardKind::FallingDebris,
                            )
                        }
                        crate::areas::events::AreaEvent::Earthquake
                        | crate::areas::events::AreaEvent::Landslide => {
                            shared::afflictions::DeathCause::Hazard(
                                shared::afflictions::HazardKind::Other,
                            )
                        }
                        _ => shared::afflictions::DeathCause::Hazard(
                            shared::afflictions::HazardKind::Other,
                        ),
                    };
                    character.statistics.killed_by = Some(cause.to_string());
                    character.status = crate::characters::statuses::CharacterStatus::RecentlyDead;
                    (name, id, cause)
                };

                let content = format!("{} falls into a sinkhole and dies.", name);
                let source = shared::messages::MessageSource::Character(id.to_string());
                let subject = format!("character:{}", id);
                let tick = self.tick_counter.next();
                let payload = shared::messages::MessagePayload::CharacterKilled {
                    victim: shared::messages::CharacterRef {
                        identifier: id.clone().into(),
                        name: name.clone(),
                    },
                    killer: None,
                    cause: cause.clone(),
                };
                // Also log to the area channel so the timeline shows the death
                let area_source =
                    shared::messages::MessageSource::Area(most_severe_event.to_string());
                let area_subject = format!("area:{}", most_severe_event);
                self.push_message(
                    area_source,
                    area_subject,
                    content.clone(),
                    payload.clone(),
                    tick,
                );
                self.push_message(source, subject, content, payload, tick);
                continue;
            }

            // Outcome messages we'll log after the character borrow is released.
            // Each entry carries an optional typed payload so death lines
            // become `MessagePayload::CharacterKilled` (and render as
            // DeathCard / count toward timeline death tallies) instead of
            // falling through to the legacy `SanityBreak` fallback.
            type PendingMsg = (
                shared::messages::MessageSource,
                String,
                String,
                Option<shared::messages::MessagePayload>,
            );
            let mut pending_messages: Vec<PendingMsg> = Vec::new();

            {
                let character = &mut self.characters[character_idx];

                // Check modifiers
                let has_affinity = character.terrain_affinity.contains(&terrain);

                // Check for protective items (shields only for physical events)
                let is_physical_event = matches!(
                    most_severe_event,
                    AreaEvent::Avalanche | AreaEvent::Rockslide | AreaEvent::Earthquake
                );
                let has_item_bonus =
                    is_physical_event && character.items.iter().any(|item| item.is_defensive());

                let is_desperate = character.effective_health() < 30;
                let current_health = character.effective_health();

                // Run survival check with config parameters
                let result = most_severe_event.survival_check(
                    &terrain,
                    has_affinity,
                    has_item_bonus,
                    is_desperate,
                    current_health,
                    self.config.instant_death_enabled,
                    self.config.catastrophic_severity_multiplier,
                    rng,
                );

                let source =
                    shared::messages::MessageSource::Character(character.identifier.to_string());
                let subject = format!("character:{}", character.identifier);
                let roll_detail = format!(
                    "[{:?} severity, rolled {}{}]",
                    result.severity,
                    result.roll,
                    if result.modifier == 0 {
                        String::new()
                    } else if result.modifier > 0 {
                        format!(" +{}", result.modifier)
                    } else {
                        format!(" {}", result.modifier)
                    }
                );

                // Apply results
                if !result.survived {
                    character.blood = 0;
                    let cause = match most_severe_event {
                        crate::areas::events::AreaEvent::Wildfire => {
                            shared::afflictions::DeathCause::Fire
                        }
                        crate::areas::events::AreaEvent::Flood => {
                            shared::afflictions::DeathCause::Drowning
                        }
                        crate::areas::events::AreaEvent::Avalanche
                        | crate::areas::events::AreaEvent::Rockslide => {
                            shared::afflictions::DeathCause::Hazard(
                                shared::afflictions::HazardKind::FallingDebris,
                            )
                        }
                        crate::areas::events::AreaEvent::Earthquake
                        | crate::areas::events::AreaEvent::Landslide => {
                            shared::afflictions::DeathCause::Hazard(
                                shared::afflictions::HazardKind::Other,
                            )
                        }
                        _ => shared::afflictions::DeathCause::Hazard(
                            shared::afflictions::HazardKind::Other,
                        ),
                    };
                    character.statistics.killed_by = Some(cause.to_string());
                    // Mark as RecentlyDead so the end-of-cycle announcement
                    // and the alliance-cascade pipeline both pick this death
                    // up. Without this, env-killed characters were silently
                    // promoted to Dead at the next cycle and never triggered
                    // "has fallen" or DeathRecorded.
                    character.status = crate::characters::statuses::CharacterStatus::RecentlyDead;

                    let content = if result.instant_death {
                        format!(
                            "{} is instantly killed by the catastrophic {}! {}",
                            character.name, most_severe_event, roll_detail
                        )
                    } else {
                        format!(
                            "{} dies from the {} {}",
                            character.name, most_severe_event, roll_detail
                        )
                    };
                    let payload = shared::messages::MessagePayload::CharacterKilled {
                        victim: shared::messages::CharacterRef {
                            identifier: character.identifier.clone().into(),
                            name: character.name.clone(),
                        },
                        killer: None,
                        cause,
                    };
                    pending_messages.push((source, subject, content, Some(payload)));
                } else {
                    // Always announce the survival itself so the narrative captures
                    // who weathered the event, even when no rewards land.
                    pending_messages.push((
                        source.clone(),
                        subject.clone(),
                        format!(
                            "{} survives the {} {}",
                            character.name, most_severe_event, roll_detail
                        ),
                        None,
                    ));

                    // Survivor - apply rewards if any
                    if result.stamina_restored > 0 {
                        character.stamina =
                            character.stamina.saturating_add(result.stamina_restored);
                        pending_messages.push((
                            source.clone(),
                            subject.clone(),
                            format!(
                                "{} recovers {} stamina from the {}",
                                character.name, result.stamina_restored, most_severe_event
                            ),
                            None,
                        ));
                    }

                    if result.sanity_restored > 0 {
                        // ponytail: sanity_restored no longer applicable — sanity is now
                        // computed from wounds/conditions. Remove this branch when
                        // survival_check result is cleaned up.
                        pending_messages.push((
                            source.clone(),
                            subject.clone(),
                            format!(
                                "{} recovers {} sanity from the {}",
                                character.name, result.sanity_restored, most_severe_event
                            ),
                            None,
                        ));
                    }

                    if result.reward_item.is_some() {
                        let item = Item::new_random_consumable();
                        let item_name = item.name.clone();
                        character.items.push(item);
                        pending_messages.push((
                            source,
                            subject,
                            format!(
                                "{} finds a {} after surviving the {}",
                                character.name, item_name, most_severe_event
                            ),
                            None,
                        ));
                    }
                }
            }

            // Each env-event message is its own action group, so advance
            // the per-phase tick before pushing so they sort after the
            // area announcement and any prior character actions.
            for (source, subject, content, payload) in pending_messages {
                let tick = self.tick_counter.next();
                let payload = payload.unwrap_or_else(|| Self::fallback_payload(&source));
                self.push_message(source, subject, content, payload, tick);
            }
        }
        Ok(())
    }

    /// Run one phase of the game. Substrate for the four-phase day model
    /// (spec `2026-05-03-four-phase-day-design.md`). Replaces the legacy
    /// `run_day_night_cycle(bool)` boundary; callers driving an entire day
    /// should use `run_full_day` instead.
    pub fn run_phase(&mut self, phase: shared::messages::Phase) -> Result<(), GameError> {
        // Phase boundary: reset transient cycle state. `tick` and `emit_index`
        // are per-phase and must restart at every flip so causal
        // ordering inside a phase is contiguous from zero.
        self.current_phase = phase;
        self.tick_counter.reset();
        self.emit_index = 0;

        // Check if the game is over, and if so, end it.
        self.check_for_winner()?;

        self.prepare_cycle(phase)?;
        self.announce_cycle_start(phase)?;
        self.do_a_cycle(phase)?;
        self.run_trauma_producers(phase);
        self.announce_cycle_end(phase)?;

        // Clean up any deaths
        self.clean_up_recent_deaths();
        Ok(())
    }

    /// Run every phase of the next game-day in canonical order. Day 1 has
    /// no Dawn (per spec §3); Day 2+ runs all four phases.
    pub fn run_full_day(&mut self) -> Result<(), GameError> {
        use shared::messages::Phase;
        let next_day = self.day.unwrap_or(0) + 1;
        let phases: &[Phase] = if next_day <= 1 {
            &[Phase::Day, Phase::Dusk, Phase::Night]
        } else {
            &[Phase::Dawn, Phase::Day, Phase::Dusk, Phase::Night]
        };
        for &p in phases {
            self.run_phase(p)?;
        }
        self.day = Some(next_day);
        Ok(())
    }
}

#[cfg(test)]
mod tests;

mod cycle;
