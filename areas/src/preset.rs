//! Arena presets: in-repo, human-editable arena definitions chosen at
//! game creation. The chosen preset is snapshotted onto the game row;
//! the game's areas are instantiated from that snapshot.
//!
//! Format documentation lives in `areas/presets/README.md`. The default
//! preset loads through the same registry loader as any future preset —
//! there is no special-cased code path for it beyond its registry entry.

use crate::Area;
use crate::hex::SUB_SLOTS;
use serde::{Deserialize, Serialize};
use std::fmt;
use world::terrain::BaseTerrain;

/// Id of the preset used when a game creator makes no arena choice.
pub const DEFAULT_PRESET_ID: &str = "classic-hex";

/// One sub-hex — the smallest map unit inside an area hex. A big hex
/// is the general biome; the sub-hexes are the actual features within
/// it (a valley inside a mountain biome reads as its own terrain).
/// Atmosphere only today; the pre-1.0 interaction layer (real combat,
/// adjacency and travel keyed on sub-hexes) promotes this container.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubHex {
    /// Base terrain of this sub-hex.
    pub terrain: BaseTerrain,
    /// Short feature name shown on the map, e.g. "Hidden Valley".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    /// One-line flavor text shown next to the feature name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flavor: Option<String>,
}

/// One area slot in an arena preset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaSlot {
    /// Which engine area this slot instantiates. Engine adjacency
    /// (`Area::neighbors`) is fixed, so every preset must cover exactly
    /// the seven known areas, each once.
    pub area: Area,
    /// Axial hex coordinate (`q`, `r`) — presentation only; game logic
    /// operates on area identity, not coordinates.
    pub q: i32,
    pub r: i32,
    /// Fixed display name. Absent = a fresh generated name per game.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The seven sub-hexes this area hex divides into, in
    /// `hex::SUB_SLOTS` order (index 0 = center, 1..6 = ring clockwise
    /// from top-right). Optional for backward compatibility; the loader
    /// requires exactly seven when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_hexes: Option<Vec<SubHex>>,
}

/// A complete arena preset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaPreset {
    /// Stable machine id used in create-game payloads.
    pub id: String,
    /// Human-facing name shown in choice UIs.
    pub name: String,
    /// One-paragraph human description shown next to the name.
    pub description: String,
    /// Area slots in label order: Hub first, then Sectors 1..6
    /// clockwise from top-right.
    pub areas: Vec<ArenaSlot>,
}

impl ArenaPreset {
    /// Concrete `(area, display name, sub-hexes)` triples for a fresh
    /// game, in slot order. Slots with a fixed name keep it; the rest
    /// draw from the same generated-name bank the pre-preset flow used,
    /// so the default preset behaves exactly like the old inline flow.
    /// Slots without authored sub-hexes yield an empty sub-hex list.
    pub fn instantiate(&self) -> Vec<(Area, String, Vec<SubHex>)> {
        let generated = crate::generate_area_names();
        debug_assert_eq!(
            generated.len(),
            self.areas.len(),
            "validated presets have one name slot per area"
        );
        self.areas
            .iter()
            .zip(generated)
            .map(|(slot, fallback)| {
                let sub_hexes = slot.sub_hexes.clone().unwrap_or_default();
                (slot.area, slot.name.clone().unwrap_or(fallback), sub_hexes)
            })
            .collect()
    }
}

/// Why a preset id or file was rejected.
#[derive(Debug, PartialEq, Eq)]
pub enum PresetError {
    /// The requested id is not in the registry.
    UnknownId {
        requested: String,
        available: Vec<String>,
    },
    /// The preset file exists but violates the format contract.
    Invalid { id: String, reason: String },
}

impl fmt::Display for PresetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PresetError::UnknownId {
                requested,
                available,
            } => write!(
                f,
                "Unknown arena preset '{requested}'. Available presets: {}",
                available.join(", ")
            ),
            PresetError::Invalid { id, reason } => {
                write!(f, "Invalid arena preset '{id}': {reason}")
            }
        }
    }
}

/// The in-repo preset registry: `(id, raw JSON)`. Adding a preset = one
/// JSON file in `areas/presets/` + one line here.
const REGISTRY: &[(&str, &str)] = &[(
    DEFAULT_PRESET_ID,
    include_str!("../presets/classic_hex.json"),
)];

