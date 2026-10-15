# Sub-hex interaction layer (pre-1.0) — design spike

Status: SPIKE FINDINGS — docs-only, no production code changed.
Base: `main` (parent of this change).

**Owner decision driving this doc:** before 1.0, the 7 sub-hexes per big arena
hex (`areas/src/hex.rs`: `SubAxial`, `SUB_SLOTS`, `SUB_SIZE_RATIO = 1/3`) become
what characters actually interact with and traverse. Big hexes = general
biomes; sub-hexes = actual features. Sub-hexes carry **combat**, **adjacency**,
and **travel costs**. The existing "presentation only; game logic operates at
the area level" comments become lies and are listed for removal in §7.1.
Atmosphere data already ships (`SubHex` in `areas/src/preset.rs:20-29`, rides
the area record — `areas/src/lib.rs:179-186`); that container is what this
design promotes to logic data. The hierarchy logic is centralized in one
module (`areas::location`, §7) so future changes stay cheap.

---

## 1. Character location today

`Character.area: Area` — `characters/src/lib.rs:197`. Plain enum
(`Hub`, `Sector1..6`, `areas/src/lib.rs:18-26`). No sub-hex, no coordinate.

**Writes (production code, exhaustive):**

| Site | What happens |
|---|---|
| `characters/src/lib.rs:431, 595, 656` | Constructors (`new`, `into_new_game`, generator) set `area: Area::Hub` |
| `characters/src/lib.rs:1129` | `act_move`: `self.area = destination` after stamina gate (1122–1140) |
| `characters/src/incidents.rs:412-413` | `Relocation` incident: `character.area = *new_area` |

**Reads (production, by subsystem):**

- Movement/travel: `characters/src/movement.rs:37, 53` (`current_area = self.area`),
  `TravelResult::Success(Area)` (`movement.rs:17-21`); `act_move`
  `lib.rs:1105-1157`; `CharacterTravelExhausted` emit `lib.rs:1134-1139`;
  hide emit `lib.rs:1189-1195`; take-item emit `lib.rs:1376-1382`.
- Brains/pathfinding: `characters/src/brains/mod.rs:252` (current-area lookup),
  `:296` (`best_goal == character.area`), `:304-314` (`plan_path` start),
  `:393` (crowd penalty excludes current area);
  `characters/src/path.rs:31-110` (`AreaGraph` keyed `HashMap<Area, _>`,
  `plan_path(start: Area, goal: Area)`).
- Encounter grouping (drives combat targeting): `game/src/games/cycle.rs:54-59`
  (`characters_by_area: HashMap<Area, Vec<Character>>`), lookups at `:470, :503,
  :698, :743, :778, :837-842, :915`.
- Afflictions/witnessing: `characters/src/afflictions/mod.rs:51`
  (Moderate-severity witness = `observer.area == target.area`),
  `afflictions/fixation.rs:180`, `afflictions/phobia/triggers.rs:23-27`,
  `rescue.rs:55` (same-area gate).
- Trauma producers: `game/src/trauma_producers/witness_ally_death.rs:34,48`,
  `witness_mass_casualty.rs:12,30,48`.
- Death cleanup / items: `game/src/games/cycle.rs:1454-1459` (drop items into
  the area), `:1469` (`get_area_details_mut(area)`).
- API/templates: `api/src/templates/game_detail.rs:982, 1011, 1023`;
  `api/src/games/mod.rs:718` (`location: t.area.to_string()`).
  ~106 non-test `.area`/`area:` sites overall; the production set above is
  complete (everything else is tests).

**Recommended persisted shape** (owner previously leaned typed
`Location { area, sub }` with serde default = center):

```rust
pub struct Location {
    #[serde(default)]            // default Hub — matches today's null-tolerant Area::deserialize (areas/src/lib.rs:49-64)
    // Owner: old rows don't matter — no backward-compat burden
    pub area: Area,
    #[serde(default)]            // default SubAxial::new(0,0) == SUB_SLOTS[0] == center
    pub sub: SubAxial,
}
// on Character:
#[serde(flatten)]
pub location: Location,
```

