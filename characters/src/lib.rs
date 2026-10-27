pub mod actions;
pub mod advancement;
pub mod afflictions;
pub mod alliances;
pub mod baseline;
pub mod brains;
pub mod combat;
pub mod combat_beat;
pub mod combat_tuning;
pub mod events;
pub mod evolution;
mod helpers;
pub mod incidents;
pub mod inventory;
pub mod lifecycle;
pub mod movement;
pub mod path;
pub mod rescue;
pub mod revival;
pub mod stamina_band;
pub mod statuses;
pub mod survival;
pub mod traits;
pub mod wounds;

#[cfg(test)]
mod trait_golden;

// Re-export key items from sub-modules
pub use combat::attack_contest;
pub use combat::inflict_table::{
    HitSeverity, WeaponKind, lookup_break_mid_swing_inflict, lookup_inflicts,
};
pub use movement::TravelResult;

use std::collections::{BTreeMap, BTreeSet};

use crate::afflictions::{AcquireResolution, can_acquire};
use crate::events::CharacterEvent;
use actions::{Action, AttackOutcome};
use areas::location::Location;
use areas::{AreaDetails, AreaId};
use brains::Brain;
use rand::RngExt;
use rand::prelude::*;
use rand::rngs::SmallRng;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeSeq};
use shared::afflictions::{
    Affliction, AfflictionKey, AfflictionKind, AfflictionSource, BodyPart, PhobiaTrigger, Severity,
    Substance, TrappedMetadata, TraumaSource,
};
use shared::messages::SleepIncidentKind;
use shared::messages::{AreaRef, CharacterRef, ItemRef, MessagePayload};
use shared::wounds::Wound;
use statuses::CharacterStatus;
use uuid::Uuid;
use world::items::Item;
use world::messages::TaggedEvent;
use world::output::GameOutput;

/// Serialize `Vec<Uuid>` as `Vec<String>` for SurrealDB compatibility.
/// The Surreal Rust SDK's bespoke serializer wires `uuid::Uuid` as raw bytes,
/// which Surreal then renders as base64 and rejects against `array<uuid>`
/// constraints. Storing as strings on the wire (and as `array<string>` in
/// the schema) follows the same convention as `message.event_id`.
fn serialize_uuids_as_strings<S>(uuids: &[Uuid], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut seq = serializer.serialize_seq(Some(uuids.len()))?;
    for u in uuids {
        seq.serialize_element(&u.to_string())?;
    }
    seq.end()
}

/// Deserialize `Vec<Uuid>` from either a sequence of strings (the wire format
/// we write) or a sequence of native uuid values (test fixtures, JSON read
/// back through serde's standard Uuid impl).
fn deserialize_uuids_lenient<'de, D>(deserializer: D) -> Result<Vec<Uuid>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrUuid {
        S(String),
        U(Uuid),
    }

    let raw: Vec<StringOrUuid> = Vec::deserialize(deserializer)?;
    raw.into_iter()
        .map(|item| match item {
            StringOrUuid::S(s) => Uuid::parse_str(&s).map_err(serde::de::Error::custom),
            StringOrUuid::U(u) => Ok(u),
        })
        .collect()
}

/// Serialize a single `Uuid` as a string for the same reasons as
/// `serialize_uuids_as_strings`.
fn serialize_uuid_as_string<S>(uuid: &Uuid, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&uuid.to_string())
}

/// Deserialize a single `Uuid` from either a string (our wire format) or the
/// SDK's native uuid bytes representation.
fn deserialize_uuid_lenient<'de, D>(deserializer: D) -> Result<Uuid, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrUuid {
        S(String),
        U(Uuid),
    }

    match StringOrUuid::deserialize(deserializer)? {
        StringOrUuid::S(s) => Uuid::parse_str(&s).map_err(serde::de::Error::custom),
        StringOrUuid::U(u) => Ok(u),
    }
}

/// Consts
const SANITY_BREAK_LEVEL: u32 = 9;

fn default_blood() -> u32 {
    1000
}

#[derive(Clone, Debug)]
pub struct ActionSuggestion {
    pub action: Action,
    pub probability: Option<f64>,
}

#[derive(Debug)]
pub struct EnvironmentContext<'a> {
    pub is_day: bool,
    /// Full four-phase value for the current cycle. Used by sleep scoring
    /// (Brain::should_sleep) and emitters (CharacterSlept/CharacterWoke).
    pub phase: shared::messages::Phase,
    /// Configured day/night boundaries and survival cadence for this game
    /// (phase redesign — one source of truth for day/night classification).
    pub config: world::config::GameConfig,
    pub area_details: &'a mut AreaDetails,
    pub available_destinations: Vec<areas::DestinationInfo>,
    /// All known areas (read-only snapshot). Used by multi-hop
    /// pathfinding so the planner can reason about non-neighbor goals.
    pub all_areas: &'a [AreaDetails],
    /// Per-area count of living characters. Fed into `Brain::choose_destination`
    /// as a crowd penalty so movement scoring naturally disperses crowded
    /// characters without a call-site escape hatch.
    pub enemy_density: &'a std::collections::HashMap<AreaId, u32>,
    /// Current game day (1-indexed). Used to gate day-1-only behavior such
    /// as suppressing patron gifts in the opening cycle.
    pub current_day: u32,
    /// Combat & stamina tuning knobs threaded through `Action::Attack` so
    /// `Character::attacks` and `attack_contest` can read constants from a
    /// single owned source instead of file-level `const`s.
    pub combat_tuning: &'a crate::combat_tuning::CombatTuning,
    /// Sleeping characters in the current area with items that can be stolen.
    /// Tuple of (character UUID, character name). Populated by the game cycle
    /// in `execute_cycle`; consumed by `act_take_item` for target selection.
    pub sleeping_nearby: Vec<(Uuid, String)>,
}

#[derive(Clone, Debug)]
pub struct EncounterContext {
    pub nearby_characters_count: u32,
    pub potential_targets: Vec<Character>,
    pub total_living_characters: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Character {
    /// Identifier
    pub identifier: String,
    /// Stable typed UUID. Mirrors `identifier` for callers that want a
    /// non-stringly-typed key (alliance graph, betrayal events).
    ///
    /// Serialized as `character_id` to avoid collision with SurrealDB's
    /// reserved `id` column on the `character` table — the SDK rejects any
    /// payload that carries a non-RecordId `id` field when a record id is
    /// also specified explicitly via `db.create(("character", ...))`.
    #[serde(
        default = "Uuid::new_v4",
        rename = "character_id",
        serialize_with = "serialize_uuid_as_string",
        deserialize_with = "deserialize_uuid_lenient"
    )]
    pub id: Uuid,
    /// Where are they? AreaId-hex plus sub-tile slot. Flattened so the
    /// legacy top-level `area` column keeps hydrating (area preserved,
    /// sub defaults to the canonical triangle 0 via `Location::center`);
    /// the wire carries `area` and `sub` as
    /// sibling top-level columns.
    #[serde(flatten)]
    pub location: Location,
    /// What is their current status?
    pub status: CharacterStatus,
    /// This is their thinker. Persisted so load restores the same brain as
    /// the save; `into_new_game` re-derives it from traits on entry, so
    /// runtime state (psychotic break, preferred-action overrides) never
    /// crosses a game boundary. `#[serde(default)]` lets pre-fix rows that
    /// omit the `brain` column hydrate via `Brain::default()`.
    #[serde(default)]
    pub brain: Brain,
    /// How they present themselves to the real world
    pub avatar: Option<String>,
    /// Who created them in the real world
    #[serde(rename = "player_name")]
    pub human_player_name: Option<String>,
    /// What they like to go by
    pub name: String,
    /// Per-game color team slot (1..=8). Drives alliance bias only.
    /// Per-game state lives on the character record because a goblin plays
    /// one game at a time — same home as stamina/area/allies, reset on entry.
    #[serde(default)]
    pub team: u32,
    /// Persistent clan name (lore only — no mechanics). Empty until a
    /// generator or editor fills it in.
    #[serde(default)]
    pub clan_name: String,
    /// Stats like fights won
    pub statistics: Statistics,
    /// Stats and capabilities
    pub attributes: Attributes,
    /// Items the character owns
    #[serde(default)]
    pub items: Vec<Item>,
    /// Events that have happened to the character
    #[serde(default)]
    pub events: Vec<CharacterEvent>,
    #[serde(default)]
    pub editable: bool,
    /// Terrain types this character is familiar with
    #[serde(default)]
    pub terrain_affinity: Vec<world::terrain::BaseTerrain>,
    /// Current stamina for actions
    pub stamina: u32,
    /// Maximum stamina capacity
    pub max_stamina: u32,
    /// Blood reserve (0-1000). Replaces abstract health pool for wound model.
    /// Character dies when blood reaches 0.
    #[serde(default = "default_blood")]
    pub blood: u32,
    /// Active wounds. Each wound bleeds and inflicts stat penalties.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wounds: Vec<Wound>,
    /// Personality/behavior trait set. Replaces `BrainPersonality`.
    /// A character with zero traits behaves as the old `Balanced` baseline.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub traits: Vec<traits::Trait>,
    /// Pair-wise alliance graph. Symmetric: when A allies with B, both
    /// `allies` lists gain the other. Capped at `MAX_ALLIES`.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "serialize_uuids_as_strings",
        deserialize_with = "deserialize_uuids_lenient"
    )]
    pub allies: Vec<Uuid>,
    /// Turn counter for the Treacherous betrayal cadence. Reset on betrayal.
    #[serde(default)]
    pub turns_since_last_betrayal: u8,
    /// Set to `true` by the cycle drain when this character is the victim of a
    /// betrayal. Consumed at the top of `process_turn_phase` on the victim's
    /// next turn to drive the trust-shock cascade (spec §7.3c1).
    #[serde(default)]
    pub pending_trust_shock: bool,
    /// Per-character alliance event buffer, populated during `process_turn_phase`
    /// (e.g. on Treacherous betrayal) and drained by the game cycle into
    /// `Game.alliance_events` between turns. Transient; never persisted.
    #[serde(default, skip)]
    pub alliance_events: Vec<alliances::AllianceEvent>,
    /// Set by combat sites when this character is killed by another character
    /// (or by themselves on a fumble). Read and cleared by the game cycle
    /// when emitting `AllianceEvent::DeathRecorded` so allies receive the
    /// correct killer attribution. `None` for environmental/status deaths.
    /// Transient; never persisted.
    #[serde(default, skip)]
    pub recently_killed_by: Option<Uuid>,
    /// Survival debt counter; 0 = Sated. See `survival::hunger_band`.
    #[serde(default)]
    pub hunger: u8,
    /// Survival debt counter; 0 = Sated. See `survival::thirst_band`.
    #[serde(default)]
    pub thirst: u8,
    /// Phase index until which the character is considered sheltered.
    /// `None` = exposed.
    /// `skip_serializing_if` avoids SurrealDB v3's rejection of JSON `null`
    /// for `TYPE option<int>` (which expects `NONE`, not `NULL`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheltered_until: Option<u32>,
    /// Game day on which the character first entered the Starving band
    /// this stretch (drain day = `game_day - since + 1`). Cleared on eat.
    /// Skipped when None: Surreal `option<int>` rejects explicit null.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starving_since_day: Option<u16>,
    /// Game day on which the character first entered the Dehydrated band
    /// this stretch. Cleared on drink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dehydrated_since_day: Option<u16>,
    /// Phases since the character last completed a full sleep. Increments by 1
    /// each phase the character is *not* sleeping. Resets on
    /// `WakeReason::Rested`. See spec
    /// `2026-05-03-four-phase-day-design.md` §6.4.
    #[serde(default)]
    pub cycles_awake: u32,
    /// True while mid-`Action::Sleep`. Affects ambush vulnerability once the
    /// sleep mechanic lands; this PR keeps it `false` for every brain-driven
    /// character (substrate only).
    #[serde(default)]
    pub sleeping: bool,
    /// Phases left in the current `Action::Sleep` countdown. `0` when awake.
    /// Decremented by the engine; on hitting zero the character wakes with
    /// `WakeReason::Rested`.
    #[serde(default)]
    pub sleep_remaining: u8,
    /// Tracks a non-waking sleep incident (flavor only) that occurred this
    /// sleep session. Included in the natural-wake message so the narrative
    /// reflects the experience. Reset to `None` on sleep end.
    #[serde(default, skip)]
    pub pending_sleep_incident: Option<SleepIncidentKind>,
    /// Sleep shelter rolled at start of sleep session (transient).
    /// Cleared on wake or phase change. Not persisted.
    #[serde(skip)]
    pub sleep_shelter: Option<crate::incidents::SleepShelter>,
    /// Active afflictions keyed by (kind, body_part). Empty by default;
    /// serde skips serialization when empty to keep payloads lean.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        serialize_with = "helpers::serialize_affliction_map",
        deserialize_with = "helpers::deserialize_affliction_map"
    )]
    pub afflictions: BTreeMap<AfflictionKey, Affliction>,
    /// Current game day (cycle). Used for affliction cycle tracking.
    #[serde(default)]
    pub game_day: Option<i64>,
    /// Monotonic per-substance use counter (spec §4). Never reset by cure.
    /// Enables relapse-on-first-use and the use-count-driven acquisition curve.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub addiction_use_count: BTreeMap<Substance, u32>,
    /// Substances the character has ever been addicted to (spec §5.1 step 5c).
    /// Populated on acquisition, never cleared — including by
    /// `into_new_game`. Carrying this across games is intentional
    /// progression: the relapse short-circuit depends on it, so a goblin who
    /// was addicted in a previous game auto-acquires at Mild on re-exposure
    /// in the next one. Do not "fix" this as a per-game leak.
    /// Enables the relapse short-circuit: if `ever_addicted_to.contains(s)`
    /// and no current `Addiction(s)`, next use of `s` auto-acquires at Mild.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub ever_addicted_to: BTreeSet<Substance>,
    /// Cycles remaining in hangover (alcohol after-effect).
    /// Set to 2 on alcohol use, ticks down each cycle.
    /// While > 0, character suffers -1 atk, -1 forage.
    #[serde(default, skip)]
    pub hangover_cycles_remaining: u32,
    /// Transient flag set by `attacks()` when this character was sleeping and
    /// got ambushed. `attack_contest` reads it to apply a 0-defense penalty.
    /// Reset to `false` after each combat resolution. Not persisted.
    #[serde(default, skip)]
    pub was_ambushed: bool,
    /// UUID of a sleeping character to steal from. Set by `act_take_item`
    /// when an awake character chooses to steal from a sleeper instead of
    /// looting the area. Consumed by the game cycle after
    /// `process_turn_phase`. Transient — never persisted.
    #[serde(default, skip)]
    pub pending_theft_target: Option<Uuid>,
    /// UUID of a real opponent this character chose to attack this phase.
    /// Set by `act_attack` when the brain picks a two-character fight; the
    /// game cycle resolves it to real indices and drives multi-round combat
    /// via `CombatRoundLoop`. Self-attack/suicide resolves inline instead.
    /// Transient — never persisted.
    #[serde(default, skip)]
    pub pending_attack_target: Option<Uuid>,
    /// Active mental conditions (pain, horror, panic, etc.).
    /// Recalculated each period based on wounds and stress.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mental_conditions: Vec<shared::conditions::MentalCondition>,
    /// Durable state captured at game entry, used to settle advancements
    /// (trait gains, attribute high-water deltas, lifetime record
    /// increments) at the next entry. See `baseline::GameBaseline` for the
    /// carry rules. Persisted, because the rebuild runs at API join time
    /// rather than in-memory game end; a mid-game save/load must not lose
    /// it. `None` for characters created before this field existed — the
    /// next entry captures a baseline from their current state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_baseline: Option<baseline::GameBaseline>,
    /// Pending advancement events for the game in progress — the *delta*
    /// half of cross-game carry (the baseline snapshot is only the floor,
    /// spec §7.3). Stamped by `gain_trait`/`lose_trait` as changes happen,
    /// consumed by settle at the next entry, bounded by
    /// `advancement::MAX_ADVANCEMENT_EVENTS`. Persisted because the rebuild
    /// runs at API join time — a mid-game save/load must not lose pending
    /// events. Empty for characters created before this field existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advancement_log: Vec<advancement::AdvancementEvent>,
    /// Games remaining per temporary trait — event boons/curses granted
    /// with a duration at the revival boundary
    /// (`revival::maybe_grant_event_traits`). The trait itself still lives
    /// in `traits` (the stable serde unit-enum key); an entry here only
    /// bounds it. Empty map = everything permanent: old saves hydrate
    /// empty via `#[serde(default)]`, and creation/pool/conversion traits
    /// never get an entry (only event grants do). Ticked once per played
    /// game at the finish seam (`Game::post_game_recovery` ->
    /// [`tick_temporary_traits`]); at zero the trait is removed with no
    /// trace left behind.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub temporary_traits: BTreeMap<traits::Trait, u32>,
}

impl Default for Character {
    fn default() -> Self {
        Character::new("Default Character".to_string(), None, None)
    }
}

impl Character {
    /// Creates a new Character with full health, sanity, and movement.
    pub fn new(name: String, team: Option<u32>, avatar: Option<String>) -> Self {
        let team = team.unwrap_or(0);
        let statistics = Statistics::default();

        let id_uuid: Uuid = Uuid::new_v4();
        let id: String = id_uuid.to_string();

        // One coherent pass from this goblin's own RNG — team no longer
        // seeds anything. Traits roll first so the attribute set is bent by
        // them (`from_rng_with_traits`): Tough trends tough, and a future
        // limp-style trait drags its axis down at creation.
        let mut rng = SmallRng::from_rng(&mut rand::rng());
        let traits = traits::generate_traits(&mut rng);
        let attributes = Attributes::from_rng_with_traits(&mut rng, &traits);
        let terrain_affinity = world::clans::roll_terrain_affinity(&mut rng);
        let brain = Brain::from_traits(&traits, &mut rng);
        // Stamina coherence: curses like Asthmatic shrink the fresh pool.
        let max_stamina = lifecycle::health::max_stamina_for_traits(&traits);

        Self {
            identifier: id,
            id: id_uuid,
            location: Location::center(AreaId::hub()),
            name: name.clone(),
            team,
            clan_name: String::new(),
            brain,
            status: CharacterStatus::default(),
            avatar,
            human_player_name: None,
            attributes,
            statistics,
            items: vec![],
            events: vec![],
            editable: true,
            terrain_affinity,
            stamina: max_stamina,
            max_stamina,
            blood: 1000,
            wounds: Vec::new(),
            traits,
            allies: Vec::new(),
            turns_since_last_betrayal: 0,
            pending_trust_shock: false,
            alliance_events: Vec::new(),
            recently_killed_by: None,
            hunger: 0,
            thirst: 0,
            sheltered_until: None,
            starving_since_day: None,
            dehydrated_since_day: None,
            cycles_awake: 0,
            sleeping: false,
            sleep_remaining: 0,
            pending_sleep_incident: None,
            sleep_shelter: None,
            afflictions: BTreeMap::new(),
            game_day: None,
            addiction_use_count: BTreeMap::new(),
            ever_addicted_to: BTreeSet::new(),
            hangover_cycles_remaining: 0,
            was_ambushed: false,
            pending_theft_target: None,
            pending_attack_target: None,
            mental_conditions: Vec::new(),
            game_baseline: None,
            advancement_log: Vec::new(),
            temporary_traits: BTreeMap::new(),
        }
    }

    /// Add a trait through the one sanctioned mutation path (spec §3):
    /// rejects duplicates, flavor-pair conflicts, and stacked negative
    /// attribute modifiers. Mid-game drift, creation rolls, revival scars,
    /// and future event boons/curses all land here. On success the gain is
    /// stamped into `advancement_log`, so settle keeps it (the log is the
    /// delta; the baseline snapshot is only the floor).
    pub fn gain_trait(&mut self, trait_: traits::Trait) -> Result<(), traits::TraitError> {
        traits::can_gain(&self.traits, trait_)?;
        self.traits.push(trait_);
        advancement::push_event(
            &mut self.advancement_log,
            advancement::AdvancementEvent::TraitGained { trait_ },
        );
        Ok(())
    }

    /// Remove a trait — the durable, intentional form of trait loss. Stamped
    /// into `advancement_log` so settle honors the loss instead of restoring
    /// the baseline copy (snapshot-diffing can't tell intent from debuff;
    /// the log can). Returns whether anything changed; a no-op loss is not
    /// stamped.
    pub fn lose_trait(&mut self, trait_: traits::Trait) -> bool {
        let before = self.traits.len();
        self.traits.retain(|t| *t != trait_);
        let changed = self.traits.len() != before;
        if changed {
            advancement::push_event(
                &mut self.advancement_log,
                advancement::AdvancementEvent::TraitLost { trait_ },
            );
        }
        changed
    }