/// Every preset id in the registry.
pub fn preset_ids() -> Vec<&'static str> {
    REGISTRY.iter().map(|(id, _)| *id).collect()
}

/// Resolve a game creator's arena choice. `None` or empty string means
/// "no choice" and yields the default preset.
pub fn resolve(id: Option<&str>) -> Result<ArenaPreset, PresetError> {
    match id {
        None | Some("") => load_preset(DEFAULT_PRESET_ID),
        Some(id) => load_preset(id),
    }
}

/// Load and validate a preset from the registry by id.
pub fn load_preset(id: &str) -> Result<ArenaPreset, PresetError> {
    let Some((_, json)) = REGISTRY.iter().find(|(registry_id, _)| *registry_id == id) else {
        return Err(PresetError::UnknownId {
            requested: id.to_string(),
            available: preset_ids().into_iter().map(str::to_string).collect(),
        });
    };
    parse_preset(json, id)
}

/// The default preset, for callers that make no arena choice.
pub fn default_preset() -> ArenaPreset {
    load_preset(DEFAULT_PRESET_ID).expect("default arena preset must load")
}

/// Every preset in the registry, for arena choice UIs.
pub fn list_presets() -> Vec<ArenaPreset> {
    preset_ids()
        .into_iter()
        .map(|id| load_preset(id).expect("registered arena presets must load"))
        .collect()
}

/// Parse one preset file and enforce the format contract.
fn parse_preset(json: &str, expected_id: &str) -> Result<ArenaPreset, PresetError> {
    let invalid = |reason: String| PresetError::Invalid {
        id: expected_id.to_string(),
        reason,
    };
    let preset: ArenaPreset =
        serde_json::from_str(json).map_err(|e| invalid(format!("not valid JSON: {e}")))?;
    if preset.id != expected_id {
        return Err(invalid(format!(
            "file id '{}' does not match its registry id",
            preset.id
        )));
    }
    validate_layout(&preset.areas).map_err(invalid)?;
    Ok(preset)
}

