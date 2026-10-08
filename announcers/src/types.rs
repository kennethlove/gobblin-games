use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum number of permanent highlights retained per character.
/// Keeps prompt size bounded while preserving a character's full arc-defining
/// moments (kills, betrayals, alliances, survivals). Oldest prunes first.
pub const MAX_HIGHLIGHTS: usize = 20;

// ---------------------------------------------------------------------------
// Event kinds — the category labels the broadcast builder assigns to each
// phase event so the narrator prompt can dispatch on kind + prose.
// ---------------------------------------------------------------------------

/// High-level category for a single phase event line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Combat,
    Death,
    Allied,
    Betrayal,
    Hazard,
    Item,
    Movement,
    Patron,
    State,
    Wound,
    Condition,
    Desperate,
    Other,
}

/// One event line in a broadcast package. Hybrid format: typed `kind` +
/// human-readable `prose` + optional structured data for high-value events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLine {
    /// Category label the narrator can dispatch on.
    pub kind: EventKind,
    /// Human-readable prose (always present — the `.content` field of
    /// `GameMessage` or a synthesised summary).
    pub prose: String,
    /// Structured sub-fields for high-value event types (combat swings,
    /// deaths, alliance formations, etc.). `None` for minor events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Game-state snapshot — phase-level context built from the engine state
// after each phase completes.
// ---------------------------------------------------------------------------

/// A character who has scored kills this phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillLeader {
    pub name: String,
    pub team: u8,
    pub kill_count: u32,
}

/// An active killing spree — a character on a multi-kill streak.
///
/// | Streak | Label |
/// |--------|-------|
/// | 2-3    | heating up |
/// | 4-5    | on fire |
/// | 6-7    | dominating |
/// | 8+     | unstoppable |
///
/// Spree labels are narrative descriptors the narrator can weave into
/// commentary ("Rendmaw is heating up!", "Snaggletooth is unstoppable!").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillingSpree {
    pub name: String,
    pub team: u8,
    pub streak: u32,
    pub label: String,
}

/// Return the spree label for a given streak count.
pub fn spree_label(streak: u32) -> &'static str {
    match streak {
        0..=1 => "",
        2..=3 => "heating up",
        4..=5 => "on fire",
        6..=7 => "dominating",
        _ => "unstoppable",
    }
}

/// Summary of an active alliance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllianceInfo {
    pub members: Vec<String>,
}

/// An area with elevated activity (combats, hazards, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AreaActivity {
    pub name: String,
    pub activity_level: String, // e.g. "quiet", "active", "hot"
}

/// Phase-level context built after each phase completes. Fed to the narrator
/// as the "header" section of the broadcast package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStateSnapshot {
    /// Current game day (1-indexed).
    pub day: u32,
    /// Current phase hour label (e.g. "00", "06", "12", "18").
    pub phase: String,
    /// How many characters are still alive.
    pub alive_count: u32,
    /// Characters with kills this phase (sorted by kill count descending).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kill_leaders: Vec<KillLeader>,
    /// Active alliances visible to the commentary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alliances: Vec<AllianceInfo>,
    /// Areas with notable activity this phase.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hot_zones: Vec<AreaActivity>,
    /// Active killing sprees (characters on streaks of 2+ consecutive kills).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub killing_sprees: Vec<KillingSpree>,
}

// ---------------------------------------------------------------------------
// Character digest — rolling per-character summary updated each phase.
// ---------------------------------------------------------------------------

/// Rolling digest for one character, updated every phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterDigest {
    /// Stable identifier (matches `CharacterRef.identifier`).
    pub identifier: String,
    pub name: String,
    pub team: u8,
    /// "alive" or "deceased"
    pub status: String,
    /// Narrative injury level (derived via severity mapping).
    pub injury_level: String,
    /// Current area name.
    pub location: String,
    /// Names of allied characters (if any).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allies: Vec<String>,
    /// Current kill streak (consecutive kills without dying or a dry phase).
    #[serde(default)]
    pub kill_streak: u32,
    /// Rolling log of notable events for this character, newest first.
    /// Capped at 30 entries (prunes oldest).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notable_events: Vec<String>,
    /// Permanent highlights — kills, betrayals, alliances, and other pivotal
    /// moments that survive the rolling cap. Never pruned except by a hard
    /// cap of MAX_HIGHLIGHTS (currently 20) to keep prompt size bounded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<String>,
}

// ---------------------------------------------------------------------------
// Broadcast package — the full structured input the narrator consumes to produce
// a commentary segment.
// ---------------------------------------------------------------------------

/// Everything the narrator needs to generate a commentary segment for one phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastPackage {
    /// Phase-level context (alive count, kill leaders, etc.).
    pub header: GameStateSnapshot,
    /// Phase events in causal order, one per game message.
    pub events: Vec<EventLine>,
    /// Rolling digests for every character (sorted by name).
    pub histories: Vec<CharacterDigest>,
}

// ---------------------------------------------------------------------------
// Commentary output — what the narrator returns.
// ---------------------------------------------------------------------------

/// One utterance by a commentator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentaryLine {
    /// Speaker name (e.g. "Verity" or "Rex").
    pub speaker: String,
    /// Spoken text.
    pub text: String,
}

/// A persisted commentary segment: one narration generation for one phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentarySegment {
    /// Storage key; omitted from reader queries (SurrealDB returns it as a
    /// record id, not a string), hence the default.
    #[serde(default)]
    pub id: String,
    pub game_id: String,
    pub day: u32,
    pub phase: String,
    pub lines: Vec<CommentaryLine>,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub model_used: String,
}

// ---------------------------------------------------------------------------
// Crate error type.
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum CommentaryError {
    #[error("failed to build broadcast package: {0}")]
    BuildPackage(String),

    #[error("failed to serialize broadcast package: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("narrator generation failed: {0}")]
    Generate(String),

    #[error("history error: {0}")]
    History(String),
}