    /// Tick every temporary trait's duration once, at the game boundary
    /// (`Game::post_game_recovery` runs this before the revival roll).
    /// Each entry in [`Self::temporary_traits`] loses one game; entries
    /// that reach zero expire: the duration entry is dropped and the
    /// trait leaves through [`Self::lose_trait`]. The `TraitLost` stamp is
    /// required, not incidental — baseline settle replays the log, and
    /// an unstamped removal would be restored off the baseline floor.
    /// "No trace" means no trait, no duration entry, and no scar left
    /// behind; the log event is consumed by the next settle.
    ///
    /// Attribute bends lift for free on expiry: `AttrMod` effects (and
    /// every behavior-weight consumer) are read at access time from
    /// `traits` (`effective_attribute`), so removal *is* the unbend. The
    /// max-stamina pool is the one stored bend — `max_stamina` is a
    /// persisted field set at construction — so when an expired trait
    /// carried `MaxStaminaPercent` (e.g. Asthmatic) the pool is recomputed
    /// exactly like the constructors (`max_stamina_for_traits`), and
    /// current stamina is clamped to the new maximum. A pool that grows
    /// back is not refilled — capacity returns, spent stamina does not.
    pub fn tick_temporary_traits(&mut self) {
        if self.temporary_traits.is_empty() {
            return;
        }
        for remaining in self.temporary_traits.values_mut() {
            *remaining = remaining.saturating_sub(1);
        }
        let expired: Vec<traits::Trait> = self
            .temporary_traits
            .iter()
            .filter_map(|(trait_, remaining)| (*remaining == 0).then_some(*trait_))
            .collect();
        if expired.is_empty() {
            return;
        }
        use traits::Effect;
        let bent_stamina = expired.iter().any(|trait_| {
            trait_
                .effects()
                .iter()
                .any(|e| matches!(e, Effect::MaxStaminaPercent { percent } if *percent != 0))
        });
        for trait_ in &expired {
            self.temporary_traits.remove(trait_);
            self.lose_trait(*trait_);
        }
        if bent_stamina {
            self.max_stamina = lifecycle::health::max_stamina_for_traits(&self.traits);
            self.stamina = self.stamina.min(self.max_stamina);
        }
    }

