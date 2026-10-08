pub mod events;
pub mod forage;
pub mod hex;
pub mod shelter;
pub mod traps;
pub mod water;
pub mod weather;

use crate::areas::events::AreaEvent;
use crate::areas::hex::{SUB_SLOTS, SubAxial};
use crate::items::OwnsItems;
use crate::items::{Item, ItemError};
use crate::terrain::{BaseTerrain, TerrainType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Display;
use std::str::FromStr;
use strum_macros::EnumIter;
use uuid::Uuid;

#[derive(Copy, Clone, Debug, Eq, PartialEq, EnumIter, Hash, Ord, PartialOrd, Default)]
pub enum Area {
    #[default]
    Hub,
    Sector1,
    Sector2,
    Sector3,
    Sector4,
    Sector5,
    Sector6,
}

// Custom Serialize/Deserialize using Display/FromStr so the on-disk
// representation ("Sector 1" — written by `area.to_string()` in
// `api/src/games.rs::create_game_area`) round-trips cleanly. The
// derived impls would emit/expect the bare variant identifier
// (`"Sector1"`, no space) and reject the spaced form, breaking the
// `/api/games/:id/areas` endpoint and the `<Map>` component.
impl Serialize for Area {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Area {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // ponytail: null → default. The SQL subquery sometimes returns null
        // for area (e.g. character created before area field existed).
        let s = Option::<String>::deserialize(d)?;
        match s {
            None => {
                tracing::warn!("Area field was null in SurrealDB response, defaulting to Hub");
                Ok(Area::default())
            }
            Some(s) if s.is_empty() => Ok(Area::default()),
            Some(s) => Area::from_str(&s).map_err(serde::de::Error::custom),
        }
    }
}

impl Display for Area {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Area::Hub => f.write_str("Hub"),
            Area::Sector1 => f.write_str("Sector 1"),
            Area::Sector2 => f.write_str("Sector 2"),
            Area::Sector3 => f.write_str("Sector 3"),
            Area::Sector4 => f.write_str("Sector 4"),
            Area::Sector5 => f.write_str("Sector 5"),
            Area::Sector6 => f.write_str("Sector 6"),
        }
    }
}

impl PartialEq<&Area> for Area {
    fn eq(&self, other: &&Area) -> bool {
        *self == **other
    }
}

impl FromStr for Area {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hub" => Ok(Area::Hub),
            "sector 1" | "sector1" => Ok(Area::Sector1),
            "sector 2" | "sector2" => Ok(Area::Sector2),
            "sector 3" | "sector3" => Ok(Area::Sector3),
            "sector 4" | "sector4" => Ok(Area::Sector4),
            "sector 5" | "sector5" => Ok(Area::Sector5),
            "sector 6" | "sector6" => Ok(Area::Sector6),
            _ => Err(format!("Invalid area: {}", s)),
        }
    }
}

impl Area {
    /// Topological neighbors derived from the v1 hex layout: Hub
    /// touches all six sectors, each sector touches Hub plus its
    /// two adjacent sectors (clockwise/counter-clockwise wrap 1..6).
    pub fn neighbors(&self) -> Vec<Area> {
        match self {
            Area::Hub => vec![
                Area::Sector1,
                Area::Sector2,
                Area::Sector3,
                Area::Sector4,
                Area::Sector5,
                Area::Sector6,
            ],
            Area::Sector1 => vec![Area::Hub, Area::Sector6, Area::Sector2],
            Area::Sector2 => vec![Area::Hub, Area::Sector1, Area::Sector3],
            Area::Sector3 => vec![Area::Hub, Area::Sector2, Area::Sector4],
            Area::Sector4 => vec![Area::Hub, Area::Sector3, Area::Sector5],
            Area::Sector5 => vec![Area::Hub, Area::Sector4, Area::Sector6],
            Area::Sector6 => vec![Area::Hub, Area::Sector5, Area::Sector1],
        }
    }
}

/// Themed display names for a fresh game's seven areas, aligned with
/// `Area::iter()` order (Hub first, then Sectors 1..6). The hub keeps a fixed
/// name; each sector gets a unique generated name from the bank below.
pub fn generate_area_names() -> Vec<String> {
    generated_area_names(&mut rand::rng())
}