Why flatten: old rows carry the top-level `"area": "Sector 1"` column today
(`Character` is persisted whole-struct, `api/src/games/persist.rs:216-228`).
With `#[serde(flatten)]`, `Location::deserialize` sees the *existing* `area`
field and adds `sub` only when present — old saves keep their area and get
`sub = center`, exactly the owner's lean. The wire format stays
backward-compatible: `area` stays a top-level column, `sub` is a new optional
sibling. Hydration is plain `serde_json::from_value` (`api/src/games/handlers.rs:610`,
`api/src/routes/games.rs:262, 486, 934`). **Fallback** if flatten misbehaves
with SurrealDB row values (flatten buffers through serde `Content`): keep
`pub area: Area` untouched and add `#[serde(default)] pub sub: SubAxial` with
a typed `character.location() -> Location` accessor. Pick one in P2 (§8), not
both.

---

## 2. Adjacency math

Geometry facts already in code (`areas/src/hex.rs`):

- `Axial::neighbors` — six unit steps, pointy-top, clockwise from top-right
  (`hex.rs:29-44`). `Axial::distance` (`hex.rs:52-59`). `to_pixel` is linear
  (`hex.rs:62-71`). Adjacent big-hex centers are `√3·R` apart (tested,
  `hex.rs:215-228`), `R` = big-hex center-to-corner size.
- `SUB_SLOTS` = 7 axial slots reusing the same system (`hex.rs:103-111`).
- `SUB_SIZE_RATIO = 1/3` (`hex.rs:127`): sub size `r = R/3`; ring sub centers
  sit at `√3·r = √3·R/3 ≈ 0.5774R` from the parent center; sub apothem
  `√3/2·r ≈ 0.2887R`; outer edge reaches `0.5774R + 0.2887R = 0.866R` =
  parent incircle radius — the ring-outer-edge-grazing fact
  (`hex.rs:121-126`; non-overlap tested `hex.rs:244-268`).

**Within-parent adjacency (trivial):** the 7 sub-hexes reuse the same axial
system, so adjacency = axial distance 1 among `SUB_SLOTS`: center↔each ring
(6 pairs) and each ring↔its two ring neighbors (6 pairs). 12 undirected pairs
per big hex. Opposite ring slots are distance 2 (not adjacent).

**Scaled global coordinate.** Because `to_pixel` is linear, define the global
sub-hex coordinate `G = 3·P + s` (P = parent axial coord, s = slot). Proof:
`G·to_pixel(r) = 3P·to_pixel(r) + s·to_pixel(r) = P·to_pixel(R) + s·to_pixel(r)`
= parent center + slot offset = the actual pixel position. So `G` lives in the
same axial grid at size `r` and reproduces geometry exactly.

**Cross-parent adjacency — answer: YES, it maps 1:1, with a caveat.**
Rule: sub `s` of parent `P` touches exactly one sub of exactly one neighbor —
sub `−s` of parent `P + s`. Proof: `G_B − G_A = 3·d + (−s) − s = 3d − 2s`; with
`d = s` this is `d`, a unit step ⇒ distance 1 ⇒ edge-sharing. For any other
`(d, s')` the difference has axial distance ≥ 2.

Scratch calculation (numbers, Hub slot `(1,-1)` ↔ Sector1 slot `(-1,1)`,
parent size R, sub size r = R/3):

```
Hub sub (1,-1) center   = ( r·√3/2,      -1.5r    ) = ( 0.2887R, -0.500R )
Sector1 parent center   = ( R·√3/2,      -1.5R    ) = ( 0.8660R, -1.500R )
Sector1 sub (-1,1) off  = (-r·√3/2,       1.5r    ) = (-0.2887R,  0.500R)
Sector1 sub (-1,1) glob = ( 0.5774R,     -1.000R )
center distance         = |(0.2887, -0.500)|R = 0.5774R = √3·r  => exactly one
                          sub-hex step: the two sub-hexes share an edge.
Both sub edges are perpendicular to the Hub→Sector1 center line and sit on the
big-hex boundary (0.866R from each parent center along that line). Each sub
edge has length r = R/3, centered on the big edge (length R): both cover the
middle third of the shared big-hex edge — identical footprint, coplanar. 1:1.
```