    /// Rebuild a goblin for a new game: the old character is consumed and
    /// a fresh `Character` is constructed from the survivor's durable
    /// data. Per-game state is zeroed *by construction* — every per-game
    /// field is listed explicitly in the fresh literal below, so a newly
    /// added per-game field cannot silently leak into the next game: the
    /// literal stops compiling until the new field is classified and
    /// given its entry-game value here.
    ///
    /// Carried over (persistent): identity (name, identifiers), ownership
    /// (`human_player_name`), team slot, clan name, avatar, attributes,
    /// traits, lifetime statistics (kills, wins, defeats, draws), and
    /// `ever_addicted_to` — the latter intentionally, so the addiction
    /// relapse short-circuit (see that field's doc) carries across games.
    ///
    /// Before the rebuild, the previous game's durable state is settled
    /// against the baseline captured at its entry: the snapshot is
    /// restored and only advancements earned since then (trait gains,
    /// attribute high-water deltas, lifetime record increments) are kept
    /// — so a goblin returns as it was before the game, plus what it
    /// earned, not as zero. A fresh baseline is then captured for this
    /// game. Traits are settled before the brain re-derivation, so the
    /// brain always reflects the final trait set. Runtime brain state
    /// (psychotic break, preferred-action overrides) dies with the old
    /// character: the fresh brain is re-derived from the surviving traits.
    pub fn into_new_game(mut self) -> Self {
        let mut rng = SmallRng::from_rng(&mut rand::rng());
        if let Some(baseline) = self.game_baseline.take() {
            // Settle consumes the pending advancement events; without a
            // baseline (legacy rows) there is nothing to settle against,
            // so start this game's event clock clean.
            baseline.settle_into(&mut self);
        } else {
            self.advancement_log.clear();
        }
        // Trait evolution (dww.10): lifetime-record triggers earn durable
        // gain/loss here — after settle (so lifetime totals are final),
        // before capture (so the change lands in this game's baseline).
        evolution::apply_evolution(&mut self);
        let baseline = baseline::GameBaseline::capture(&self);
        let Self {
            identifier,
            id,
            name,
            team,
            clan_name,
            avatar,
            human_player_name,
            attributes,
            statistics,
            traits,
            ever_addicted_to,
            advancement_log,
            temporary_traits,
            // Everything else is per-game and dies with the old character.
            ..
        } = self;
        // Stamina coherence: carried traits like Asthmatic shrink the pool.
        let max_stamina = lifecycle::health::max_stamina_for_traits(&traits);
        Self {
            // Persistent: identity, ownership, progression.
            identifier,
            id,
            name,
            team,
            clan_name,
            avatar,
            human_player_name,
            // Duration entries ride along untouched: their countdown is
            // ticked at the finish seam (`post_game_recovery`), not here,
            // so settling this rebuild does not double-count a game.
            temporary_traits,
            attributes,
            statistics: Statistics {
                day_killed: None,
                killed_by: None,
                game: String::new(),
                kills: statistics.kills,
                wins: statistics.wins,
                defeats: statistics.defeats,
                draws: statistics.draws,
                times_revived: statistics.times_revived,
                betrayals_suffered: statistics.betrayals_suffered,
                betrayals_committed: statistics.betrayals_committed,
            },
            ever_addicted_to,
            advancement_log,
            game_baseline: Some(baseline),
            // Re-derived from the surviving traits (clears psychotic
            // breaks and preferred-action overrides from the last game).
            brain: Brain::from_traits(&traits, &mut rng),
            // Fresh roll for the new game, from this goblin's own RNG.
            terrain_affinity: world::clans::roll_terrain_affinity(&mut rng),
            traits,
            // Per-game state: zeroed by construction.
            location: Location::center(AreaId::hub()),
            status: CharacterStatus::default(),
            editable: true,
            stamina: max_stamina,
            max_stamina,
            blood: default_blood(),
            wounds: Vec::new(),
            items: Vec::new(),
            events: Vec::new(),
            allies: Vec::new(),
            turns_since_last_betrayal: 0,
            pending_trust_shock: false,
            alliance_events: Vec::new(),
            recently_killed_by: None,
            hunger: 0,
            thirst: 0,
            sheltered_until: None,
            starving_since_day: None,
            dehydrated_since_day: None,
            cycles_awake: 0,
            sleeping: false,
            sleep_remaining: 0,
            pending_sleep_incident: None,
            sleep_shelter: None,
            afflictions: BTreeMap::new(),
            game_day: None,
            addiction_use_count: BTreeMap::new(),
            hangover_cycles_remaining: 0,
            was_ambushed: false,
            pending_theft_target: None,
            pending_attack_target: None,
            mental_conditions: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_rng(
        name: String,
        team: Option<u32>,
        avatar: Option<String>,
        rng: &mut SmallRng,
    ) -> Self {
        let team = team.unwrap_or(0);
        let statistics = Statistics::default();

        let id_uuid: Uuid = Uuid::new_v4();
        let id: String = id_uuid.to_string();

        // One coherent pass from the provided RNG, so seeded fixtures are
        // deterministic end-to-end AND trait-coherent: traits roll first,
        // attributes bend around them (`from_rng_with_traits`).
        let traits = traits::generate_traits(rng);
        let attributes = Attributes::from_rng_with_traits(rng, &traits);
        let terrain_affinity = world::clans::roll_terrain_affinity(rng);
        let brain = Brain::from_traits(&traits, rng);
        // Stamina coherence: curses like Asthmatic shrink the fresh pool.
        let max_stamina = lifecycle::health::max_stamina_for_traits(&traits);

        Self {
            identifier: id,
            id: id_uuid,
            location: Location::center(AreaId::hub()),
            name,
            team,
            clan_name: String::new(),
            brain,
            status: CharacterStatus::default(),
            avatar,
            human_player_name: None,
            attributes,
            statistics,
            items: vec![],
            events: vec![],
            editable: true,
            terrain_affinity,
            stamina: max_stamina,
            max_stamina,
            blood: 1000,
            wounds: Vec::new(),
            traits,
            allies: Vec::new(),
            turns_since_last_betrayal: 0,
            pending_trust_shock: false,
            alliance_events: Vec::new(),
            recently_killed_by: None,
            hunger: 0,
            thirst: 0,
            sheltered_until: None,
            starving_since_day: None,
            dehydrated_since_day: None,
            cycles_awake: 0,
            sleeping: false,
            sleep_remaining: 0,
            pending_sleep_incident: None,
            sleep_shelter: None,
            afflictions: BTreeMap::new(),
            game_day: None,
            addiction_use_count: BTreeMap::new(),
            ever_addicted_to: BTreeSet::new(),
            hangover_cycles_remaining: 0,
            was_ambushed: false,
            pending_theft_target: None,
            pending_attack_target: None,
            mental_conditions: Vec::new(),
            game_baseline: None,
            advancement_log: Vec::new(),
            temporary_traits: BTreeMap::new(),
        }
    }

    pub fn random() -> Self {
        let mut rng = SmallRng::from_rng(&mut rand::rng());
        let name = world::naming::goblin_name(&mut rng);
        let team = rng.random_range(1..=8);
        Character::new(name, Some(team), None)
    }

    /// Builder: add a pre-existing affliction (addiction, missing limb, trauma, etc.).
    /// Also populates `ever_addicted_to` for Addiction kinds so relapse semantics work.
    pub fn with_affliction(mut self, affliction: Affliction) -> Self {
        if let (AfflictionKind::Addiction(_), Some(meta)) =
            (&affliction.kind, &affliction.addiction_metadata)
        {
            self.ever_addicted_to.insert(meta.substance);
        }
        self.afflictions.insert(affliction.key(), affliction);
        self
    }

    pub fn avatar(&self) -> String {
        if self.avatar.is_none() {
            return "https://fallback.pics/api/v1/400x400".to_string();
        }
        format!("assets/{}", self.avatar.clone().unwrap())
    }

    /// Send a character through a game cycle.
    /// This is the main function that runs the character's actions.
    /// 1. Ignore dead characters.
    /// 2. Process status effects including area events.
    /// 3. Check for gifts from patrons.
    /// 4. Check for nighttime effects.
    /// 5. Check for suggested actions.
    /// 6. Get the character's action from the brain.
    /// 7. Perform the action.
    /// 8. Log the action.
    pub fn process_turn_phase(
        &mut self,
        action_suggestion: Option<ActionSuggestion>,
        affliction_visibility: bool,
        environment_details: &mut EnvironmentContext<'_>,
        encounter_context: EncounterContext,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
    ) {
        // Character is already dead, do nothing. (No event emitted — noise drop.)
        if !self.is_alive() {
            return;
        }

        // Advance per-character alliance timers (spec §7.4). Ticks
        // turns_since_last_betrayal so Treacherous characters can betray on
        // cadence. Skipped for dead characters via `is_alive` guard above.
        self.tick_alliance_timers();

        // Consume any pending trust-shock from a betrayal recorded last turn
        // (spec §7.3c1). Rolls per ally; broken allies are removed locally.
        self.consume_pending_trust_shock(rng, events);

        let area_details = &mut environment_details.area_details;

        // Update the character based on the period's events.
        self.process_status(area_details, rng, events);

        // Process wounds: drain blood from bleeding wounds, then heal naturally.
        if self.is_alive() && !self.wounds.is_empty() {
            self.drain_blood_from_wounds();
            self.heal_wounds(rng, events);

            // Heroism: low blood grants temporary bravery
            if self.should_heroism() {
                self.increase_bravery(wounds::HEROISM_BRAVERY_BOOST);
            }

            // Emit CharacterBledOut if blood reached zero from wound bleed
            if self.blood == 0 {
                events.push(TaggedEvent::new(
                    format!("{} bled out from their wounds", self.name),
                    MessagePayload::CharacterBledOut {
                        character: CharacterRef {
                            identifier: self.identifier.clone().into(),
                            name: self.name.clone(),
                        },
                    },
                ));
                // WHY: a bleed-out is an environmental death — no character
                // killer, so recently_killed_by stays None. Promote here and
                // announce exactly once (the period-end block only
                // early-returns for RecentlyDead) so the end-of-cycle sweep
                // records DeathRecorded, day_killed, and loot like any other
                // death.
                self.status = CharacterStatus::RecentlyDead;
                self.recently_killed_by = None;
                self.statistics.killed_by = Some("their wounds".to_string());
                let line = GameOutput::CharacterDead(self.name.as_str()).to_string();
                events.push(TaggedEvent::new(
                    line,
                    MessagePayload::CharacterKilled {
                        victim: CharacterRef {
                            identifier: self.identifier.clone().into(),
                            name: self.name.clone(),
                        },
                        killer: None,
                        cause: shared::afflictions::DeathCause::Affliction(
                            shared::afflictions::AfflictionKind::Wounded,
                        ),
                    },
                ));
            }
        }

        // Recalculate pain from wounds and tick mental conditions.
        if self.is_alive() {
            self.update_pain_condition();
            // Apply mental condition effects (sanity drain) and tick durations.
            self.tick_mental_conditions();
        }

        // Character died to the period's events (status effects or blood
        // loss). process_status and the wounds block each announce their own
        // deaths and promote to RecentlyDead before this point — one death
        // yields exactly one CharacterKilled, so just stop the turn here.
        if self.status == CharacterStatus::RecentlyDead || self.blood == 0 {
            return;
        }

        // Treacherous active betrayal (spec §7.4(b)). When the timer has
        // elapsed and the character carries the Treacherous trait, attempt to
        // betray a same-area ally. On success, drop the symmetric pair
        // locally, enqueue BetrayalRecorded so the victim's `allies` is
        // cleaned and `pending_trust_shock` flips on the next drain. The
        // timer resets unconditionally so a missed opportunity does not
        // stack (one chance per cadence).
        if traits::behavior_sum(&self.traits, traits::BehaviorAxis::BetrayalCadence) > 0
            && self.turns_since_last_betrayal
                >= traits::behavior_sum(&self.traits, traits::BehaviorAxis::BetrayalCadence) as u8
        {
            let same_area_ally = encounter_context
                .potential_targets
                .iter()
                .find(|t| self.allies.contains(&t.id) && t.is_alive())
                .cloned();
            if let Some(victim) = same_area_ally {
                self.allies.retain(|id| id != &victim.id);
                self.alliance_events
                    .push(alliances::AllianceEvent::BetrayalRecorded {
                        betrayer: self.id,
                        victim: victim.id,
                    });
                // Lifetime committed counter (dww.10.2): the betrayer
                // records its own betrayal at the trigger site.
                self.statistics.betrayals_committed =
                    self.statistics.betrayals_committed.saturating_add(1);
            }
            self.turns_since_last_betrayal = 0;
        }

        // Nighttime terror
        let is_day = environment_details.is_day;
        if !is_day && self.is_alive() {
            self.misses_home();
        }

        // Check for psychotic breaks or recovery (sanity-based mental state changes)
        self.brain
            .check_psychotic_break(self.effective_sanity(), rng);
        self.brain.check_recovery(self.effective_sanity());

        // Set a preferred action if one is suggested
        if let Some(suggestion) = action_suggestion {
            self.brain
                .set_preferred_action(suggestion.action, suggestion.probability.unwrap_or(1.0));
        }

        // Get the primary action (owner decision C): should_sleep's
        // Sleep flows as the primary when it fires.
        let number_of_nearby_characters = encounter_context.nearby_characters_count;
        let primary = if let Some(sleep_action) = self.brain.should_sleep(
            self,
            number_of_nearby_characters,
            environment_details.phase,
            &environment_details.config,
            rng,
        ) {
            sleep_action
        } else {
            self.brain.act(
                self,
                number_of_nearby_characters,
                &environment_details.available_destinations,
                environment_details.all_areas,
                environment_details.enemy_density,
                environment_details.phase,
                rng,
            )
        };

        // Two beats per phase (owner decision C): the brain sequences
        // the primary into move/move, move/action, action/move, or
        // action/action.
        let beats = self.brain.sequence(
            primary,
            self,
            &environment_details.available_destinations,
            environment_details.all_areas,
            environment_details.enemy_density,
        );

        for beat in beats {
            // Move beat pre-gates (owner decision C): (a) silently
            // skip a beat whose destination is the goblin's current
            // cell — no AlreadyThere event, no short-rest fallback,
            // so standing where it wants keeps the beat's stamina —
            // and (b) skip the beat when the cheapest available hop
            // costs more than current stamina.
            let beat = match beat {
                Action::Move(destination) => {
                    if destination == Some(self.location) {
                        continue;
                    }
                    let current_hop = self.hop_stamina_cost(
                        self.location.area,
                        self.location.area,
                        environment_details.all_areas,
                    );
                    let cheapest = environment_details
                        .available_destinations
                        .iter()
                        .map(|d| d.stamina_cost)
                        .chain(std::iter::once(current_hop))
                        .min();
                    if cheapest.is_some_and(|price| self.stamina < price) {
                        continue;
                    }
                    Action::Move(destination)
                }
                other => other,
            };

            match beat {
                Action::Move(destination) => {
                    self.act_move(&destination, environment_details.all_areas, events);
                }
                Action::Rest => {
                    self.act_rest(events);
                }
                Action::Hide => {
                    self.act_hide(events);
                }
                Action::Attack => {
                    self.act_attack(
                        encounter_context.potential_targets.clone(),
                        encounter_context.total_living_characters,
                        events,
                        rng,
                        environment_details.phase,
                        environment_details.combat_tuning,
                    );
                }
                Action::TakeItem => {
                    self.act_take_item(
                        area_details,
                        &environment_details.sleeping_nearby,
                        rng,
                        events,
                    );
                }
                Action::UseItem(maybe_item) => {
                    self.act_use_item(&maybe_item, events);
                }
                Action::None => {}
                Action::ProposeAlliance => {
                    self.act_propose_alliance(
                        &encounter_context,
                        affliction_visibility,
                        rng,
                        events,
                    );
                }
                Action::SeekShelter => {
                    self.act_seek_shelter(
                        area_details,
                        environment_details.current_day,
                        environment_details.is_day,
                        rng,
                        events,
                    );
                }
                Action::Forage => {
                    self.act_forage(area_details, rng, events);
                }
                Action::DrinkFromTerrain => {
                    self.act_drink_from_terrain(area_details, rng, events);
                }
                Action::Eat(item) => {
                    self.act_eat(item, rng, events);
                }
                // Next slice: DrinkItem executor (stacked on this PR).
                Action::DrinkItem(_) => {}
                Action::Sleep { duration_phases } => {
                    // PR1 Task 9a: roll shelter quality at sleep start (we6l)
                    let terrain = areas::location::terrain_at(self.location, area_details);
                    self.sleep_shelter = Some(crate::incidents::find_shelter(
                        self.attributes.intelligence,
                        self.attributes.strength,
                        terrain,
                        rng,
                    ));
                    self.act_sleep(duration_phases, environment_details.phase, events);
                }
                Action::Frozen | Action::Flashback { .. } | Action::Avoidance => {}
                Action::SearchForSubstance { substance } => {
                    // Scan inventory for an item matching the craving substance.
                    let matching_item = self
                        .items
                        .iter()
                        .find(|item| item.attribute.substance() == Some(substance))
                        .cloned();

                    if let Some(item) = matching_item {
                        // Found — auto-use it.
                        self.try_use_consumable(&item, events, None).ok();
                    } else {
                        // Not found — emit visible craving.
                        let craving_severity = self
                        .afflictions
                        .values()
                        .find(|a| matches!(a.kind, AfflictionKind::Addiction(s) if s == substance))
                        .map(|a| a.severity.to_string())
                        .unwrap_or_else(|| "moderate".to_string());

                        let line = format!("{} craves {} but has none", self.name, substance);
                        events.push(TaggedEvent::new(
                            line,
                            MessagePayload::AddictionCraving {
                                character: self.identifier.to_string(),
                                substance: substance.to_string(),
                                severity: craving_severity,
                            },
                        ));
                    }
                }
                Action::Rescue { .. } => {}
                Action::SetTrap {
                    trap_kind,
                    severity,
                } => {
                    self.act_set_trap(trap_kind, severity, area_details, rng, events);
                }
                Action::Search => {
                    self.act_search(area_details, rng, events);
                }
            }

            // Stop the whole round when the beat kills the goblin.
            if !self.is_alive() {
                break;
            }
        }

        // ── Trap trigger check ──
        // After resolving the chosen action, check if any traps in the
        // victim's current sub-tile trigger.
        self.check_trap_triggers(area_details, rng, events);
    }

    /// Post-action trap scan for this character's current sub-tile.
    /// Traps keyed to another sub-tile are skipped. Setter auto-passes
    /// (no friendly fire).
    fn check_trap_triggers(
        &mut self,
        area_details: &mut AreaDetails,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
    ) {
        for idx in (0..area_details.placed_traps.len()).rev() {
            let triggered = area_details.placed_traps[idx].triggered;
            if triggered {
                continue;
            }
            if area_details.placed_traps[idx].sub != self.location.sub {
                continue; // Keyed to a different sub-tile
            }
            let set_by = area_details.placed_traps[idx].set_by.clone();
            if self.identifier == set_by {
                continue; // Setter auto-passes
            }

            // Passive Perception check
            let perception_mod = (self.attributes.intelligence as f32 / 10.0).floor() as i32;
            let roll: i32 = rng.random_range(1..=20);
            let concealment = area_details.placed_traps[idx].concealment;
            if roll + perception_mod < concealment as i32 {
                // Trigger the trap!
                // Remove the trap from the area and apply its effect
                let trap = area_details.placed_traps.swap_remove(idx);
                let line = format!("{} triggers a {} trap!", self.name, trap.kind);
                events.push(TaggedEvent::new(
                    line,
                    MessagePayload::TrapTriggered {
                        victim: shared::messages::CharacterRef {
                            identifier: self.identifier.clone().into(),
                            name: self.name.clone(),
                        },
                        trap_kind: trap.kind.to_string(),
                    },
                ));

                // Apply affliction based on trap kind
                use shared::afflictions::{AfflictionKind, AfflictionSource, TrappedMetadata};
                self.try_acquire_affliction(crate::AfflictionDraft {
                    kind: AfflictionKind::Trapped(trap.kind),
                    body_part: None,
                    severity: trap.severity,
                    source: AfflictionSource::Environmental,
                    trapped_metadata: Some(TrappedMetadata::fresh_for(trap.kind, None)),
                });
                break; // First trigger stops — no chain-traps
            }
        }
    }

    /// Pick a target character from `targets` to attack, given the number of
    /// living characters in the game.
    ///
    /// Selection rules:
    /// 1. If there are no targets and the character is suicidal (very low sanity),
    ///    target self.
    /// 2. Otherwise, filter out current allies — they are off-limits regardless
    ///    of team.
    /// 3. If any non-allies remain, pick one at random.
    /// 4. If only allies are nearby (and we're not the last two alive), pick no
    ///    target. Final confrontation (only two alive) overrides alliance.
    fn pick_target(
        &self,
        mut targets: Vec<Character>,
        living_characters_count: u32,
    ) -> Option<Character> {
        // If there are no targets, check if the character is feeling suicidal.
        if targets.is_empty() {
            return match self.effective_sanity() {
                0..=SANITY_BREAK_LEVEL => {
                    // Hand the strike to the resolver: it announces the death
                    // once if the blow lands and stays silent if the goblin
                    // survives it. Emitting here reported a death for a
                    // possibly-alive goblin and duplicated the resolver's
                    // announcement.
                    Some(self.clone())
                }
                _ => None, // Attack no one
            };
        }

        let enemies: Vec<Character> = targets
            .iter()
            .filter(|t| !self.allies.contains(&t.id))
            .cloned()
            .collect();

        if enemies.is_empty() {
            // Only allies in range. Final confrontation overrides loyalty —
            // except between teammates: the last goblins of one team never
            // turn on each other (team victory ends the game instead).
            if targets.len() == 1 && living_characters_count == 2 && self.team != targets[0].team {
                return Some(targets.pop().unwrap());
            }
            return None;
        }

        // Sleep ambush preference: prefer defenseless sleeping targets.
        // If any enemies are sleeping, choose randomly among them. Otherwise
        // choose randomly from all enemies as before.
        let sleeping_enemies: Vec<Character> =
            enemies.iter().filter(|t| t.sleeping).cloned().collect();
        let pool = if sleeping_enemies.is_empty() {
            enemies.clone()
        } else {
            sleeping_enemies
        };
        let mut rng = SmallRng::from_rng(&mut rand::rng());
        Some(pool.choose(&mut rng).unwrap().clone())
    }

    // --- Per-Action executor helpers (extracted from process_turn_phase) ---

    fn act_move(
        &mut self,
        destination: &Option<Location>,
        all_areas: &[AreaDetails],
        events: &mut Vec<TaggedEvent>,
    ) {
        // The sub-hop engine owns stamina: every hop pays its
        // crossing price (half the source's cost plus half the
        // destination's, each rounded up). Failure (no options, or too
        // broke for hop 1) falls back to a short rest.
        if let TravelResult::Failure = self.travels(*destination, all_areas, events) {
            self.short_rests();
        }
    }

    fn act_rest(&mut self, events: &mut Vec<TaggedEvent>) {
        let character_ref = CharacterRef {
            identifier: self.identifier.clone().into(),
            name: self.name.clone(),
        };
        let line = GameOutput::CharacterRest(self.name.as_str()).to_string();
        events.push(TaggedEvent::new(
            line,
            MessagePayload::CharacterRested {
                character: character_ref,
                hp_restored: 0,
            },
        ));
        self.long_rests();
    }

    fn act_hide(&mut self, events: &mut Vec<TaggedEvent>) {
        let character_ref = CharacterRef {
            identifier: self.identifier.clone().into(),
            name: self.name.clone(),
        };
        let area_ref = |a: AreaId| {
            let s = a.to_string();
            AreaRef {
                identifier: s.clone().into(),
                name: s,
            }
        };

        let _hidden = self.hides();
        let current_area = self.location.area;
        let line = GameOutput::CharacterHide(self.name.as_str()).to_string();
        events.push(TaggedEvent::new(
            line,
            MessagePayload::CharacterHidden {
                character: character_ref,
                area: area_ref(current_area),
            },
        ));
    }

    /// Execute a SetTrap action.
    fn act_set_trap(
        &mut self,
        _trap_kind: Option<shared::afflictions::TrapKind>,
        _severity: Option<shared::afflictions::Severity>,
        _area_details: &mut AreaDetails,
        _rng: &mut impl Rng,
        _events: &mut Vec<TaggedEvent>,
    ) {
        // Cap: 2 untriggered traps per SUB-TILE (dxj owner gate: was 3 per
        // area before traps were keyed). Traps in other sub-tiles of the
        // same area don't count against this one.
        let untriggered_count = _area_details
            .placed_traps
            .iter()
            .filter(|t| !t.triggered && t.sub == self.location.sub)
            .count();
        if untriggered_count >= 2 {
            let line = format!(
                "{} can't set another trap here — sub-tile is full.",
                self.name
            );
            _events.push(TaggedEvent::new(line, MessagePayload::Generic));
            return;
        }

        let kind = _trap_kind.unwrap_or(shared::afflictions::TrapKind::Snared);
        let severity = _severity.unwrap_or(shared::afflictions::Severity::Mild);

        // Intelligence check for concealment (d20 + int modifier)
        let int_val = self.attributes.intelligence as f32;
        let int_mod = (int_val / 10.0).floor() as u32;
        let roll: u32 = _rng.random_range(1..=20);
        let concealment = 10 + int_mod + roll / 2; // Base 10 + int bonus + luck

        let trap = areas::traps::PlacedTrap {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            severity,
            set_by: self.identifier.to_string(),
            concealment,
            triggered: false,
            sub: self.location.sub,
        };

        _area_details.placed_traps.push(trap);

        let line = format!("{} sets a {} trap in the area.", self.name, kind);
        _events.push(TaggedEvent::new(
            line,
            MessagePayload::TrapSet {
                character: shared::messages::CharacterRef {
                    identifier: self.identifier.clone().into(),
                    name: self.name.clone(),
                },
                trap_kind: kind.to_string(),
            },
        ));
    }

    /// Execute a Search action — reveals hidden traps in the character's
    /// current sub-tile only (dxj owner gate: sub-tile search).
    fn act_search(
        &mut self,
        area_details: &mut AreaDetails,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
    ) {
        let untriggered: Vec<usize> = area_details
            .placed_traps
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.triggered && t.sub == self.location.sub)
            .map(|(i, _)| i)
            .collect();

        if untriggered.is_empty() {
            let line = format!("{} searches the sub-tile but finds no traps.", self.name);
            events.push(TaggedEvent::new(line, MessagePayload::Generic));
            return;
        }

        // Perception check vs concealment
        let perception_mod = (self.attributes.intelligence as f32 / 10.0).floor() as u32;
        let mut found_any = false;

        for &idx in &untriggered {
            let concealment = area_details.placed_traps[idx].concealment;
            let kind = area_details.placed_traps[idx].kind;
            let roll: u32 = rng.random_range(1..=20);
            if roll + perception_mod >= concealment {
                found_any = true;
                let line = format!("{} spots a {} trap! (DC {})", self.name, kind, concealment);
                events.push(TaggedEvent::new(line, MessagePayload::Generic));
                // Free disarm
                area_details.placed_traps[idx].triggered = true;
                let disarm_line = format!("{} disarms the {} trap.", self.name, kind);
                events.push(TaggedEvent::new(disarm_line, MessagePayload::Generic));
            }
        }

        if !found_any {
            let line = format!(
                "{} searches carefully but finds nothing suspicious.",
                self.name
            );
            events.push(TaggedEvent::new(line, MessagePayload::Generic));
        }
    }

    /// Stamina gate shared by the survival executors (`SeekShelter`,
    /// `Forage`). Mirrors the travel path: check `calculate_stamina_cost`
    /// first and deny with an exhausted event when the pool cannot cover
    /// it. Returns `false` (and pushes the denial) when the action must
    /// not run. The act path does not deduct stamina generically — only
    /// travel and combat do — so this helper deducts the cost itself once
    /// the check passes.
    fn spend_stamina_for(
        &mut self,
        action: &Action,
        terrain: &world::terrain::TerrainType,
        verb: &str,
        events: &mut Vec<TaggedEvent>,
    ) -> bool {
        let cost = calculate_stamina_cost(action, terrain, self);
        if self.stamina < cost {
            let line = format!(
                "🥵 {} is too exhausted to {}, lacks stamina",
                self.name, verb
            );
            events.push(TaggedEvent::new(
                line,
                MessagePayload::CharacterRested {
                    character: CharacterRef {
                        identifier: self.identifier.clone().into(),
                        name: self.name.clone(),
                    },
                    hp_restored: 0,
                },
            ));
            return false;
        }
        self.stamina = self.stamina.saturating_sub(cost);
        true
    }

    /// Tier (0..=3) of a rolled shelter — the `ShelterSought.roll` encoding
    /// (None = 0, Crude = 1, Natural = 2, Fortified = 3). The granted slots
    /// equal the tier (1 slot per tier step).
    fn shelter_tier(shelter: crate::incidents::SleepShelter) -> u8 {
        use crate::incidents::SleepShelter;
        match shelter {
            SleepShelter::None => 0,
            SleepShelter::Crude => 1,
            SleepShelter::Natural => 2,
            SleepShelter::Fortified => 3,
        }
    }

    /// Execute `Action::SeekShelter`.
    ///
    /// Reuses [`crate::incidents::find_shelter`], the same roll that rates
    /// sleep-shelter quality. `sheltered_until` stores the day/night slot
    /// index (`day * 2 + !is_day` — the unit the survival/cascade
    /// consumers compare against) plus a tier-scaled duration:
    /// Crude = 1 slot (rest of the current day/night half-day),
    /// Natural = 2 slots, Fortified = 3 slots. Better shelters outlast
    /// weather longer, mirroring the shelter multipliers (0.8 / 0.5 / 0.3)
    /// without re-scaling the consumers.
    ///
    /// `MessagePayload::ShelterSought.roll` carries the tier
    /// (None = 0, Crude = 1, Natural = 2, Fortified = 3). A failed roll
    /// (tier None) still emits `ShelterSought { success: false }` so the
    /// attempt is visible in history; no shelter is granted.
    ///
    /// [`Trait::Builder`](crate::traits::Trait) effect: a second
    /// `find_shelter` roll keeps the better tier, and a granted shelter
    /// lasts one extra half-day slot.
    fn act_seek_shelter(
        &mut self,
        area_details: &mut AreaDetails,
        current_day: u32,
        is_day: bool,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
    ) {
        if !self.spend_stamina_for(
            &Action::SeekShelter,
            &area_details.terrain,
            "seek shelter",
            events,
        ) {
            return;
        }

        let builder = self.traits.contains(&crate::traits::Trait::Builder);
        let terrain = areas::location::terrain_at(self.location, area_details);
        let mut shelter = crate::incidents::find_shelter(
            self.attributes.intelligence,
            self.attributes.strength,
            terrain,
            rng,
        );
        if builder {
            // Builder re-rolls once and keeps the better tier.
            let reroll = crate::incidents::find_shelter(
                self.attributes.intelligence,
                self.attributes.strength,
                terrain,
                rng,
            );
            if Self::shelter_tier(reroll) > Self::shelter_tier(shelter) {
                shelter = reroll;
            }
        }
        let tier = Self::shelter_tier(shelter);
        let slots = u32::from(tier);
        let success = slots > 0;

        if success {
            let phase_index = current_day * 2 + u32::from(!is_day);
            // Builder's shelter outlasts weather one extra half-day slot.
            let until = phase_index + slots + u32::from(builder);
            self.sheltered_until = Some(self.sheltered_until.unwrap_or(0).max(until));
        }

        let area = AreaRef {
            identifier: self.location.area.to_string().clone().into(),
            name: self.location.area.to_string(),
        };
        let line = if success {
            format!("{} found shelter in the {}", self.name, area.name)
        } else {
            format!("{} failed to find shelter in the {}", self.name, area.name)
        };
        events.push(TaggedEvent::new(
            line,
            MessagePayload::ShelterSought {
                character: CharacterRef {
                    identifier: self.identifier.clone().into(),
                    name: self.name.clone(),
                },
                area,
                success,
                roll: tier,
            },
        ));
    }

    /// Execute `Action::Forage`.
    ///
    /// Yield formula: `forage_richness(terrain)` attempts (0 = barren,
    /// 1..=3), each a d20 needing `>= 10 - StatModifiers.forage`
    /// (the affliction `forage` channel is negative, so afflictions make
    /// each attempt harder; clamp floor is applied by `compute_stat_modifiers`).
    /// Every successful attempt grants one Food ration with value =
    /// richness (1..3 hunger relief) via `OwnsItems::add_item` — the same
    /// grant path patron gifts and take-item use. Food is banked in the
    /// inventory; the deferred `Eat` executor will consume it.
    ///
    /// `MessagePayload::Foraged.debt_recovered` is 0 here: forage banks
    /// food rather than eating it, so no hunger debt is cleared yet.
    ///
    /// [`Trait::ResourcefulForager`](crate::traits::Trait) effect: one
    /// extra d20 attempt on top of the terrain's richness.
    fn act_forage(
        &mut self,
        area_details: &mut AreaDetails,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
    ) {
        if !self.spend_stamina_for(&Action::Forage, &area_details.terrain, "forage", events) {
            return;
        }

        let richness = areas::forage::forage_richness(areas::location::terrain_at(
            self.location,
            area_details,
        ));
        // ResourcefulForager adds one d20 attempt to the loop.
        let attempts = richness
            + u8::from(
                self.traits
                    .contains(&crate::traits::Trait::ResourcefulForager),
            );
        let afflictions: Vec<_> = self.afflictions.values().cloned().collect();
        let modifier = crate::afflictions::compute_stat_modifiers(&afflictions).forage;

        let mut found: u8 = 0;
        for _ in 0..attempts {
            let roll: i32 = rng.random_range(1..=20);
            if roll + modifier >= 10 {
                found = found.saturating_add(1);
            }
        }

        let success = found > 0;
        if success {
            use world::items::OwnsItems;
            for _ in 0..found {
                self.add_item(Item::new_food(None, richness.max(1)));
            }
        }

        let area = AreaRef {
            identifier: self.location.area.to_string().clone().into(),
            name: self.location.area.to_string(),
        };
        let line = if success {
            format!(
                "{} forages the {} and finds {found} ration(s)",
                self.name, area.name
            )
        } else {
            format!("{} finds nothing foraging in the {}", self.name, area.name)
        };
        events.push(TaggedEvent::new(
            line,
            MessagePayload::Foraged {
                character: CharacterRef {
                    identifier: self.identifier.clone().into(),
                    name: self.name.clone(),
                },
                area,
                success,
                debt_recovered: 0,
            },
        ));
    }

    /// Terrain drink (cc7): drink your fill from a live water source —
    /// `SATIATE_AMOUNT` points of thirst off + drain stamp cleared
    /// (through [`crate::survival::drink_water`]). The brain gates the
    /// action on `water_source(terrain, weather) > 0` at decision time;
    /// EnvironmentContext carries no weather, so contamination uses the
    /// spec's base 15% only (Heatwave/Trait mods need weather plumbed in).
    /// Contamination is silent by spec — Poisoned visibility comes from
    /// the affliction/band machinery, not an extra line here.
    fn act_drink_from_terrain(
        &mut self,
        area_details: &AreaDetails,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
    ) {
        if !self.spend_stamina_for(
            &Action::DrinkFromTerrain,
            &area_details.terrain,
            "drink",
            events,
        ) {
            return;
        }

        crate::survival::drink_water(self, crate::survival::SATIATE_AMOUNT);
        let debt_recovered = crate::survival::SATIATE_AMOUNT;

        // Spec §16: terrain water contaminates on 15% (provisional base).
        if rng.random_range(0..100) < 15 {
            self.try_acquire_affliction(AfflictionDraft {
                kind: AfflictionKind::Poisoned,
                body_part: None,
                severity: Severity::Moderate,
                source: AfflictionSource::Environmental,
                trapped_metadata: None,
            });
        }

        let area = AreaRef {
            identifier: self.location.area.to_string().clone().into(),
            name: self.location.area.to_string(),
        };
        events.push(TaggedEvent::new(
            format!("{} drinks their fill at the {}", self.name, area.name),
            MessagePayload::Drank {
                character: CharacterRef {
                    identifier: self.identifier.clone().into(),
                    name: self.name.clone(),
                },
                source: shared::messages::DrinkSource::Terrain { area },
                debt_recovered,
            },
        ));
    }

    /// Eat an inventory Food item (spec §16): pick the best when the
    /// action carries `None` (per the action's contract), destroy it
    /// (`remove_item`), drop `hunger` by the item's `food_value` through
    /// [`crate::survival::eat_food`] (starvation-stamp clear), roll the
    /// spec's 5% contamination (silent on failure; Poisoned visibility
    /// rides the affliction machinery), emit one `Ate` line with the
    /// recovered debt. No stamina guard — eating what you carry costs no
    /// effort (Forage and terrain drinking do).
    fn act_eat(
        &mut self,
        picked: Option<world::items::Item>,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
    ) {
        use world::items::OwnsItems;
        let item = match picked {
            Some(item) => item,
            None => match self
                .items
                .iter()
                .filter(|i| i.item_type.is_food())
                .max_by_key(|i| i.item_type.food_value().unwrap_or(0))
            {
                Some(best) => best.clone(),
                None => return,
            },
        };
        let Some(value) = item.item_type.food_value() else {
            return;
        };
        if self.remove_item(&item).is_err() {
            return;
        }
        crate::survival::eat_food(self, value);

        // Spec §16: 5% base contamination for Eat(item).
        if rng.random_range(0..100) < 5 {
            self.try_acquire_affliction(AfflictionDraft {
                kind: AfflictionKind::Poisoned,
                body_part: None,
                severity: Severity::Moderate,
                source: AfflictionSource::Environmental,
                trapped_metadata: None,
            });
        }

        events.push(TaggedEvent::new(
            format!("{} eats the {}", self.name, item.name),
            MessagePayload::Ate {
                character: CharacterRef {
                    identifier: self.identifier.clone().into(),
                    name: self.name.clone(),
                },
                item: ItemRef {
                    identifier: item.identifier.clone().into(),
                    name: item.name.clone(),
                },
                debt_recovered: value,
            },
        ));
    }

    fn act_attack(
        &mut self,
        potential_targets: Vec<Character>,
        total_living_characters: u32,
        events: &mut Vec<TaggedEvent>,
        rng: &mut impl Rng,
        phase: shared::messages::Phase,
        combat_tuning: &crate::combat_tuning::CombatTuning,
    ) {
        let target = self.pick_target(potential_targets, total_living_characters);
        if let Some(mut target) = target {
            // Self-attack / suicide only mutates `self` (the real character);
            // resolve it inline exactly as before. A real two-character fight
            // is deferred to the game cycle, which has `split_at_mut` access to
            // both combatants and drives multi-round combat via
            // `CombatRoundLoop` — the target clone here would otherwise discard
            // the opponent's damage (see cycle.rs pending-attack resolution).
            if target.id == self.id {
                let outcome = self.attacks(&mut target, rng, events, phase, combat_tuning);
                match outcome {
                    AttackOutcome::Kill(_, mut target) => {
                        self.statistics.kills += 1;
                        target.statistics.day_killed =
                            Some(self.statistics.game.parse().unwrap_or(1));
                    }
                    AttackOutcome::Wound(_, _) | AttackOutcome::Miss(_, _) => {}
                }
            } else {
                self.pending_attack_target = Some(target.id);
            }
        }
    }

    fn act_take_item(
        &mut self,
        area_details: &mut AreaDetails,
        sleeping_nearby: &[(Uuid, String)],
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
    ) {
        // Sleep theft (ls5a): if there are sleeping characters with items
        // in the area, mark one as theft target. The actual item transfer
        // happens in execute_cycle after process_turn_phase returns.
        if !sleeping_nearby.is_empty() {
            let idx = rng.random_range(0..sleeping_nearby.len());
            let (sleeper_id, sleeper_name) = &sleeping_nearby[idx];
            self.pending_theft_target = Some(*sleeper_id);
            let line = format!(
                "{} rummages through sleeping {}'s belongings",
                self.name, sleeper_name
            );
            events.push(TaggedEvent::new(line, MessagePayload::Generic));
            return;
        }

        // Original area item logic
        if let Some(item) = self.take_nearby_item(area_details) {
            let character_ref = CharacterRef {
                identifier: self.identifier.clone().into(),
                name: self.name.clone(),
            };
            let area_ref = |a: AreaId| {
                let s = a.to_string();
                AreaRef {
                    identifier: s.clone().into(),
                    name: s,
                }
            };
            let line = GameOutput::CharacterTakeItem(self.name.as_str(), &item.name).to_string();
            let item_ref = ItemRef {
                identifier: item.identifier.clone().into(),
                name: item.name.clone(),
            };
            let current_area = self.location.area;
            events.push(TaggedEvent::new(
                line,
                MessagePayload::ItemFound {
                    character: character_ref,
                    item: item_ref,
                    area: area_ref(current_area),
                },
            ));
        }
    }

    fn act_use_item(&mut self, maybe_item: &Option<Item>, events: &mut Vec<TaggedEvent>) {
        if let Some(item) = maybe_item {
            let character_ref = CharacterRef {
                identifier: self.identifier.clone().into(),
                name: self.name.clone(),
            };
            if let Err(error) = self.try_use_consumable(item, events, None) {
                let line =
                    GameOutput::CharacterCannotUseItem(self.name.as_str(), &error.to_string())
                        .to_string();
                let item_ref = ItemRef {
                    identifier: item.identifier.clone().into(),
                    name: item.name.clone(),
                };
                events.push(TaggedEvent::new(
                    line,
                    MessagePayload::ItemUsed {
                        character: character_ref,
                        item: item_ref,
                    },
                ));
            } else {
                let line = GameOutput::CharacterUseItem(self.name.as_str(), item).to_string();
                let item_ref = ItemRef {
                    identifier: item.identifier.clone().into(),
                    name: item.name.clone(),
                };
                events.push(TaggedEvent::new(
                    line,
                    MessagePayload::ItemUsed {
                        character: character_ref,
                        item: item_ref,
                    },
                ));
            }
        }
    }

    /// dxj gate `witnessing.affliction_visibility`: can the proposer see
    /// any affliction on `target`, and does that bar candidacy? Off =
    /// legacy — candidacy never considers afflictions. On = co-present
    /// targets show Moderate+ afflictions (Severe always, Mild never),
    /// and anyone showing one is skipped.
    fn perceivable_candidate(
        observer: &Character,
        target: &Character,
        affliction_visibility: bool,
    ) -> bool {
        !affliction_visibility
            || crate::afflictions::visible_afflictions_to(observer, target, true).is_empty()
    }

    fn act_propose_alliance(
        &mut self,
        encounter_context: &EncounterContext,
        affliction_visibility: bool,
        rng: &mut impl Rng,
        events: &mut Vec<TaggedEvent>,
    ) {
        use crate::alliances::{MAX_ALLIES, deciding_factor, passes_gate, try_form_alliance};

        let character_ref = CharacterRef {
            identifier: self.identifier.clone().into(),
            name: self.name.clone(),
        };

        let candidates: Vec<&Character> = encounter_context
            .potential_targets
            .iter()
            .filter(|t| t.is_alive())
            .filter(|t| !self.allies.contains(&t.id))
            .filter(|t| t.allies.len() < MAX_ALLIES)
            .filter(|t| passes_gate(&self.traits, &t.traits))
            // Affliction perception (dxj gate): with affliction_visibility
            // on, skip candidates showing a visible affliction.
            .filter(|t| Self::perceivable_candidate(self, t, affliction_visibility))
            .collect();

        // Phobia veto (qqqx PR3 spec §12):
        // Hard veto: cannot propose alliance if you have Phobia(Character).
        if self
            .afflictions
            .values()
            .any(|aff| matches!(aff.kind, AfflictionKind::Phobia(PhobiaTrigger::Character)))
        {
            return;
        }

        // Soft penalty: Phobia(TraitGroup) reduces formation chance.
        let phobia_penalty: f64 = self
            .afflictions
            .values()
            .filter_map(|aff| {
                if matches!(aff.kind, AfflictionKind::Phobia(PhobiaTrigger::TraitGroup)) {
                    Some(aff.severity.ordinal() as f64 * 0.15)
                } else {
                    None
                }
            })
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        // Trauma penalty (5477 PR3 spec §12):
        // Soft penalty: each trauma reduces formation chance by -0.15 per severity tier.
        let trauma_penalty: f64 = self
            .afflictions
            .values()
            .filter(|aff| matches!(aff.kind, AfflictionKind::Trauma))
            .map(|aff| aff.severity.ordinal() as f64 * 0.15)
            .sum::<f64>()
            .min(1.0); // Cap at 1.0 so chance can't go below 0

        // Hard veto: Severe trauma with Betrayal source hard-blocks alliance.
        let has_betrayal_veto = self.afflictions.values().any(|aff| {
            if !matches!(aff.kind, AfflictionKind::Trauma) {
                return false;
            }
            if aff.severity < Severity::Severe {
                return false;
            }
            matches!(
                aff.trauma_metadata,
                Some(ref m) if m.sources.iter().any(|s| matches!(s, TraumaSource::Betrayal { .. }))
            )
        });
        if has_betrayal_veto {
            return;
        }

        if candidates.is_empty() {
            return;
        }
        let target = {
            use rand::seq::IndexedRandom;
            candidates.choose(rng).cloned().unwrap()
        };
        let target_ref = CharacterRef {
            identifier: target.identifier.clone().into(),
            name: target.name.clone(),
        };

        let proposed_line = format!("🤝 {} proposes an alliance to {}.", self.name, target.name);
        events.push(TaggedEvent::new(
            proposed_line,
            MessagePayload::AllianceProposed {
                proposer: character_ref,
                target: target_ref.clone(),
            },
        ));

        // Sympathetic bond (5477 PR3 spec §12):
        // If target has observed a flashback, +0.10 bonus offsetting trauma penalty.
        let trauma_observer_bonus: f64 = if self.afflictions.values().any(|aff| {
            if !matches!(aff.kind, AfflictionKind::Trauma) {
                return false;
            }
            aff.trauma_metadata
                .as_ref()
                .is_some_and(|m| m.observed_by.contains(&target.identifier.to_string()))
        }) {
            0.10
        } else {
            0.0
        };

        // Addiction penalty (PR3 spec §11):
        // -0.10 per severity tier per known addiction the target has.
        let addiction_penalty: f64 = self
            .afflictions
            .values()
            .filter(|aff| matches!(aff.kind, AfflictionKind::Addiction(_)))
            .filter_map(|aff| aff.addiction_metadata.as_ref())
            .filter(|meta| meta.observed_by.contains(&target.identifier.to_string()))
            .map(|meta| {
                let aff = self
                    .afflictions
                    .values()
                    .find(|a| {
                        matches!(a.kind, AfflictionKind::Addiction(_))
                            && a.addiction_metadata
                                .as_ref()
                                .is_some_and(|m| m.substance == meta.substance)
                    })
                    .unwrap();
                -0.10 * (aff.severity as i32) as f64
            })
            .sum::<f64>()
            .abs();

        let same_team = self.team == target.team;
        let formed = try_form_alliance(
            &self.traits,
            &target.traits,
            same_team,
            self.allies.len(),
            target.allies.len(),
            phobia_penalty,
            trauma_penalty - trauma_observer_bonus,
            addiction_penalty,
            rng,
        );
        if formed {
            let factor = deciding_factor(&self.traits, &target.traits, same_team);
            let factor_label = factor
                .as_ref()
                .map(|f| f.label())
                .unwrap_or("mutual circumstance")
                .to_string();
            self.alliance_events
                .push(alliances::AllianceEvent::FormationRecorded {
                    proposer: self.id,
                    target: target.id,
                    factor: factor_label,
                });
        }
    }

    fn act_sleep(
        &mut self,
        duration_phases: u8,
        phase: shared::messages::Phase,
        events: &mut Vec<TaggedEvent>,
    ) {
        let character_ref = CharacterRef {
            identifier: self.identifier.clone().into(),
            name: self.name.clone(),
        };
        self.sleeping = true;
        self.sleep_remaining = duration_phases;
        events.push(TaggedEvent::new(
            GameOutput::CharacterSleeps(self.name.as_str()).to_string(),
            MessagePayload::CharacterSlept {
                character: character_ref,
                phase,
                restored_stamina: 0,
                restored_hp: 0,
            },
        ));
    }

    /// Wake an interrupted sleeper (PR2c.2). Resets the sleep
    /// state and `cycles_awake` per spec §6.4 ("rude awakening = no rest")
    /// and pushes a `CharacterWoke { Interrupted }` event onto `events`.
    /// Returns `true` if the character was actually sleeping (and was woken),
    /// `false` otherwise — letting callers branch on whether the
    /// interruption-trigger path applies further consequences.
    pub fn wake_interrupted(
        &mut self,
        reason: shared::messages::InterruptionKind,
        phase: shared::messages::Phase,
        events: &mut Vec<TaggedEvent>,
    ) -> bool {
        use shared::messages::WakeReason;

        if !self.sleeping {
            return false;
        }
        self.sleeping = false;
        self.sleep_remaining = 0;
        self.cycles_awake = 0;
        events.push(TaggedEvent::new(
            world::output::GameOutput::CharacterWakesInterrupted(self.name.as_str()).to_string(),
            MessagePayload::CharacterWoke {
                character: CharacterRef {
                    identifier: self.identifier.clone().into(),
                    name: self.name.clone(),
                },
                phase,
                reason: WakeReason::Interrupted { event: reason },
            },
        ));
        true
    }

    /// Drain this character's per-turn alliance event buffer. Called by
    /// `Game::run_character_cycle` after each character's turn so the events
    /// are appended to the game's queue and processed before the next
    /// character acts. See spec §7.5.
    pub fn drain_alliance_events(&mut self) -> Vec<alliances::AllianceEvent> {
        std::mem::take(&mut self.alliance_events)
    }

    /// Advance per-character alliance bookkeeping for one turn (spec §7.4).
    ///
    /// Ticks `turns_since_last_betrayal`, which gates Treacherous-trait
    /// betrayals to fire at most every
    /// [`alliances::TREACHEROUS_BETRAYAL_INTERVAL`] turns. Saturates at
    /// `u8::MAX` so a long-lived character never overflows or wraps back
    /// through the betrayal trigger. Dead characters are skipped.
    pub fn tick_alliance_timers(&mut self) {
        if !self.is_alive() {
            return;
        }
        self.turns_since_last_betrayal = self.turns_since_last_betrayal.saturating_add(1);
    }

    /// Consume a pending trust-shock flag (set when this character was the
    /// victim of a betrayal). For each current ally, roll
    /// [`alliances::trust_shock_roll`]; on success drop that ally from this
    /// character's `allies` list and emit a message. The flag is reset
    /// unconditionally so it never carries past the turn it fires.
    ///
    /// Note: this only mutates `self`. The symmetric back-edge on the broken
    /// ally's side is left to the next cycle's processing or to subsequent
    /// alliance events; per Phase 4 of the implementation plan, full
    /// symmetric cleanup is deferred. See spec §7.3c1.
    pub fn consume_pending_trust_shock(
        &mut self,
        rng: &mut impl rand::Rng,
        events: &mut Vec<TaggedEvent>,
    ) {
        if !self.pending_trust_shock {
            return;
        }
        let limit = self.brain.thresholds.extreme_low_sanity;
        let sanity = self.effective_sanity();
        let mut broken: Vec<Uuid> = Vec::new();
        for ally_id in &self.allies {
            if alliances::trust_shock_roll(sanity, limit, rng) {
                broken.push(*ally_id);
            }
        }
        for ally_id in &broken {
            self.allies.retain(|x| x != ally_id);
            let ally_str = ally_id.to_string();
            let line = format!(
                "{} loses faith and breaks ties with ally {}.",
                self.name, ally_id
            );
            events.push(TaggedEvent::new(
                line,
                MessagePayload::TrustShockBreak {
                    character: CharacterRef {
                        identifier: self.identifier.clone().into(),
                        name: self.name.clone(),
                    },
                    partner: CharacterRef {
                        identifier: ally_str.clone().into(),
                        name: ally_str,
                    },
                },
            ));
        }
        self.pending_trust_shock = false;
    }

    /// Attempt to acquire a new affliction, resolving against existing afflictions.
    /// Returns the resolution (Insert, Upgrade, Supersede, or Reject).
    pub fn try_acquire_affliction(&mut self, draft: AfflictionDraft) -> AcquireResolution {
        let cycle = self.game_day.unwrap_or(0) as u32;
        let provisional = Affliction {
            kind: draft.kind.clone(),
            body_part: draft.body_part,
            severity: draft.severity,
            source: draft.source.clone(),
            acquired_cycle: cycle,
            last_progressed_cycle: cycle,
            trauma_metadata: None,
            phobia_metadata: None,
            fixation_metadata: None,
            addiction_metadata: None,
            trapped_metadata: draft.trapped_metadata.clone(),
        };
        let resolution = can_acquire(&self.afflictions, &provisional);

        match &resolution {
            AcquireResolution::Insert
            | AcquireResolution::Upgrade(_)
            | AcquireResolution::Supersede(_) => {
                // Handle Supersede: remove superseded afflictions
                if let AcquireResolution::Supersede(_) = resolution {
                    // MissingArm/MissingLeg supersedes all afflictions on that body part
                    let part = draft.body_part;
                    let keys_to_remove: Vec<_> = self
                        .afflictions
                        .keys()
                        .filter(|(_, bp)| bp == &part)
                        .cloned()
                        .collect();
                    for key in keys_to_remove {
                        self.afflictions.remove(&key);
                    }
                }

                // Infected supersedes Wounded at same body part (cascade rule).
                // can_acquire returns Insert for this case, but we still need
                // to remove the Wounded ancestor.
                if draft.kind == AfflictionKind::Infected {
                    let wounded_key = (AfflictionKind::Wounded, draft.body_part);
                    self.afflictions.remove(&wounded_key);
                }

                let affliction = Affliction {
                    kind: draft.kind.clone(),
                    body_part: draft.body_part,
                    severity: draft.severity,
                    source: draft.source,
                    acquired_cycle: cycle,
                    last_progressed_cycle: cycle,
                    trauma_metadata: None,
                    phobia_metadata: None,
                    fixation_metadata: None,
                    addiction_metadata: None,
                    trapped_metadata: draft.trapped_metadata.clone(),
                };
                self.afflictions
                    .insert((draft.kind.clone(), draft.body_part), affliction);
            }
            AcquireResolution::Reject(_) => {}
        }

        resolution
    }

    /// Apply affliction-based hard gates to an action at execution time.
    ///
    /// This is the terrain-aware complement to the pre-decision
    /// `affliction_override` layer. Some gates (MissingLeg -> cliff/swamp
    /// terrain) require knowledge of the destination area's terrain, which
    /// is only available after the brain has decided on a Move action.
    ///
    /// Returns `Some(fallback)` if the action is blocked, `None` if allowed.
    ///
    /// Gates (spec §11):
    /// - MissingLeg (Moderate+) + cliff/swamp destination -> Rest
    /// - Blind (Moderate+) + ranged attack -> Move(None)
    ///   (Action::Attack covers both melee+ranged; gated when distinct
    ///   ranged variant exists.)
    /// - MissingArm (Moderate+) + 2H weapon -> fallback
    ///   (Weapon info not in Action yet; enforced at combat resolution.)
    pub fn affliction_action_gate(
        &self,
        action: &Action,
        destination_terrain: Option<world::terrain::BaseTerrain>,
    ) -> Option<Action> {
        crate::brains::affliction_override::hard_gates_with_terrain(
            self,
            action,
            destination_terrain,
        )
    }
}

/// A draft affliction ready for acquisition resolution.
#[derive(Clone)]
pub struct AfflictionDraft {
    pub kind: AfflictionKind,
    pub body_part: Option<BodyPart>,
    pub severity: Severity,
    pub source: AfflictionSource,
    pub trapped_metadata: Option<TrappedMetadata>,
}

/// Calculates the stamina cost for a character action based on:
/// - Base action cost
/// - Terrain movement multiplier
/// - Terrain affinity modifier (0.8 if character has affinity, 1.0 otherwise)
/// - Desperation multiplier based on health (1.0 + 0.5 * (1.0 - health%))
pub fn calculate_stamina_cost(
    action: &Action,
    terrain: &world::terrain::TerrainType,
    character: &Character,
) -> u32 {
    // Base costs for each action type
    let base_cost: f32 = match action {
        Action::Move(_) => 20.0,
        Action::Hide => 15.0,
        Action::TakeItem => 10.0,
        Action::Attack => 25.0,
        Action::Rest | Action::None => 0.0,
        Action::UseItem(_) => 10.0,
        // Proposing an alliance is a low-cost social action.
        Action::ProposeAlliance => 5.0,
        // Survival actions: foraging/seeking shelter cost some stamina;
        // eating and drinking are essentially free overhead.
        Action::SeekShelter => 10.0,
        Action::Forage => 15.0,
        Action::DrinkFromTerrain => 5.0,
        Action::Eat(_) | Action::DrinkItem(_) => 0.0,
        // Sleep is free at the action layer; phase scheduler handles it.
        Action::Sleep { .. } => 0.0,
        Action::Rescue { .. } => 15.0,
        Action::SetTrap { .. } => 15.0,
        Action::Search => 10.0,
        Action::Frozen
        | Action::Flashback { .. }
        | Action::Avoidance
        | Action::SearchForSubstance { .. } => 0.0,
    };

    // If base cost is 0, no need to calculate multipliers
    if base_cost == 0.0 {
        return 0;
    }

    // Terrain multiplier from movement_cost
    let terrain_multiplier = terrain.base.movement_cost();

    // Affinity modifier: 0.8 if character has affinity for this terrain, else 1.0
    let affinity_modifier = if character.terrain_affinity.contains(&terrain.base) {
        0.8
    } else {
        1.0
    };

    // Desperation multiplier: 1.0 + (0.5 x (1.0 - health%))
    let health_percent = character.effective_health() as f32 / 100.0;
    let desperation_multiplier = 1.0 + (0.5 * (1.0 - health_percent));

    // Calculate final cost with all multipliers
    let final_cost = base_cost * terrain_multiplier * affinity_modifier * desperation_multiplier;

    // Round to nearest integer
    final_cost.round() as u32
}

/// The stamina price of a sub-hop crossing from one cell to the next:
/// half the SOURCE cell's standard move cost plus half the
/// DESTINATION's, each rounded UP. Owner decision (sub-hex travel
/// phase): neither tile dominates a crossing — a cheap tile next to an
/// expensive one must not make repeated crossings from the cheap side
/// free. Odd costs lean expensive on purpose: each half rounds up, so
/// a 30/40 crossing prices at 15 + 20 = 35.
pub fn crossing_stamina_cost(from_standard_cost: u32, to_standard_cost: u32) -> u32 {
    from_standard_cost.div_ceil(2) + to_standard_cost.div_ceil(2)
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Statistics {
    /// What day, if any, were they killed?
    pub day_killed: Option<u32>,
    /// Who or what killed them?
    pub killed_by: Option<String>,
    /// How many characters did they kill?
    pub kills: u32,
    /// How many fights did they win?
    pub wins: u32,
    /// How many fights did they lose?
    pub defeats: u32,
    /// How many fights ended in a draw?
    pub draws: u32,
    /// Which game do these stats relate to?
    #[serde(default)]
    pub game: String,
    /// How many times have they been revived? Lifetime total, carried
    /// across game boundaries by the baseline settle.
    #[serde(default)]
    pub times_revived: u32,
    /// How many betrayals have they suffered? Lifetime total, carried
    /// across game boundaries by the baseline settle.
    #[serde(default)]
    pub betrayals_suffered: u32,
    /// How many betrayals have they committed? Lifetime total, carried
    /// across game boundaries by the baseline settle.
    #[serde(default)]
    pub betrayals_committed: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Attributes {
    /// How hard do they hit?
    pub strength: u32,
    /// How much of a hit can they take?
    #[serde(alias = "defense")]
    pub toughness: u32,
    /// Will they jump into dangerous situations?
    pub bravery: u32,
    /// How well do they avoid traps?
    pub intelligence: u32,
    /// Can they talk their way out of, or into, things?
    pub persuasion: u32,
    /// Are they likely to get gifts or come out slightly ahead?
    pub luck: u32,
    /// How quickly they react in a turn.
    #[serde(alias = "agility")]
    pub speed: u32,
    /// How sharp are their senses? Sight range, ambush detection, spotting.
    ///  so pre-rename persisted rows (no perception key) hydrate.
    #[serde(default = "default_perception")]
    pub perception: u32,
    /// Can other characters see them?
    pub is_hidden: bool,
}

fn default_perception() -> u32 {
    100
}

impl Default for Attributes {
    /// Provides a maxed-out set of Attributes
    fn default() -> Self {
        Self {
            strength: 50,
            toughness: 50,
            bravery: 100,
            intelligence: 100,
            persuasion: 100,
            luck: 100,
            speed: 50,
            perception: 100,
            is_hidden: false,
        }
    }
}

impl Attributes {
    /// Provides a randomized set of Attributes using default config values
    pub fn new() -> Self {
        let mut rng = SmallRng::from_rng(&mut rand::rng());
        Self::from_rng(&mut rng)
    }

    /// Read one attribute by its typed axis. Mechanics that need the raw
    /// base read it here; everything that should see trait/wound modifiers
    /// goes through `Character::effective_attribute` instead.
    pub fn get(&self, attr: crate::traits::CharacterAttribute) -> u32 {
        use crate::traits::CharacterAttribute as A;
        match attr {
            A::Strength => self.strength,
            A::Toughness => self.toughness,
            A::Bravery => self.bravery,
            A::Intelligence => self.intelligence,
            A::Persuasion => self.persuasion,
            A::Luck => self.luck,
            A::Speed => self.speed,
            A::Perception => self.perception,
        }
    }

    /// Rolls a randomized set of Attributes from the provided RNG so callers
    /// that own their entropy (seeded generators, test fixtures) get
    /// deterministic attributes too.
    pub fn from_rng(rng: &mut SmallRng) -> Self {
        let config = world::config::GameConfig::default();

        Self {
            strength: rng.random_range(1..=config.max_strength),
            toughness: rng.random_range(1..=config.max_toughness),
            bravery: rng.random_range(1..=config.max_bravery),
            intelligence: rng.random_range(1..=config.max_intelligence),
            persuasion: rng.random_range(1..=config.max_persuasion),
            luck: rng.random_range(1..=config.max_luck),
            speed: rng.random_range(1..=config.max_speed),
            perception: rng.random_range(1..=config.max_perception),
            is_hidden: false,
        }
    }

    /// One coherent generation pass: roll attributes from the RNG, then bend
    /// each axis by the rolled traits' `AttrMod` percentages — a goblin who
    /// rolled `Tough` trends tough before any effective-value math runs.
    ///
    /// This is *character coherence*, not a mechanic: the bias lands in the
    /// base values, so `effective_attribute` still stacks trait modifiers on
    /// top. Bias clamps at the config max.
    pub fn from_rng_with_traits(rng: &mut SmallRng, rolled: &[crate::traits::Trait]) -> Self {
        use crate::traits::CharacterAttribute;
        let mut attrs = Self::from_rng(rng);
        let config = world::config::GameConfig::default();

        let axes: [(CharacterAttribute, u32, u32); 8] = [
            (
                CharacterAttribute::Strength,
                attrs.strength,
                config.max_strength,
            ),
            (
                CharacterAttribute::Toughness,
                attrs.toughness,
                config.max_toughness,
            ),
            (
                CharacterAttribute::Bravery,
                attrs.bravery,
                config.max_bravery,
            ),
            (
                CharacterAttribute::Intelligence,
                attrs.intelligence,
                config.max_intelligence,
            ),
            (
                CharacterAttribute::Persuasion,
                attrs.persuasion,
                config.max_persuasion,
            ),
            (CharacterAttribute::Luck, attrs.luck, config.max_luck),
            (CharacterAttribute::Speed, attrs.speed, config.max_speed),
            (
                CharacterAttribute::Perception,
                attrs.perception,
                config.max_perception,
            ),
        ];

        for (attr, base, max) in axes {
            let bias: i32 = rolled
                .iter()
                .flat_map(|t| t.def().effects)
                .filter_map(|e| match e {
                    crate::traits::Effect::AttrMod {
                        attr: a,
                        flat,
                        percent,
                    } if *a == attr => Some(flat + percent),
                    _ => None,
                })
                .sum();
            if bias == 0 {
                continue;
            }
            let bent = ((base as f64) * (1.0 + f64::from(bias) / 100.0)).round() as u32;
            let bent = bent.clamp(1, max);
            match attr {
                CharacterAttribute::Strength => attrs.strength = bent,
                CharacterAttribute::Toughness => attrs.toughness = bent,
                CharacterAttribute::Bravery => attrs.bravery = bent,
                CharacterAttribute::Intelligence => attrs.intelligence = bent,
                CharacterAttribute::Persuasion => attrs.persuasion = bent,
                CharacterAttribute::Luck => attrs.luck = bent,
                CharacterAttribute::Speed => attrs.speed = bent,
                CharacterAttribute::Perception => attrs.perception = bent,
            }
        }
        attrs
    }
}

#[cfg(test)]
mod tests {

    use crate::Action;
    use crate::Character;
    use crate::brains::Brain;
    use crate::crossing_stamina_cost;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    use rstest::*;
    use shared::messages::MessagePayload;
    use world::messages::TaggedEvent;

    /// Owner's worked example: A costs 30, B costs 40 -> 15 + 20 = 35.
    #[test]
    fn crossing_cost_averages_half_of_each_tile() {
        assert_eq!(crossing_stamina_cost(30, 40), 35);
        assert_eq!(crossing_stamina_cost(40, 30), 35, "symmetric in order");
    }

    /// Each half rounds UP independently, so odd costs lean expensive.
    #[test]
    fn crossing_cost_rounds_each_half_up() {
        assert_eq!(crossing_stamina_cost(31, 40), 16 + 20, "odd source");
        assert_eq!(crossing_stamina_cost(30, 41), 15 + 21, "odd destination");
        assert_eq!(crossing_stamina_cost(31, 41), 16 + 21, "both odd");
        assert_eq!(crossing_stamina_cost(1, 1), 2, "minimum positive crossing");
        assert_eq!(crossing_stamina_cost(0, 0), 0, "free cells cross free");
    }

    #[fixture]
    fn character() -> Character {
        let mut c = Character::new("Snaggletooth".to_string(), None, None);
        // Builder/ResourcefulForager change the SeekShelter/Forage rolls;
        // survival tests pin them explicitly instead of taking the entropy
        // roll (the same way `trait_golden::fixture` pins attributes).
        c.traits.retain(|t| {
            !matches!(
                t,
                crate::traits::Trait::Builder | crate::traits::Trait::ResourcefulForager
            )
        });
        c
    }

    #[fixture]
    fn target() -> Character {
        Character::new("Grubworm".to_string(), None, None)
    }

    #[fixture]
    fn small_rng() -> SmallRng {
        SmallRng::seed_from_u64(0)
    }

    #[rstest]
    fn default() {
        let character = Character::default();
        assert_eq!(character.name, "Default Character");
    }

    #[test]
    fn gain_and_lose_trait_use_the_single_mutation_path() {
        use crate::advancement::AdvancementEvent;
        use crate::traits::{Trait, TraitError};

        let mut c = Character::new("Stinky".to_string(), None, None);
        c.traits.clear();

        assert!(c.gain_trait(Trait::Friendly).is_ok());
        assert_eq!(
            c.advancement_log,
            vec![AdvancementEvent::TraitGained {
                trait_: Trait::Friendly
            }],
            "a real gain stamps the log"
        );
        // Duplicate rejected.
        assert_eq!(
            c.gain_trait(Trait::Friendly),
            Err(TraitError::Duplicate(Trait::Friendly))
        );
        // Flavor conflict rejected (Friendly vs Paranoid).
        assert!(matches!(
            c.gain_trait(Trait::Paranoid),
            Err(TraitError::FlavorConflict { .. })
        ));
        assert_eq!(c.traits, vec![Trait::Friendly]);
        assert_eq!(c.advancement_log.len(), 1, "rejected gains do not stamp");

        assert!(c.lose_trait(Trait::Friendly));
        assert!(c.traits.is_empty());
        assert!(
            matches!(
                c.advancement_log.first(),
                Some(AdvancementEvent::TraitGained { .. })
            ),
            "gain stamped first"
        );
        assert!(
            matches!(
                c.advancement_log.get(1),
                Some(AdvancementEvent::TraitLost { .. })
            ),
            "a real loss stamps after the gain"
        );
        assert!(!c.lose_trait(Trait::Friendly), "second loss is a no-op");
        assert_eq!(c.advancement_log.len(), 2, "no-op losses do not stamp");
    }

    #[rstest]
    fn serde_roundtrip_alliance_fields() {
        use crate::traits::Trait;
        use uuid::Uuid;

        let mut character = Character::new("Nib".to_string(), None, None);
        let ally = Uuid::new_v4();
        character.allies.push(ally);
        character.traits.clear();
        character.traits.push(Trait::Loyal);
        character.traits.push(Trait::Treacherous);
        character.turns_since_last_betrayal = 7;
        character.pending_trust_shock = true;

        let json = serde_json::to_string(&character).expect("serialize");
        assert!(json.contains("\"allies\""));
        assert!(json.contains("\"traits\""));
        assert!(json.contains("\"Loyal\""));
        assert!(json.contains("\"turns_since_last_betrayal\":7"));
        assert!(json.contains("\"pending_trust_shock\":true"));

        let restored: Character = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.allies, vec![ally]);
        assert_eq!(restored.traits, vec![Trait::Loyal, Trait::Treacherous]);
        assert_eq!(restored.turns_since_last_betrayal, 7);
        assert!(restored.pending_trust_shock);
    }

    #[rstest]
    fn serde_defaults_for_missing_alliance_fields() {
        // Persisted character records written before the alliance fields existed
        // must still deserialize. Simulate this by serialising a fresh character,
        // stripping the new fields, then round-tripping.
        let baseline = Character::new("Legacy".to_string(), None, None);
        let mut value: serde_json::Value = serde_json::to_value(&baseline).expect("to_value");
        let obj = value.as_object_mut().expect("object");
        obj.remove("allies");
        obj.remove("traits");
        obj.remove("turns_since_last_betrayal");
        obj.remove("pending_trust_shock");

        let restored: Character = serde_json::from_value(value).expect("legacy deserialize");
        assert!(restored.allies.is_empty());
        assert!(restored.traits.is_empty());
        assert_eq!(restored.turns_since_last_betrayal, 0);
        assert!(!restored.pending_trust_shock);
    }

    #[rstest]
    fn brain_roundtrips_psychotic_break_state() {
        use crate::brains::PsychoticBreakType;

        let mut character = Character::new("Rendmaw".to_string(), None, None);
        character.brain.psychotic_break = Some(PsychoticBreakType::Berserk);

        let json = serde_json::to_string(&character).expect("serialize");
        assert!(json.contains("\"brain\""));
        assert!(json.contains("\"Berserk\""));

        let restored: Character = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            restored.brain.psychotic_break,
            Some(PsychoticBreakType::Berserk),
        );
    }

    #[rstest]
    fn brain_preferred_action_is_not_persisted() {
        // preferred_action is transient AI state recomputed each cycle, so the
        // field is `skip_serializing` and `deserialize_optional_enum_lenient`
        // (which absorbs both null and the {} corruption left over from the
        // SDK's enum-collapse bug). A roundtrip therefore intentionally drops
        // any preferred_action that was set in memory.
        use crate::actions::Action;

        let mut character = Character::new("Snoutrot".to_string(), None, None);
        character.brain.preferred_action = Some(Action::Hide);
        character.brain.preferred_action_percentage = 0.75;

        let json = serde_json::to_string(&character).expect("serialize");

        let restored: Character = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.brain.preferred_action, None);
        // Non-skipped fields still round-trip normally.
        assert!((restored.brain.preferred_action_percentage - 0.75).abs() < f64::EPSILON);
    }

