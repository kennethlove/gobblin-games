use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

use crate::ids::{AreaId, CharacterId, ItemId};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum MessageSource {
    #[serde(rename = "Game")]
    Game(String), // Game identifier
    #[serde(rename = "Area")]
    Area(String), // Area name
    #[serde(rename = "Character")]
    Character(String), // Character identifier
}

/// One phase of the game day: an even-hour slot in `00..=22`.
///
/// 12 phases per day (2 h each). The game-day cycle starts at
/// [`Phase::DAY_START_HOUR`] (06 by default) and runs
/// `06, 08, …, 22, 00, 02, 04`, so night hours (20–04) sit at the tail
/// of the game day. Ordinal order (`ord()` = position in that cycle)
/// drives chronological sorting of `GameMessage`s.
///
/// Named hour anchors ([`Phase::DAWN`] / [`DAY`] / [`DUSK`] / [`NIGHT`])
/// exist for code that reasons about the day's narrative slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Phase(u8);

impl Phase {
    /// Phases per game day (12 × 2 h slots).
    pub const PHASES_PER_DAY: u8 = 12;
    /// Default game-day start hour (first phase of every game day).
    pub const DAY_START_HOUR: u8 = 6;
    /// Default nightfall hour (phases from here to day start are night).
    pub const NIGHTFALL_HOUR: u8 = 20;
    /// Every valid even hour, ascending.
    pub const HOURS: [u8; 12] = [0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22];

    // Four-phase-day anchors, kept as named constants for code that
    // reasons about the day's narrative slots.
    /// First phase of the game day (06).
    pub const DAWN: Phase = Phase(Self::DAY_START_HOUR); // 06
    /// Midday (12).
    pub const DAY: Phase = Phase(12);
    /// Evening (18).
    pub const DUSK: Phase = Phase(18);
    /// Late night (00).
    pub const NIGHT: Phase = Phase(0);
    /// First phase of the game day (alias of [`Self::DAWN`]).
    pub const DAY_START: Phase = Phase(Self::DAY_START_HOUR);
    /// Last phase of the game day (04 — followed by the day start).
    pub const DAY_END: Phase = Phase(4);

    /// The hour label of this phase (`00` … `22`).
    pub const fn hour(self) -> u8 {
        self.0
    }

    /// Phase for an even hour in `00..=22`; `None` for odd or ≥ 24.
    pub const fn from_hour(hour: u8) -> Option<Phase> {
        if hour < 24 && hour.is_multiple_of(2) {
            Some(Phase(hour))
        } else {
            None
        }
    }

    /// Chronological ordinal: position in the game-day cycle starting at
    /// [`Self::DAY_START_HOUR`] (`06` → 0, …, `04` → 11). Stable across
    /// the wire format because `summarize_periods` relies on it.
    pub const fn ord(self) -> u8 {
        if self.0 >= Self::DAY_START_HOUR {
            (self.0 - Self::DAY_START_HOUR) / 2
        } else {
            (self.0 + 24 - Self::DAY_START_HOUR) / 2
        }
    }

    /// Next phase in the canonical `06 → 08 → … → 04 → 06` cycle.
    /// Day-boundary handling lives in the engine driver, not here.
    pub const fn next(self) -> Phase {
        Phase((self.0 + 2) % 24)
    }

    /// All 12 phases in cycle order, starting at [`Self::DAY_START`].
    pub const fn all() -> [Phase; 12] {
        [
            Phase(6),
            Phase(8),
            Phase(10),
            Phase(12),
            Phase(14),
            Phase(16),
            Phase(18),
            Phase(20),
            Phase(22),
            Phase(0),
            Phase(2),
            Phase(4),
        ]
    }

    /// Whether this phase counts as night for the given day boundary
    /// hours. Handles wrap-around (`day_start > nightfall`, the default:
    /// night runs 20 → 06).
    pub const fn is_night(self, day_start: u8, nightfall: u8) -> bool {
        if day_start <= nightfall {
            self.0 >= nightfall || self.0 < day_start
        } else {
            self.0 >= nightfall && self.0 < day_start
        }
    }

    /// Night check with the default boundaries (day starts 06, night
    /// falls 20). Configurable per game once day/night hours land.
    pub const fn is_night_default(self) -> bool {
        self.is_night(Self::DAY_START_HOUR, Self::NIGHTFALL_HOUR)
    }

