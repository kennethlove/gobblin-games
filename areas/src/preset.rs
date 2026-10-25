//! Arena presets: in-repo, human-editable arena definitions chosen at
//! game creation. The chosen preset is snapshotted onto the game row;
//! the game's areas are instantiated from that snapshot.
//!
//! Format documentation lives in `areas/presets/README.md`. The default
//! preset loads through the same registry loader as any future preset —
//! there is no special-cased code path for it beyond its registry entry.

use crate::Area;
use crate::hex::SubTriangle;
use serde::{Deserialize, Serialize};
use std::fmt;
use world::terrain::BaseTerrain;

/// Id of the preset used when a game creator makes no arena choice.
pub const DEFAULT_PRESET_ID: &str = "classic-hex";

/// One sub-tile — the smallest map unit inside an area hex. A big hex
/// is the general biome; the sub-tiles (one per triangle sub-tile) are
/// the actual features within it (a valley inside a mountain biome
/// reads as its own terrain).
/// Atmosphere only today; the pre-1.0 interaction layer (real combat,
/// adjacency and travel keyed on sub-tiles) promotes this container.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubTile {
    /// Base terrain of this sub-tile.
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
    /// Which engine area this slot instantiates. Identity still comes
    /// from the enum while coverage is the known seven (validated by
    /// `validate_layout`); adjacency derives from the slot's q/r via
    /// `neighbors_from_layout` (zkp.2).
    pub area: Area,
    /// Axial hex coordinate (`q`, `r`) — authoritative: instantiate
    /// passes it to the area record and layout-aware adjacency reads it
    /// back through `AreaDetails::axial` (zkp.2; pre-zkp.2 it was
    /// presentation-only).
    pub q: i32,
    pub r: i32,
    /// Fixed display name. Absent = a fresh generated name per game.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The six sub-tiles this area hex divides into, in
    /// `hex::SubTriangle::index` order (entry `i` describes the
    /// triangle whose base spans this hex's edge in axial direction
    /// `i` — the same six directions as the top-level layout).
    /// Optional for backward compatibility; the loader requires exactly
    /// six when present.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "sub_hexes")]
    pub sub_tiles: Option<Vec<SubTile>>,
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
    /// Concrete `(area, display name, sub-tiles)` triples for a fresh
    /// game, in slot order. Slots with a fixed name keep it; the rest
    /// draw from the same generated-name bank the pre-preset flow used,
    /// so the default preset behaves exactly like the old inline flow.
    /// Slots without authored sub-tiles yield an empty sub-tile list.
    /// Returns `(area, name, sub_tiles, axial)` — the coordinate rides
    /// the caller into the area record (zkp.2).
    pub fn instantiate(&self) -> Vec<(Area, String, Vec<SubTile>, crate::hex::Axial)> {
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
                let sub_tiles = slot.sub_tiles.clone().unwrap_or_default();
                (
                    slot.area,
                    slot.name.clone().unwrap_or(fallback),
                    sub_tiles,
                    crate::hex::Axial::new(slot.q, slot.r),
                )
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
            .sub_tiles
            .as_ref()
            .filter(|subs| subs.len() != SubTriangle::COUNT)
        {
            return Err(format!(
                "area {} has {} sub-tiles; expected {}",
                slot.area,
                subs.len(),
                SubTriangle::COUNT
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
    fn neighbors_from_layout_matches_default_neighbors() {
        let layout = crate::hex::default_layout();
        for (kind, _) in layout {
            assert_eq!(
                kind.neighbors_from_layout(&layout),
                kind.neighbors(),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn neighbors_from_layout_follows_translated_coordinates() {
        // Same seven slots, every coordinate shifted (+1, 0): adjacency
        // is relative geometry, so the ring must come out identical —
        // this is the invariant presets rely on (zkp.2).
        let layout: Vec<(Area, crate::hex::Axial)> = crate::hex::default_layout()
            .into_iter()
            .map(|(kind, ax)| (kind, crate::hex::Axial::new(ax.q + 1, ax.r)))
            .collect();
        for (kind, _) in crate::hex::default_layout() {
            assert_eq!(
                kind.neighbors_from_layout(&layout),
                kind.neighbors(),
                "translation preserves the ring for {kind:?}"
            );
        }
    }

    #[test]
    fn instantiate_default_preset_covers_all_areas_with_unique_names() {
        let triples = default_preset().instantiate();
        assert_eq!(triples.len(), 7);
        assert_eq!(triples[0].0, Area::Hub);
        assert_eq!(triples[0].1, "The Hub".to_string());
        let kinds: HashSet<Area> = triples.iter().map(|(area, _, _, _)| *area).collect();
        assert_eq!(kinds.len(), 7);
        let names: HashSet<String> = triples.iter().map(|(_, name, _, _)| name.clone()).collect();
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
        assert!(triples[0].2.is_empty(), "no sub-tiles authored");
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
    fn slot_without_sub_tiles_still_deserializes() {
        let json = r#"{
            "id": "test-preset",
            "name": "Test",
            "description": "no sub-tiles anywhere",
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
        assert!(preset.areas.iter().all(|slot| slot.sub_tiles.is_none()));
    }

    #[test]
    fn slot_with_wrong_sub_tile_count_is_rejected() {
        let json = r#"{
            "id": "test-preset",
            "name": "Test",
            "description": "seven sub-tiles instead of six",
            "areas": [
                { "area": "Hub", "q": 0, "r": 0, "sub_tiles": [
                    { "terrain": "Clearing" }, { "terrain": "Clearing" },
                    { "terrain": "Clearing" }, { "terrain": "Clearing" },
                    { "terrain": "Clearing" }, { "terrain": "Clearing" },
                    { "terrain": "Clearing" }
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
        assert!(message.contains("7 sub-tiles"), "message: {message}");
        assert!(message.contains("expected 6"), "message: {message}");
    }

    #[test]
    fn default_preset_authors_six_sub_tiles_per_slot() {
        let preset = default_preset();
        for slot in &preset.areas {
            let subs = slot
                .sub_tiles
                .as_ref()
                .unwrap_or_else(|| panic!("{} must author sub-tiles", slot.area));
            assert_eq!(subs.len(), SubTriangle::COUNT, "{}", slot.area);
            assert!(
                subs.iter()
                    .all(|s| !s.feature.as_deref().unwrap_or("").is_empty())
            );
        }
    }

    #[test]
    fn legacy_sub_hexes_key_still_deserializes() {
        // Rows and snapshots written before the rename carry `sub_hexes`.
        let json = format!(
            r#"{{
            "id": "test-preset",
            "name": "Test",
            "description": "legacy key",
            "areas": [
                {{ "area": "Hub", "q": 0, "r": 0, "sub_hexes": [{}] }},
                {{ "area": "Sector 1", "q": 1, "r": -1 }},
                {{ "area": "Sector 2", "q": 1, "r": 0 }},
                {{ "area": "Sector 3", "q": 0, "r": 1 }},
                {{ "area": "Sector 4", "q": -1, "r": 1 }},
                {{ "area": "Sector 5", "q": -1, "r": 0 }},
                {{ "area": "Sector 6", "q": 0, "r": -1 }}
            ]
        }}"#,
            [r#"{"terrain": "Clearing"}"#; 6].join(",")
        );
        let preset = parse_preset(&json, "test-preset").expect("legacy key must load");
        assert_eq!(
            preset.areas[0]
                .sub_tiles
                .as_ref()
                .expect("alias populated")
                .len(),
            6
        );
    }

    #[test]
    fn sub_tiles_round_trip_through_loader_format() {
        let preset = default_preset();
        let encoded = serde_json::to_string(&preset).expect("serialize");
        assert!(encoded.contains("sub_tiles"), "encoded: {encoded}");
        let decoded: ArenaPreset = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, preset);
        let triples = decoded.instantiate();
        assert!(
            triples
                .iter()
                .all(|(_, _, subs, _)| subs.len() == SubTriangle::COUNT)
        );
    }
}

/// ── SPIKE PoC: seed-driven arena generation ──────────────────────
///
/// Throwaway proof of concept for the arena-generation spike
/// (`docs/superpowers/plans/2026-10-16-arena-generation-spike.md`).
/// Test-only: no production path calls this, nothing is registered or
/// wired. It proves the concept — `generate(seed) -> ArenaPreset` that
/// is deterministic per seed and passes the *existing* preset
/// validation — it is **not** the shipping API (see spike §6).
///
/// Draw order (the versioning contract, spike §2):
///   1. ring orientation: rotation 0..6 + reflection flag (the 12
///      dihedral rearrangements of the Hub+6 wheel),
///   2. terrains: Hub random_safe, six sectors random, then
///      `enforce_balance_constraint` (max 3 Harsh),
///   3. sub-tiles: 7 per slot via `TerrainType::random`,
///   4. names: the existing 14-name bank, seeded.
///
/// Known debt (documented in the spike): one shared RNG, and step 2
/// consumes a variable number of draws. The production module should
/// use per-concern sub-RNGs and add a golden test.
#[cfg(test)]
mod arena_generation_poc {
    use super::*;
    use crate::hex::{Axial, default_layout};
    use rand::RngExt;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    use std::str::FromStr;
    use strum::IntoEnumIterator;
    use world::terrain::{TerrainType, enforce_balance_constraint};

    /// Deterministic-by-seed arena generator. PoC only — see module doc.
    fn generate(seed: u64) -> ArenaPreset {
        let mut rng = SmallRng::seed_from_u64(seed);

        // 1. Ring orientation: sector i (1..=6) lands at ring position
        //    (+/-(i-1) + rotation) mod 6. Hub stays at (0, 0).
        let rotation: i32 = rng.random_range(0..6);
        let reflected = rng.random_bool(0.5);
        let ring: Vec<Axial> = default_layout().iter().skip(1).map(|(_, ax)| *ax).collect();
        let sector_axial = |i: usize| -> Axial {
            let label = (i - 1) as i32;
            let pos = if reflected { -label } else { label };
            ring[((pos + rotation).rem_euclid(6)) as usize]
        };

        // 2. Terrains, with the shared balance rule applied.
        let mut terrains = Vec::with_capacity(7);
        terrains.push(TerrainType::random_safe(&mut rng));
        for _ in 0..6 {
            terrains.push(TerrainType::random(&mut rng));
        }
        enforce_balance_constraint(&mut terrains, &mut rng);

        // 3. Sub-tiles: 6 triangles per slot, all seven areas, drawn
        //    independently of the parent biome (divergence is the point
        //    per areas/presets/README.md).
        let sub_tiles: Vec<Vec<SubTile>> = (0..7)
            .map(|_| {
                (0..SubTriangle::COUNT)
                    .map(|_| SubTile {
                        terrain: TerrainType::random(&mut rng).base,
                        feature: None,
                        flavor: None,
                    })
                    .collect()
            })
            .collect();

        // 4. Seeded names from the existing bank (the public wrapper is
        //    unseeded; the internal fn takes an rng).
        let names = crate::generated_area_names(&mut rng);

        let mut areas = Vec::with_capacity(7);
        for i in 0..7 {
            let (area, q, r) = if i == 0 {
                (Area::Hub, 0, 0)
            } else {
                let ax = sector_axial(i);
                (
                    Area::from_str(&format!("Sector {}", i)).expect("known sector"),
                    ax.q,
                    ax.r,
                )
            };
            areas.push(ArenaSlot {
                area,
                q,
                r,
                name: Some(names[i].clone()),
                sub_tiles: Some(sub_tiles[i].clone()),
            });
        }

        ArenaPreset {
            id: format!("generated-poc-{seed}"),
            name: format!("Generated Arena {seed}"),
            description: "Spike PoC: seed-driven generated arena.".to_string(),
            areas,
        }
    }

    /// The spike's verify test: same seed twice = identical, different
    /// seeds = different.
    #[test]
    fn generated_arena_is_deterministic_for_seed() {
        let a = generate(20261016);
        let b = generate(20261016);
        assert_eq!(a, b, "same seed must reproduce the identical arena");

        let c = generate(42);
        assert_ne!(a, c, "different seeds must produce different arenas");
    }

    /// Every generated arena passes the existing preset validation —
    /// via the real loader path (`parse_preset` enforces
    /// `validate_layout`), not a reimplementation.
    #[test]
    fn every_generated_arena_passes_existing_validation() {
        for seed in [0u64, 1, 7, 42, 999, 20261016, u64::MAX] {
            let preset = generate(seed);
            validate_layout(&preset.areas)
                .unwrap_or_else(|e| panic!("seed {seed} failed layout validation: {e}"));
            let json = serde_json::to_string(&preset).expect("serialize generated preset");
            let parsed = parse_preset(&json, &preset.id)
                .unwrap_or_else(|e| panic!("seed {seed} failed loader validation: {e}"));
            assert_eq!(parsed, preset, "seed {seed} must round-trip");
        }
    }

    /// The generated shape stays hex-compatible with the engine wheel:
    /// all seven areas, Hub first at the origin, unique coords, and the
    /// coordinate ring preserves `Area::neighbors` adjacency (spike §1,
    /// §5: no pathfinding changes needed for this topology).
    #[test]
    fn generated_arena_preserves_wheel_adjacency() {
        use crate::hex::Axial;
        let preset = generate(7);
        assert_eq!(preset.areas[0].area, Area::Hub);
        assert_eq!((preset.areas[0].q, preset.areas[0].r), (0, 0));

        let coord_of = |area: Area| -> Axial {
            let slot = preset
                .areas
                .iter()
                .find(|s| s.area == area)
                .expect("every area present");
            Axial::new(slot.q, slot.r)
        };
        for area in Area::iter() {
            let here = coord_of(area);
            for neighbor in area.neighbors() {
                let there = coord_of(neighbor);
                assert_eq!(
                    here.distance(there),
                    1,
                    "{area} -> {neighbor} must be coordinate-adjacent"
                );
            }
        }
    }
}
