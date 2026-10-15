//! Sub-hex `Location` type: hierarchical adjacency and BFS over the 49
//! sub-hex cells of the 7-area arena.
//!
//! **This module is the single source of truth for sub-hex hierarchy
//! logic** (spike §7). Pure geometry — axial math, slot constants, pixel
//! layout — stays in [`crate::hex`]; everything that combines an area
//! with a sub-slot lives here, so a future change to sub-hex topology or
//! costing touches one file.
//!
//! Math source: `docs/superpowers/plans/2026-10-15-subhex-interaction-spike.md`
//! §2 — the scaled global coordinate `G = 3·P + s` (P = parent axial,
//! s = slot), the cross-parent facing rule (sub `s` of parent `P` touches
//! sub `-s` of parent `P + s`, and nothing else crosses a big-hex edge),
//! and the lower-bound caveat: scaled axial distance underestimates
//! walkable distance because corner wedges are dead space (7 sub-hexes of
//! radius `R/3` cover `7/9` of the parent hex).

use crate::Area;
use crate::hex::{Axial, SUB_SLOTS, SubAxial, default_layout};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// A concrete position in the arena: an area-hex plus one of its 7
/// sub-hex slots.
///
/// Serde shape (spike §1): plain struct. `area` defaults to Hub and `sub`
/// defaults to the center slot, so partial payloads hydrate to the hub
/// center.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Location {
    #[serde(default = "default_area")]
    pub area: Area,
    #[serde(default = "default_sub")]
    pub sub: SubAxial,
}

fn default_area() -> Area {
    Area::default()
}

fn default_sub() -> SubAxial {
    SUB_SLOTS[0]
}

impl Default for Location {
    /// Hub center — matches the serde defaults.
    fn default() -> Self {
        Self::center(Area::Hub)
    }
}

impl Location {
    pub const fn new(area: Area, sub: SubAxial) -> Self {
        Self { area, sub }
    }

    /// The center sub-hex (`SUB_SLOTS[0]`) of an area.
    pub const fn center(area: Area) -> Self {
        Self {
            area,
            sub: SUB_SLOTS[0],
        }
    }
}

/// The 7 sub-hex locations of an area, in [`SUB_SLOTS`] order (center
/// first).
pub fn sub_slots(area: Area) -> impl Iterator<Item = Location> {
    SUB_SLOTS.into_iter().map(move |sub| Location { area, sub })
}

/// All 49 occupied sub-hex cells of the arena (7 areas × 7 slots), area
/// order per [`default_layout`], slot order per [`SUB_SLOTS`].
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

/// Scaled global sub-hex coordinate `G = 3·P + s` (spike §2). Because
/// `to_pixel` is linear and `SUB_SIZE_RATIO = 1/3`, `G` lives on the
/// same axial grid at sub-hex size and reproduces the true pixel
/// geometry exactly.
pub fn scaled_axial(location: Location) -> Axial {
    let parent = area_axial(location.area);
    Axial::new(parent.q * 3 + location.sub.q, parent.r * 3 + location.sub.r)
}

/// Scaled axial distance between two locations — a **lower bound** on
/// walkable (BFS) distance, not equal to it: dead corner wedges between
/// big-hex edges can force a longer occupied-cell path (spike §2).
pub fn scaled_distance(from: Location, to: Location) -> i32 {
    scaled_axial(from).distance(scaled_axial(to))
}

/// Whether the location is one of the 49 occupied cells (its sub is a
/// valid slot). Every `Area` variant is part of the layout, so only the
/// sub can make a location unoccupied.
pub fn is_occupied(location: Location) -> bool {
    SUB_SLOTS.contains(&location.sub)
}

/// The six unit steps, clockwise from top-right — the same order as
/// [`Axial::neighbors`] and the ring slots `SUB_SLOTS[1..=6]`.
fn direction(step: usize) -> SubAxial {
    SUB_SLOTS[step + 1]
}