    /// Two-digit hour label (`"00"` … `"22"`).
    pub fn label(self) -> String {
        format!("{:02}", self.0)
    }
}

impl PartialOrd for Phase {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Phase {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Chronological (cycle) order, not raw hour: 04 is the last phase
        // of the day even though its hour is smallest.
        self.ord().cmp(&other.ord())
    }
}

impl Serialize for Phase {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Phase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let raw = String::deserialize(deserializer)?;
        Phase::from_str(&raw).map_err(D::Error::custom)
    }
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsePhaseError;

impl std::fmt::Display for ParsePhaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "phase must be an even hour '00'..'22'")
    }
}

impl std::error::Error for ParsePhaseError {}

impl FromStr for Phase {
    type Err = ParsePhaseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hour: u8 = s.parse().map_err(|_| ParsePhaseError)?;
        Phase::from_hour(hour).ok_or(ParsePhaseError)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CharacterRef {
    pub identifier: CharacterId,
    pub name: String,
}

/// A team that won a game: slot number plus display label.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamRef {
    /// Team slot (1..=8).
    pub team: u32,
    /// Display label, e.g. "Orange Mangletooths".
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AreaRef {
    pub identifier: AreaId,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemRef {
    pub identifier: ItemId,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaEventKind {
    Hazard,
    Storm,
    Mutts,
    Earthquake,
    Flood,
    Fire,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatEngagement {
    pub attacker: CharacterRef,
    pub target: CharacterRef,
    pub outcome: CombatOutcome,
    pub detail_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombatOutcome {
    Killed,
    Wounded,
    TargetFled,
    AttackerFled,
    Stalemate,
}

/// Source of a `Drank` event: either a terrain water source or a Water item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DrinkSource {
    Terrain { area: AreaRef },
    Item { item: ItemRef },
}

/// Visible fatigue band derived from a character's stamina/max_stamina ratio.
/// Lives in `shared/` because it is wire-visible via
/// `MessagePayload::StaminaBandChanged`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StaminaBand {
    Fresh,
    Winded,
    Exhausted,
}

/// Visible hunger band derived from a character's hunger counter. Lives in
/// `shared/` because it is wire-visible via
/// `MessagePayload::HungerBandChanged`. The mapping (counter → band) lives
/// in `characters::survival::hunger_band`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HungerBand {
    Sated,
    Peckish,
    Hungry,
    Starving,
}

/// Visible thirst band derived from a character's thirst counter. Lives in
/// `shared/` because it is wire-visible via
/// `MessagePayload::ThirstBandChanged`. The mapping (counter → band) lives
/// in `characters::survival::thirst_band`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThirstBand {
    Sated,
    Thirsty,
    Parched,
    Dehydrated,
}

impl StaminaBand {
    pub fn as_str(self) -> &'static str {
        match self {
            StaminaBand::Fresh => "Fresh",
            StaminaBand::Winded => "Winded",
            StaminaBand::Exhausted => "Exhausted",
        }
    }
}

impl std::fmt::Display for StaminaBand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl HungerBand {
    pub fn as_str(self) -> &'static str {
        match self {
            HungerBand::Sated => "Sated",
            HungerBand::Peckish => "Peckish",
            HungerBand::Hungry => "Hungry",
            HungerBand::Starving => "Starving",
        }
    }
}

impl std::fmt::Display for HungerBand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ThirstBand {
    pub fn as_str(self) -> &'static str {
        match self {
            ThirstBand::Sated => "Sated",
            ThirstBand::Thirsty => "Thirsty",
            ThirstBand::Parched => "Parched",
            ThirstBand::Dehydrated => "Dehydrated",
        }
    }
}

impl std::fmt::Display for ThirstBand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a sleeping character woke. Pairs with `MessagePayload::CharacterWoke`.
/// See spec `2026-05-03-four-phase-day-design.md` §6.4 / §8.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "reason")]
pub enum WakeReason {
    /// The character slept the planned duration without interruption.
    Rested,
    /// Sleep was cut short. `event` describes the interrupting cause.
    Interrupted { event: InterruptionKind },
}

