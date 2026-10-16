//! Sub-tile `Location` type: hierarchical adjacency and BFS over the 42
//! triangle cells of the 7-area arena.
//!
//! **This module is the single source of truth for sub-tile hierarchy
//! logic.** Pure geometry — axial math, direction indexing, the
//! [`SubTriangle`] identity — stays in [`crate::hex`]; everything that
//! combines an area with a sub-tile lives here, so a future change to
//! sub-tile topology or costing touches one file.
//!
//! Geometry: each area-hex divides into **6 isosceles triangles** — apex
//! at the parent center, base spanning the parent's full edge in axial
//! direction `i` (index `i` = [`crate::hex::SubTriangle::new`]`(i)`).
//! Six triangles of side `R` tile a hexagon of circumradius `R` exactly:
//! `6 · (√3/4)R² = (3√3/2)R²`, so the parent is covered with no gaps and
//! no dead wedges (the retired 7-sub-tile cluster covered only `7/9` of
//! the parent and forced BFS detours around dead corner wedges).
//!
//! Walkable distance is therefore the BFS hop count over the 42-cell
//! graph — the honest measure. The old scaled axial coordinate
//! (`G = 3·P + s`) existed only because the sub-tile grid sat on its own
//! axial lattice; triangles do not, and there is no coverage gap left
//! for it to under-count, so `scaled_axial`/`scaled_distance` were
//! deleted rather than adapted.

use crate::Area;
use crate::hex::{Axial, SubTriangle, default_layout};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// A concrete position in the arena: an area-hex plus one of its 6
/// sub-tile triangles.
///
/// Serde shape: plain struct. `area` defaults to Hub and `sub` defaults
/// to triangle `0`, so partial payloads hydrate to the hub's default
/// sub-tile. `sub` serializes as the bare index (`0..=5`) — a JSON
/// number, not an object.
///
/// **Legacy sub-tile objects are rejected**: rows written by the old
/// sub-tile layout carried `sub` as `{"q": .., "r": ..}`. Those axial
/// coordinates have no unambiguous triangle mapping (the old center slot
/// no longer exists as a cell), so deserialization fails loudly instead
/// of fabricating a position. (Owner decision: old in-flight rows are
/// not worth preserving.)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Location {
    #[serde(default = "default_area")]
    pub area: Area,
    #[serde(default = "default_sub")]
    pub sub: SubTriangle,
}

fn default_area() -> Area {
    Area::default()
}

fn default_sub() -> SubTriangle {
    SubTriangle::new(0)
}

impl Default for Location {
    /// Hub, sub-tile triangle 0 — matches the serde defaults.
    fn default() -> Self {
        Self::center(Area::Hub)
    }
}

impl Location {
    pub const fn new(area: Area, sub: SubTriangle) -> Self {
        Self { area, sub }
    }

    /// The default sub-tile of an area: triangle 0, whose apex touches
    /// the hex center. (Name kept from the sub-tile era, where the center
    /// slot was literally the hex center; in the triangle layout every
    /// triangle reaches the center, and triangle 0 is the canonical
    /// spawn/default position.)
    pub const fn center(area: Area) -> Self {
        Self {
            area,
            sub: SubTriangle::new(0),
        }
    }
}

/// The 6 sub-tile triangle locations of an area, in
/// [`SubTriangle::index`] order (`0..=6`).
pub fn sub_slots(area: Area) -> impl Iterator<Item = Location> {
    (0..SubTriangle::COUNT as u8).map(move |i| Location {
        area,
        sub: SubTriangle::new(i),
    })
}

/// All 42 occupied sub-tile cells of the arena (7 areas × 6 triangles),
/// area order per [`default_layout`], triangle order per
/// [`SubTriangle::index`].
pub fn all_locations() -> impl Iterator<Item = Location> {
    default_layout()
        .into_iter()
        .flat_map(|(area, _)| sub_slots(area))
}