Full enumeration over the 49 cells `G = 3P + s` (P over the 7 areas from
`default_layout()` `hex.rs:75-85`, s over `SUB_SLOTS`): **exactly 12
cross-parent adjacent pairs** = one per big-hex edge of the cluster (6 Hub
edges + 6 sector↔sector edges), every pair the `s ↔ −s` facing pair. No other
cross-parent contact exists — in particular **no corner/diagonal adjacency**:
the outer two-thirds of each big-hex edge are unoccupied wedges (7 sub-hexes
of radius R/3 cover `7/9 ≈ 77.8%` of the parent hex; the corner wedges are
dead space the grid never tiles).

**Caveat — distance vs adjacency.** Scaled axial distance is a *lower bound*
on walkable distance, not equal to it, because dead cells sit near big-hex
corners: BFS over the 49 occupied cells disagrees with scaled axial distance
on 336 of 2352 ordered pairs (e.g. Hub slot `(1,-1)` → Sector2 slot `(0,-1)`:
scaled distance 2, shortest occupied path 3 — the would-be midpoint `(2,-1)`
is dead space). Consequences:

- *Adjacency predicate:* scaled distance == 1 with both endpoints occupied is
  correct (it yields exactly the 12 + 84 undirected pairs).
- *Distance/costing:* run BFS/`world::pathfinding::astar` over the occupied
  cells (`world/src/pathfinding.rs` is already generic). The full movement
  graph is tiny: 49 nodes, 96 undirected edges (84 within-parent + 12
  cross-parent), fully connected (verified).

The 12-facing-pair chokepoint fact (one door per big-hex edge) is the load-
bearing design constraint for combat scoping and crowd flow (§4, §8 risks).

---

## 3. Travel today and proposal

**Today:**

- `Action::Move(Option<Area>)` (`characters/src/actions.rs:25`, parse "move"
  `:126`). One Move = at most one big-hex hop per phase.
- Gating: `calculate_stamina_cost` (`characters/src/lib.rs:1828-1885`) —
  base 20 for Move × destination `TerrainType::base.movement_cost()` ×
  affinity 0.8 × desperation. Charged in `act_move`
  (`lib.rs:1122-1140`); insufficient stamina ⇒ pop the event, `short_rests`,
  emit `CharacterTravelExhausted` (`world/src/output.rs:41,192`;
  `game/src/events/types.rs:111-115`). Destination costs precomputed per
  neighbor in `game/src/games/cycle.rs:872-893` (`DestinationInfo`,
  `areas/src/lib.rs:156-163`); brain-side gate `brains/mod.rs:307-327`.
- Brain pathfinding: `characters/src/path.rs` — A* over 7 `Area` nodes, edge
  cost = stamina + harshness (0/10/20) + `CLOSED_PENALTY = 1000`
  (`path.rs:28`); `Brain::act` takes first hop (`brains/mod.rs:280-345`);
  destination scoring `choose_destination` `brains/mod.rs:369-430` (affinity,
  harshness, crowd penalty `CROWD_PENALTY_PER_ENEMY` `brains/mod.rs:31`).
- Events: `MessagePayload::CharacterMoved { character, from: AreaRef,
  to: AreaRef }` (`shared/src/messages/mod.rs:491-495`), `CharacterHidden`
  `:497-500`; derived `GameEvent::CharacterMoved { from_area, to_area }`
  (`game/src/events/types.rs:84-90`) plus the travel-message family
  `:106-135`. No position beyond area anywhere on the wire.

**Proposal:**

- **Within a hex vs between hexes = same primitive.** A move is one *sub-hop*
  along the occupied-cell graph (§2). "Within hex" = hop between same-parent
  slots; "between hexes" = the facing-pair hop `s ↔ −s`. Area change becomes
  a side effect of the sub-hop (parent component of `Location` flips) — no
  special case, no separate cross-hex code path. The `s ↔ −s` single-door
  rule means every cross-boundary move is exactly one hop.
