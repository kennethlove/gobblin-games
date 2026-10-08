//! Area-level pathfinding: builds a graph over the v1 7-area hex topology
//! and exposes a path-planning helper for characters.
//!
//! Edge cost is a composite per the design decision (8pq Q2):
//! `stamina_cost + harshness_penalty + closed_penalty`.
//! - `stamina_cost`: per-character, per-terrain via `calculate_stamina_cost`
//! - `harshness_penalty`: 0/10/20 for Mild/Moderate/Harsh
//! - `closed_penalty`: high additive cost (`CLOSED_PENALTY`) so closed
//!   areas are routed around when alternatives exist, but still
//!   traversable as a last resort (8pq Q4 = K, "high penalty").

use super::actions::Action;
use super::{Character, calculate_stamina_cost};
use crate::areas::{Area, AreaDetails};
use std::collections::HashMap;
use strum::IntoEnumIterator;
use world::pathfinding::{Graph, astar};
use world::terrain::Harshness;

/// Penalty added to any edge entering a closed area. Picked so that even
/// the cheapest detour through 2 open areas is preferred over a single
/// closed-area hop (typical edge costs are ~20-50).
pub const CLOSED_PENALTY: u32 = 1000;

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
    /// Set of areas currently closed (e.g., due to area events).
    pub closed: std::collections::HashSet<Area>,
}

impl<'a> AreaGraph<'a> {
    pub fn new(areas: &'a [AreaDetails], closed: &[Area], character: &'a Character) -> Self {
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
            closed: closed.iter().copied().collect(),
        }
    }

    fn edge_cost(&self, to: Area) -> u32 {
        let detail = self.details.get(&to).copied();
        let stamina = if let Some(ad) = detail {
            calculate_stamina_cost(&Action::Move(Some(to)), &ad.terrain, self.character)
        } else {
            // No detail known — fall back to base move cost.
            20
        };
        let harshness = if let Some(ad) = detail {
            match ad.terrain.base.harshness() {
                Harshness::Mild => 0,
                Harshness::Moderate => 10,
                Harshness::Harsh => 20,
            }
        } else {
            0
        };
        let closed = if self.closed.contains(&to) {
            CLOSED_PENALTY
        } else {
            0
        };
        stamina + harshness + closed
    }
}

impl<'a> Graph for AreaGraph<'a> {
    type Node = Area;
    type Cost = u32;

    fn neighbors(&self, node: Area) -> Vec<(Area, u32)> {
        node.neighbors()
            .into_iter()
            .map(|n| (n, self.edge_cost(n)))
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
    closed: &[Area],
    character: &Character,
    start: Area,
    goal: Area,
) -> Option<(Vec<Area>, u32)> {
    let g = AreaGraph::new(areas, closed, character);
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
        let (path, cost) = plan_path(&areas, &[], &t, Area::Hub, Area::Hub).unwrap();
        assert_eq!(path, vec![Area::Hub]);
        assert_eq!(cost, 0);
    }

    #[test]
    fn plan_neighbor_is_two_node_path() {
        let areas = standard_areas();
        let t = fresh_character();
        let (path, _) = plan_path(&areas, &[], &t, Area::Hub, Area::Sector1).unwrap();
        assert_eq!(path, vec![Area::Hub, Area::Sector1]);
    }

    #[test]
    fn plan_opposite_sectors_routes_through_hub() {
        // Sector1 (top-right) and Sector4 (bottom-left) are not adjacent.
        // The fastest route is Sector1 -> Hub -> Sector4.
        let areas = standard_areas();
        let t = fresh_character();
        let (path, _) = plan_path(&areas, &[], &t, Area::Sector1, Area::Sector4).unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], Area::Sector1);
        assert_eq!(path[1], Area::Hub);
        assert_eq!(path[2], Area::Sector4);
    }

    #[test]
    fn plan_routes_around_closed_area_when_possible() {
        // Sector1 -> Sector4 normally goes via Hub. If Hub
        // is closed, the routing must wrap around the ring (e.g. through
        // Sector2 + Sector3).
        let areas = standard_areas();
        let t = fresh_character();
        let closed = [Area::Hub];
        let (path, cost) = plan_path(&areas, &closed, &t, Area::Sector1, Area::Sector4).unwrap();
        assert!(
            !path.contains(&Area::Hub),
            "path detoured around closed hub: {path:?}"
        );
        assert!(
            cost < CLOSED_PENALTY,
            "should not pay closed-penalty when an open route exists"
        );
    }

    #[test]
    fn plan_traverses_closed_area_as_last_resort() {
        // Close every ring sector, leaving only the path Hub -><br/>
        // closed sector. Then Hub -> Sector1 must traverse a
        // closed area and pay the penalty.
        let areas = standard_areas();
        let t = fresh_character();
        let closed = [Area::Sector1];
        let (path, cost) = plan_path(&areas, &closed, &t, Area::Hub, Area::Sector1).unwrap();
        assert_eq!(path, vec![Area::Hub, Area::Sector1]);
        assert!(
            cost >= CLOSED_PENALTY,
            "expected closed-penalty in cost, got {cost}"
        );
    }
}