/// Concrete cause of a sleep interruption (`WakeReason::Interrupted`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "interruption")]
pub enum InterruptionKind {
    /// Another character attacked the sleeper.
    Ambush { attacker: CharacterRef },
    /// An area event (storm, mutts, etc.) hit the sleeper's area.
    AreaEvent { kind: AreaEventKind },
    /// An ally summoned the character (alliance event cascade).
    AllianceSummons { ally: CharacterRef },
    /// A sleep incident (theft, relocation, animal, etc.) woke the character.
    Incident { kind: SleepIncidentKind },
}

/// Category of sleep incident that can occur while a character is unconscious.
/// Carried in `InterruptionKind::Incident` for typed wire format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "incident", rename_all = "snake_case")]
pub enum SleepIncidentKind {
    /// Annoying but harmless (squirrel on chest, weird dream). No mechanical effect.
    Annoying,
    /// A random item was stolen from the sleeper.
    Theft,
    /// The sleeper was relocated to a different area while unconscious.
    Relocation,
    /// An animal (named) disturbed the sleep.
    AnimalEncounter { animal: String },
    /// Hallucination or bad dream — sanity damage.
    Hallucination,
    /// Nightmare (bad dream) — sanity damage, does not wake the sleeper.
    Nightmare,
    /// Night terror — sanity damage, wakes the sleeper.
    NightTerror,
    /// An ally abandoned the sleeper during the night.
    AllyAbandonment,
    /// Comedic limb issue (leg fell asleep, etc.) — temporary affliction.
    LimbInjury,
}

/// Effect category for a `PhobiaTriggered` event. Mirrors the game-layer
/// `PhobiaEffect` enum so the wire format is self-contained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhobiaEffect {
    Penalty,
    Flee,
    Freeze,
}

/// Kill leader line inside a [`MessagePayload::DaySummary`] rollup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayKillLeader {
    pub character: CharacterRef,
    pub kills: u32,
}

/// Game-day rollup: counts folded from the day's stored messages plus
/// end-of-day roster state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayRollup {
    pub deaths: u32,
    /// Top killers of the day (most kills first, capped at 3).
    pub kill_leaders: Vec<DayKillLeader>,
    pub alliances_formed: u32,
    pub alliances_dissolved: u32,
    pub betrayals: u32,
    pub items_found: u32,
    pub items_used: u32,
    /// Roster state at day end.
    pub survivors: u32,
    pub fallen: u32,
}

/// Per-goblin digest for one game day, inside a
/// [`MessagePayload::DaySummary`]. End-of-day state is snapshotted here
/// so the log reads without joining character rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoblinDaySummary {
    pub character: CharacterRef,
    pub team: u32,
    pub kills: u32,
    pub wounds_taken: u32,
    pub items_found: u32,
    pub items_used: u32,
    /// Formed/proposed/dissolved/betrayal events involving this goblin.
    pub alliance_changes: u32,
    /// End-of-day snapshot.
    pub alive: bool,
    pub blood: u32,
    pub sanity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[derive(strum::EnumDiscriminants)]
#[strum_discriminants(name(MessageKind))]
#[strum_discriminants(derive(Serialize, Deserialize))]
pub enum MessagePayload {
    /// Generic narrative event — prose-only, no structured payload consumers.
    Generic,
    CharacterKilled {
        victim: CharacterRef,
        killer: Option<CharacterRef>,
        cause: crate::afflictions::DeathCause,
    },
    CharacterWounded {
        victim: CharacterRef,
        attacker: Option<CharacterRef>,
        hp_lost: u32,
    },
    CharacterAttacked {
        victim: CharacterRef,
        attacker: Option<CharacterRef>,
    },

    Combat(CombatEngagement),
    /// One physical-combat swing in fully typed form (see `CombatBeat`).
    /// Emitted alongside the existing `Combat`/`CharacterKilled`/`CharacterWounded`
    /// payloads so consumers can render structured swing data without parsing
    /// `detail_lines` strings.
    CombatSwing(crate::combat_beat::CombatBeat),

    AllianceFormed {
        members: Vec<CharacterRef>,
    },
    AllianceProposed {
        proposer: CharacterRef,
        target: CharacterRef,
    },
    AllianceDissolved {
        members: Vec<CharacterRef>,
        reason: String,
    },
    BetrayalTriggered {
        betrayer: CharacterRef,
        victim: CharacterRef,
    },
    TrustShockBreak {
        character: CharacterRef,
        partner: CharacterRef,
    },

