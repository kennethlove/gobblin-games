pub mod events;
pub mod forage;
pub mod hex;
pub mod location;
pub mod preset;
pub mod shelter;
pub mod traps;
pub mod water;
pub mod weather;

use crate::events::AreaEvent;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::str::FromStr;
use uuid::Uuid;
use world::items::OwnsItems;
use world::items::{Item, ItemError};
use world::terrain::{BaseTerrain, TerrainType};

/// Slot identity (zkp.3): the fixed seven-variant `AreaId` enum is gone;
/// identity is a data string now. Values preserve the enum's wire form
/// exactly — Display writes `"Hub"` / `"Sector 1"` (the spaced form the
/// DB `original_name`, `/api/games/:id/areas`, and `<Map>` consume), so
/// every boundary keeps agreeing. The count lives in the layout
/// (preset q/r + `AreaDetails`), not in this type.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct AreaId(&'static str);

impl AreaId {
    pub fn hub() -> Self {
        Self("Hub")
    }
    pub fn sector1() -> Self {
        Self("Sector 1")
    }
    pub fn sector2() -> Self {
        Self("Sector 2")
    }
    pub fn sector3() -> Self {
        Self("Sector 3")
    }
    pub fn sector4() -> Self {
        Self("Sector 4")
    }
    pub fn sector5() -> Self {
        Self("Sector 5")
    }
    pub fn sector6() -> Self {
        Self("Sector 6")
    }

    /// The classic seven in label order. `generate_area_names`' doc
    /// pins this order (hub first, then sectors); call sites iterate it
    /// with `.into_iter()`. Deliberately not an enum-derive: the known
    /// set is a convention for content/name generation, not a validity
    /// constraint on identity.
    pub fn iter() -> [AreaId; 7] {
        [
            Self::hub(),
            Self::sector1(),
            Self::sector2(),
            Self::sector3(),
            Self::sector4(),
            Self::sector5(),
            Self::sector6(),
        ]
    }

    pub fn as_str(&self) -> &str {
        self.0
    }
}

/// WHY `&'static str`: `AreaId` must stay `Copy` — `Location` holds one
/// and pathfinding moves `Location`s by value through arrays and BFS
/// (losing `Copy` cascaded into ~30 move errors). Known spellings map
/// to their static wire form; an unknown slot name (future preset or
/// generated slots — zkp.3's point) is leaked once into `'static`.
/// Vocabulary = preset slot labels (a dozen strings), so the leak is
/// bounded by the same vocabulary.
fn intern(slot: &str) -> &'static str {
    match slot.to_lowercase().as_str() {
        "hub" => "Hub",
        "sector 1" | "sector1" => "Sector 1",
        "sector 2" | "sector2" => "Sector 2",
        "sector 3" | "sector3" => "Sector 3",
        "sector 4" | "sector4" => "Sector 4",
        "sector 5" | "sector5" => "Sector 5",
        "sector 6" | "sector6" => "Sector 6",
        other => Box::leak(other.to_string().into_boxed_str()),
    }
}

impl Default for AreaId {
    fn default() -> Self {
        Self::hub()
    }
}

impl Serialize for AreaId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AreaId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // ponytail: null/empty -> Hub, same as the enum's custom impl
        // (SQL subqueries return null for areas written without one).
        // Any other string is a valid identity — validity of a slot is
        // the layout's (preset) concern, not this type's (zkp.3).
        let s = Option::<String>::deserialize(d)?;
        match s {
            None => {
                tracing::warn!("Area field was null in SurrealDB response, defaulting to Hub");
                Ok(AreaId::hub())
            }
            Some(s) if s.is_empty() => Ok(AreaId::hub()),
            Some(s) => Ok(Self(intern(&s))),
        }
    }
}

impl Display for AreaId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl PartialEq<&AreaId> for AreaId {
    fn eq(&self, other: &&AreaId) -> bool {
        *self == **other
    }
}

impl FromStr for AreaId {
    type Err = String;