/// Sub-hex neighbors of `location`, indexed by direction (clockwise from
/// top-right, per [`direction`]).
///
/// Two rules only (spike §2):
///
/// - **Within-parent:** `sub + step` is a slot ⇒ same-parent neighbor.
/// - **Cross-parent (facing rule):** ring sub `s` of parent `P` touches
///   sub `-s` of parent `P + s` — exactly one sub of exactly one
///   neighbor, the single "door" on that big-hex edge.
///
/// `None` marks a direction with no occupied neighbor: corner wedges
/// inside the parent (dead space, `7/9` coverage) and parent neighbors
/// outside the 7-hex cluster.
pub fn sub_neighbors(location: Location) -> [Option<Location>; 6] {
    let parent = area_axial(location.area);
    let mut out: [Option<Location>; 6] = [None; 6];
    for (step, slot) in out.iter_mut().enumerate() {
        let d = direction(step);
        let within = SubAxial::new(location.sub.q + d.q, location.sub.r + d.r);
        *slot = if SUB_SLOTS.contains(&within) {
            Some(Location {
                area: location.area,
                sub: within,
            })
        } else if location.sub == d {
            area_at(Axial::new(parent.q + d.q, parent.r + d.r)).map(|area| Location {
                area,
                sub: SubAxial::new(-d.q, -d.r),
            })
        } else {
            None
        };
    }
    out
}

/// BFS distance in sub-hops over the 49-cell occupied graph. `None` when
/// either location is not an occupied cell. Equal locations give `Some(0)`.
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