    CharacterMoved {
        character: CharacterRef,
        from: AreaRef,
        to: AreaRef,
    },
    CharacterHidden {
        character: CharacterRef,
        area: AreaRef,
    },
    AreaClosed {
        area: AreaRef,
    },
    AreaEvent {
        area: AreaRef,
        kind: AreaEventKind,
        description: String,
    },

    ItemFound {
        character: CharacterRef,
        item: ItemRef,
        area: AreaRef,
    },
    ItemUsed {
        character: CharacterRef,
        item: ItemRef,
    },
    ItemDropped {
        character: CharacterRef,
        item: ItemRef,
        area: AreaRef,
    },
    PatronGift {
        recipient: CharacterRef,
        item: ItemRef,
        donor: String,
    },

    CharacterRested {
        character: CharacterRef,
        hp_restored: u32,
    },
    CharacterStarved {
        character: CharacterRef,
        hp_lost: u32,
    },
    CharacterDehydrated {
        character: CharacterRef,
        hp_lost: u32,
    },
    SanityBreak {
        character: CharacterRef,
    },

    // Wound model events (blood/sanity wound system).
    /// Emitted when a character dies from blood loss (blood reaches 0).
    CharacterBledOut {
        character: CharacterRef,
    },
    /// Emitted when a wound becomes infected.
    WoundInfected {
        character: CharacterRef,
        body_part: String,
    },
    /// Emitted when a wound heals naturally.
    WoundHealed {
        character: CharacterRef,
        body_part: String,
    },
    /// Emitted when a wound is inflicted on a character.
    WoundInflicted {
        character: CharacterRef,
        wound_type: String,
        severity: String,
        body_part: String,
    },
    /// Emitted when a character bleeds from wounds.
    WoundBled {
        character: CharacterRef,
        blood_lost: u32,
    },
    /// Emitted when a wound is treated (bleeding stopped).
    WoundTreated {
        character: CharacterRef,
        body_part: String,
    },
    /// Emitted when a limb is amputated due to severe wound.
    WoundAmputated {
        character: CharacterRef,
        body_part: String,
    },
    /// Emitted when a mental condition is acquired.
    ConditionAcquired {
        character: CharacterRef,
        condition: String,
        severity: String,
    },
    /// Emitted when a mental condition resolves.
    ConditionResolved {
        character: CharacterRef,
        condition: String,
    },
    /// Emitted when a character becomes desperate (very low blood/sanity).
    CharacterDesperate {
        character: CharacterRef,
        reason: String,
    },

    // Survival events (shelter + hunger/thirst spec).
    HungerBandChanged {
        character: CharacterRef,
        from: HungerBand,
        to: HungerBand,
    },
    ThirstBandChanged {
        character: CharacterRef,
        from: ThirstBand,
        to: ThirstBand,
    },
    StaminaBandChanged {
        character: CharacterRef,
        from: StaminaBand,
        to: StaminaBand,
    },
    ShelterSought {
        character: CharacterRef,
        area: AreaRef,
        success: bool,
        roll: u8,
    },
    Foraged {
        character: CharacterRef,
        area: AreaRef,
        success: bool,
        debt_recovered: u8,
    },
    Drank {
        character: CharacterRef,
        source: DrinkSource,
        debt_recovered: u8,
    },
    Ate {
        character: CharacterRef,
        item: ItemRef,
        debt_recovered: u8,
    },