/// The engine's adjacency (`Area::neighbors`) covers exactly the seven
/// known areas, so a preset must list each exactly once, Hub first, at
/// unique axial coordinates.
fn validate_layout(areas: &[ArenaSlot]) -> Result<(), String> {
    use std::collections::HashSet;
    use strum::IntoEnumIterator;

    let kinds: HashSet<Area> = areas.iter().map(|s| s.area).collect();
    if kinds.len() != areas.len() {
        return Err("duplicate area kinds".to_string());
    }
    for kind in Area::iter() {
        if !kinds.contains(&kind) {
            return Err(format!("missing area {kind}"));
        }
    }
    if areas[0].area != Area::Hub {
        return Err("Hub must be the first area".to_string());
    }
    let coords: HashSet<(i32, i32)> = areas.iter().map(|s| (s.q, s.r)).collect();
    if coords.len() != areas.len() {
        return Err("duplicate axial coordinates".to_string());
    }
    for slot in areas {
        if let Some(subs) = slot
            .sub_hexes
            .as_ref()
            .filter(|subs| subs.len() != SUB_SLOTS.len())
        {
            return Err(format!(
                "area {} has {} sub-hexes; expected {}",
                slot.area,
                subs.len(),
                SUB_SLOTS.len()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::default_layout;
    use std::collections::HashSet;

    #[test]
    fn default_preset_loads_via_registry_loader() {
        let preset = load_preset(DEFAULT_PRESET_ID).expect("default preset must load");
        assert_eq!(preset.id, DEFAULT_PRESET_ID);
        assert_eq!(preset.areas.len(), 7);
        assert!(!preset.name.is_empty());
        assert!(!preset.description.is_empty());
    }

    #[test]
    fn default_preset_matches_default_layout() {
        let preset = default_preset();
        let expected: Vec<(Area, i32, i32)> = default_layout()
            .iter()
            .map(|(area, ax)| (*area, ax.q, ax.r))
            .collect();
        let got: Vec<(Area, i32, i32)> = preset
            .areas
            .iter()
            .map(|slot| (slot.area, slot.q, slot.r))
            .collect();
        assert_eq!(got, expected);
        assert!(
            preset.areas.iter().all(|slot| slot.name.is_none()),
            "default preset keeps per-game generated names"
        );
    }

    #[test]
    fn default_preset_round_trips_through_loader_format() {
        let preset = default_preset();
        let encoded = serde_json::to_string(&preset).expect("serialize preset");
        let decoded: ArenaPreset = serde_json::from_str(&encoded).expect("deserialize preset");
        assert_eq!(decoded, preset);
    }

    #[test]
    fn every_registered_preset_round_trips() {
        for preset in list_presets() {
            let encoded = serde_json::to_string(&preset).expect("serialize preset");
            let decoded: ArenaPreset = serde_json::from_str(&encoded).expect("deserialize preset");
            assert_eq!(decoded, preset, "preset {} must round-trip", preset.id);
        }
    }

    #[test]
    fn unknown_preset_id_lists_available_presets() {
        let err = load_preset("volcano").expect_err("unknown id must fail");
        assert_eq!(
            err,
            PresetError::UnknownId {
                requested: "volcano".to_string(),
                available: vec![DEFAULT_PRESET_ID.to_string()],
            }
        );
        let message = err.to_string();
        assert!(message.contains("volcano"), "message: {message}");
        assert!(message.contains(DEFAULT_PRESET_ID), "message: {message}");
    }

    #[test]
    fn resolve_treats_no_choice_as_default() {
        assert_eq!(resolve(None).expect("None"), default_preset());
        assert_eq!(resolve(Some("")).expect("empty"), default_preset());
        assert_eq!(
            resolve(Some(DEFAULT_PRESET_ID)).expect("explicit id"),
            default_preset()
        );
    }

    #[test]
    fn instantiate_default_preset_covers_all_areas_with_unique_names() {
        let triples = default_preset().instantiate();
        assert_eq!(triples.len(), 7);
        assert_eq!(triples[0].0, Area::Hub);
        assert_eq!(triples[0].1, "The Hub".to_string());
        let kinds: HashSet<Area> = triples.iter().map(|(area, _, _)| *area).collect();
        assert_eq!(kinds.len(), 7);
        let names: HashSet<String> = triples.iter().map(|(_, name, _)| name.clone()).collect();
        assert_eq!(names.len(), 7);
    }

    #[test]
    fn fixed_name_slot_wins_over_generated_name() {
        let json = r#"{
            "id": "test-preset",
            "name": "Test",
            "description": "test",
            "areas": [
                { "area": "Hub", "q": 0, "r": 0, "name": "Home" },
                { "area": "Sector 1", "q": 1, "r": -1 },
                { "area": "Sector 2", "q": 1, "r": 0 },
                { "area": "Sector 3", "q": 0, "r": 1 },
                { "area": "Sector 4", "q": -1, "r": 1 },
                { "area": "Sector 5", "q": -1, "r": 0 },
                { "area": "Sector 6", "q": 0, "r": -1 }
            ]
        }"#;
        let preset = parse_preset(json, "test-preset").expect("valid preset");
        let encoded = serde_json::to_string(&preset).expect("serialize");
        let decoded: ArenaPreset = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, preset);
        assert_eq!(decoded.areas[0].name.as_deref(), Some("Home"));
        let triples = decoded.instantiate();
        assert_eq!(triples[0].0, Area::Hub);
        assert_eq!(triples[0].1, "Home".to_string());
        assert!(triples[0].2.is_empty(), "no sub-hexes authored");
    }

    #[test]
    fn preset_missing_area_is_rejected() {
        let json = r#"{
            "id": "bad",
            "name": "Bad",
            "description": "missing Sector 6",
            "areas": [
                { "area": "Hub", "q": 0, "r": 0 },
                { "area": "Sector 1", "q": 1, "r": -1 },
                { "area": "Sector 2", "q": 1, "r": 0 },
                { "area": "Sector 3", "q": 0, "r": 1 },
                { "area": "Sector 4", "q": -1, "r": 1 },
                { "area": "Sector 5", "q": -1, "r": 0 }
            ]
        }"#;
        let err = parse_preset(json, "bad").expect_err("missing area must fail");
        assert!(err.to_string().contains("Sector 6"), "message: {err}");
    }

    #[test]
    fn preset_with_duplicate_coordinates_is_rejected() {
        let json = r#"{
            "id": "bad",
            "name": "Bad",
            "description": "two sectors share a coord",
            "areas": [
                { "area": "Hub", "q": 0, "r": 0 },
                { "area": "Sector 1", "q": 1, "r": -1 },
                { "area": "Sector 2", "q": 1, "r": -1 },
                { "area": "Sector 3", "q": 0, "r": 1 },
                { "area": "Sector 4", "q": -1, "r": 1 },
                { "area": "Sector 5", "q": -1, "r": 0 },
                { "area": "Sector 6", "q": 0, "r": -1 }
            ]
        }"#;
        let err = parse_preset(json, "bad").expect_err("duplicate coords must fail");
        assert!(err.to_string().contains("coordinates"), "message: {err}");
    }

    #[test]
    fn preset_with_mismatched_id_is_rejected() {
        let json = r#"{
            "id": "other",
            "name": "Other",
            "description": "id drift",
            "areas": [
                { "area": "Hub", "q": 0, "r": 0 },
                { "area": "Sector 1", "q": 1, "r": -1 },
                { "area": "Sector 2", "q": 1, "r": 0 },
                { "area": "Sector 3", "q": 0, "r": 1 },
                { "area": "Sector 4", "q": -1, "r": 1 },
                { "area": "Sector 5", "q": -1, "r": 0 },
                { "area": "Sector 6", "q": 0, "r": -1 }
            ]
        }"#;
        let err = parse_preset(json, "bad").expect_err("id mismatch must fail");
        assert!(err.to_string().contains("registry id"), "message: {err}");
    }

    #[test]
    fn slot_without_sub_hexes_still_deserializes() {
        let json = r#"{
            "id": "test-preset",
            "name": "Test",
            "description": "no sub-hexes anywhere",
            "areas": [
                { "area": "Hub", "q": 0, "r": 0 },
                { "area": "Sector 1", "q": 1, "r": -1 },
                { "area": "Sector 2", "q": 1, "r": 0 },
                { "area": "Sector 3", "q": 0, "r": 1 },
                { "area": "Sector 4", "q": -1, "r": 1 },
                { "area": "Sector 5", "q": -1, "r": 0 },
                { "area": "Sector 6", "q": 0, "r": -1 }
            ]
        }"#;
        let preset = parse_preset(json, "test-preset").expect("valid preset");
        assert!(preset.areas.iter().all(|slot| slot.sub_hexes.is_none()));
    }

    #[test]
    fn slot_with_wrong_sub_hex_count_is_rejected() {
        let json = r#"{
            "id": "test-preset",
            "name": "Test",
            "description": "six sub-hexes instead of seven",
            "areas": [
                { "area": "Hub", "q": 0, "r": 0, "sub_hexes": [
                    { "terrain": "Clearing" }, { "terrain": "Clearing" },
                    { "terrain": "Clearing" }, { "terrain": "Clearing" },
                    { "terrain": "Clearing" }, { "terrain": "Clearing" }
                ] },
                { "area": "Sector 1", "q": 1, "r": -1 },
                { "area": "Sector 2", "q": 1, "r": 0 },
                { "area": "Sector 3", "q": 0, "r": 1 },
                { "area": "Sector 4", "q": -1, "r": 1 },
                { "area": "Sector 5", "q": -1, "r": 0 },
                { "area": "Sector 6", "q": 0, "r": -1 }
            ]
        }"#;
        let err = parse_preset(json, "test-preset").expect_err("wrong count must fail");
        let message = err.to_string();
        assert!(message.contains("6 sub-hexes"), "message: {message}");
        assert!(message.contains("expected 7"), "message: {message}");
    }

    #[test]
    fn default_preset_authors_seven_sub_hexes_per_slot() {
        let preset = default_preset();
        for slot in &preset.areas {
            let subs = slot
                .sub_hexes
                .as_ref()
                .unwrap_or_else(|| panic!("{} must author sub-hexes", slot.area));
            assert_eq!(subs.len(), SUB_SLOTS.len(), "{}", slot.area);
            assert!(
                subs.iter()
                    .all(|s| !s.feature.as_deref().unwrap_or("").is_empty())
            );
        }
    }

    #[test]
    fn sub_hexes_round_trip_through_loader_format() {
        let preset = default_preset();
        let encoded = serde_json::to_string(&preset).expect("serialize");
        assert!(encoded.contains("sub_hexes"), "encoded: {encoded}");
        let decoded: ArenaPreset = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, preset);
        let triples = decoded.instantiate();
        assert!(
            triples
                .iter()
                .all(|(_, _, subs)| subs.len() == SUB_SLOTS.len())
        );
    }
}