- **Brains stay area-level in v1.** `Brain::act`/`choose_destination` keep
  scoring areas; the cycle resolves the area goal to a sub-path (BFS over 49
  cells from current `Location` to the goal area's entry sub — the facing
  sub for the taken direction — then toward its center) and advances ONE
  sub-hop per `Move` action, deducting that hop's stamina. Migration for
  brains is therefore minimal: `plan_path` gains a thin wrapper, or is kept
  as the area-level heuristic layered over the sub graph.
- **Travel cost = terrain-derived, no new table.** Per-hop cost =
  `calculate_stamina_cost(&Action::Move, &sub_terrain, character)` using the
  *destination* sub-hex's terrain (`AreaDetails.sub_hexes[i].terrain`,
  `areas/src/preset.rs:21-22`; fallback: parent `AreaDetails.terrain` when
  `sub_hexes` is empty, `areas/src/lib.rs:179-186`). The per-`BaseTerrain`
  tables (`world` terrain `movement_cost`) already exist and are tuned;
  feature-based surcharges (`SubHex.feature`) are a later, optional layer.
  A dedicated per-sub-hex cost *table* is unnecessary — terrain IS the table.
- **Event schema change (additive, backward compatible):**
  `CharacterMoved` gains `#[serde(default)] from_sub: Option<u8>` /
  `to_sub: Option<u8>` — slot index into `SUB_SLOTS` (0..6; compact, stable,
  trivially mapped to `SubAxial`). Old persisted messages hydrate to `None`;
  timeline rendering can keep prose area-level while the UI map reads slots.
  `Action::Move` becomes `Move(Option<Location>)` (Display/parse string stays
  "move", `actions.rs:96, 126`).

**Open balance consequence:** crossing one big hex center-to-center is now
~4 hops (center→ring→cross→ring→center) ≈ 4× today's stamina price for the
same travel. This is intentional (distance becomes meaningful) but needs a
compensating knob — see open question Q1.

---

## 4. Combat space

**Today (no positions anywhere):**

- Scoping = "same area". Targets come from `characters_by_area`
  (`game/src/games/cycle.rs:915-925` → `EncounterContext.potential_targets`,
  `characters/src/lib.rs:176-181`); `pick_target` (`lib.rs:1042`) filters
  visibility/alliances only. The deferred fight re-resolves by UUID with NO
  same-area re-check at resolution time (`cycle.rs:1020-1075`).
- Resolution is a pure two-body contest: `attack_contest`
  (`characters/src/combat/resolve.rs:116`), `resolve` (`:453`), per-swing
  flat stamina (`CombatTuning.stamina_cost_attacker/target`,
  `characters/src/combat_tuning.rs:24-29`).
- Multi-round loop: `CombatRoundLoop` (`characters/src/combat/round_loop.rs:80`)
  over exactly two borrowed combatants (`split_at_mut`, `cycle.rs:1064-1075`),
  action selection behind `CombatActionProvider` (`round_loop.rs:29-44`).
  Flee = contested roll; success sets `CombatResult::Fled` and ends the fight
  — **no relocation** (`round_loop.rs:312-371`; emit `cycle.rs:1148-1150`).
  Identity is a `combat_id` string, unlocated (`cycle.rs:1034-1037`).
- Area events (`AreaEvent`, `areas/src/events.rs:10-20`) close whole areas
  (`is_open` `areas/src/lib.rs:276-278`) — the only "map pressure" on fights.

**Options for sub-hex combat:**

- **(A) Fight scoped to shared sub-hex.** Attack requires `attacker.location
  == target.location` (same area AND sub). No positional movement inside
  combat; flee resolves the fleer into an adjacent occupied sub (within-parent
  preferred; cross-parent facing pair allowed). Everything else (round loop,
  contest, beats) unchanged.
- (B) A + positional movement inside combat (each round includes a
  reposition beat on the 7-slot local cluster, zones/cover per sub).
- (C) Sub-hex as flavor only (status quo with location stamps on messages).

