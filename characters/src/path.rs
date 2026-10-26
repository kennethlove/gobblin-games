//! AreaId-level pathfinding: builds a graph over the v1 7-area hex topology
//! and exposes a path-planning helper for characters.
//!
//! Edge stamina cost is a composite per the design decision (8pq Q2):
//! `crossing_stamina_cost + harshness_penalty`.
//! - `crossing_stamina_cost`: per-character, per-terrain — half the
//!   source area's standard move cost plus half the destination's,
//!   each rounded up (same crossing price the sub-hop engine charges)
//! - `harshness_penalty`: 0/10/20 for Mild/Moderate/Harsh
//!   (closed_penalty deleted with area closure — q7k: hazards never
//!   block or penalise pathing; avoidance is soft, in the brain.)

use super::actions::Action;
use super::{Character, calculate_stamina_cost, crossing_stamina_cost};
use areas::location::Location;
use areas::{AreaDetails, AreaId};
use std::collections::HashMap;
use world::pathfinding::{Graph, astar};
use world::terrain::Harshness;

/// Snapshot of the area graph from one character's perspective at one
/// moment in time. Built per planning call — do not cache across cycles.
pub struct AreaGraph<'a> {
    /// Node set = the areas this game actually has (`area_details`,
    /// zkp.5): N-hex layouts contribute exactly their own slots.
    pub areas: Vec<AreaId>,
    /// Character doing the planning. Used for stamina-cost computation.
    pub character: &'a Character,
    /// Per-area details lookup (terrain, items, etc.).
    pub details: HashMap<AreaId, &'a AreaDetails>,
}

impl<'a> AreaGraph<'a> {
    pub fn new(areas: &'a [AreaDetails], character: &'a Character) -> Self {
        let mut details: HashMap<AreaId, &AreaDetails> = HashMap::new();
        for ad in areas {
            if let Some(a) = ad.area {
                details.insert(a, ad);
            }
        }
        Self {
            areas: areas.iter().filter_map(|ad| ad.area).collect(),
            character,
            details,
        }
    }

    /// The standard move cost of entering `area` from this
    /// perspective: per-character, per-terrain; `20` (the base move
    /// cost) when no details are known.
    fn standard_move_cost(&self, area: AreaId) -> u32 {
        let detail = self.details.get(&area).copied();
        if let Some(ad) = detail {
            calculate_stamina_cost(
                &Action::Move(Some(Location::center(area))),
                &ad.terrain,
                self.character,
            )
        } else {
            // No detail known — fall back to base move cost.
            20
        }
    }

    fn edge_cost(&self, from: AreaId, to: AreaId) -> u32 {
        let stamina =
            crossing_stamina_cost(self.standard_move_cost(from), self.standard_move_cost(to));
        let detail = self.details.get(&to).copied();
        let harshness = if let Some(ad) = detail {
            match ad.terrain.base.harshness() {
                Harshness::Mild => 0,
                Harshness::Moderate => 10,
                Harshness::Harsh => 20,
            }
        } else {
            0
        };
        stamina + harshness
    }
}

impl<'a> Graph for AreaGraph<'a> {
    type Node = AreaId;
    type Cost = u32;

    fn neighbors(&self, node: AreaId) -> Vec<(AreaId, u32)> {
        let layout: Vec<(AreaId, areas::hex::Axial)> = self
            .areas
            .iter()
            .map(|kind| {
                (
                    *kind,
                    self.details
                        .get(kind)
                        .and_then(|d| d.axial)
                        .unwrap_or_else(|| areas::location::area_axial(*kind)),
                )
            })
            .collect();
        node.neighbors_from_layout(&layout)
            .into_iter()
            .map(|n| (n, self.edge_cost(node, n)))
            .collect()
    }

    fn heuristic(&self, _from: AreaId, _to: AreaId) -> u32 {
        // Hop-count is 0 or 1 in this 7-node graph (Hub is
        // adjacent to everything; sectors are at most 2 hops apart).
        // A constant zero (Dijkstra) is admissible and optimal here.
        0
    }
}

/// Plan a stamina-aware path from `start` to `goal`. Returns the full
/// path including endpoints and the total cost. Returns `None` only if
/// `goal` is unreachable from `start` (impossible in the v1 topology
/// since the graph is fully connected via Hub).
pub fn plan_path(
    areas: &[AreaDetails],
    character: &Character,
    start: AreaId,
    goal: AreaId,
) -> Option<(Vec<AreaId>, u32)> {
    let g = AreaGraph::new(areas, character);
    astar(&g, start, goal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::terrain::{BaseTerrain, TerrainType};

    fn area(name: &str, a: AreaId, base: BaseTerrain) -> AreaDetails {
        AreaDetails::new_with_terrain(
            Some(name.to_string()),
            a,
            TerrainType::new(base, vec![]).unwrap(),
        )
    }

    fn standard_areas() -> Vec<AreaDetails> {
        vec![
            area("c", AreaId::hub(), BaseTerrain::Clearing),
            area("s1", AreaId::sector1(), BaseTerrain::Forest),
            area("s2", AreaId::sector2(), BaseTerrain::Mountains),
            area("s3", AreaId::sector3(), BaseTerrain::Grasslands),
            area("s4", AreaId::sector4(), BaseTerrain::Desert),
            area("s5", AreaId::sector5(), BaseTerrain::Wetlands),
            area("s6", AreaId::sector6(), BaseTerrain::Tundra),
        ]
    }

    fn fresh_character() -> Character {
        Character::new("Pathfinder".to_string(), Some(0), None)
    }

    #[test]
    fn plan_to_self_returns_singleton() {
        let areas = standard_areas();
        let t = fresh_character();
        let (path, cost) = plan_path(&areas, &t, AreaId::hub(), AreaId::hub()).unwrap();
        assert_eq!(path, vec![AreaId::hub()]);
        assert_eq!(cost, 0);
    }

    #[test]
    fn plan_neighbor_is_two_node_path() {
        let areas = standard_areas();
        let t = fresh_character();
        let (path, _) = plan_path(&areas, &t, AreaId::hub(), AreaId::sector1()).unwrap();
        assert_eq!(path, vec![AreaId::hub(), AreaId::sector1()]);
    }

    #[test]
    fn plan_opposite_sectors_routes_through_hub() {
        // Sector1 (top-right) and Sector4 (bottom-left) are not adjacent.
        // The fastest route is Sector1 -> Hub -> Sector4.
        let areas = standard_areas();
        let t = fresh_character();
        let (path, _) = plan_path(&areas, &t, AreaId::sector1(), AreaId::sector4()).unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], AreaId::sector1());
        assert_eq!(path[1], AreaId::hub());
        assert_eq!(path[2], AreaId::sector4());
    }
}
