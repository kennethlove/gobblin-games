//! PlacedTrap state — traps set by characters via Action::SetTrap.
//!
//! See `docs/superpowers/specs/2026-05-29-trap-expansion-design.md`.

use crate::hex::SubTriangle;
use serde::{Deserialize, Serialize};
use shared::afflictions::{Severity, TrapKind};

/// Serde fallback for rows written before traps were keyed per sub-tile:
/// hydrate sub-centered, the same default as `Location`.
fn default_sub() -> SubTriangle {
    SubTriangle::new(0)
}

/// A trap placed by a character in a specific sub-tile of an area.
/// Lives on `AreaDetails::placed_traps`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacedTrap {
    /// Unique identifier for this trap instance.
    pub id: String,
    /// Trap kind determines what happens on trigger.
    pub kind: TrapKind,
    /// Severity scales damage and escape difficulty.
    pub severity: Severity,
    /// ID of the character who set this trap.
    pub set_by: String,
    /// Concealment DC — Perception check threshold to spot before triggering.
    pub concealment: u32,
    /// Whether this trap has already been triggered.
    #[serde(default)]
    pub triggered: bool,
    /// Sub-tile the trap is keyed to: it only triggers for goblins
    /// standing in this sub-tile.
    #[serde(default = "default_sub")]
    pub sub: SubTriangle,
}