    // Lifecycle / cycle-boundary announcements.
    /// Emitted at the very start of a day or night phase.
    CycleStart {
        day: u32,
        phase: Phase,
    },
    /// Emitted at the very end of a day or night phase.
    CycleEnd {
        day: u32,
        phase: Phase,
    },
    /// Emitted at the start of each phase (Dawn/Day/Dusk/Night).
    /// Replaces the legacy `GameDayStart`/`GameNightStart` events.
    PhaseStarted {
        day: u32,
        phase: Phase,
        weather_summary: Option<String>,
    },
    /// Emitted at the end of each phase.
    /// Replaces the legacy `GameDayEnd`/`GameNightEnd` events.
    PhaseEnded {
        day: u32,
        phase: Phase,
    },
    /// Emitted when a character begins sleeping (resolution of `Action::Sleep`).
    /// `restored_*` fields are zero on the first phase of a multi-phase sleep
    /// and accumulate per phase as the engine ticks the sleeper. See spec
    /// `2026-05-03-four-phase-day-design.md` §6.4 / §8.
    CharacterSlept {
        character: CharacterRef,
        phase: Phase,
        restored_stamina: u32,
        restored_hp: u32,
    },
    /// Emitted when a sleeping character wakes — either because the planned
    /// sleep duration elapsed (`WakeReason::Rested`) or because something
    /// interrupted them (`WakeReason::Interrupted`).
    CharacterWoke {
        character: CharacterRef,
        phase: Phase,
        reason: WakeReason,
    },
    /// Sleep incident event (flavor-only or wake-causing). Emitted alongside
    /// or instead of `CharacterWoke` when a sleeping character experiences an
    /// incident (nightmare, night terror, theft, animal encounter, etc.).
    /// The `kind` field discriminates the incident type; `description` holds
    /// the narrative text.
    SleepIncident {
        character: CharacterRef,
        kind: SleepIncidentKind,
        description: String,
    },
    /// Emitted when the game ends. `winning_team` is `Some` for the last
    /// team standing; `winner` is the legacy individual winner, kept so
    /// pre-team-victory records still deserialize.
    GameEnded {
        #[serde(default)]
        winner: Option<CharacterRef>,
        #[serde(default)]
        winning_team: Option<TeamRef>,
    },

    // Affliction events (health conditions PR2).
    AfflictionAcquired {
        character_id: String,
        affliction: String,
        severity: String,
    },
    AfflictionProgressed {
        character_id: String,
        affliction: String,
        from_severity: String,
        to_severity: String,
    },
    AfflictionHealed {
        character_id: String,
        affliction: String,
    },
    AfflictionCascaded {
        character_id: String,
        from_affliction: String,
        to_affliction: String,
    },

    // Trauma events (trauma producer pipeline PR2).
    TraumaAcquired {
        character: String,
        severity: String,
        source: String,
    },
    TraumaReinforced {
        character: String,
        from_severity: String,
        to_severity: String,
        floor_bumped: bool,
    },

    // Phobia events (phobia brain layer PR2).
    PhobiaAcquired {
        character: String,
        trigger: String,
        severity: String,
        origin: String,
    },
    PhobiaTriggered {
        character: String,
        trigger: String,
        severity: String,
        effect: PhobiaEffect,
    },
    /// Phobia escalation (Traumatic origin only).
    PhobiaEscalated {
        character: String,
        trigger: String,
        from_severity: String,
        to_severity: String,
    },
    /// Phobia habituation (Traumatic origin, severity decayed or cured).
    PhobiaHabituated {
        character: String,
        trigger: String,
        from_severity: String,
        to_severity: Option<String>,
    },
    /// A character observed someone else's phobia firing.
    PhobiaObserved {
        observer: String,
        subject: String,
        trigger: String,
    },
    /// A character forgot someone else's phobia (observer decay).
    PhobiaForgotten {
        observer: String,
        subject: String,
        trigger: String,
    },
    // Trauma escalation/effects (trauma PR3 brain layer).
    /// Trauma severity escalated (producer reinforcement roll).
    TraumaEscalated {
        character: String,
        from_severity: String,
        to_severity: String,
    },
    /// Character experienced a trauma flashback.
    TraumaFlashback {
        character: String,
        severity: String,
        source: String,
    },
    /// Character avoided an action due to trauma avoidance.
    TraumaAvoidance {
        character: String,
        source: String,
        prevented_action: String,
    },
    /// A character observed someone else's trauma firing.
    TraumaObserved {
        observer: String,
        subject: String,
        source: String,
    },
    /// A character forgot someone else's trauma (observer decay).
    TraumaForgotten {
        observer: String,
        subject: String,
        source: String,
    },
    /// Trauma severity decayed or cured (habituation).
    TraumaHabituated {
        character: String,
        from_severity: String,
        to_severity: Option<String>,
    },