**Recommendation: (A).** It is the smallest change that makes the owner
decision real: sub-hexes determine *who can fight whom* and *where fights
happen*, and the facing-pair doors give fights natural chokepoints and
escape routes — with zero changes to `attack_contest`, `round_loop`, or the
beat wire. (B) adds a second decision layer per round (provider trait must
grow, beat schema must grow, UI must render motion) before (A) has even
shipped and settled; it is explicitly deferred to a later combat iteration.

**Honest size estimate for (A):** ~3–5 days. Same-sub filter in target
building (`cycle.rs:915-925`) and a defensive re-check at pending-attack
resolution (~10 lines); flee destination resolution (~40 lines in
`cycle.rs` post-fight handling using `areas::location` neighbors); location
stamp on `CombatRoundStart`/`CombatEnded` payloads (~30 lines incl. wire);
tests (same-sub fight, cross-sub no-engage, flee-into-sub, chokepoint
crowding). Plus P3 (§3) as prerequisite. Note the migration center-crunch:
all pre-existing characters hydrate into the center sub (§5), so every old
game becomes one big hub-center brawl on first load — moot per owner:
no pre-existing characters exist (PR #58 answers).

**Multi-round combat v2 beads (spec 2026-09/10-10-09 referenced by
`characters/src/combat_tuning.rs:45,63`, `round_loop.rs:1`) — assumptions that
break when this lands (to be replanned, not deleted):**

1. *Same area ⇒ may fight.* Targeting is built from per-area lists
   (`cycle.rs:915-925`); `EncounterContext` has no position. Sub-scoping
   invalidates any v2 work (follow-up attacks, revenge targeting, pack logic)
   that iterates "everyone in my area".
2. *Flee = fight over, position unchanged* (`round_loop.rs:366-371`). Under
   sub-hexes flee must produce a destination; v2 flee/chase designs assuming
   no relocation break.
3. *Combat is unlocated.* `combat_id` + beats carry no map position
   (`cycle.rs:1034-1037`; `shared/src/combat_beat.rs`). Sub-scoped fights
   need location stamps for UI/spectator scoping; any v2 feature assuming
   arena-wide combat visibility (narration, patron reactions) must be
   re-keyed.
4. *Exactly two combatants* via `split_at_mut` (`cycle.rs:1064-1075`).
   Sub-hex chokepoints make 3+ characters stacked in one 7-slot sub likely;
   v2 group-combat assumptions collide with the pair-only state machine.
5. *Flat per-swing stamina* (`combat_tuning.rs:24-29`) — no terrain/position
   modifiers; v2 stamina pacing numbers were tuned pre-sub-hex.
6. *Cower/spare verdict is a closed pair* (`round_loop.rs` §7 verdict,
   `CombatActionProvider::verdict` `round_loop.rs:36-44`) — no representation
   for third-party intervention from an adjacent sub.

---

## 5. Persistence and migration

**Character save shape.** Characters persist as whole-struct JSON blobs:
`serde_json::to_value(&character_without_items)` → `UPDATE character:…`
(`api/src/games/persist.rs:210-234`); hydration is `serde_json::from_value`
over `fn::get_characters_by_game` rows (§1). The `character` table is
SCHEMALESS (`schemas/codemap.md:54`) — **no `.surql` schema migration is
required**; the change is purely serde-driven:

- New shape: `#[serde(flatten)] location: Location` (§1). Old row
  `{"area": "Sector 1", …}` → `Location { area: Sector1, sub: (0,0) center }`.
  Owner-decided default (center) for free.
- New column: optional `sub` (slot coords) written from P2 on. Old readers
  (none after the P2 sweep) would ignore it — but every production reader
  moves in the same PR, so there is no mixed-state window.
- Fallback shape if flatten fails SurrealDB round-trip: sibling
  `#[serde(default)] sub: SubAxial` field (even simpler migration; loses the
  nested `Location` type on the wire — decide at P2 kickoff, Q-free, this is
  an implementation detail the P2 PR proves with a round-trip test).

**Area record shape.** `AreaDetails` (`areas/src/lib.rs:158-197`):
`area: Option<Area>`, `terrain`, `events`, `placed_traps`, and — already
shipped — `sub_hexes: Vec<SubHex>` in `SUB_SLOTS` order with
`#[serde(default)]` (empty for pre-sub-hex rows) and
`character_slots: HashMap<String, SubAxial>` (`:188-196`). `sub_hexes` is
written at game creation, riding the area record directly
(`api/src/games/mod.rs:118-127`) and asserted by
`api/tests/games_tests.rs:675-700`. Area rows persist via the same
whole-struct UPDATE (`api/src/games/persist.rs:180-195`) — also schemaless.

**Promotion path (atmosphere → logic):**

1. `SubHex.terrain` becomes the source for per-hop travel cost (§3) and
   later for shelter/forage/water (§6, item 7). Same container, same
   snapshot, no new data model.
2. Rows with empty `sub_hexes` (pre-sub-hex areas) get a **derived default**:
   all 7 slots inherit `AreaDetails.terrain` (and no feature/flavor). This is
   mandatory because old games' arena-preset snapshot on the game row may be
   `{}` (`schemas/game.surql:27`, "DEFAULT {} covers pre-migration rows") —
   there is nothing to backfill from. An optional backfill
   (`UPDATE area SET sub_hexes = …` from the snapshot) is only possible for
   games created post-snapshot and is a separate, owner-approval-gated task.
3. `AreaDetails::assign_slot`/`release_slot` + `character_slots`
   (`areas/src/lib.rs:283-313`) have **zero production callers** (grep: only
   `areas/src/lib.rs` + its own tests). With `Location.sub` on the character
   this parallel bookkeeping is deleted in P2, not migrated — the character
   record becomes the single source of truth for position.

---

## 6. Subsystems keying on `Area` today (file:line) and sub-hex impact

| # | Subsystem | Sites | Sub-hex keying effect |
|---|---|---|---|
| 1 | Encounter grouping (`characters_by_area`) | `game/src/games/cycle.rs:54-59`; reads `:470, :503, :698, :743, :778, :837-842, :915` | Re-key by `Location` → co-presence becomes same-sub; phobia/trauma/addiction observers, sleeping-theft proximity, and combat targeting all inherit sub-scoping from one key change |
| 2 | Enemy density / crowd penalty | `cycle.rs:62-66` → `brains/mod.rs:31-35, 393-405` | Key becomes `Location` (or keep area-level for brain scoring — cheap either way) |
| 3 | Destinations + stamina gate | `cycle.rs:872-893`; `brains/mod.rs:307-327`; `lib.rs:1122-1140` | Costs computed per destination sub-hop (§3) |
| 4 | Pathfinding | `characters/src/path.rs:31-110` | `AreaGraph` gains a sub-graph sibling (49 cells) reusing `world::pathfinding` |
| 5 | Movement events/wire | `actions.rs:25,126`; `shared/src/messages/mod.rs:491-507`; `game/src/events/types.rs:84-135` | `CharacterMoved` gains slot fields (§3) |
| 6 | Traps | `areas/src/lib.rs:189` (storage); cap 3/area `characters/src/lib.rs:1206-1217`; placement `:1230-1253`; search `:1255-1301`; post-action whole-area trigger scan `:985-1010` | `PlacedTrap` gains `sub: SubAxial`; trigger scan filters by victim's current sub; cap becomes per-sub or per-area (owner knob) |
| 7 | Water / forage / shelter | pure fns `areas/src/water.rs:6`, `areas/src/forage.rs:5`, `areas/src/shelter.rs:6`; called with AREA terrain at `brains/scoring.rs:40, 61` (terrain plumbed `brains/mod.rs:633-638`) and `characters/src/incidents.rs:78, 187` | One-line per call site: pass `location::terrain_at(character.location, area_details)` instead of `area_details.terrain` |
| 8 | Area events / closure / survival checks | `areas/src/lib.rs:165-166, 276-278`; `cycle.rs:1419-1423`; `cycle_helpers.rs:66-111`; `game/src/games/mod.rs:674-700` (filter `t.area == area` at `:698`), survival roll `mod.rs:823`; wire `shared/src/messages/mod.rs:499-507` | v1 stays area-scoped (closing a whole biome is the point of "big hex = general biome"); sub-scoped events are a future layer — no change now |
| 9 | Affliction witnessing / rescue | `afflictions/mod.rs:51`; `phobia/triggers.rs:23-27`; `trauma.rs:48`; `addiction.rs:255`; `fixation.rs:180`; `rescue.rs:55` | Each compares area equality — becomes `Location` equality when witnessing should require co-presence (owner knob per subsystem; default keep area-level) |
| 10 | Death cleanup | `cycle.rs:1448-1459` | Items drop into the area — optionally into the death sub-hex |
| 11 | Trauma producers | `witness_ally_death.rs:34,48`; `witness_mass_casualty.rs:12,30,48` | HashMaps keyed `Area` — same knob as #9 |
| 12 | API/templates | `api/src/templates/game_detail.rs:982,1011,1023`; `api/src/games/mod.rs:718`; `announcers/src/lib.rs:168`; `game/src/games/day_summary.rs:226`; `game/src/games/messages.rs:23` | Mechanical `.area` → `.location.area`; slot rendering optional |

**a0x note (explicit).** The SeekShelter/Forage action resolvers (in flight
separately; on main only the action variants + costs exist — `actions.rs:40-42`,
`lib.rs:1845-1846`) are being built **at area level** and this design does
not change that: resolvers keep reading `EnvironmentContext.area_details.terrain`
(`EnvironmentContext` `characters/src/lib.rs:140-169`). The upgrade path is a
one-line swap per resolver to `areas::location::terrain_at(...)` (item 7)
whenever the owner wants per-sub results. Nothing in the location migration
blocks or forces a0x.

**"Presentation only" comments that become lies (removal list, §7.1):**

- `areas/src/hex.rs:83-85` — "Sub-tiles are presentation/positioning only —
  game logic still operates at the area level."
- `areas/src/hex.rs:13-16` — module doc "Game logic does not depend on the
  pixel layout — only on neighbor adjacency and area identity."
- `areas/src/preset.rs:42-43` — `ArenaSlot.q/r` "presentation only; game logic
  operates on area identity".
- `areas/src/preset.rs:20-22` — `SubHex` "Atmosphere only today…promotes this
  container" (update, not delete).
- `areas/src/lib.rs:183-185` — `sub_hexes` "Atmosphere only — the pre-1.0
  interaction layer…promotes this data" (update).
- `areas/src/lib.rs:190-192` — `character_slots` "Presentation/positioning
  only…" (delete with the field, §5).
- `areas/src/lib.rs:288-290` — `assign_slot` overflow note "sub-tiles are
  presentation-only" (delete with the fn).
- `api/src/games/mod.rs:118-119` — "Sub-hex atmosphere rides the area record
  directly" (reword: logic data).

---

## 7. Centralization: `areas::location`

One module — **`areas::location`** (`areas/src/location.rs`) — owns every
hierarchy rule. `hex.rs` keeps pure grid geometry (Axial, SubAxial, SUB_SLOTS,
SUB_SIZE_RATIO, to_pixel + their tests). `location.rs` owns:

- `Location { area, sub }` + its serde contract (flatten/defaults) +
  `Display` + slot-index (0..6) conversions.
- `sub_neighbors(Location) -> [Option<Location>; 6]` — within-parent slots
  plus the cross-parent `s ↔ −s` facing rule, derived from `Area::neighbors`
  ordering (`areas/src/lib.rs:107-124`, same clockwise-from-top-right order
  as `hex.rs:29-44`) so direction indexing has exactly one spelling.
- Occupancy-aware graph helpers: `walkable_neighbors(details, loc)`,
  `shortest_path(from, to, details)` (BFS/astar over the ≤49 occupied cells;
  the §2 lower-bound trap is fixed here, once).
- `terrain_at(location, details) -> BaseTerrain` (sub-hex terrain with
  empty-`sub_hexes` fallback to parent terrain) — the single source every
  shelter/forage/water/trap upgrade consumes.
- `travel_cost(from, to, details, character)` wrapping
  `calculate_stamina_cost` with destination-sub terrain resolution.

What moves in / out: `assign_slot`/`release_slot` logic is deleted (§5), not
moved; `Area::neighbors` stays where it is and is *called* by the facing rule.
Result: a future change to sub-hex topology, sizing, or costing touches
`areas/src/location.rs` (+ `hex.rs` for pure geometry) and nothing else.

---

## 8. Work order, risks, open questions

**Phased plan (each phase independently mergeable):**

- **P1 — `areas::location` (pure addition).** Location type, adjacency
  (`sub_neighbors`, facing rule), BFS helpers + golden tests: the 12
  cross-parent pairs, the `s ↔ −s` rule, the walkable-vs-scaled mismatch
  (Hub `(1,-1)` → Sector2 `(0,-1)` = 2 scaled / 3 walkable), the 7/9
  coverage fact. No behavior change anywhere. ~1–2 days.
- **P2 — Persistence + type migration.** `Character.location` (flatten or
  sibling, proven by serde round-trip test incl. an old-shape fixture row),
  mechanical `.area` → `.location.area` across the ~30 production sites (§1),
  delete `character_slots`/`assign_slot`/`release_slot`, API templates.
  Old saves hydrate area-preserved/sub-centered. ~2–3 days, mostly mechanical.
- **P3 — Sub-hex travel.** `Action::Move(Option<Location>)`; sub-hop engine
  in the cycle (BFS path, one hop per Move, per-hop stamina);
  `CharacterMoved` slot fields (additive wire); brains unchanged (area-level
  goals). Balance pass after first sim. ~3–5 days.
- **P4 — Combat scoping (option A).** Same-sub targeting + defensive
  re-check at fight resolution; flee resolves into an adjacent sub;
  location stamps on combat payloads. Then **replan the combat-v2 beads**
  (§4 assumption list). ~3–5 days.
- **P5 — Subsystem upgrades (optional, one PR each).** Traps per sub (#6);
  shelter/forage/water via `terrain_at` (#7); witnessing co-presence knobs
  (#9). Each is a small, owner-gated change. ~1 day each.

**Risks:**

- Pacing/stamina economy shifts ~4× on cross-hex travel (Q1) — needs a
  compensating knob (e.g. movement attribute ⇒ hops per phase) and a sim
  pass before any tuning PR.
- Migration center-crunch: every pre-existing character hydrates into the
  center sub (Q2) — hub-center becomes a forced 7-slot jumble and, with P4,
  a permanent brawl.
- Facing-pair chokepoints (one door per big-hex edge) concentrate traffic;
  >7 characters converging reintroduces stacking (today's accepted overflow,
  `areas/src/lib.rs:304-310`) and multi-combatant melees the pair-only round
  loop cannot represent (§4 assumption 4).
- Serde flatten vs SurrealDB row hydration — mitigated by the P2 round-trip
  test and the documented sibling-field fallback (§1/§5).
- Wire/UI consumers (announcers prose, game-detail templates, timeline)
  assume area-only location; slot rendering is optional in P2/P3 and must
  not block backend phases.

**Open questions for the owner — RESOLVED (owner answers on PR #58):**

1. **Move granularity — RESOLVED: two sub-hops max per phase.** A goblin
   gets a move and an action; the action can be moving again. First sub-hop
   costs 20% of stamina, a second sub-hop costs the remaining 80%. This
   implies reworking the `movement` model (move + action shape) — flagged
   as a design task in P3.
2. **Spawn/migration spread — RESOLVED: no pre-existing characters exist,
   so migration is moot** (serde center default stays as the harmless
   fallback). New characters spawn spread across the 7 sub-hexes to
   disperse violence; sub-hexes immediately across a big-hex boundary are
   also valid spawn targets. Every goblin in its own hex would be ideal
   but is silly at 128 goblins. Related owner note: arena size should
   scale up/down with goblin counts as a DEFAULT SUGGESTION only —
   user-set values are never overridden.
3. **Combat scope — RESOLVED: option A confirmed.** Fights start AND end
   in the same sub-hex; only a successfully fleeing goblin moves to a new
   sub-hex. Cross-sub ranged attacks and adjacent-sub assists stay OUT
   for v1.