    /// Canonicalizes the enum's known spellings (case-insensitive, with
    /// or without the space) to their wire form; anything else passes
    /// through verbatim — count-agnostic means the parser never rejects
    /// a slot name (zkp.3).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(intern(s)))
    }
}

impl AreaId {
    /// Parent adjacency by geometry: an area touches every area at axial
    /// distance 1 from its own coordinate. The ring/wrap falls out of the
    /// layout's coordinates instead of a hardcoded 7-arm match — zkp.1.
    pub fn neighbors(&self) -> Vec<AreaId> {
        self.neighbors_from_layout(&crate::hex::default_layout())
    }

    /// Adjacency from a caller-supplied layout: distance-1 of `self`'s
    /// coordinate, resolved back to identities in that layout (missing
    /// entries fall back to `default_layout`, so details-less areas
    /// behave exactly like [`AreaId::neighbors`]). zkp.2: presets feed
    /// their q/r through this and the ring becomes the layout's
    /// property, not the enum's.
    pub fn neighbors_from_layout(&self, layout: &[(AreaId, crate::hex::Axial)]) -> Vec<AreaId> {
        let coord = |a: AreaId| {
            layout
                .iter()
                .find(|(kind, _)| *kind == a)
                .map(|(_, ax)| *ax)
                .unwrap_or_else(|| crate::location::area_axial(a))
        };
        crate::hex::Axial::neighbors(coord(*self))
            .into_iter()
            .filter_map(|ax| {
                layout
                    .iter()
                    .find(|(kind, _)| coord(*kind) == ax)
                    .map(|(kind, _)| *kind)
            })
            .collect()
    }
}

/// Themed display names for a fresh game's seven areas, aligned with
/// `AreaId::iter().into_iter()` order (Hub first, then Sectors 1..6). The hub keeps a fixed
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
    pub area: AreaId,
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
    pub area: Option<AreaId>,
    /// Authoritative axial coordinate from the arena preset (zkp.2).
    /// Hydrates from the area record's `axial` field (serde default).
    /// `None` = rows written before coords rode the record, or helpers
    /// without a preset — readers fall back to `area_axial`
    /// (`default_layout`), which is exactly the pre-zkp.2 behavior.
    #[serde(default)]
    pub axial: Option<crate::hex::Axial>,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub events: Vec<AreaEvent>,
    /// Blocks (time blocks) this area's hazards stay active; None =
    /// no hazards. Set when an event arrives, aged down each cycle,
    /// and cleared with `events` at 0 (q7k: max 3 blocks lifetime).
    #[serde(default)]
    pub hazard_blocks: Option<u8>,
    #[serde(default = "default_terrain")]
    pub terrain: TerrainType,
    /// Authored sub-tile atmosphere from the arena preset snapshot, in
    /// `hex::SubTriangle::index` order (entry `i` covers the triangle
    /// whose base spans this area hex's edge in axial direction `i`,
    /// apex at the hex center). Empty for rows created before sub-tiles
    /// existed. Atmosphere only — the pre-1.0 interaction layer (real
    /// combat/adjacency/travel keyed on sub-tiles) promotes this data.
    #[serde(default, alias = "sub_hexes")]
    pub sub_tiles: Vec<crate::preset::SubTile>,
    /// Traps placed by characters in this area.
    #[serde(default)]
    pub placed_traps: Vec<traps::PlacedTrap>,
}

pub fn default_terrain() -> TerrainType {
    TerrainType::new(BaseTerrain::Clearing, vec![]).unwrap()
}

