//! World layer: the domain substrate characters live in.
//!
//! Terrain, topology/pathfinding, threats, naming, and game
//! configuration — extracted from `game` so the domain entity stops
//! living inside the simulation orchestrator.

pub mod clans;
pub mod config;
pub mod items;
pub mod messages;
pub mod naming;
pub mod output;
pub mod pathfinding;
pub mod terrain;
pub mod threats;