fn generated_area_names(rng: &mut impl rand::Rng) -> Vec<String> {
    use rand::RngExt;
    const SECTOR_NAMES: [&str; 14] = [
        "Whisperwood",
        "Mirefall",
        "Bonefield",
        "Duskmere",
        "Gnawhollow",
        "Cinderrow",
        "Tanglebog",
        "Rustwarren",
        "Mushroom Deep",
        "Snapwillow",
        "Willowfen",
        "Old Burrow",
        "Thistlecrack",
        "Mudwhistle",
    ];
    let mut pool = SECTOR_NAMES.to_vec();
    let mut names = Vec::with_capacity(7);
    names.push("The Hub".to_string());
    for _ in 0..6 {
        let idx = rng.random_range(0..pool.len());
        names.push(pool.swap_remove(idx).to_string());
    }
    names
}

/// Information about a destination area that characters can use to make movement decisions
#[derive(Clone, Debug)]
pub struct DestinationInfo {
    pub area: Area,
    pub terrain: TerrainType,
    pub active_events: Vec<AreaEvent>,
    pub stamina_cost: u32,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct AreaDetails {
    #[serde(default)]
    pub identifier: String,
    #[serde(default)]
    pub name: String,
    pub area: Option<Area>,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub events: Vec<AreaEvent>,
    #[serde(default = "default_terrain")]
    pub terrain: TerrainType,
    /// Traps placed by characters in this area.
    #[serde(default)]
    pub placed_traps: Vec<traps::PlacedTrap>,
    /// Per-character sub-tile slot assignments within this area-hex.
    /// Presentation/positioning only — game logic operates at the area
    /// level. Keys are character identifiers; values are area-local sub
    /// coordinates from `hex::SUB_SLOTS`.
    #[serde(default)]
    pub character_slots: HashMap<String, SubAxial>,
}

fn default_terrain() -> TerrainType {
    TerrainType::new(BaseTerrain::Clearing, vec![]).unwrap()
}

impl Default for AreaDetails {
    fn default() -> Self {
        Self {
            identifier: Uuid::new_v4().to_string(),
            name: String::new(),
            area: None,
            items: vec![],
            events: vec![],
            placed_traps: vec![],
            terrain: TerrainType::new(BaseTerrain::Clearing, vec![]).unwrap(),
            character_slots: HashMap::new(),
        }
    }
}

impl OwnsItems for AreaDetails {
    fn add_item(&mut self, item: Item) {
        self.items.push(item);
    }

    fn has_item(&self, item: &Item) -> bool {
        self.items.iter().any(|i| i == item)
    }

    fn use_item(&mut self, item: &Item) -> Result<(), ItemError> {
        let index = self.items.iter().position(|i| i == item);
        let used_item = self.items.swap_remove(index.unwrap());

        if used_item.current_durability > 0 {
            Ok(())
        } else {
            Err(ItemError::ItemNotFound)
        }
    }

    fn remove_item(&mut self, item: &Item) -> Result<(), ItemError> {
        let index = self
            .items
            .iter()
            .position(|i| i.identifier == item.identifier);
        if let Some(index) = index {
            self.items.remove(index);
            Ok(())
        } else {
            Err(ItemError::ItemNotFound)
        }
    }
}

impl AreaDetails {
    pub fn new(name: Option<String>, area: Area) -> Self {
        Self {
            identifier: Uuid::new_v4().to_string(),
            name: name.unwrap_or(area.to_string()),
            area: Some(area),
            items: vec![],
            events: vec![],
            placed_traps: vec![],
            terrain: TerrainType::new(BaseTerrain::Clearing, vec![]).unwrap(),
            character_slots: HashMap::new(),
        }
    }