/// Axial coordinate of an area-hex in the top-level layout.
pub fn area_axial(area: Area) -> Axial {
    default_layout()
        .iter()
        .find(|(a, _)| *a == area)
        .map(|(_, ax)| *ax)
        .expect("every Area variant appears in default_layout")
}

/// The area whose layout hex sits at `axial`, or `None` when the coord
/// is outside the 7-hex cluster.
pub fn area_at(axial: Axial) -> Option<Area> {
    default_layout()
        .iter()
        .find(|(_, ax)| *ax == axial)
        .map(|(a, _)| *a)
}

/// Whether the location is one of the 42 occupied cells (its sub index
/// is in `0..6`). Every `Area` variant is part of the layout, so only
/// the sub can make a location unoccupied.
pub fn is_occupied(location: Location) -> bool {
    location.sub.index() < SubTriangle::COUNT as u8
}

/// Sub-tile neighbors of `location`, indexed by the parent's axial
/// direction slot `i` (the six directions of [`Axial::neighbors`]).
///
/// For an occupied triangle `t = sub.index()`, exactly three slots can
/// be set — `t`, `(t+1) % 6`, and `(t+5) % 6`:
///
/// - **slot `t` (own base direction) — cross-parent door:** triangle
///   `(t+3) % 6` of the parent one step in direction `t`. Base-to-base
///   across the shared full edge: `t`'s base and `(t+3) % 6`'s base are
///   the same physical edge, so each of the 7 parents' 6 edges carries
///   exactly one door pair (12 undirected doors over the cluster).
///   `None` when that parent neighbor is outside the cluster.
/// - **slot `(t+1) % 6` — within-parent:** ring neighbor triangle
///   `(t+1) % 6`, across the radial edge they share. Always occupied.
/// - **slot `(t+5) % 6` — within-parent:** ring neighbor triangle
///   `(t+5) % 6`, across the other radial edge. Always occupied.
/// - **every other slot:** `None`.
///
/// The rule is symmetric: if `b` appears in `sub_neighbors(a)`, then
/// `a` appears in `sub_neighbors(b)`. Each triangle has at most 3
/// neighbors (2 within + 1 cross); map-edge triangles lose only the
/// cross one.
///
/// An unoccupied sub (`index >= 6`) yields all `None`.
pub fn sub_neighbors(location: Location) -> [Option<Location>; 6] {
    let mut out: [Option<Location>; 6] = [None; 6];
    if !is_occupied(location) {
        return out;
    }
    let t = location.sub.base_direction();
    let ring = |i: usize| Location {
        area: location.area,
        sub: SubTriangle::new(i as u8),
    };
    // Within-parent radial edges: triangle t shares one edge with each
    // of its ring-neighbors t±1 (mod 6).
    out[(t + 1) % 6] = Some(ring((t + 1) % 6));
    out[(t + 5) % 6] = Some(ring((t + 5) % 6));
    // Cross-parent door: base-to-base across the full edge in direction t.
    out[t] = area_at(area_axial(location.area).neighbors()[t]).map(|area| Location {
        area,
        sub: SubTriangle::new(((t + 3) % 6) as u8),
    });
    out
}

/// BFS distance in sub-hops over the 42-cell occupied graph. `None`
/// when either location is not an occupied cell. Equal locations give
/// `Some(0)`.
pub fn distance(from: Location, to: Location) -> Option<u32> {
    if !is_occupied(from) || !is_occupied(to) {
        return None;
    }
    let mut dist: HashMap<Location, u32> = HashMap::new();
    let mut queue = VecDeque::new();
    dist.insert(from, 0);
    queue.push_back(from);
    while let Some(current) = queue.pop_front() {
        let d = dist[&current];
        if current == to {
            return Some(d);
        }
        for neighbor in sub_neighbors(current).into_iter().flatten() {
            if let std::collections::hash_map::Entry::Vacant(e) = dist.entry(neighbor) {
                e.insert(d + 1);
                queue.push_back(neighbor);
            }
        }
    }
    None
}