/// A shortest sub-hop path over the 49-cell occupied graph (BFS). The
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
    use crate::hex::SUB_SIZE_RATIO;
    use std::collections::HashSet;

    fn hub(sub: (i32, i32)) -> Location {
        Location::new(Area::Hub, SubAxial::new(sub.0, sub.1))
    }

    // ---- serde contract (spike §1) ------------------------------------

    #[test]
    fn serde_round_trips_plain_struct_shape() {
        let loc = Location::new(Area::Sector2, SubAxial::new(1, 0));
        let json = serde_json::to_value(loc).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"area": "Sector 2", "sub": {"q": 1, "r": 0}})
        );
        let back: Location = serde_json::from_value(json).unwrap();
        assert_eq!(back, loc);
    }

    #[test]
    fn serde_old_shape_defaults_sub_to_center() {
        let loc: Location =
            serde_json::from_value(serde_json::json!({"area": "Sector 1"})).unwrap();
        assert_eq!(loc, Location::new(Area::Sector1, SubAxial::new(0, 0)));
    }

    #[test]
    fn serde_empty_payload_defaults_to_hub_center() {
        let loc: Location = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(loc, Location::default());
        assert_eq!(loc, Location::center(Area::Hub));
    }

    // ---- structure ----------------------------------------------------

    #[test]
    fn sub_slots_iterates_in_slot_order_center_first() {
        let slots: Vec<Location> = sub_slots(Area::Sector3).collect();
        assert_eq!(slots.len(), 7);
        assert_eq!(slots[0], Location::center(Area::Sector3));
        for (i, slot) in SUB_SLOTS.iter().enumerate() {
            assert_eq!(slots[i], Location::new(Area::Sector3, *slot));
        }
    }

    #[test]
    fn all_locations_covers_49_unique_cells() {
        let all: Vec<Location> = all_locations().collect();
        let set: HashSet<Location> = all.iter().copied().collect();
        assert_eq!(all.len(), 49);
        assert_eq!(set.len(), 49, "duplicate cells in all_locations()");
        assert!(all.iter().all(|l| is_occupied(*l)));
    }

    // ---- adjacency ----------------------------------------------------

    #[test]
    fn neighbors_of_center_are_the_six_ring_slots_in_direction_order() {
        let n = sub_neighbors(Location::center(Area::Hub));
        for (step, slot) in SUB_SLOTS[1..].iter().enumerate() {
            assert_eq!(n[step], Some(Location::new(Area::Hub, *slot)));
        }
    }

    #[test]
    fn neighbors_of_hub_ring_slot_pin_facing_rule_and_dead_wedges() {
        // Hub ring slot (1,-1): door to Sector1 slot (-1,1) in the NE
        // step; the E step would cross the dead-space midpoint (2,-1)
        // (the spike's walkable-vs-scaled mismatch case); the NW step is
        // the dead corner wedge (1,-2); the rest are within-parent.
        assert_eq!(
            sub_neighbors(hub((1, -1))),
            [
                Some(Location::new(Area::Sector1, SubAxial::new(-1, 1))),
                None,
                Some(hub((1, 0))),
                Some(hub((0, 0))),
                Some(hub((0, -1))),
                None,
            ]
        );
    }

    #[test]
    fn outer_ring_slots_have_only_within_parent_neighbors() {
        // Sector1's outward ring slots point outside the 7-hex cluster:
        // no cross-parent door exists in those directions.
        let outer = sub_neighbors(Location::new(Area::Sector1, SubAxial::new(1, -1)));
        assert_eq!(outer.into_iter().flatten().count(), 3);
        assert_eq!(outer[0], None, "(2,-2) is outside the cluster");
        // Sector1's three door slots do cross: toward Hub, Sector2, Sector6.
        for door in [
            SubAxial::new(-1, 1),
            SubAxial::new(0, 1),
            SubAxial::new(-1, 0),
        ] {
            let n = sub_neighbors(Location::new(Area::Sector1, door));
            assert!(
                n.into_iter().flatten().any(|loc| loc.area != Area::Sector1),
                "slot {door:?} should have a cross-parent door"
            );
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

    #[test]
    fn edges_total_ninety_six() {
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
        // 84 undirected within-parent pairs + 12 cross-parent doors = 96.
        assert_eq!(within_directed, 168);
        assert_eq!(cross_directed, 24);
    }

    #[test]
    fn cross_parent_pairs_are_exactly_twelve_facing_doors() {
        let mut cross: HashSet<((i32, i32), (i32, i32))> = HashSet::new();
        for loc in all_locations() {
            for neighbor in sub_neighbors(loc).into_iter().flatten() {
                if neighbor.area == loc.area {
                    continue;
                }
                // Undirected key via scaled coords.
                let (x, y) = (scaled_axial(loc), scaled_axial(neighbor));
                let key = if (x.q, x.r) <= (y.q, y.r) {
                    ((x.q, x.r), (y.q, y.r))
                } else {
                    ((y.q, y.r), (x.q, x.r))
                };
                cross.insert(key);
                // Facing rule: subs negate, parents step by the sub.
                assert_eq!(
                    neighbor.sub,
                    SubAxial::new(-loc.sub.q, -loc.sub.r),
                    "{loc:?} cross neighbor not a facing pair"
                );
                assert_eq!(
                    area_at(Axial::new(
                        area_axial(loc.area).q + loc.sub.q,
                        area_axial(loc.area).r + loc.sub.r,
                    )),
                    Some(neighbor.area),
                    "{loc:?} cross neighbor parent not layout-adjacent"
                );
            }
        }
        assert_eq!(cross.len(), 12, "expected exactly 12 cross-parent doors");

        // Independent enumeration: 6 Hub-edge doors + 6 ring-ring doors,
        // built from layout geometry (parent axial differences).
        let layout = default_layout();
        let mut expected: HashSet<((i32, i32), (i32, i32))> = HashSet::new();
        let door_key = |a: Location, b: Location| {
            let (x, y) = (scaled_axial(a), scaled_axial(b));
            if (x.q, x.r) <= (y.q, y.r) {
                ((x.q, x.r), (y.q, y.r))
            } else {
                ((y.q, y.r), (x.q, x.r))
            }
        };
        for (i, (area, ax)) in layout.iter().enumerate().skip(1) {
            // Hub-edge door: Hub slot toward this sector <-> sector slot toward Hub.
            expected.insert(door_key(
                Location::new(Area::Hub, SubAxial::new(ax.q, ax.r)),
                Location::new(*area, SubAxial::new(-ax.q, -ax.r)),
            ));
            // Ring-ring doors with the next sector in layout order
            // (Sector i <-> Sector i+1, wrap 6 -> 1).
            let (next_area, next_ax) = if i == 6 {
                (layout[1].0, layout[1].1)
            } else {
                (layout[i + 1].0, layout[i + 1].1)
            };
            let step = Axial::new(next_ax.q - ax.q, next_ax.r - ax.r);
            if step.distance(Axial::new(0, 0)) == 1 {
                expected.insert(door_key(
                    Location::new(*area, SubAxial::new(step.q, step.r)),
                    Location::new(next_area, SubAxial::new(-step.q, -step.r)),
                ));
            }
        }
        assert_eq!(cross, expected, "door set != Hub edges + ring-ring edges");
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

    // ---- scaled math (spike §2) ---------------------------------------

    #[test]
    fn sub_cluster_covers_seven_ninths_of_parent_area() {
        // Spike §2: 7 sub-hexes of radius r = R/3 tile 7·(r/R)² = 7/9 of
        // the parent hex area; the remaining 2/9 are the dead corner wedges.
        assert_eq!(SUB_SIZE_RATIO, 1.0 / 3.0);
        let coverage = 7.0 * SUB_SIZE_RATIO * SUB_SIZE_RATIO;
        assert!((coverage - 7.0 / 9.0).abs() < 1e-12, "coverage {coverage}");
    }

    #[test]
    fn hub_ring_to_sector2_ring_scaled_2_walkable_3() {
        // The spike's mismatch case: the would-be midpoint (2,-1) is
        // dead space, so BFS must detour.
        let from = hub((1, -1));
        let to = Location::new(Area::Sector2, SubAxial::new(0, -1));
        assert_eq!(scaled_distance(from, to), 2);
        assert_eq!(distance(from, to), Some(3));
    }

    #[test]
    fn scaled_distance_is_lower_bound_and_disagrees_on_336_pairs() {
        let mut ordered = 0;
        let mut disagreements = 0;
        for a in all_locations() {
            for b in all_locations() {
                if a == b {
                    continue;
                }
                ordered += 1;
                let scaled = scaled_distance(a, b);
                let walkable = distance(a, b).expect("graph fully connected");
                assert!(
                    walkable as i32 >= scaled,
                    "walkable {walkable} < scaled {scaled} for {a:?} -> {b:?}"
                );
                if walkable as i32 != scaled {
                    disagreements += 1;
                }
            }
        }
        assert_eq!(ordered, 2352);
        assert_eq!(disagreements, 336, "spike §2 pins 336 of 2352");
    }

    // ---- BFS sanity ---------------------------------------------------

    #[test]
    fn center_to_ring_is_one_hop_within_parent() {
        assert_eq!(distance(Location::center(Area::Hub), hub((1, -1))), Some(1));
    }

    #[test]
    fn opposite_ring_slots_are_two_hops_within_parent() {
        assert_eq!(distance(hub((1, -1)), hub((-1, 1))), Some(2));
    }

    #[test]
    fn hub_center_to_sector1_center_is_three_hops_via_door() {
        let from = Location::center(Area::Hub);
        let to = Location::center(Area::Sector1);
        assert_eq!(distance(from, to), Some(3));
        assert_eq!(
            shortest_path(from, to).unwrap(),
            vec![
                Location::center(Area::Hub),
                hub((1, -1)),
                Location::new(Area::Sector1, SubAxial::new(-1, 1)),
                Location::center(Area::Sector1),
            ]
        );
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
    fn all_forty_nine_cells_reachable_from_hub_center() {
        let from = Location::center(Area::Hub);
        let mut reached = 0;
        for target in all_locations() {
            let d = distance(from, target).expect("every cell reachable");
            let path = shortest_path(from, target).unwrap();
            assert_eq!(path.len() as u32 - 1, d);
            reached += 1;
        }
        assert_eq!(reached, 49);
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
    fn unoccupied_locations_have_no_distance_or_path() {
        let dead = Location::new(Area::Hub, SubAxial::new(2, -1));
        assert!(!is_occupied(dead));
        assert_eq!(distance(hub((0, 0)), dead), None);
        assert_eq!(shortest_path(hub((0, 0)), dead), None);
        assert_eq!(distance(dead, hub((0, 0))), None);
        assert_eq!(shortest_path(dead, hub((0, 0))), None);
    }
}