    // Fixation events (fixation brain layer PR2).
    /// A character acquired a fixation on a target.
    FixationAcquired {
        character_id: String,
        target: String,
        severity: String,
        origin: String,
    },
    /// A fixation's severity escalated.
    FixationEscalated {
        character_id: String,
        target: String,
        old_severity: String,
        new_severity: String,
    },
    /// A fixation fired — the brain is overriding toward the target.
    FixationFired {
        character_id: String,
        target: String,
        severity: String,
        action: String,
    },
    /// A fixation was consummated (target reached/acquired).
    FixationConsummated {
        character_id: String,
        target: String,
    },
    /// A fixation was thwarted (target lost/unreachable).
    FixationThwarted {
        character_id: String,
        target: String,
        reason: String,
    },
    /// A fixation faded (severity decayed to nothing).
    FixationFaded {
        character_id: String,
        target: String,
    },
    // Addiction events (addiction PR2).
    /// Character used a substance (addictive consumable).
    SubstanceUsed {
        character: String,
        item: String,
        substance: String,
    },
    /// Character acquired a new addiction.
    AddictionAcquired {
        character: String,
        substance: String,
        severity: String,
        use_count: u32,
    },
    /// Existing addiction reinforced (used while already addicted).
    AddictionReinforced {
        character: String,
        substance: String,
        severity: String,
    },
    /// Addiction severity escalated (12% sensitization roll).
    AddictionEscalated {
        character: String,
        substance: String,
        from_severity: String,
        to_severity: String,
    },
    /// Addiction acquisition prevented (at cap, or roll failed).
    AddictionResisted {
        character: String,
        substance: String,
        reason: String,
    },
    /// Relapse — cured character auto-reacquired on first use.
    AddictionRelapse {
        character: String,
        substance: String,
        prior_uses: u32,
    },
    /// Character is craving a substance (visible to observers).
    AddictionCraving {
        character: String,
        substance: String,
        severity: String,
    },
    /// A character observed someone's addiction behavior.
    AddictionObserved {
        observer: String,
        subject: String,
        substance: String,
    },
    /// A character forgot someone's addiction (observer decay).
    AddictionForgotten {
        observer: String,
        subject: String,
        substance: String,
    },
    /// Addiction severity decayed or cured (habituation).
    AddictionHabituated {
        character: String,
        substance: String,
        from_severity: String,
        to_severity: Option<String>,
    },
    /// Character became trapped by a hazard (drowning, buried, etc.).
    CharacterTrapped {
        character: String,
        kind: crate::afflictions::TrapKind,
        severity: crate::afflictions::Severity,
    },
    /// Character is still trapped — ongoing struggle with cumulative effect.
    Struggling {
        character: String,
        kind: crate::afflictions::TrapKind,
        severity: crate::afflictions::Severity,
        cycles_trapped: u8,
    },
    /// Character escaped the trap (may have been rescued).
    TrappedEscaped {
        character: String,
        kind: crate::afflictions::TrapKind,
        cycles_trapped: u8,
        rescued_by: Vec<String>,
    },
    /// Character died while trapped — could not escape in time.
    CharacterDiedWhileTrapped {
        character: String,
        kind: crate::afflictions::TrapKind,
    },
    /// A character set a trap.
    TrapSet {
        character: CharacterRef,
        trap_kind: String,
    },
    /// A character triggered a trap.
    TrapTriggered {
        victim: CharacterRef,
        trap_kind: String,
    },
    /// Rescuer attempted to free a trapped character this cycle.
    /// Emitted every cycle a character performs Action::Rescue.
    RescueAttempted {
        rescuer: String,
        target: String,
        kind: crate::afflictions::TrapKind,
        severity: crate::afflictions::Severity,
        bonus: f32,
    },
    /// Partial rescue progress accumulated — rescue bonus increased.
    /// Emitted when a rescue attempt contributes toward the escape threshold,
    /// but the target is not yet freed.
    PartialRescueProgress {
        rescuer: String,
        target: String,
        kind: crate::afflictions::TrapKind,
        severity: crate::afflictions::Severity,
        bonus: f32,
        progress: u8,
        /// How many rescue cycles are needed before the rescue bonus applies.
        /// Only meaningful at Severe; always `PARTIAL_RESCUE_THRESHOLD` (2).
        threshold: u8,
    },

    /// Compiled end-of-day digest — the one stored summary row per game
    /// day: day rollup plus a per-goblin array. Emitted at the end of
    /// `run_full_day`; `day` matches the stored messages' `game_day`.
    DaySummary {
        day: u32,
        rollup: DayRollup,
        goblins: Vec<GoblinDaySummary>,
    },
}