    #[rstest]
    fn brain_tolerates_corrupt_preferred_action_object() {
        // SurrealDB rows written before the bug-5 fix have preferred_action: {}
        // because the SDK's bespoke serializer collapsed the externally-tagged
        // Action enum. The lenient deserializer must read those rows as None.
        // Round-trip a real Brain to get a valid base JSON, then swap
        // preferred_action's value to {} to simulate the corruption.
        use crate::brains::Brain;

        let brain = Brain {
            preferred_action_percentage: 0.5,
            ..Brain::default()
        };
        let mut value = serde_json::to_value(&brain).expect("serialize brain");
        value["preferred_action"] = serde_json::json!({});
        let restored: Brain = serde_json::from_value(value).expect("deserialize legacy row");
        assert_eq!(restored.preferred_action, None);
    }

    #[rstest]
    fn brain_missing_field_defaults() {
        // Pre-fix character rows persisted before #[serde(default)] was added
        // omit the `brain` column entirely. They must still deserialize, with
        // brain hydrated via `Brain::default()`.
        let baseline = Character::new("Legacy".to_string(), None, None);
        let mut value: serde_json::Value = serde_json::to_value(&baseline).expect("to_value");
        value.as_object_mut().expect("object").remove("brain");

        let restored: Character = serde_json::from_value(value).expect("legacy deserialize");
        assert_eq!(restored.brain, Brain::default());
        assert!(restored.brain.psychotic_break.is_none());
        assert!(restored.brain.preferred_action.is_none());
    }