impl Default for AreaDetails {
    fn default() -> Self {
        Self {
            identifier: Uuid::new_v4().to_string(),
            name: String::new(),
            area: None,
            axial: None,
            items: vec![],
            events: vec![],
            hazard_blocks: None,
            placed_traps: vec![],
            terrain: TerrainType::new(BaseTerrain::Clearing, vec![]).unwrap(),
            sub_tiles: vec![],
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
    pub fn new(name: Option<String>, area: AreaId) -> Self {
        Self {
            identifier: Uuid::new_v4().to_string(),
            name: name.unwrap_or(area.to_string()),
            area: Some(area),
            axial: None,
            items: vec![],
            events: vec![],
            hazard_blocks: None,
            placed_traps: vec![],
            terrain: TerrainType::new(BaseTerrain::Clearing, vec![]).unwrap(),
            sub_tiles: vec![],
        }
    }

    pub fn new_with_terrain(name: Option<String>, area: AreaId, terrain: TerrainType) -> Self {
        Self {
            identifier: Uuid::new_v4().to_string(),
            name: name.unwrap_or(area.to_string()),
            area: Some(area),
            axial: None,
            items: vec![],
            events: vec![],
            hazard_blocks: None,
            placed_traps: vec![],
            terrain,
            sub_tiles: vec![],
        }
    }

    pub fn is_open(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str() {
        let area = AreaId::from_str("hub");
        assert_eq!(area.unwrap(), AreaId::hub());
    }

    #[test]
    fn to_str() {
        assert_eq!(AreaId::hub().to_string(), "Hub");
    }

    #[test]
    fn iter() {
        let areas: Vec<AreaId> = AreaId::iter().into_iter().collect();
        assert_eq!(areas.len(), 7);
        assert_eq!(areas[0], AreaId::hub());
        assert_eq!(areas[1], AreaId::sector1());
        assert_eq!(areas[2], AreaId::sector2());
        assert_eq!(areas[3], AreaId::sector3());
        assert_eq!(areas[4], AreaId::sector4());
        assert_eq!(areas[5], AreaId::sector5());
        assert_eq!(areas[6], AreaId::sector6());
    }

    #[test]
    fn add_item() {
        let mut area_details = AreaDetails::new(None, AreaId::sector4());
        let item = Item::new_random_weapon();
        area_details.add_item(item.clone());
        assert!(area_details.items.contains(&item));
    }

    #[test]
    fn remove_item() {
        let mut area_details = AreaDetails::new(None, AreaId::sector4());
        let item = Item::new_random_weapon();
        area_details.add_item(item.clone());
        assert!(area_details.items.contains(&item));
        area_details.remove_item(&item).unwrap();
        assert!(!area_details.items.contains(&item));
    }

    #[test]
    fn add_event() {
        let mut area_details = AreaDetails::new(None, AreaId::sector1());
        let event = AreaEvent::Wildfire;
        area_details.events.push(event.clone());
        assert!(area_details.events.contains(&event));
    }

    #[test]
    fn process_events_closes_area() {
        let mut area_details = AreaDetails::new(None, AreaId::sector1());
        assert!(area_details.is_open());
        let event = AreaEvent::Wildfire;
        area_details.events.push(event.clone());
        assert!(!area_details.is_open());
    }

    #[test]
    fn partial_eq_with_reference() {
        let area = AreaId::hub();
        assert_eq!(area, &area);
    }

    /// Regression: `AreaId`'s serde representation
    /// must use the human-readable `Display` form ("Sector 1" with a
    /// space) so it round-trips cleanly through SurrealDB rows written
    /// by `api/src/games.rs::create_game_area` (which stores
    /// `area.to_string()`).
    #[test]
    fn area_serde_roundtrip_uses_display_form() {
        for variant in AreaId::iter().into_iter() {
            let json = serde_json::to_string(&variant).unwrap();
            // Display form: "Hub", "Sector 1", ..., "Sector 6"
            assert_eq!(json, format!("\"{}\"", variant));
            let back: AreaId = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
        // Tolerant of the no-space form too (FromStr accepts both).
        let no_space: AreaId = serde_json::from_str("\"Sector1\"").unwrap();
        assert_eq!(no_space, AreaId::sector1());
        // And case-insensitive (FromStr lowercases).
        let lower: AreaId = serde_json::from_str("\"hub\"").unwrap();
        assert_eq!(lower, AreaId::hub());
    }
}
