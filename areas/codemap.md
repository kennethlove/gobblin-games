# areas crate — Arena Topology

Extracted from `game/src/areas/` in dww.18. Deps: `world` (terrain, items), `shared`.

### **areas/** (1799 lines total) — **Arena Topology**
Hex-graph arena with 7+ areas, item inventories, and dynamic closures.

| File | Lines | Purpose |
|------|-------|---------|
| `lib.rs` | 394 | `Area` enum, `AreaDetails` struct, area lifecycle |
| `events.rs` | 660 | `AreaEvent` enum, event triggering, hazard spawning |
| `hex.rs` | 272 | Hex-graph topology, adjacency, pathfinding integration |
| `water.rs` | 91 | Water source mechanics, dehydration effects |
| `shelter.rs` | 90 | Shelter mechanics, protection from weather |
| `forage.rs` | 39 | Foraging mechanics, resource gathering |
| `traps.rs` | 25 | `PlacedTrap` struct — lives on `AreaDetails::placed_traps` |
| `weather.rs` | 34 | `Weather` enum (Clear, Rain, Storm, etc.) |