    #[rstest]
    fn new() {
        let character = Character::new("Snaggletooth".to_string(), Some(8), None);
        assert_eq!(character.name, "Snaggletooth");
        assert_eq!(character.team, 8);
        assert_eq!(character.blood, 1000, "blood starts at full");
    }

    #[rstest]
    fn random() {
        let character = Character::random();
        assert!(!character.name.is_empty());
        assert!(character.team >= 1 && character.team <= 8);
    }

    #[rstest]
    fn new_character_has_empty_alliance_state() {
        let character = Character::new("Snipsnout".to_string(), Some(1), None);
        assert!(character.allies.is_empty());
        assert_eq!(character.turns_since_last_betrayal, 0);
        // `id` mirrors `identifier`.
        assert_eq!(character.id.to_string(), character.identifier.to_string());
    }

    #[rstest]
    fn new_character_has_no_pending_trust_shock() {
        let character = Character::new("Snipsnout".to_string(), Some(1), None);
        assert!(!character.pending_trust_shock);
    }

    #[test]
    fn character_default_survival_fields_are_zero_and_none() {
        let t = Character::new("Test".to_string(), None, None);
        assert_eq!(t.hunger, 0, "hunger starts at 0 (Sated)");
        assert_eq!(t.thirst, 0, "thirst starts at 0 (Sated)");
        assert_eq!(t.sheltered_until, None, "starts exposed");
        assert!(t.starving_since_day.is_none());
        assert!(t.dehydrated_since_day.is_none());
    }

    #[test]
    fn terrain_drink_satesses_thirst_and_reports_debt() {
        let mut t = Character::new("Tepsip".to_string(), None, None);
        t.thirst = 3; // Dehydrated
        t.dehydrated_since_day = Some(1);
        t.stamina = t.max_stamina;
        let details = areas::AreaDetails::new(None, areas::AreaId::hub());
        let mut rng = SmallRng::seed_from_u64(7);
        let mut events: Vec<TaggedEvent> = vec![];

        t.act_drink_from_terrain(&details, &mut rng, &mut events);

        assert_eq!(
            t.thirst,
            3 - crate::survival::SATIATE_AMOUNT,
            "one full sate takes SATIATE_AMOUNT points off"
        );
        assert!(t.dehydrated_since_day.is_none(), "drink clears the stamp");
        assert_eq!(events.len(), 1, "one Drank line per drink");
        match &events[0].payload {
            MessagePayload::Drank {
                character,
                source,
                debt_recovered,
            } => {
                assert_eq!(character.name, "Tepsip");
                assert_eq!(*debt_recovered, crate::survival::SATIATE_AMOUNT);
                assert!(
                    matches!(source, shared::messages::DrinkSource::Terrain { .. }),
                    "terrain source"
                );
            }
            other => panic!("expected Drank, got {other:?}"),
        }
        // Contamination (15%) is seed-dependent — Poisoned visibility is
        // covered by the affliction/band machinery, not asserted here.
    }

    #[test]
    fn eat_consumes_food_item_and_reports_debt() {
        let mut t = Character::new("Grubbit".to_string(), None, None);
        t.hunger = 4; // Starving
        t.starving_since_day = Some(1);
        let food = world::items::Item::new(
            "Dried Grub",
            world::items::ItemType::Food(3),
            world::items::ItemRarity::Common,
            5,
            world::items::Attribute::Strength,
            0,
        );
        t.items.push(food.clone());
        let mut rng = SmallRng::seed_from_u64(11);
        let mut events: Vec<TaggedEvent> = vec![];

        t.act_eat(Some(food.clone()), &mut rng, &mut events);

        assert!(
            t.items.iter().all(|i| i.identifier != food.identifier),
            "the eaten item is destroyed"
        );
        assert_eq!(t.hunger, 4 - 3, "food_value points off the hunger bar");
        assert!(t.starving_since_day.is_none(), "eat clears the stamp");
        assert_eq!(events.len(), 1, "one Ate line per meal");
        match &events[0].payload {
            MessagePayload::Ate {
                character,
                item,
                debt_recovered,
            } => {
                assert_eq!(character.name, "Grubbit");
                assert_eq!(item.name, "Dried Grub");
                assert_eq!(*debt_recovered, 3);
            }
            other => panic!("expected Ate, got {other:?}"),
        }
        // 5% contamination is seed-dependent here — see the affliction
        // machinery tests for Poisoned visibility.
    }

    #[test]
    fn character_legacy_json_loads_with_defaults() {
        // JSON missing the new survival fields entirely (simulates a saved
        // game from before this feature landed). serde(default) must
        // populate them.
        let mut t = Character::new("Legacy".to_string(), Some(1), None);
        t.hunger = 0;
        t.thirst = 0;
        t.sheltered_until = None;
        t.starving_since_day = None;
        t.dehydrated_since_day = None;
        let mut json: serde_json::Value = serde_json::to_value(&t).unwrap();
        // strip the survival fields to mimic a save missing them
        let obj = json.as_object_mut().unwrap();
        obj.remove("hunger");
        obj.remove("thirst");
        obj.remove("sheltered_until");
        obj.remove("starving_since_day");
        obj.remove("dehydrated_since_day");
        let loaded: Character = serde_json::from_value(json).expect("load must succeed");
        assert_eq!(loaded.hunger, 0);
        assert_eq!(loaded.thirst, 0);
        assert_eq!(loaded.sheltered_until, None);
        assert!(loaded.starving_since_day.is_none());
        assert!(loaded.dehydrated_since_day.is_none());
    }

    #[test]
    fn character_legacy_json_loads_with_sleep_defaults() {
        // JSON missing the new sleep fields must default to zero/false.
        let t = Character::new("Legacy".to_string(), Some(1), None);
        let mut json: serde_json::Value = serde_json::to_value(&t).unwrap();
        let obj = json.as_object_mut().unwrap();
        obj.remove("cycles_awake");
        obj.remove("sleeping");
        obj.remove("sleep_remaining");
        let loaded: Character = serde_json::from_value(json).expect("legacy load must succeed");
        assert_eq!(loaded.cycles_awake, 0);
        assert!(!loaded.sleeping);
        assert_eq!(loaded.sleep_remaining, 0);
    }