pub mod impls;

pub use impls::Importance;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameMessage {
    pub identifier: String,
    pub source: MessageSource,
    pub game_day: u32,
    pub phase: Phase,
    pub tick: u32,
    pub emit_index: u32,
    pub subject: String,
    #[serde(with = "chrono::serde::ts_nanoseconds")]
    pub timestamp: DateTime<Utc>,
    pub content: String,
    pub payload: MessagePayload,
}

impl PartialEq for GameMessage {
    /// Identity equality via `identifier`. `MessagePayload` is not `PartialEq`
    /// (would require deriving across the entire payload graph); identity
    /// equality is sufficient for cache dedup since each persisted message
    /// has a unique identifier.
    fn eq(&self, other: &Self) -> bool {
        self.identifier == other.identifier
    }
}

impl GameMessage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: MessageSource,
        game_day: u32,
        phase: Phase,
        tick: u32,
        emit_index: u32,
        subject: String,
        content: String,
        payload: MessagePayload,
    ) -> Self {
        Self {
            identifier: Uuid::new_v4().to_string(),
            source,
            game_day,
            phase,
            tick,
            emit_index,
            subject,
            timestamp: Utc::now(),
            content,
            payload,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeriodSummary {
    pub day: u32,
    pub phase: Phase,
    pub deaths: u32,
    pub event_count: u32,
    pub is_current: bool,
}

/// Newtype wrapper around the period list returned by `summarize_periods`.
/// Exists so the API surface stays a stable named type as the timeline payload
/// grows (e.g. recap, totals) without breaking clients.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineSummary {
    pub periods: Vec<PeriodSummary>,
}

/// Aggregate messages into one summary per (day, phase). Includes empty periods
/// up to and including `current` so the hub shows the live period even when
/// nothing has been emitted there yet. Periods past `current` are not emitted.
///
/// Day 1 is special-cased per the four-phase day spec (§3): characters rise on
/// the pedestals at `Day` so `Dawn1` is never emitted by the engine and is
/// excluded from the back-fill. Subsequent days walk all four phases.
pub fn summarize_periods(messages: &[GameMessage], current: (u32, Phase)) -> Vec<PeriodSummary> {
    use std::collections::BTreeMap;

    let (current_day, current_phase) = current;
    let mut bucket: BTreeMap<(u32, u8), (u32, u32)> = BTreeMap::new();

    for m in messages {
        let key = (m.game_day, m.phase.ord());
        let entry = bucket.entry(key).or_insert((0, 0));
        if !matches!(
            m.payload,
            MessagePayload::PhaseStarted { .. } | MessagePayload::PhaseEnded { .. }
        ) {
            entry.1 += 1;
        }
        if matches!(m.payload, MessagePayload::CharacterKilled { .. })
            || matches!(
                &m.payload,
                MessagePayload::Combat(engagement)
                    if engagement.outcome == CombatOutcome::Killed
            )
        {
            entry.0 += 1;
        }
    }

    // Always seed the current period so the hub shows it even before any
    // events are emitted (e.g. day 0 of a NotStarted game). Then back-fill
    // every prior (day, phase) pair starting at day 1 so the summary list
    // is dense up to the live period without gaps for empty cycles.
    //
    // Day 0 only ever has the day-start phase (NotStarted seed). Every
    // game day runs the full 12-phase cycle from the day start.
    bucket
        .entry((current_day, current_phase.ord()))
        .or_insert((0, 0));
    for d in 1..=current_day {
        let max_ord = if d < current_day {
            Phase::DAY_END.ord()
        } else {
            current_phase.ord()
        };
        for phase in Phase::all() {
            if phase.ord() <= max_ord {
                bucket.entry((d, phase.ord())).or_insert((0, 0));
            }
        }
    }

    bucket
        .into_iter()
        .map(|((day, p), (deaths, count))| {
            let phase = Phase::all()
                .get(p as usize)
                .copied()
                .unwrap_or(Phase::DAY_END);
            PeriodSummary {
                day,
                phase,
                deaths,
                event_count: count,
                is_current: day == current_day && phase == current_phase,
            }
        })
        .collect()
}

#[cfg(test)]
mod survival_event_tests;
#[cfg(test)]
mod tests;
