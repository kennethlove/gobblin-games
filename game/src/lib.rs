pub mod areas;
pub mod characters;
pub mod clans;
pub mod config;
pub mod events;
pub mod games;
pub mod items;
pub mod messages;
pub mod naming;
pub mod output;
pub mod pathfinding;
pub mod patrons;
pub mod phases;
pub mod terrain;
pub mod threats;
mod witty_phrase_generator;

// Re-export key terrain types
pub use terrain::{BaseTerrain, TerrainDescriptor, TerrainType};