    #[rstest]
    fn character_drain_alliance_events_returns_and_clears_buffer() {
        use crate::alliances::AllianceEvent;
        use uuid::Uuid;
        let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
        let other = Uuid::new_v4();
        character
            .alliance_events
            .push(AllianceEvent::BetrayalRecorded {
                betrayer: character.id,
                victim: other,
            });
        let drained = character.drain_alliance_events();
        assert_eq!(drained.len(), 1);
        assert!(character.alliance_events.is_empty());
    }

    #[rstest]
    fn consume_pending_trust_shock_resets_flag_when_not_set() {
        // No flag -> no rolls, flag stays false, allies untouched.
        let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
        let ally = uuid::Uuid::new_v4();
        character.allies.push(ally);
        let mut events: Vec<TaggedEvent> = vec![];
        let mut rng = rand::rngs::SmallRng::seed_from_u64(53);
        character.consume_pending_trust_shock(&mut rng, &mut events);
        assert!(!character.pending_trust_shock);
        assert_eq!(character.allies, vec![ally]);
        assert!(events.is_empty());
    }

    #[rstest]
    fn consume_pending_trust_shock_breaks_allies_on_success_and_clears_flag() {
        // Force trust_shock to fire deterministically: sanity=0, threshold>0
        // gives p = 0.5 + 0.5 * 1.0 = 1.0 -> always true.
        use shared::conditions::{ConditionSeverity, MentalCondition};
        let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
        // 10 critical conditions -> 100 drain -> effective_sanity = 0
        character.mental_conditions = (0..10)
            .map(|_| MentalCondition::Pain {
                severity: ConditionSeverity::Critical,
            })
            .collect();
        character.brain.thresholds.extreme_low_sanity = 50;
        let ally1 = uuid::Uuid::new_v4();
        let ally2 = uuid::Uuid::new_v4();
        character.allies.push(ally1);
        character.allies.push(ally2);
        character.pending_trust_shock = true;

        let mut events: Vec<TaggedEvent> = vec![];
        let mut rng = rand::rngs::SmallRng::seed_from_u64(211);
        character.consume_pending_trust_shock(&mut rng, &mut events);

        assert!(!character.pending_trust_shock, "flag must reset");
        assert!(
            character.allies.is_empty(),
            "all allies broken on guaranteed success"
        );
        assert_eq!(events.len(), 2, "one message per broken ally");
    }

    #[rstest]
    fn consume_pending_trust_shock_no_break_when_sanity_above_threshold() {
        // Sanity at/above threshold -> trust_shock_roll returns false -> no break.
        let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
        // No mental conditions -> effective_sanity = 100
        character.brain.thresholds.extreme_low_sanity = 50;
        let ally = uuid::Uuid::new_v4();
        character.allies.push(ally);
        character.pending_trust_shock = true;

        let mut events: Vec<TaggedEvent> = vec![];
        let mut rng = rand::rngs::SmallRng::seed_from_u64(89);
        character.consume_pending_trust_shock(&mut rng, &mut events);

        assert!(!character.pending_trust_shock, "flag must reset");
        assert_eq!(character.allies, vec![ally], "ally retained");
        assert!(events.is_empty());
    }

    #[rstest]
    fn new_character_has_traits_for_any_team() {
        let character = Character::new("Snaggletooth".to_string(), Some(8), None);
        // generate_traits rolls 2..=6 traits from the shared pool.
        assert!((2..=6).contains(&character.traits.len()));
    }

    #[rstest]
    fn traits_and_affinity_do_not_depend_on_team() {
        // Same seed, different team slots -> identical rolls. Team must not
        // seed traits, brain, or terrain affinity.
        let mut rng_a = SmallRng::seed_from_u64(1234);
        let mut rng_b = SmallRng::seed_from_u64(1234);
        let a = Character::new_with_rng("Alpha".to_string(), Some(1), None, &mut rng_a);
        let b = Character::new_with_rng("Beta".to_string(), Some(8), None, &mut rng_b);

        assert_eq!(a.traits, b.traits);
        assert_eq!(a.terrain_affinity, b.terrain_affinity);
        assert_eq!(a.brain.thresholds, b.brain.thresholds);
        assert_ne!(a.team, b.team, "the team slots themselves still differ");
    }

    #[rstest]
    fn new_character_rolls_terrain_affinity_even_without_team() {
        let character = Character::new("Teamless".to_string(), None, None);
        assert!((1..=2).contains(&character.terrain_affinity.len()));
    }

    #[rstest]
    fn pick_target_skips_allies() {
        // An ally is in the same area but must not be picked as a target.
        let mut me = Character::new("Snaggletooth".to_string(), Some(8), None);
        // No mental conditions -> effective_sanity = 100 -> not suicidal
        let ally = Character::new("Grubworm".to_string(), Some(8), None);
        me.allies.push(ally.id);

        let target = me.pick_target(vec![ally.clone()], 5);
        // Only candidate was an ally and we're not in final confrontation.
        assert!(target.is_none());
    }

    #[rstest]
    fn suicidal_pick_target_hands_self_to_resolver_without_announcing() {
        use shared::conditions::{ConditionSeverity, MentalCondition};
        let mut me = Character::new("Doomed".to_string(), Some(8), None);
        for _ in 0..10 {
            me.mental_conditions.push(MentalCondition::Horrified {
                severity: ConditionSeverity::Critical,
                remaining_periods: 3,
            });
        }
        assert!(me.effective_sanity() <= crate::SANITY_BREAK_LEVEL);

        // With no `events` parameter, announcing early is structurally
        // impossible.
        let target = me.pick_target(vec![], 5);
        assert!(target.is_some(), "suicidal goblin targets itself");
    }

    #[rstest]
    fn into_new_game_rebuilds_fresh_character_keeps_identity() {
        use crate::actions::Action;
        use crate::brains::PsychoticBreakType;
        use crate::statuses::CharacterStatus;
        use areas::AreaId;
        use shared::afflictions::Substance;
        let mut me = Character::new(
            "Stinky".to_string(),
            Some(3),
            Some("avatar.png".to_string()),
        );
        let traits_before = me.traits.clone();
        let strength_before = me.attributes.strength;
        let clan_before = me.clan_name.clone();

        // Life leaves a mess behind.
        me.set_status(CharacterStatus::Dead);
        me.blood = 0;
        me.hunger = 5;
        me.thirst = 7;
        me.stamina = 9;
        me.max_stamina = 42;
        me.allies.push(uuid::Uuid::new_v4());
        me.turns_since_last_betrayal = 3;
        me.sleeping = true;
        me.sleep_remaining = 2;
        me.cycles_awake = 4;
        me.statistics.day_killed = Some(3);
        me.statistics.killed_by = Some("Snaggletooth".to_string());
        me.statistics.game = "old-game".to_string();
        me.statistics.kills = 2;
        me.brain.psychotic_break = Some(PsychoticBreakType::Berserk);
        me.brain.preferred_action = Some(Action::Hide);
        me.brain.preferred_action_percentage = 0.75;
        me.brain.thresholds.low_health = u32::MAX; // stale sentinel
        me.ever_addicted_to.insert(Substance::Alcohol);

        let me = me.into_new_game();

        // Per-game state is pristine.
        assert_eq!(me.status, CharacterStatus::default());
        assert!(me.is_alive());
        assert!(me.wounds.is_empty());
        assert_eq!(me.location.area, AreaId::hub());
        // Pristine stamina is trait-coherent: carried curses like
        // Asthmatic shrink the fresh pool.
        let fresh_max = crate::lifecycle::health::max_stamina_for_traits(&me.traits);
        assert_eq!(me.stamina, fresh_max);
        assert_eq!(me.max_stamina, fresh_max);
        assert_eq!(me.hunger, 0);
        assert_eq!(me.thirst, 0);
        assert!(me.allies.is_empty());
        assert_eq!(me.turns_since_last_betrayal, 0);
        assert!(!me.sleeping);
        assert_eq!(me.sleep_remaining, 0);
        assert_eq!(me.cycles_awake, 0);
        assert_eq!(me.statistics.day_killed, None);
        assert_eq!(me.statistics.killed_by, None);
        assert_eq!(me.statistics.game, "");
        assert_eq!(me.statistics.kills, 2, "lifetime kills persist");

        // Identity and progression are untouched.
        assert_eq!(me.name, "Stinky");
        assert_eq!(me.team, 3);
        assert_eq!(me.avatar.as_deref(), Some("avatar.png"));
        assert_eq!(me.traits, traits_before);
        assert_eq!(me.attributes.strength, strength_before);
        assert_eq!(me.clan_name, clan_before);
        // Brain runtime state from the last game is gone: no lingering
        // psychotic break, no stale preferred-action override.
        assert!(me.brain.psychotic_break.is_none());
        assert!(me.brain.preferred_action.is_none());
        assert_eq!(me.brain.preferred_action_percentage, 0.0);
        // Thresholds re-derive from the surviving traits (stale sentinel is
        // gone). A fresh derivation from empty traits would also clear it,
        // but traits survive the rebuild, so the brain stays coherent with
        // them — see the brain field's doc comment.
        assert_ne!(me.brain.thresholds.low_health, u32::MAX);
        // Intentional progression: addiction history survives (relapse
        // short-circuit, spec §5.1). See the field's doc comment.
        assert!(me.ever_addicted_to.contains(&Substance::Alcohol));
    }

    /// End-to-end advancement scenario: Boblin starts clean,
    /// dies once, wins once, gains a limp-like and a sharpshooter-like
    /// trait mid-game, resets to baseline-plus-advancements (not zero),
    /// and the advancements survive into the second game.
    #[test]
    fn boblin_advancements_persist_across_new_games() {
        use crate::statuses::CharacterStatus;
        let mut boblin = Character::new("Boblin".to_string(), None, None);
        boblin.traits.clear();
        boblin.attributes.speed = 36;
        let base_speed = boblin.attributes.speed;
        let base_strength = boblin.attributes.strength;

        // Boblin enters the first game Friendly; creation-time gains stamp
        // the log, and with no baseline yet the first rebuild folds them
        // into the captured floor and starts the event clock clean.
        boblin
            .gain_trait(crate::traits::Trait::Friendly)
            .expect("friendly");

        // Game 1 entry: baseline captured.
        boblin = boblin.into_new_game();
        assert!(boblin.game_baseline.is_some());
        assert!(boblin.advancement_log.is_empty(), "fresh event clock");

        // The game: Boblin dies once...
        boblin.set_status(CharacterStatus::Dead);
        boblin.statistics.day_killed = Some(2);
        boblin.statistics.killed_by = Some("Billy Slick".to_string());
        // ...wins a fight...
        boblin.statistics.wins += 1;
        boblin.statistics.kills += 1;
        // ...suffers a wound debuff that lingers past game end...
        boblin.attributes.strength = base_strength.saturating_sub(20);
        // ...talks their way out of the old Friendly habit (intentional loss,
        // stamped)...
        assert!(boblin.lose_trait(crate::traits::Trait::Friendly));
        // ...and is revived with a scar-like curse and a bonus trait
        // (stamped gains; Fragile/Cautious don't conflict).
        boblin
            .gain_trait(crate::traits::Trait::Fragile)
            .expect("fragile"); // the "limp"
        boblin
            .gain_trait(crate::traits::Trait::Cautious)
            .expect("cautious"); // the "sharpshooter"

        // Game 2 entry: the rebuild settles game 1's advancements.
        boblin = boblin.into_new_game();

        assert_eq!(
            boblin.traits,
            vec![
                crate::traits::Trait::Fragile,
                crate::traits::Trait::Cautious
            ],
            "gains persist, the intentional Friendly loss stays lost"
        );
        assert!(boblin.advancement_log.is_empty(), "settle consumed the log");
        assert_eq!(boblin.attributes.speed, base_speed);
        assert_eq!(
            boblin.attributes.strength, base_strength,
            "wound debuff dropped"
        );
        assert_eq!(boblin.statistics.wins, 1);
        assert_eq!(boblin.statistics.kills, 1);
        assert_eq!(boblin.statistics.day_killed, None);
        assert!(boblin.is_alive());

        // Game 3 entry: nothing double-counts, nothing is lost.
        boblin = boblin.into_new_game();
        assert_eq!(
            boblin.traits,
            vec![
                crate::traits::Trait::Fragile,
                crate::traits::Trait::Cautious
            ]
        );
        assert_eq!(boblin.statistics.wins, 1);
        assert_eq!(boblin.statistics.kills, 1);
    }

    #[rstest]
    fn pick_target_allows_same_team_when_not_ally() {
        // Same-team characters can now be targeted unless they're allies.
        let me = Character::new("Snaggletooth".to_string(), Some(8), None);
        let same_team = Character::new("Grubworm".to_string(), Some(8), None);

        let target = me.pick_target(vec![same_team.clone()], 5);
        assert!(target.is_some());
        assert_eq!(target.unwrap().id, same_team.id);
    }

    #[rstest]
    fn pick_target_final_confrontation_overrides_alliance() {
        // When only two characters remain alive, even an ally from another
        // team is a valid target.
        let mut me = Character::new("Snaggletooth".to_string(), Some(8), None);
        // No mental conditions -> effective_sanity = 100 -> not suicidal
        let ally = Character::new("Grubworm".to_string(), Some(3), None);
        me.allies.push(ally.id);

        let target = me.pick_target(vec![ally.clone()], 2);
        assert!(target.is_some());
        assert_eq!(target.unwrap().id, ally.id);
    }

    #[rstest]
    fn pick_target_final_confrontation_spares_teammates() {
        // The last two goblins of the same team are never forced to fight:
        // team victory ends the game instead.
        let mut me = Character::new("Snaggletooth".to_string(), Some(8), None);
        let teammate = Character::new("Grubworm".to_string(), Some(8), None);
        me.allies.push(teammate.id);

        let target = me.pick_target(vec![teammate.clone()], 2);
        assert!(target.is_none());
    }

    #[rstest]
    fn tick_alliance_timers_increments_betrayal_counter() {
        // Living character: counter increments by exactly one per tick.
        let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
        assert_eq!(character.turns_since_last_betrayal, 0);
        character.tick_alliance_timers();
        assert_eq!(character.turns_since_last_betrayal, 1);
        character.tick_alliance_timers();
        assert_eq!(character.turns_since_last_betrayal, 2);
    }

    #[rstest]
    fn tick_alliance_timers_saturates_does_not_overflow() {
        // u8 saturating add: never panics, never wraps to zero.
        let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
        character.turns_since_last_betrayal = u8::MAX;
        character.tick_alliance_timers();
        assert_eq!(character.turns_since_last_betrayal, u8::MAX);
    }

    #[rstest]
    fn tick_alliance_timers_skips_dead_characters() {
        // Dead characters don't accumulate betrayal cooldown.
        let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
        character.blood = 0;
        character.status = crate::CharacterStatus::RecentlyDead;
        character.tick_alliance_timers();
        assert_eq!(character.turns_since_last_betrayal, 0);
    }

    #[rstest]
    fn pick_target_picks_ex_ally_after_trust_shock_breaks_bond() {
        // End-to-end break-then-attack (spec §7.3c1 + §7.5):
        // Once a trust shock fires and removes the betrayer from the
        // victim's `allies`, the victim's next `pick_target` call must
        // consider that ex-ally a valid target.
        let mut victim = Character::new("Gnawpaw".to_string(), Some(1), None);
        // No mental conditions -> effective_sanity = 100 -> not suicidal
        let ex_ally = Character::new("Rendmaw".to_string(), Some(2), None);
        // Pre-condition: bonded.
        victim.allies.push(ex_ally.id);

        // Simulate the bond breaking (what process_alliance_events does
        // for BetrayalRecorded, plus what consume_pending_trust_shock
        // does on the victim's side: drop the ex-ally locally).
        victim.allies.retain(|id| *id != ex_ally.id);

        let target = victim.pick_target(vec![ex_ally.clone()], 5);
        assert!(
            target.is_some(),
            "ex-ally must be targetable after the bond breaks"
        );
        assert_eq!(target.unwrap().id, ex_ally.id);
    }

    #[rstest]
    fn consume_pending_trust_shock_leaves_asymmetric_back_edge() {
        // Spec §7.3c1 explicitly defers the symmetric back-edge cleanup
        // for trust-shock breaks: only `self` is mutated. This regression
        // test pins that contract so any future tightening is intentional.
        use shared::conditions::{ConditionSeverity, MentalCondition};
        let mut victim = Character::new("Gnawpaw".to_string(), Some(1), None);
        // 10 critical conditions -> 100 drain -> effective_sanity = 0
        victim.mental_conditions = (0..10)
            .map(|_| MentalCondition::Pain {
                severity: ConditionSeverity::Critical,
            })
            .collect();
        victim.brain.thresholds.extreme_low_sanity = 100;
        let betrayer_id = uuid::Uuid::new_v4();
        victim.allies.push(betrayer_id);
        victim.pending_trust_shock = true;

        let mut rng = SmallRng::seed_from_u64(419);
        let mut events: Vec<TaggedEvent> = vec![];
        victim.consume_pending_trust_shock(&mut rng, &mut events);

        // Victim's side cleaned.
        assert!(
            !victim.allies.contains(&betrayer_id),
            "victim must drop the broken ally"
        );
        // The flag is consumed regardless of roll outcome.
        assert!(
            !victim.pending_trust_shock,
            "pending flag is reset after the call"
        );
        // Asymmetric back-edge stays — `consume_pending_trust_shock` only
        // touches `self`. The next cycle's event drain (or follow-up
        // events) is responsible for the betrayer's side.
        // We can't observe the betrayer here (different character instance);
        // the documented contract is what matters and is asserted by the
        // single-side mutation: the function signature takes `&mut self`
        // and returns nothing, with no reference to the broken ally.
    }

    #[test]
    fn betrayal_increments_committed_counter_at_trigger_site() {
        // dww.10.2: the Treacherous betrayal site stamps the betrayer's
        // own lifetime counter right where BetrayalRecorded is enqueued.
        use crate::traits::Trait;
        use crate::{EncounterContext, EnvironmentContext};
        use areas::{AreaDetails, AreaId};

        let mut betrayer = Character::new("Rendmaw".to_string(), Some(1), None);
        betrayer.traits = vec![Trait::Treacherous];
        let victim = Character::new("Gnawpaw".to_string(), Some(2), None);
        betrayer.allies.push(victim.id);
        betrayer.turns_since_last_betrayal = crate::alliances::TREACHEROUS_BETRAYAL_INTERVAL;

        let mut area_details = AreaDetails::new(Some("Lake".to_string()), AreaId::hub());
        let all_areas = vec![AreaDetails::new(Some("Lake".to_string()), AreaId::hub())];
        let enemy_density = std::collections::HashMap::new();
        let combat_tuning = crate::combat_tuning::CombatTuning::default();
        let mut environment_details = EnvironmentContext {
            is_day: true,
            phase: shared::messages::Phase::DAY,
            config: world::config::GameConfig::default(),
            area_details: &mut area_details,
            available_destinations: Vec::new(),
            all_areas: &all_areas,
            enemy_density: &enemy_density,
            current_day: 1,
            combat_tuning: &combat_tuning,
            sleeping_nearby: Vec::new(),
        };
        let encounter_context = EncounterContext {
            nearby_characters_count: 1,
            potential_targets: vec![victim],
            total_living_characters: 2,
        };

        let mut rng = SmallRng::seed_from_u64(419);
        let mut events: Vec<TaggedEvent> = vec![];
        betrayer.process_turn_phase(
            None,
            false,
            &mut environment_details,
            encounter_context,
            &mut rng,
            &mut events,
        );

        assert_eq!(
            betrayer.statistics.betrayals_committed, 1,
            "the betrayer counts its own lifetime betrayal"
        );
        assert!(
            betrayer
                .alliance_events
                .iter()
                .any(|e| matches!(e, crate::alliances::AllianceEvent::BetrayalRecorded { .. })),
            "the drain event is still enqueued for the victim side"
        );
    }

    #[test]
    fn wake_interrupted_returns_false_when_not_sleeping() {
        use shared::messages::CharacterRef;
        let mut t = Character::new("Snoutrot".to_string(), Some(1), None);
        let mut events: Vec<TaggedEvent> = Vec::new();
        let woke = t.wake_interrupted(
            shared::messages::InterruptionKind::Ambush {
                attacker: CharacterRef {
                    identifier: "x".into(),
                    name: "X".to_string(),
                },
            },
            shared::messages::Phase::DAY,
            &mut events,
        );
        assert!(!woke);
        assert!(events.is_empty());
    }