    pub fn new_with_terrain(name: Option<String>, area: Area, terrain: TerrainType) -> Self {
        Self {
            identifier: Uuid::new_v4().to_string(),
            name: name.unwrap_or(area.to_string()),
            area: Some(area),
            items: vec![],
            events: vec![],
            placed_traps: vec![],
            terrain,
            character_slots: HashMap::new(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.events.is_empty()
    }

    /// Assign the next available sub-tile slot to `character_id`. If the
    /// character is already assigned, returns its existing slot. If all 7
    /// slots are taken, falls back to the center slot (overflow — v1
    /// accepts visual stacking past 7 characters; sub-tiles are
    /// presentation-only).
    pub fn assign_slot(&mut self, character_id: &str) -> SubAxial {
        if let Some(slot) = self.character_slots.get(character_id) {
            return *slot;
        }
        let used: std::collections::HashSet<SubAxial> =
            self.character_slots.values().copied().collect();
        let chosen = SUB_SLOTS
            .iter()
            .copied()
            .find(|s| !used.contains(s))
            .unwrap_or(SUB_SLOTS[0]);
        self.character_slots
            .insert(character_id.to_string(), chosen);
        chosen
    }

    /// Release the slot currently held by `character_id`, if any.
    pub fn release_slot(&mut self, character_id: &str) -> Option<SubAxial> {
        self.character_slots.remove(character_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn from_str() {
        let area = Area::from_str("hub");
        assert_eq!(area.unwrap(), Area::Hub);
    }

    #[test]
    fn to_str() {
        assert_eq!(Area::Hub.to_string(), "Hub");
    }

    #[test]
    fn iter() {
        let areas: Vec<Area> = Area::iter().collect();
        assert_eq!(areas.len(), 7);
        assert_eq!(areas[0], Area::Hub);
        assert_eq!(areas[1], Area::Sector1);
        assert_eq!(areas[2], Area::Sector2);
        assert_eq!(areas[3], Area::Sector3);
        assert_eq!(areas[4], Area::Sector4);
        assert_eq!(areas[5], Area::Sector5);
        assert_eq!(areas[6], Area::Sector6);
    }

    #[test]
    fn add_item() {
        let mut area_details = AreaDetails::new(None, Area::Sector4);
        let item = Item::new_random_weapon();
        area_details.add_item(item.clone());
        assert!(area_details.items.contains(&item));
    }

    #[test]
    fn remove_item() {
        let mut area_details = AreaDetails::new(None, Area::Sector4);
        let item = Item::new_random_weapon();
        area_details.add_item(item.clone());
        assert!(area_details.items.contains(&item));
        area_details.remove_item(&item).unwrap();
        assert!(!area_details.items.contains(&item));
    }

    #[test]
    fn add_event() {
        let mut area_details = AreaDetails::new(None, Area::Sector1);
        let event = AreaEvent::Wildfire;
        area_details.events.push(event.clone());
        assert!(area_details.events.contains(&event));
    }

    #[test]
    fn process_events_closes_area() {
        let mut area_details = AreaDetails::new(None, Area::Sector1);
        assert!(area_details.is_open());
        let event = AreaEvent::Wildfire;
        area_details.events.push(event.clone());
        assert!(!area_details.is_open());
    }

    #[test]
    fn partial_eq_with_reference() {
        let area = Area::Hub;
        assert_eq!(area, &area);
    }

    #[test]
    fn assign_slot_returns_center_first() {
        let mut a = AreaDetails::new(None, Area::Hub);
        let s = a.assign_slot("t1");
        assert_eq!(s, SUB_SLOTS[0]);
    }

    #[test]
    fn assign_slot_is_idempotent_for_same_character() {
        let mut a = AreaDetails::new(None, Area::Hub);
        let s1 = a.assign_slot("t1");
        let s2 = a.assign_slot("t1");
        assert_eq!(s1, s2);
        assert_eq!(a.character_slots.len(), 1);
    }

    #[test]
    fn assign_slot_gives_unique_slots_until_full() {
        let mut a = AreaDetails::new(None, Area::Hub);
        let mut slots = std::collections::HashSet::new();
        for i in 0..7 {
            let s = a.assign_slot(&format!("t{i}"));
            assert!(slots.insert(s), "duplicate slot {:?} on character t{i}", s);
        }
        assert_eq!(slots.len(), 7);
    }

    #[test]
    fn assign_slot_overflows_to_center_when_full() {
        let mut a = AreaDetails::new(None, Area::Hub);
        for i in 0..7 {
            a.assign_slot(&format!("t{i}"));
        }
        let overflow = a.assign_slot("t7");
        assert_eq!(overflow, SUB_SLOTS[0]);
    }

    #[test]
    fn release_slot_frees_slot_for_reassignment() {
        let mut a = AreaDetails::new(None, Area::Hub);
        let original = a.assign_slot("t1");
        let released = a.release_slot("t1");
        assert_eq!(released, Some(original));
        assert!(a.character_slots.is_empty());
        let again = a.assign_slot("t2");
        assert_eq!(again, original);
    }

    /// Regression: `Area`'s serde representation
    /// must use the human-readable `Display` form ("Sector 1" with a
    /// space) so it round-trips cleanly through SurrealDB rows written
    /// by `api/src/games.rs::create_game_area` (which stores
    /// `area.to_string()`).
    #[test]
    fn area_serde_roundtrip_uses_display_form() {
        for variant in Area::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            // Display form: "Hub", "Sector 1", ..., "Sector 6"
            assert_eq!(json, format!("\"{}\"", variant));
            let back: Area = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
        // Tolerant of the no-space form too (FromStr accepts both).
        let no_space: Area = serde_json::from_str("\"Sector1\"").unwrap();
        assert_eq!(no_space, Area::Sector1);
        // And case-insensitive (FromStr lowercases).
        let lower: Area = serde_json::from_str("\"hub\"").unwrap();
        assert_eq!(lower, Area::Hub);
    }
}
