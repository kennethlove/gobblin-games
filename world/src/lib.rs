//! World layer: the domain substrate characters live in.
//!
//! Terrain, topology/pathfinding, threats, naming, and game
//! configuration — extracted from `game` so the domain entity stops
//! living inside the simulation orchestrator (see gobblin-games-dww.16
//! decision notes).

pub mod config;
pub mod naming;
pub mod pathfinding;
pub mod terrain;
pub mod threats;