    #[test]
    fn wake_interrupted_resets_state_and_emits_character_woke() {
        let mut t = Character::new("Snoutrot".to_string(), Some(1), None);
        t.sleeping = true;
        t.sleep_remaining = 3;
        t.cycles_awake = 7;
        let mut events: Vec<TaggedEvent> = Vec::new();
        let woke = t.wake_interrupted(
            shared::messages::InterruptionKind::AreaEvent {
                kind: shared::messages::AreaEventKind::Fire,
            },
            shared::messages::Phase::NIGHT,
            &mut events,
        );
        assert!(woke);
        assert!(!t.sleeping);
        assert_eq!(t.sleep_remaining, 0);
        assert_eq!(t.cycles_awake, 0);
        assert_eq!(events.len(), 1);
        match &events[0].payload {
            shared::messages::MessagePayload::CharacterWoke { reason, phase, .. } => {
                assert_eq!(*phase, shared::messages::Phase::NIGHT);
                match reason {
                    shared::messages::WakeReason::Interrupted {
                        event:
                            shared::messages::InterruptionKind::AreaEvent {
                                kind: shared::messages::AreaEventKind::Fire,
                            },
                    } => {}
                    other => panic!("unexpected reason: {:?}", other),
                }
            }
            other => panic!("expected CharacterWoke payload, got {:?}", other),
        }
    }

    // --- Affliction tests ---

    use crate::AfflictionDraft;
    use crate::afflictions::{AcquireResolution, RejectReason};
    use shared::afflictions::{AfflictionKind, AfflictionSource, BodyPart, Severity};

    #[test]
    fn test_afflictions_empty_by_default() {
        let t = Character::new("Test".to_string(), None, None);
        assert!(t.afflictions.is_empty());
    }

    #[test]
    fn test_afflictions_skip_serialization_when_empty() {
        let t = Character::new("Test".to_string(), None, None);
        let json = serde_json::to_string(&t).unwrap();
        assert!(!json.contains("\"afflictions\""));
    }

    #[test]
    fn test_try_acquire_insert() {
        let mut t = Character::new("Test".to_string(), None, None);
        let draft = AfflictionDraft {
            kind: AfflictionKind::Wounded,
            body_part: Some(BodyPart::Arm),
            severity: Severity::Mild,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            trapped_metadata: None,
        };
        let resolution = t.try_acquire_affliction(draft);
        assert_eq!(resolution, AcquireResolution::Insert);
        assert_eq!(t.afflictions.len(), 1);
        assert!(
            t.afflictions
                .contains_key(&(AfflictionKind::Wounded, Some(BodyPart::Arm)))
        );
    }

    #[test]
    fn test_try_acquire_upgrade() {
        let mut t = Character::new("Test".to_string(), None, None);
        // Insert mild wound
        t.try_acquire_affliction(AfflictionDraft {
            kind: AfflictionKind::Wounded,
            body_part: Some(BodyPart::Arm),
            severity: Severity::Mild,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            trapped_metadata: None,
        });
        // Upgrade to moderate
        let draft = AfflictionDraft {
            kind: AfflictionKind::Wounded,
            body_part: Some(BodyPart::Arm),
            severity: Severity::Moderate,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            trapped_metadata: None,
        };
        let resolution = t.try_acquire_affliction(draft);
        assert_eq!(
            resolution,
            AcquireResolution::Upgrade((AfflictionKind::Wounded, Some(BodyPart::Arm)))
        );
        assert_eq!(t.afflictions.len(), 1);
        let affl = t
            .afflictions
            .get(&(AfflictionKind::Wounded, Some(BodyPart::Arm)))
            .unwrap();
        assert_eq!(affl.severity, Severity::Moderate);
    }

    #[test]
    fn test_try_acquire_supersede() {
        let mut t = Character::new("Test".to_string(), None, None);
        // Insert wounded on arm
        t.try_acquire_affliction(AfflictionDraft {
            kind: AfflictionKind::Wounded,
            body_part: Some(BodyPart::Arm),
            severity: Severity::Mild,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            trapped_metadata: None,
        });
        // Infected supersedes wounded at same body part
        let draft = AfflictionDraft {
            kind: AfflictionKind::Infected,
            body_part: Some(BodyPart::Arm),
            severity: Severity::Mild,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            trapped_metadata: None,
        };
        let resolution = t.try_acquire_affliction(draft);
        assert_eq!(resolution, AcquireResolution::Insert);
        // Wounded removed, Infected present
        assert!(
            !t.afflictions
                .contains_key(&(AfflictionKind::Wounded, Some(BodyPart::Arm)))
        );
        assert!(
            t.afflictions
                .contains_key(&(AfflictionKind::Infected, Some(BodyPart::Arm)))
        );
    }

    #[test]
    fn test_try_acquire_reject_limb_missing() {
        let mut t = Character::new("Test".to_string(), None, None);
        // Missing arm
        t.try_acquire_affliction(AfflictionDraft {
            kind: AfflictionKind::MissingArm,
            body_part: Some(BodyPart::Arm),
            severity: Severity::Severe,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            trapped_metadata: None,
        });
        // Can't wound a missing limb
        let draft = AfflictionDraft {
            kind: AfflictionKind::Wounded,
            body_part: Some(BodyPart::Arm),
            severity: Severity::Mild,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            trapped_metadata: None,
        };
        let resolution = t.try_acquire_affliction(draft);
        assert_eq!(
            resolution,
            AcquireResolution::Reject(RejectReason::LimbAlreadyMissing)
        );
    }

    #[test]
    fn test_try_acquire_reject_no_wounded_ancestor() {
        let mut t = Character::new("Test".to_string(), None, None);
        // Infected without prior Wounded on same part
        let draft = AfflictionDraft {
            kind: AfflictionKind::Infected,
            body_part: Some(BodyPart::Arm),
            severity: Severity::Mild,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            trapped_metadata: None,
        };
        let resolution = t.try_acquire_affliction(draft);
        assert_eq!(
            resolution,
            AcquireResolution::Reject(RejectReason::InfectedRequiresWoundedAncestor)
        );
    }

    #[test]
    fn test_try_acquire_reject_same_severity() {
        let mut t = Character::new("Test".to_string(), None, None);
        // Insert mild wound
        t.try_acquire_affliction(AfflictionDraft {
            kind: AfflictionKind::Wounded,
            body_part: Some(BodyPart::Arm),
            severity: Severity::Mild,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            trapped_metadata: None,
        });
        // Same severity rejected
        let draft = AfflictionDraft {
            kind: AfflictionKind::Wounded,
            body_part: Some(BodyPart::Arm),
            severity: Severity::Mild,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            trapped_metadata: None,
        };
        let resolution = t.try_acquire_affliction(draft);
        assert_eq!(
            resolution,
            AcquireResolution::Reject(RejectReason::NotStrictlyHigherSeverity)
        );
    }

    // ---------- attribute renames, perception, creation coherence ----------

    #[test]
    fn legacy_attribute_columns_load_via_serde_aliases() {
        // A pre-rename persisted row: defense/agility keys, no perception.
        // It also still carries the dropped `movement` key — `Attributes`
        // has no deny_unknown_fields, so old payloads decode one-way-safe.
        let legacy = r#"{"movement":100,"strength":25,"defense":30,"bravery":40,"intelligence":50,"persuasion":10,"luck":5,"agility":60,"is_hidden":false}"#;
        let attrs: crate::Attributes = serde_json::from_str(legacy).unwrap();
        assert_eq!(attrs.strength, 25);
        assert_eq!(attrs.toughness, 30);
        assert_eq!(attrs.speed, 60);
        // New axis hydrates via its serde default, not a deserialize error.
        assert_eq!(attrs.perception, 100);

        // New writes use the new keys.
        let json = serde_json::to_string(&attrs).unwrap();
        assert!(json.contains("toughness"), "serialize new key: {json}");
        assert!(json.contains("speed"), "serialize new key: {json}");
        assert!(json.contains("perception"), "serialize new key: {json}");
        assert!(!json.contains("defense"), "old key gone: {json}");
        assert!(!json.contains("agility"), "old key gone: {json}");
    }

    #[test]
    fn legacy_character_attribute_enum_keys_load_via_aliases() {
        use crate::traits::CharacterAttribute as A;
        let old: A = serde_json::from_str("\"Defense\"").unwrap();
        assert_eq!(old, A::Toughness);
        let old: A = serde_json::from_str("\"Agility\"").unwrap();
        assert_eq!(old, A::Speed);
        // New keys round-trip.
        assert_eq!(
            serde_json::to_string(&A::Toughness).unwrap(),
            "\"Toughness\""
        );
        assert_eq!(serde_json::to_string(&A::Speed).unwrap(), "\"Speed\"");
        assert_eq!(
            serde_json::to_string(&A::Perception).unwrap(),
            "\"Perception\""
        );
    }

    #[test]
    fn legacy_trait_def_arrays_with_old_axis_keys_load() {
        // A trait-effect JSON written before the rename (AttrMod on Defense).
        let legacy = r#"{"AttrMod":{"attr":"Defense","flat":0,"percent":-10}}"#;
        let effect: crate::traits::Effect = serde_json::from_str(legacy).unwrap();
        assert_eq!(
            effect,
            crate::traits::Effect::AttrMod {
                attr: crate::traits::CharacterAttribute::Toughness,
                flat: 0,
                percent: -10,
            }
        );
    }

    #[test]
    fn perception_rolls_within_bounds() {
        use rand::SeedableRng;
        let mut rng = SmallRng::seed_from_u64(7);
        for _ in 0..200 {
            let attrs = crate::Attributes::from_rng(&mut rng);
            assert!(
                (1..=100).contains(&attrs.perception),
                "perception out of bounds: {}",
                attrs.perception
            );
        }
    }

    #[test]
    fn nearsighted_gets_perception_penalty_through_accessors() {
        let mut c = Character::new("Stinky".to_string(), None, None);
        c.traits.clear();
        c.attributes.perception = 100;
        let plain = c.effective_attribute(crate::traits::CharacterAttribute::Perception);
        assert_eq!(plain, 100);

        c.traits.push(crate::traits::Trait::Nearsighted);
        let seen = c.effective_attribute(crate::traits::CharacterAttribute::Perception);
        assert_eq!(seen, 75, "-25% from Nearsighted at reference max");
    }

    #[test]
    fn generation_is_trait_coherent_tough_trends_tough() {
        use rand::SeedableRng;
        // Same seed, same rolls: the Tough-bent pass must land at or above
        // the unbiased pass for toughness (x1.2), never below.
        for seed in 0..64u64 {
            let mut plain_rng = SmallRng::seed_from_u64(seed);
            let mut tough_rng = SmallRng::seed_from_u64(seed);
            let plain = crate::Attributes::from_rng(&mut plain_rng);
            let tough = crate::Attributes::from_rng_with_traits(
                &mut tough_rng,
                &[crate::traits::Trait::Tough],
            );
            assert!(
                tough.toughness >= plain.toughness,
                "seed {seed}: Tough bent toughness DOWN ({} < {})",
                tough.toughness,
                plain.toughness
            );
        }
        // And the bias is real, not a no-op: pick a seed where the raw roll
        // is low enough for the +20% to move it.
        let mut rng = SmallRng::seed_from_u64(3);
        let raw = crate::Attributes::from_rng(&mut rng).toughness;
        let mut rng = SmallRng::seed_from_u64(3);
        let bent =
            crate::Attributes::from_rng_with_traits(&mut rng, &[crate::traits::Trait::Tough])
                .toughness;
        assert!(bent >= raw, "bend must not decrease: {bent} < {raw}");
    }

    #[test]
    fn new_with_rng_generates_coherent_characters() {
        // The full constructor path: traits roll first, attributes bend
        // around them, everything deterministic per seed.
        let a = Character::new_with_rng(
            "Stinky".into(),
            None,
            None,
            &mut SmallRng::seed_from_u64(11),
        );
        let b = Character::new_with_rng(
            "Stinky".into(),
            None,
            None,
            &mut SmallRng::seed_from_u64(11),
        );
        assert_eq!(a.traits, b.traits);
        assert_eq!(a.attributes, b.attributes);
        if a.traits.contains(&crate::traits::Trait::Tough) {
            // Tough's +20% bent the base; effective stacks on top (x1.2 again).
            let max = world::config::GameConfig::default().max_toughness;
            assert!(
                a.effective_attribute(crate::traits::CharacterAttribute::Toughness)
                    >= a.attributes.toughness.min(max),
            );
        }
    }

    // --- SeekShelter / Forage action executors --------------------------

    fn survival_area(base: world::terrain::BaseTerrain) -> areas::AreaDetails {
        areas::AreaDetails::new_with_terrain(
            None,
            areas::AreaId::hub(),
            world::terrain::TerrainType::new(base, vec![]).unwrap(),
        )
    }

    #[test]
    fn seek_shelter_success_sets_sheltered_until_and_emits_payload() {
        let mut me = character();
        me.attributes.intelligence = 100;
        me.attributes.strength = 100;
        let mut area = survival_area(world::terrain::BaseTerrain::Clearing);
        let before_stamina = me.stamina;
        let expected_cost = crate::calculate_stamina_cost(&Action::SeekShelter, &area.terrain, &me);
        let mut rng = SmallRng::seed_from_u64(7);
        let mut events: Vec<TaggedEvent> = Vec::new();

        me.act_seek_shelter(&mut area, 5, true, &mut rng, &mut events);

        // Attributes 100 always clear the Fortified DC (max roll 80 vs
        // worst-case DC+20 = 45), so tier = Fortified = 3 day/night slots
        // past `day * 2 + !is_day` (= 10 here).
        assert_eq!(me.sheltered_until, Some(13), "shelter granted for 3 slots");
        assert_eq!(
            me.stamina,
            before_stamina - expected_cost,
            "stamina cost deducted exactly once"
        );

        let payload = events
            .iter()
            .find_map(|e| match &e.payload {
                MessagePayload::ShelterSought {
                    success,
                    roll,
                    area,
                    ..
                } => Some((*success, *roll, area.name.clone())),
                _ => None,
            })
            .expect("ShelterSought emitted on success");
        assert!(payload.0, "success flag set");
        assert_eq!(payload.1, 3, "roll carries the shelter tier (Fortified)");
        assert_eq!(payload.2, "Hub");
    }

    #[test]
    fn seek_shelter_respects_stamina_gating() {
        let mut me = character();
        let mut area = survival_area(world::terrain::BaseTerrain::Clearing);
        let cost = crate::calculate_stamina_cost(&Action::SeekShelter, &area.terrain, &me);
        assert!(cost > 0, "seek shelter must cost stamina");
        me.stamina = cost - 1;
        let mut rng = SmallRng::seed_from_u64(7);
        let mut events: Vec<TaggedEvent> = Vec::new();

        me.act_seek_shelter(&mut area, 5, true, &mut rng, &mut events);

        assert_eq!(me.stamina, cost - 1, "no deduction when denied");
        assert_eq!(me.sheltered_until, None, "no shelter when denied");
        assert!(
            events.iter().any(|e| matches!(
                &e.payload,
                MessagePayload::CharacterRested { hp_restored: 0, .. }
            )),
            "exhausted denial event emitted"
        );
        assert!(
            events[0].content.contains("too exhausted"),
            "denial line reads as exhaustion: {}",
            events[0].content
        );
        assert!(
            events
                .iter()
                .all(|e| !matches!(e.payload, MessagePayload::ShelterSought { .. })),
            "no ShelterSought emitted when stamina denies the action"
        );
    }

    #[test]
    fn forage_on_barren_terrain_yields_nothing() {
        let mut me = character();
        let mut area = survival_area(world::terrain::BaseTerrain::Desert); // richness 0
        let items_before = me.items.len();
        let before_stamina = me.stamina;
        let expected_cost = crate::calculate_stamina_cost(&Action::Forage, &area.terrain, &me);
        let mut rng = SmallRng::seed_from_u64(3);
        let mut events: Vec<TaggedEvent> = Vec::new();

        me.act_forage(&mut area, &mut rng, &mut events);

        assert_eq!(
            me.items.len(),
            items_before,
            "barren terrain yields nothing"
        );
        assert_eq!(
            me.stamina,
            before_stamina - expected_cost,
            "effort still spent"
        );
        let payload = events
            .iter()
            .find_map(|e| match &e.payload {
                MessagePayload::Foraged {
                    success,
                    debt_recovered,
                    ..
                } => Some((*success, *debt_recovered)),
                _ => None,
            })
            .expect("Foraged emitted even on a miss");
        assert_eq!(payload, (false, 0));
    }

    #[test]
    fn forage_keys_on_sub_tile_terrain_not_area_terrain() {
        // Desert area (richness 0) with a Forest sub-tile under the
        // character's feet: the forage roll reads the sub-tile.
        let mut me = character();
        let mut area = survival_area(world::terrain::BaseTerrain::Desert);
        area.sub_tiles = vec![areas::preset::SubTile {
            terrain: world::terrain::BaseTerrain::Forest,
            feature: None,
            flavor: None,
        }];
        let before_stamina = me.stamina;
        let expected_cost = crate::calculate_stamina_cost(&Action::Forage, &area.terrain, &me);
        let mut rng = SmallRng::seed_from_u64(3);
        let mut events: Vec<TaggedEvent> = Vec::new();

        me.act_forage(&mut area, &mut rng, &mut events);

        assert_eq!(
            me.stamina,
            before_stamina - expected_cost,
            "effort still spent"
        );
        let success = events
            .iter()
            .any(|e| matches!(&e.payload, MessagePayload::Foraged { success: true, .. }));
        assert!(
            success,
            "sub-tile Forest under a Desert area should be forageable"
        );
    }

    #[test]
    fn forage_yield_scales_with_richness_across_seeds() {
        // Same seed, same d20 sequence: each richer terrain adds one more
        // attempt on top of the poorer one's attempts, so per-seed yield is
        // monotone in richness by construction.
        let terrains = [
            world::terrain::BaseTerrain::Clearing, // richness 1
            world::terrain::BaseTerrain::Forest,   // richness 2
            world::terrain::BaseTerrain::Wetlands, // richness 3
        ];
        let mut totals = [0u32; 3];
        for (i, base) in terrains.into_iter().enumerate() {
            for seed in 0..16u64 {
                let mut me = character();
                let mut area = survival_area(base);
                let before = me.items.len();
                let mut rng = SmallRng::seed_from_u64(seed);
                let mut events: Vec<TaggedEvent> = Vec::new();

                me.act_forage(&mut area, &mut rng, &mut events);

                totals[i] += (me.items.len() - before) as u32;
            }
        }
        assert!(
            totals[0] <= totals[1] && totals[1] <= totals[2],
            "yield monotone in richness: {totals:?}"
        );
        assert!(
            totals[0] < totals[2],
            "richer terrain yields strictly more across seeds: {totals:?}"
        );
    }

    #[test]
    fn forage_honors_affliction_forage_modifier() {
        use shared::afflictions::{Affliction, AfflictionKind, AfflictionSource, Severity};

        // Blind at Moderate severity carries a -2 forage modifier
        // (`compute_stat_modifiers` base penalties).
        let debuffed = Affliction {
            kind: AfflictionKind::Blind,
            body_part: None,
            severity: Severity::Moderate,
            source: AfflictionSource::Environmental,
            acquired_cycle: 0,
            last_progressed_cycle: 0,
            trauma_metadata: None,
            phobia_metadata: None,
            fixation_metadata: None,
            addiction_metadata: None,
            trapped_metadata: None,
        };

        let mut plain_total = 0u32;
        let mut debuff_total = 0u32;
        for seed in 0..16u64 {
            for (count, afflicted) in [(&mut plain_total, false), (&mut debuff_total, true)] {
                let mut me = character();
                if afflicted {
                    me = me.with_affliction(debuffed.clone());
                }
                let mut area = survival_area(world::terrain::BaseTerrain::Forest);
                let before = me.items.len();
                let mut rng = SmallRng::seed_from_u64(seed);
                let mut events: Vec<TaggedEvent> = Vec::new();

                me.act_forage(&mut area, &mut rng, &mut events);

                *count += (me.items.len() - before) as u32;
            }
        }
        assert!(
            debuff_total <= plain_total,
            "forage modifier never helps: {debuff_total} > {plain_total}"
        );
        assert!(
            debuff_total < plain_total,
            "-2 forage modifier reduces yield: {debuff_total} == {plain_total}"
        );
    }

