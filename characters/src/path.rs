//! Area-level pathfinding: builds a graph over the v1 7-area hex topology
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
use areas::{Area, AreaDetails};
use std::collections::HashMap;
use strum::IntoEnumIterator;
use world::pathfinding::{Graph, astar};
use world::terrain::Harshness;

/// Snapshot of the area graph from one character's perspective at one
/// moment in time. Built per planning call — do not cache across cycles.
pub struct AreaGraph<'a> {
    /// All known areas (whether represented in `area_details` or not).
    /// Used to define the node set; missing details still produce a node
    /// connected by topology, just with default-terrain cost.
    pub areas: Vec<Area>,
    /// Character doing the planning. Used for stamina-cost computation.
    pub character: &'a Character,
    /// Per-area details lookup (terrain, items, etc.).
    pub details: HashMap<Area, &'a AreaDetails>,
}

impl<'a> AreaGraph<'a> {
    pub fn new(areas: &'a [AreaDetails], character: &'a Character) -> Self {
        let mut details: HashMap<Area, &AreaDetails> = HashMap::new();
        for ad in areas {
            if let Some(a) = ad.area {
                details.insert(a, ad);
            }
        }
        Self {
            areas: Area::iter().collect(),
            character,
            details,
        }
    }

    /// The standard move cost of entering `area` from this
    /// perspective: per-character, per-terrain; `20` (the base move
    /// cost) when no details are known.
    fn standard_move_cost(&self, area: Area) -> u32 {
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

    fn edge_cost(&self, from: Area, to: Area) -> u32 {
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
    type Node = Area;
    type Cost = u32;

    fn neighbors(&self, node: Area) -> Vec<(Area, u32)> {
        node.neighbors()
            .into_iter()
            .map(|n| (n, self.edge_cost(node, n)))
            .collect()
    }

    fn heuristic(&self, _from: Area, _to: Area) -> u32 {
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
    start: Area,
    goal: Area,
) -> Option<(Vec<Area>, u32)> {
    let g = AreaGraph::new(areas, character);
    astar(&g, start, goal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::terrain::{BaseTerrain, TerrainType};

    fn area(name: &str, a: Area, base: BaseTerrain) -> AreaDetails {
        AreaDetails::new_with_terrain(
            Some(name.to_string()),
            a,
            TerrainType::new(base, vec![]).unwrap(),
        )
    }

    fn standard_areas() -> Vec<AreaDetails> {
        vec![
            area("c", Area::Hub, BaseTerrain::Clearing),
            area("s1", Area::Sector1, BaseTerrain::Forest),
            area("s2", Area::Sector2, BaseTerrain::Mountains),
            area("s3", Area::Sector3, BaseTerrain::Grasslands),
            area("s4", Area::Sector4, BaseTerrain::Desert),
            area("s5", Area::Sector5, BaseTerrain::Wetlands),
            area("s6", Area::Sector6, BaseTerrain::Tundra),
        ]
    }

    fn fresh_character() -> Character {
        Character::new("Pathfinder".to_string(), Some(0), None)
    }

    #[test]
    fn plan_to_self_returns_singleton() {
        let areas = standard_areas();
        let t = fresh_character();
        let (path, cost) = plan_path(&areas, &t, Area::Hub, Area::Hub).unwrap();
        assert_eq!(path, vec![Area::Hub]);
        assert_eq!(cost, 0);
    }

    #[test]
    fn plan_neighbor_is_two_node_path() {
        let areas = standard_areas();
        let t = fresh_character();
        let (path, _) = plan_path(&areas, &t, Area::Hub, Area::Sector1).unwrap();
        assert_eq!(path, vec![Area::Hub, Area::Sector1]);
    }

    #[test]
    fn plan_opposite_sectors_routes_through_hub() {
        // Sector1 (top-right) and Sector4 (bottom-left) are not adjacent.
        // The fastest route is Sector1 -> Hub -> Sector4.
        let areas = standard_areas();
        let t = fresh_character();
        let (path, _) = plan_path(&areas, &t, Area::Sector1, Area::Sector4).unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], Area::Sector1);
        assert_eq!(path[1], Area::Hub);
        assert_eq!(path[2], Area::Sector4);
    }
}