/// A shortest sub-hop path over the 42-cell occupied graph (BFS). The
/// returned path starts with `from`, ends with `to`, and consecutive
/// entries are mutual [`sub_neighbors`]; hop count = `len - 1` =
/// [`distance`]. `None` when either location is not an occupied cell.
pub fn shortest_path(from: Location, to: Location) -> Option<Vec<Location>> {
    if !is_occupied(from) || !is_occupied(to) {
        return None;
    }
    if from == to {
        return Some(vec![from]);
    }
    let mut came_from: HashMap<Location, Location> = HashMap::new();
    let mut visited: std::collections::HashSet<Location> = std::collections::HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(from);
    queue.push_back(from);
    while let Some(current) = queue.pop_front() {
        for neighbor in sub_neighbors(current).into_iter().flatten() {
            if visited.insert(neighbor) {
                came_from.insert(neighbor, current);
                queue.push_back(neighbor);
            }
        }
    }
    let mut path = vec![to];
    let mut current = to;
    while current != from {
        current = came_from[&current];
        path.push(current);
    }
    path.reverse();
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn hub(index: u8) -> Location {
        Location::new(Area::Hub, SubTriangle::new(index))
    }

    fn at(area: Area, index: u8) -> Location {
        Location::new(area, SubTriangle::new(index))
    }

    // ---- serde contract ----------------------------------------------

    #[test]
    fn serde_round_trips_plain_struct_shape() {
        let loc = at(Area::Sector2, 2);
        let json = serde_json::to_value(loc).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"area": "Sector 2", "sub": 2}),
            "sub serializes as the bare triangle index"
        );
        let back: Location = serde_json::from_value(json).unwrap();
        assert_eq!(back, loc);
    }

    #[test]
    fn serde_missing_sub_defaults_to_triangle_zero() {
        let loc: Location =
            serde_json::from_value(serde_json::json!({"area": "Sector 1"})).unwrap();
        assert_eq!(loc, at(Area::Sector1, 0));
    }

    #[test]
    fn serde_empty_payload_defaults_to_hub_center() {
        let loc: Location = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(loc, Location::default());
        assert_eq!(loc, Location::center(Area::Hub));
    }

    /// Legacy sub-tile rows carried `sub` as `{"q": .., "r": ..}`. That
    /// shape has no unambiguous triangle mapping, so hydration rejects
    /// it loudly instead of inventing a position.
    #[test]
    fn serde_legacy_sub_object_is_rejected() {
        let err = serde_json::from_value::<Location>(
            serde_json::json!({"area": "Hub", "sub": {"q": 1, "r": -1}}),
        )
        .expect_err("legacy sub-tile object must not hydrate");
        let message = err.to_string();
        assert!(
            message.contains("u8"),
            "error should name the expected triangle index type: {message}"
        );
    }

    // ---- structure ----------------------------------------------------

    #[test]
    fn sub_slots_iterates_in_triangle_index_order() {
        let slots: Vec<Location> = sub_slots(Area::Sector3).collect();
        assert_eq!(slots.len(), SubTriangle::COUNT);
        for (i, slot) in slots.iter().enumerate() {
            assert_eq!(*slot, at(Area::Sector3, i as u8));
        }
    }

    #[test]
    fn all_locations_covers_42_unique_cells() {
        let all: Vec<Location> = all_locations().collect();
        let set: HashSet<Location> = all.iter().copied().collect();
        assert_eq!(all.len(), 42);
        assert_eq!(set.len(), 42, "duplicate cells in all_locations()");
        assert!(all.iter().all(|l| is_occupied(*l)));
    }

    // ---- tiling math --------------------------------------------------

    /// The6 sub-tiles tile the parent exactly — the golden zero-gap
    /// fact. Parent hex: circumradius `R`, area `(3√3/2)R²`. Triangle
    /// `i`: apex at the center, base = the chord between the corners at
    /// direction angle `i` ± 30° — a 60° chord of radius `R`, i.e.
    /// length `R` (the FULL parent edge) — so it is equilateral with
    /// side `R`, area `(√3/4)R²`. Six of those sum to the hex area:
    /// coverage is exactly 1, no gaps, no overlaps.
    #[test]
    fn six_triangles_tile_parent_hex_exactly() {
        let r = 90.0_f64;
        // Direction angle of each base edge, derived from the SAME
        // axial direction indexing Axial::neighbors uses.
        let dirs = Axial::new(0, 0).neighbors();
        let mut total = 0.0_f64;
        for (i, dir) in dirs.iter().enumerate() {
            let (x, y) = dir.to_pixel(1.0);
            let angle = y.atan2(x); // base-midpoint direction of triangle i
            let corner = |deg: f64| {
                let a = angle + deg.to_radians();
                (r * a.cos(), r * a.sin())
            };
            let c1 = corner(-30.0);
            let c2 = corner(30.0);
            let side = ((c1.0 - c2.0).powi(2) + (c1.1 - c2.1).powi(2)).sqrt();
            assert!(
                (side - r).abs() < 1e-9,
                "triangle {i} base {side} != full edge {r}"
            );
            // Shoelace with the apex at the origin.
            let area = 0.5 * (c1.0 * c2.1 - c2.0 * c1.1).abs();
            assert!(
                (area - 3.0_f64.sqrt() / 4.0 * r * r).abs() < 1e-9,
                "triangle {i} area {area} != equilateral (√3/4)R²"
            );
            total += area;
        }
        let hex_area = 3.0 * 3.0_f64.sqrt() / 2.0 * r * r;
        assert!(
            (total - hex_area).abs() < 1e-9,
            "coverage {total} vs {hex_area}"
        );
        let coverage = total / hex_area;
        assert!((coverage - 1.0).abs() < 1e-12, "coverage {coverage}");
    }

    // ---- adjacency ----------------------------------------------------

    #[test]
    fn neighbor_slots_are_only_own_and_adjacent_directions() {
        for loc in all_locations() {
            let t = loc.sub.base_direction();
            let n = sub_neighbors(loc);
            for (i, neighbor) in n.iter().enumerate() {
                let adjacent = i == t || i == (t + 1) % 6 || i == (t + 5) % 6;
                if adjacent {
                    // Ring slots always exist; the own-base slot may be
                    // None only at the cluster edge.
                    if i != t {
                        assert!(neighbor.is_some(), "{loc:?} ring slot {i} empty");
                    }
                } else {
                    assert!(neighbor.is_none(), "{loc:?} slot {i} should be empty");
                }
            }
        }
    }

    #[test]
    fn within_parent_neighbors_are_the_adjacent_ring_triangles() {
        for (area, _) in default_layout() {
            for t in 0..SubTriangle::COUNT {
                let n = sub_neighbors(at(area, t as u8));
                assert_eq!(n[(t + 1) % 6], Some(at(area, ((t + 1) % 6) as u8)));
                assert_eq!(n[(t + 5) % 6], Some(at(area, ((t + 5) % 6) as u8)));
            }
        }
    }

    #[test]
    fn cross_parent_door_is_base_to_base_across_shared_edge() {
        for loc in all_locations() {
            let t = loc.sub.base_direction();
            let n = sub_neighbors(loc);
            let parent = area_axial(loc.area);
            let expected = area_at(parent.neighbors()[t]).map(|area| at(area, ((t + 3) % 6) as u8));
            assert_eq!(n[t], expected, "{loc:?} cross slot {t}");
            if let Some(other) = n[t] {
                // Base-to-base: the door lands on the triangle three
                // steps away in the neighbor, whose own base faces back.
                assert_eq!(other.sub, SubTriangle::new(((t + 3) % 6) as u8));
                // Mutuality: the reverse door leads back.
                assert!(
                    sub_neighbors(other).contains(&Some(loc)),
                    "{loc:?} <-> {other:?} door not mutual"
                );
            }
        }
    }

    #[test]
    fn cross_parent_pairs_are_exactly_twelve_full_edge_doors() {
        let door_key = |a: Location, b: Location| {
            let x = (a.area, a.sub.index());
            let y = (b.area, b.sub.index());
            if x <= y { (x, y) } else { (y, x) }
        };
        let mut actual: HashSet<((Area, u8), (Area, u8))> = HashSet::new();
        for loc in all_locations() {
            for neighbor in sub_neighbors(loc).into_iter().flatten() {
                if neighbor.area == loc.area {
                    continue;
                }
                actual.insert(door_key(loc, neighbor));
            }
        }
        assert_eq!(actual.len(), 12, "expected exactly 12 cross-parent doors");

        // Independent enumeration straight from the layout: every big-hex
        // edge whose direction has a parent neighbor carries one door.
        let mut expected = HashSet::new();
        for (area, ax) in default_layout() {
            for (i, neighbor_ax) in ax.neighbors().into_iter().enumerate() {
                if let Some(nb) = area_at(neighbor_ax) {
                    expected.insert(door_key(at(area, i as u8), at(nb, ((i + 3) % 6) as u8)));
                }
            }
        }
        assert_eq!(actual, expected, "door set != layout-edge enumeration");
    }

    #[test]
    fn cross_parent_doors_agree_with_area_neighbors() {
        for loc in all_locations() {
            for neighbor in sub_neighbors(loc).into_iter().flatten() {
                if neighbor.area != loc.area {
                    assert!(
                        loc.area.neighbors().contains(&neighbor.area),
                        "{:?} door to {:?} not in Area::neighbors",
                        loc,
                        neighbor.area
                    );
                }
            }
        }
    }

    #[test]
    fn neighbors_are_never_duplicated() {
        for loc in all_locations() {
            let n = sub_neighbors(loc);
            let occupied: Vec<Location> = n.into_iter().flatten().collect();
            let set: HashSet<Location> = occupied.iter().copied().collect();
            assert_eq!(set.len(), occupied.len(), "duplicates for {loc:?}");
        }
    }

    /// Total edge count of the42-cell graph: each parent contributes a
    /// 6-cycle of radial edges (6 × 7 = 42 undirected), plus 12
    /// cross-parent doors → 54 undirected = 108 directed.
    #[test]
    fn edge_count_is_84_within_and_24_cross_directed() {
        let mut within_directed = 0;
        let mut cross_directed = 0;
        for loc in all_locations() {
            for neighbor in sub_neighbors(loc).into_iter().flatten() {
                if neighbor.area == loc.area {
                    within_directed += 1;
                } else {
                    cross_directed += 1;
                }
            }
        }
        assert_eq!(within_directed, 84, "42 within-parent undirected pairs");
        assert_eq!(cross_directed, 24, "12 cross-parent undirected doors");
    }

    // ---- BFS sanity ---------------------------------------------------

    #[test]
    fn center_out_distances_within_parent() {
        // From the Hub default sub-tile (triangle 0): the two adjacent
        // ring triangles at 1 hop, the next pair at 2, the opposite
        // triangle at 3 — the6-cycle structure with no shortcuts.
        assert_eq!(distance(Location::center(Area::Hub), hub(1)), Some(1));
        assert_eq!(distance(Location::center(Area::Hub), hub(5)), Some(1));
        assert_eq!(distance(Location::center(Area::Hub), hub(2)), Some(2));
        assert_eq!(distance(Location::center(Area::Hub), hub(4)), Some(2));
        assert_eq!(distance(Location::center(Area::Hub), hub(3)), Some(3));
    }

    /// Full-ring pin: within-parent distance is exactly the distance on
    /// the triangle ring — no shorter path detours through a neighbor
    /// parent and back, because every door is a fixed triangle-to-
    /// triangle pair.
    #[test]
    fn within_parent_distance_is_the_ring_cycle_distance() {
        for (area, _) in default_layout() {
            for i in 0..SubTriangle::COUNT {
                for j in 0..SubTriangle::COUNT {
                    let straight = (i as i32 - j as i32).abs();
                    let want = straight.min(SubTriangle::COUNT as i32 - straight) as u32;
                    assert_eq!(
                        distance(at(area, i as u8), at(area, j as u8)),
                        Some(want),
                        "{area:?} triangle {i} -> {j}"
                    );
                }
            }
        }
    }

    #[test]
    fn cross_parent_door_is_one_hop() {
        let from = hub(0);
        let door = at(Area::Sector1, 3);
        assert_eq!(sub_neighbors(from)[0], Some(door));
        assert_eq!(distance(from, door), Some(1));
        assert_eq!(distance(door, from), Some(1));
    }

    /// A cross-hex path: Hub triangle 0 to Sector 1 triangle 0 must
    /// cross the shared full edge (1 hop to the door triangle) and then
    /// walk the Sector 1 ring (3 more hops) —4 hops total.
    #[test]
    fn hub_center_to_sector1_triangle0_is_four_hops_via_door() {
        let from = Location::center(Area::Hub);
        let to = at(Area::Sector1, 0);
        assert_eq!(distance(from, to), Some(4));
        let path = shortest_path(from, to).unwrap();
        assert_eq!(path.len(), 5, "path: {path:?}");
        assert_eq!(path.first(), Some(&from));
        assert_eq!(path[1], at(Area::Sector1, 3), "must enter through the door");
        assert_eq!(path.last(), Some(&to));
    }

    #[test]
    fn shortest_path_steps_are_mutual_neighbors() {
        let from = Location::center(Area::Hub);
        let to = Location::center(Area::Sector5);
        let path = shortest_path(from, to).unwrap();
        assert_eq!(path.first(), Some(&from));
        assert_eq!(path.last(), Some(&to));
        for window in path.windows(2) {
            let (a, b) = (window[0], window[1]);
            assert!(sub_neighbors(a).contains(&Some(b)), "{a:?} !-> {b:?}");
            assert!(sub_neighbors(b).contains(&Some(a)), "{b:?} !-> {a:?}");
        }
        assert_eq!(path.len() as u32 - 1, distance(from, to).unwrap());
    }

    #[test]
    fn all_forty_two_cells_reachable_from_hub_center() {
        let from = Location::center(Area::Hub);
        let mut reached = 0;
        for target in all_locations() {
            let d = distance(from, target).expect("every cell reachable");
            let path = shortest_path(from, target).unwrap();
            assert_eq!(path.len() as u32 - 1, d);
            reached += 1;
        }
        assert_eq!(reached, 42);
    }

    #[test]
    fn distance_is_symmetric_and_matches_path_length() {
        for a in all_locations() {
            for b in all_locations() {
                let ab = distance(a, b).unwrap();
                let ba = distance(b, a).unwrap();
                assert_eq!(ab, ba, "asymmetric for {a:?} <-> {b:?}");
                let path = shortest_path(a, b).unwrap();
                assert_eq!(path.len() as u32 - 1, ab);
            }
        }
    }

    #[test]
    fn unoccupied_sub_triangles_have_no_distance_or_path() {
        let dead = Location::new(Area::Hub, SubTriangle::new(6));
        assert!(!is_occupied(dead));
        assert_eq!(distance(hub(0), dead), None);
        assert_eq!(shortest_path(hub(0), dead), None);
        assert_eq!(distance(dead, hub(0)), None);
        assert_eq!(shortest_path(dead, hub(0)), None);
        // An unoccupied sub has no neighbors at all.
        assert_eq!(sub_neighbors(dead), [None; 6]);
    }
}