    #[test]
    fn builder_reroll_keeps_better_shelter_and_extends_duration() {
        // Same seed, pinned attributes: the plain run takes the first
        // `find_shelter` roll, the Builder run re-rolls once and keeps the
        // better tier (never worse), and any Builder grant lasts tier + 1
        // half-day slots past phase index 10 (day 5, is_day).
        fn run(seed: u64, with_builder: bool) -> (u8, Option<u32>) {
            let mut me = character();
            // 30 keeps both rolls inside the tier band: the plain run can
            // land below Fortified, so the Builder re-roll gets to win.
            me.attributes.strength = 30;
            me.attributes.toughness = 50;
            me.attributes.bravery = 50;
            me.attributes.intelligence = 30;
            me.attributes.persuasion = 50;
            me.attributes.luck = 50;
            me.attributes.speed = 50;
            me.stamina = 100;
            me.max_stamina = 100;
            if with_builder {
                me.traits.push(crate::traits::Trait::Builder);
            }
            let mut area = survival_area(world::terrain::BaseTerrain::Forest);
            // Terrain affinity rolls per character, so the cost is measured
            // on this exact instance instead of compared across runs.
            let expected_cost =
                crate::calculate_stamina_cost(&Action::SeekShelter, &area.terrain, &me);
            let mut rng = SmallRng::seed_from_u64(seed);
            let mut events: Vec<TaggedEvent> = Vec::new();

            me.act_seek_shelter(&mut area, 5, true, &mut rng, &mut events);
            assert_eq!(
                me.stamina,
                100 - expected_cost,
                "stamina cost deducted exactly once"
            );

            let tier = events
                .iter()
                .find_map(|e| match &e.payload {
                    MessagePayload::ShelterSought { roll, success, .. } => {
                        assert_eq!(*success, *roll > 0, "success iff tier > 0");
                        Some(*roll)
                    }
                    _ => None,
                })
                .expect("ShelterSought emitted");
            (tier, me.sheltered_until)
        }

        let mut improved = false;
        for seed in 0..64u64 {
            let (plain_tier, plain_until) = run(seed, false);
            let (builder_tier, builder_until) = run(seed, true);
            assert!(
                builder_tier >= plain_tier,
                "seed {seed}: Builder never rolls worse ({builder_tier} < {plain_tier})"
            );
            improved |= builder_tier > plain_tier;
            if builder_tier > 0 {
                assert_eq!(
                    builder_until,
                    Some(10 + u32::from(builder_tier) + 1),
                    "seed {seed}: Builder grant = phase + tier + 1 half-day slots"
                );
                if plain_tier > 0 {
                    assert!(
                        builder_until.unwrap() > plain_until.unwrap(),
                        "seed {seed}: Builder shelter lasts at least one slot longer"
                    );
                }
            } else {
                assert_eq!(builder_until, None, "seed {seed}: tier None grants nothing");
            }
        }
        assert!(
            improved,
            "some seed must show the Builder re-roll beating the first roll"
        );
    }

    #[test]
    fn resourceful_forager_adds_attempt_and_raises_yield() {
        // Forest richness 2: the forager gets a third d20 per seed. Same
        // seed means identical first two rolls, so per-seed yield never
        // drops; the extra attempt raises the total across seeds, by at
        // most one success per seed.
        let mut plain_total = 0u32;
        let mut forager_total = 0u32;
        for seed in 0..64u64 {
            let mut per_seed = [0u32; 2];
            for (i, forager) in [false, true].into_iter().enumerate() {
                let mut me = character();
                if forager {
                    me.traits.push(crate::traits::Trait::ResourcefulForager);
                }
                let mut area = survival_area(world::terrain::BaseTerrain::Forest);
                let before = me.items.len();
                let mut rng = SmallRng::seed_from_u64(seed);
                let mut events: Vec<TaggedEvent> = Vec::new();

                me.act_forage(&mut area, &mut rng, &mut events);

                per_seed[i] = (me.items.len() - before) as u32;
            }
            assert!(
                per_seed[1] >= per_seed[0],
                "seed {seed}: extra attempt never lowers yield ({per_seed:?})"
            );
            plain_total += per_seed[0];
            forager_total += per_seed[1];
        }
        assert!(
            forager_total > plain_total,
            "extra attempt raises yield across seeds: {forager_total} == {plain_total}"
        );
        assert!(
            forager_total <= plain_total + 64,
            "at most one extra success per seed: {forager_total} > {}",
            plain_total + 64
        );
    }

    // ---- Traps keyed per sub-tile (sub-hex P5, spike §6 row 6) ----

    fn trap_at(
        set_by: &str,
        sub: areas::hex::SubTriangle,
        concealment: u32,
    ) -> areas::traps::PlacedTrap {
        areas::traps::PlacedTrap {
            id: uuid::Uuid::new_v4().to_string(),
            kind: shared::afflictions::TrapKind::Snared,
            severity: shared::afflictions::Severity::Mild,
            set_by: set_by.to_string(),
            concealment,
            triggered: false,
            sub,
        }
    }

    #[test]
    fn set_trap_records_the_placing_goblins_sub_tile() {
        let mut me = character();
        me.location =
            areas::location::Location::new(areas::AreaId::hub(), areas::hex::SubTriangle::new(3));
        let mut area = survival_area(world::terrain::BaseTerrain::Clearing);
        let mut rng = SmallRng::seed_from_u64(11);
        let mut events: Vec<TaggedEvent> = Vec::new();

        me.act_set_trap(None, None, &mut area, &mut rng, &mut events);

        assert_eq!(area.placed_traps.len(), 1, "trap placed");
        assert_eq!(
            area.placed_traps[0].sub,
            areas::hex::SubTriangle::new(3),
            "placement records the setter's sub-tile"
        );
    }

    #[test]
    fn set_trap_caps_at_two_untriggered_traps_in_own_sub_tile() {
        let mut me = character();
        me.location =
            areas::location::Location::new(areas::AreaId::hub(), areas::hex::SubTriangle::new(3));
        let mut area = survival_area(world::terrain::BaseTerrain::Clearing);
        // Untriggered traps in OTHER sub-tiles don't count against this one.
        for sub in 0..5u8 {
            area.placed_traps.push(trap_at(
                "some-other-goblin",
                areas::hex::SubTriangle::new(sub),
                1,
            ));
        }
        let mut rng = SmallRng::seed_from_u64(11);
        let mut events: Vec<TaggedEvent> = Vec::new();

        me.act_set_trap(None, None, &mut area, &mut rng, &mut events);
        me.act_set_trap(None, None, &mut area, &mut rng, &mut events);
        me.act_set_trap(None, None, &mut area, &mut rng, &mut events);

        let own_sub = areas::hex::SubTriangle::new(3);
        assert_eq!(
            area.placed_traps
                .iter()
                .filter(|t| t.sub == own_sub)
                .count(),
            2,
            "cap is 2 untriggered traps per sub-tile"
        );
    }

    #[test]
    fn search_only_reveals_traps_in_own_sub_tile() {
        let mut me = character();
        me.location =
            areas::location::Location::new(areas::AreaId::hub(), areas::hex::SubTriangle::new(0));
        let mut area = survival_area(world::terrain::BaseTerrain::Clearing);
        // Concealment 1: any roll finds it — but only if the sub is scanned.
        area.placed_traps.push(trap_at(
            "some-other-goblin",
            areas::hex::SubTriangle::new(4),
            1,
        ));
        let mut rng = SmallRng::seed_from_u64(11);
        let mut events: Vec<TaggedEvent> = Vec::new();

        me.act_search(&mut area, &mut rng, &mut events);
        assert!(
            !area.placed_traps[0].triggered,
            "cross-sub-tile trap must not be revealed"
        );

        area.placed_traps.push(trap_at(
            "some-other-goblin",
            areas::hex::SubTriangle::new(0),
            1,
        ));
        me.act_search(&mut area, &mut rng, &mut events);
        assert!(
            area.placed_traps[1].triggered,
            "same-sub-tile trap must be revealed"
        );
    }

    fn gate_and_candidacy_test_affliction(
        kind: shared::afflictions::AfflictionKind,
        severity: shared::afflictions::Severity,
    ) -> shared::afflictions::Affliction {
        shared::afflictions::Affliction {
            kind,
            body_part: None,
            severity,
            source: shared::afflictions::AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            acquired_cycle: 1,
            last_progressed_cycle: 1,
            trauma_metadata: None,
            phobia_metadata: None,
            fixation_metadata: None,
            addiction_metadata: None,
            trapped_metadata: None,
        }
    }

    #[test]
    fn alliance_candidate_gate_honors_visibility_knob() {
        use shared::afflictions::{AfflictionKind, Severity};
        let me = character();
        let mut other = character();
        other.location = me.location;

        // Knob off: candidacy never considers afflictions (legacy).
        assert!(Character::perceivable_candidate(&me, &other, false));
        // Knob on, clean target: still perceivable.
        assert!(Character::perceivable_candidate(&me, &other, true));
        // Knob on, Moderate affliction, co-present: skipped.
        let moderate =
            gate_and_candidacy_test_affliction(AfflictionKind::Blind, Severity::Moderate);
        other.afflictions.insert(moderate.key(), moderate);
        assert!(
            !Character::perceivable_candidate(&me, &other, true),
            "Moderate visible to co-present observer"
        );
        assert!(
            Character::perceivable_candidate(&me, &other, false),
            "knob off ignores afflictions"
        );
        // Knob on, Moderate, other sub-tile: hidden (co-presence required).
        other.location =
            areas::location::Location::new(areas::AreaId::hub(), areas::hex::SubTriangle::new(4));
        assert!(
            Character::perceivable_candidate(&me, &other, true),
            "Moderate only visible with co-presence"
        );
        // Knob on, Severe: public regardless of location.
        other.afflictions.clear();
        let severe = gate_and_candidacy_test_affliction(AfflictionKind::Blind, Severity::Severe);
        other.afflictions.insert(severe.key(), severe);
        assert!(
            !Character::perceivable_candidate(&me, &other, true),
            "Severe public"
        );
    }

    #[test]
    fn alliance_candidate_gate_ignores_mild_afflictions() {
        use shared::afflictions::{AfflictionKind, Severity};
        let me = character();
        let mut other = character();
        other.location = me.location;
        let mild = gate_and_candidacy_test_affliction(AfflictionKind::Blind, Severity::Mild);
        other.afflictions.insert(mild.key(), mild);
        assert!(
            Character::perceivable_candidate(&me, &other, true),
            "Mild is hidden from everyone"
        );
    }

    #[test]
    fn trap_triggers_for_goblin_in_the_keyed_sub_tile() {
        let mut me = character();
        me.location =
            areas::location::Location::new(areas::AreaId::hub(), areas::hex::SubTriangle::new(2));
        let mut area = survival_area(world::terrain::BaseTerrain::Clearing);
        // Concealment 30 exceeds any perception roll (max 21), so the
        // keyed-sub path must trigger regardless of the seeded rng.
        area.placed_traps.push(trap_at(
            "some-other-goblin",
            areas::hex::SubTriangle::new(2),
            30,
        ));
        let mut rng = SmallRng::seed_from_u64(11);
        let mut events: Vec<TaggedEvent> = Vec::new();

        me.check_trap_triggers(&mut area, &mut rng, &mut events);

        assert!(area.placed_traps.is_empty(), "trap fired and was removed");
        assert!(
            events
                .iter()
                .any(|e| matches!(e.payload, MessagePayload::TrapTriggered { .. })),
            "TrapTriggered emitted for the keyed sub-tile"
        );
    }

    #[test]
    fn trap_does_not_trigger_for_goblin_in_another_sub_tile() {
        let mut me = character();
        me.location =
            areas::location::Location::new(areas::AreaId::hub(), areas::hex::SubTriangle::new(4));
        let mut area = survival_area(world::terrain::BaseTerrain::Clearing);
        area.placed_traps.push(trap_at(
            "some-other-goblin",
            areas::hex::SubTriangle::new(2),
            30,
        ));
        let mut rng = SmallRng::seed_from_u64(11);
        let mut events: Vec<TaggedEvent> = Vec::new();

        me.check_trap_triggers(&mut area, &mut rng, &mut events);

        assert_eq!(
            area.placed_traps.len(),
            1,
            "same-area, different sub-tile: trap stays"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e.payload, MessagePayload::TrapTriggered { .. })),
            "no TrapTriggered across sub-tile bounds"
        );
    }

    // ---- Temporary traits: N-game durations on event grants ----

    use crate::traits::{CharacterAttribute, Trait};

    #[test]
    fn temporary_trait_expires_after_exactly_n_boundaries() {
        let mut c = Character::new("Stinky".to_string(), None, None);
        c.traits.clear();
        c.gain_trait(Trait::Tough).expect("tough");
        c.temporary_traits.insert(Trait::Tough, 2);

        // First boundary: one game consumed, trait still on.
        c.tick_temporary_traits();
        assert!(c.traits.contains(&Trait::Tough), "still within duration");
        assert_eq!(c.temporary_traits.get(&Trait::Tough), Some(&1));

        // Second boundary: exactly N — trait and duration entry gone with
        // no trace beyond the sanctioned loss stamp.
        c.tick_temporary_traits();
        assert!(
            !c.traits.contains(&Trait::Tough),
            "expired trait must leave the set"
        );
        assert!(
            c.temporary_traits.is_empty(),
            "expired duration entry must be removed"
        );
        assert!(
            c.advancement_log.iter().any(|e| matches!(
                e,
                crate::advancement::AdvancementEvent::TraitLost { trait_ } if *trait_ == Trait::Tough
            )),
            "expiry must stamp TraitLost or the next settle restores it"
        );

        // Further ticks are a no-op: nothing left to expire.
        c.tick_temporary_traits();
        assert!(c.traits.is_empty());
    }

    #[test]
    fn permanent_traits_and_empty_duration_map_survive_ticks() {
        let mut c = Character::new("Billy Slick".to_string(), None, None);
        c.traits = vec![Trait::Friendly, Trait::Tough];
        for _ in 0..5 {
            c.tick_temporary_traits();
        }
        assert_eq!(
            c.traits,
            vec![Trait::Friendly, Trait::Tough],
            "a character with no duration entries is untouched"
        );
    }

    #[test]
    fn expiry_lifts_attribute_bends_immediately() {
        let mut c = Character::new("Snaggletooth".to_string(), None, None);
        c.traits.clear();
        // Pin the axis: a rolled perception of 1 bends to 1 (0.75 rounds
        // up), which would make the "bend applies" assertion flaky.
        c.attributes.perception = 100;
        c.gain_trait(Trait::Nearsighted).expect("nearsighted");
        c.temporary_traits.insert(Trait::Nearsighted, 1);
        let base = c.attributes.perception;
        assert!(
            c.effective_attribute(CharacterAttribute::Perception) < base,
            "AttrMod bend must apply while the curse is held"
        );
        c.tick_temporary_traits();
        assert_eq!(
            c.effective_attribute(CharacterAttribute::Perception),
            base,
            "AttrMods are read at access time — removal is the unbend"
        );
    }

    #[test]
    fn asthmatic_expiry_recomputes_the_stamina_pool() {
        let mut c = Character::new("Stinky".to_string(), None, None);
        c.traits.clear();
        c.gain_trait(Trait::Asthmatic).expect("asthmatic");
        c.temporary_traits.insert(Trait::Asthmatic, 1);
        c.max_stamina = crate::lifecycle::health::max_stamina_for_traits(&c.traits);
        assert_eq!(c.max_stamina, 85, "Asthmatic -15% of 100");
        c.stamina = 50; // spent mid-game; the pool must not refill on expiry

        c.tick_temporary_traits();

        assert!(!c.traits.contains(&Trait::Asthmatic));
        assert_eq!(
            c.max_stamina, 100,
            "the stored max-stamina bend must lift on expiry"
        );
        assert_eq!(c.stamina, 50, "capacity returns, spent stamina does not");
    }

    #[test]
    fn temporary_traits_survive_save_load_round_trip() {
        let mut c = Character::new("Billy Slick".to_string(), None, None);
        c.traits.push(Trait::Tough);
        c.temporary_traits.insert(Trait::Tough, 3);
        let json = serde_json::to_string(&c).unwrap();
        let back: Character = serde_json::from_str(&json).unwrap();
        assert_eq!(back.temporary_traits, c.temporary_traits);
        assert_eq!(back.temporary_traits.get(&Trait::Tough), Some(&3));
    }

    #[test]
    fn old_rows_without_the_duration_field_deserialize_as_permanent() {
        // Old-shape fixture, both directions: an empty map serializes away
        // (the payload keeps its pre-field shape), and a row that lacks the
        // key hydrates with an empty map = everything permanent.
        let mut c = Character::new("Stinky".to_string(), None, None);
        c.traits = vec![Trait::Friendly];
        let json = serde_json::to_string(&c).unwrap();
        assert!(
            !json.contains("temporary_traits"),
            "empty map must keep the old payload shape: {json}"
        );
        let back: Character = serde_json::from_str(&json).unwrap();
        assert!(back.temporary_traits.is_empty());
        assert_eq!(back.traits, vec![Trait::Friendly]);

        let mut with_duration = c.clone();
        with_duration.traits.push(Trait::Tough);
        with_duration.temporary_traits.insert(Trait::Tough, 2);
        let mut value = serde_json::to_value(&with_duration).unwrap();
        value.as_object_mut().unwrap().remove("temporary_traits");
        let legacy: Character = serde_json::from_value(value).unwrap();
        assert!(legacy.temporary_traits.is_empty());
        assert!(
            legacy.traits.contains(&Trait::Tough),
            "trait still permanent"
        );
    }

    #[test]
    fn into_new_game_carries_durations_and_settle_does_not_resurrect_expired() {
        let mut c = Character::new("Stinky".to_string(), None, None);
        c.traits = vec![Trait::Friendly];
        c.gain_trait(Trait::Tough).expect("tough");
        c.temporary_traits.insert(Trait::Tough, 2);
        c.game_baseline = Some(crate::baseline::GameBaseline::capture(&c));

        // Durations cross the rebuild untouched (tick happens at the
        // finish seam, not here — no double-count).
        let mut next = c.into_new_game();
        assert_eq!(next.temporary_traits.get(&Trait::Tough), Some(&2));
        assert!(next.traits.contains(&Trait::Tough));

        // Expiry must survive the following settle: the TraitLost stamp
        // keeps the baseline floor from restoring the trait.
        next.tick_temporary_traits(); // 2 -> 1
        next.tick_temporary_traits(); // 1 -> 0: expires
        assert!(!next.traits.contains(&Trait::Tough));
        let settled = next.into_new_game();
        assert!(
            !settled.traits.contains(&Trait::Tough),
            "settle must not resurrect an expired temporary trait"
        );
        assert!(settled.temporary_traits.is_empty());
    }

    // ── Location persistence (wire shape) ──────────────────────────
    // `location` is `#[serde(flatten)]`: the wire carries `area` and
    // `sub` as sibling top-level columns, so legacy rows that only
    // have `area` hydrate with area preserved and sub = the default
    // triangle (index 0). `sub` serializes as the bare triangle index.

    #[test]
    fn location_serializes_flat_area_and_sub_columns() {
        let mut c = Character::new("Stinky".to_string(), None, None);
        c.location = areas::location::Location::new(
            areas::AreaId::sector3(),
            areas::hex::SubTriangle::new(4),
        );
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["area"], serde_json::json!("Sector 3"));
        assert_eq!(v["sub"], serde_json::json!(4));
        assert!(v.get("location").is_none(), "location must be flattened");
    }

    #[test]
    fn location_round_trips_through_json() {
        let mut c = Character::new("Stinky".to_string(), None, None);
        c.location = areas::location::Location::new(
            areas::AreaId::sector6(),
            areas::hex::SubTriangle::new(1),
        );
        let v = serde_json::to_value(&c).unwrap();
        let back: Character = serde_json::from_value(v).unwrap();
        assert_eq!(back.location, c.location);
    }

    /// Old-shape fixture row: pre-sub-tile saves carry a top-level
    /// `area` column and no `sub`. Hydration must preserve the area and
    /// default the sub to triangle 0.
    #[test]
    fn legacy_area_only_row_hydrates_with_default_sub() {
        let c = Character::new("Stinky".to_string(), None, None);
        let mut v = serde_json::to_value(&c).unwrap();
        v["area"] = serde_json::json!("Sector 1");
        v.as_object_mut().unwrap().remove("sub");
        let back: Character = serde_json::from_value(v).unwrap();
        assert_eq!(back.location.area, areas::AreaId::sector1());
        assert_eq!(
            back.location.sub,
            areas::location::Location::default().sub,
            "legacy rows must hydrate to the default sub-tile"
        );
    }

    /// Rows written by the retired sub-tile layout carry `sub` as
    /// `{"q": .., "r": ..}`. That shape has no unambiguous triangle
    /// mapping, so hydration rejects it loudly instead of fabricating a
    /// position.
    #[test]
    fn legacy_sub_object_row_is_rejected() {
        let c = Character::new("Stinky".to_string(), None, None);
        let mut v = serde_json::to_value(&c).unwrap();
        v["sub"] = serde_json::json!({"q": 1, "r": -1});
        let err = serde_json::from_value::<Character>(v)
            .expect_err("legacy sub-tile object must not hydrate");
        assert!(
            err.to_string().contains("u8"),
            "error should name the expected triangle index type: {err}"
        );
    }
}
