# Arena Generation Spike: Seed-Driven Topology and Terrain

**Date:** 2026-10-16
**Spike:** Arena generation: seed-driven topology and terrain
**Base:** main (`be2a4329`, post-#60 sub-tile Location merge)
**Status:** spike complete — findings, recommendations, PoC test

This spike answers six questions about generating arenas from a seed
instead of (or alongside) hand-authored presets. The proof of concept is
a test-only module appended to `areas/src/preset.rs`
(`#[cfg(test)] mod arena_generation_poc`): `generate(seed) -> ArenaPreset`
plus the verify test `generated_arena_is_deterministic_for_seed`. No
production path changed — no registry entries, no handler wiring.

---

## 1. TOPOLOGY

### Findings

The engine's area graph has exactly seven nodes and one fixed shape:

- `Area` has exactly 7 variants (`areas/src/lib.rs:24-32`): Hub + Sector1..6.
- `Area::neighbors` (`areas/src/lib.rs:104-124`) is a hardcoded wheel:
  Hub touches all six sectors; each sector touches Hub and its two ring
  neighbors. No code path constructs adjacency any other way.
- Preset validation (`areas/src/preset.rs:191-229`, `validate_layout`)
  requires all seven variants exactly once, Hub first, unique `(q, r)`.
  A 6-area or 8-area arena cannot pass today.
- `ArenaSlot.q/r` is documented presentation-only (`areas/src/preset.rs:42-45`);
  game logic keys on area identity, not coordinates. `hex::default_layout`
  (`areas/src/hex.rs:70-85`) is the canonical cosmetic placement.
- The sub-tile layer (`areas/src/location.rs`) also bakes in the 7-area
  cluster: `all_locations()` (`location.rs:74-79`) and `area_axial()`
  (`location.rs:81-88`) iterate `default_layout()`. Its scaled-coordinate
  math (`G = 3·P + s`) is linear over axial isometries, so it survives a
  rotation/reflection of the ring unchanged.
- **Owner geometry decision (PR #62 review): the 7-sub-tile cluster fails
  the no-gaps condition (7/9 coverage, dead wedges), so sub-tiles change
  to 6 isosceles triangles per area — point to center, flat sides out.
  This rework is IN PROGRESS (sub-tile geometry rework, stacked on the
  character-location work). The PoC still emits 7 sub-tiles per slot and
  will follow the6-triangle shape —7 entries become6, ring slots remap
  to triangle bases — when the rework lands. The parent-level math above
  (dihedral wheel, scaled isometry) is unaffected.

**Constraint reality-check.** With the fixed enum, "generation" of
topology can only *permute labels over the existing wheel*. The full set
of permutations that keeps coordinate adjacency consistent with
`Area::neighbors` is the 12 dihedral rearrangements of the ring
(6 rotations × optional reflection) — an automorphism group of the ring
graph. Anything beyond that (Sector1 placed where Sector4's coord is and
vice versa breaks cyclic adjacency; N≠7 areas is impossible) would make
q/r lie about the engine graph, and 7±N is flat-out rejected by
`validate_layout` and by `AreaGraph`'s node set (see §5).

So today, generation coexists with the fixed enum as: **cosmetic
orientation (q/r), names, terrain, sub-tiles — identity and adjacency
untouched.** That is honest but limited: map rotation is the *only*
topology-flavored variance, and it changes nothing mechanical.

**The seam for later.** The data-driven layouts work (N-hex layouts)
owns the generalization. Its seams are already visible and localized:

- `AreaGraph::new` injects its node set from `Area::iter()`
  (`characters/src/path.rs:49`) — a free graph needs that to come from
  data (a node list + adjacency map).
- `validate_layout` (`areas/src/preset.rs:191`) — needs relaxing from
  "all seven variants" to "connected graph, Hub reachable, closure
  slots consistent with pathfinding" once areas are data, not enum
  variants.
- `areas::location`'s `all_locations`/`area_axial` (`location.rs:74-98`)
  — needs the layout as input rather than `default_layout()`.

This spike does not build any of that; it only names the seams.

### Recommendation

**v1: generate within the fixed 7-node wheel — the 12 dihedral
rearrangements plus seeded names/terrain/sub-tiles.** Do not attempt
free-graph generation in this spike. The wheel covers the spike's stated
shape (topology + terrain vary per seed); real topology freedom is
blocked on the enum and belongs to the data-driven layouts work, which
can promote the PoC's `generate(seed)` into whatever graph shape it
lands on. Note the draw order and constraint checks (§6) carry over
directly: same shape, generalized node source.

---

## 2. SEED

### Findings

There is no seed anywhere in the game-creation path today:

- `CreateGame` (`shared/src/lib.rs:111+`) has no seed field.
- The game row schema (`schemas/game.surql:9-29`) has no seed column.
- The game id is a random UUID v4 (`api/src/games/handlers.rs:42`,
  `api/src/games/handlers.rs:209`) — random, but stable once minted.
- Everything unseeded today: `ArenaPreset::instantiate` calls
  `generate_area_names()` which uses `rand::rng()` (`areas/src/lib.rs:127-128`);
  the game cycle, items, events all use OS randomness.

The one existing precedent for an optional seed parameter is per-call,
not per-game: `Character` addiction rolls take `rng_seed: Option<u64>`
with `SmallRng::seed_from_u64(seed)` / `from_rng(&mut rand::rng())`
fallback (`characters/src/inventory.rs:179-180`).

**Where the seed comes from — recommendation:** an explicit optional
`seed: Option<u64>` field on `CreateGame`. When absent, the server mints
one (`rand::rng().random()`) and **persists it on the game row**. Do not
silently derive it from the game id: the id is opaque format-wise, and a
derived seed that can't be read back is undebuggable. Persisting the
minted seed costs a schema addition but makes regeneration and support
trivial. Wire/serde work if the column lands: `shared::CreateGame`
(additive optional field, old clients unaffected), `schemas/game.surql`
(`DEFINE FIELD seed ON game TYPE int DEFAULT ...` — mind SurrealDB u64
range; store as string if full u64 fidelity is wanted), `Game` struct +
`save_game` explicit SET list (`api/src/games/persist.rs`, which writes
fields one by one precisely because the table is SCHEMAFULL).

**How determinism flows:** one seeded RNG per generation, consumed in a
documented fixed order. The PoC's draw order (§6) is the template.
Two rules the shipping version must keep:

1. **Draw order is a versioning contract.** Any insert/reorder changes
   the output for every existing seed. Document it next to the
   generator and add a golden test (fixed seed → pinned JSON) so a
   silent break is caught.
2. **No conditional draws.** `enforce_balance_constraint`
   (`world/src/terrain/assignment.rs:76-99`) consumes a *variable*
   number of RNG draws (reroll count depends on harsh terrain count).
   That's tolerable inside one concern, but for cross-version stability
   prefer per-concern sub-RNGs derived from the seed (e.g. seed ⊕
   domain constant per concern) so adding draws in terrain naming never
   shifts sub-tile layout. The PoC uses one RNG for simplicity and
   documents this as known debt.

Also note `instantiate()` currently injects unseeded randomness
(`areas/src/preset.rs:77-90` → `crate::generate_area_names()` →
`rand::rng()`), so a generated preset must **author its names** (or
`instantiate` needs a seeded twin) or the seed dies at instantiation.
The PoC authors names from the seed.

---

## 3. TERRAIN

### Findings

- The balance machinery exists and is generic over any slice length:
  `TerrainType::random` / `random_safe` / `random_moderate`
  (`world/src/terrain/assignment.rs:9-73`) and
  `enforce_balance_constraint` (`assignment.rs:76-99`), re-exported as
  `world::terrain::enforce_balance_constraint` (`world/src/terrain/mod.rs:6`).
  The rule is "max 3 Harsh-base areas per game; reroll extras to
  Moderate."
- **It is dormant.** The only consumers are tests
  (`world/src/terrain/assignment.rs` tests, `game/tests/terrain_assignment_test.rs`).
  No production path calls it. Area rows are created with
  identifier/name/area/sub-tiles only (`api/src/games/mod.rs:99-131`);
  top-level `AreaDetails.terrain` serde-defaults to Clearing
  (`areas/src/lib.rs:199`, `areas/src/lib.rs:185` `#[serde(default = "default_terrain")]`).
  A generated arena would be the *first* production consumer of the
  assignment rules — which is exactly the spike's intent.
- Terrain is a `TerrainType { base, descriptors }` pair
  (`world/src/terrain/types.rs:39-43`); descriptor compatibility is
  validated in `TerrainType::new`. Sub-hexes carry only a `BaseTerrain`
  each (`areas/src/preset.rs:26-36`).

**How it plugs in:** generated topology feeds the assignment constraints
directly — the topology is always the 7-node wheel, so the input is a
plain `[TerrainType; 7]` in slot order (Hub + Sectors 1..6), and
`enforce_balance_constraint` works unchanged. **No generalizing needed
for v1.** The rules are already slice-generic and the topology is fixed.
(If/when N-hex layouts land, `enforce_balance_constraint` needs nothing —
it takes `&mut [TerrainType]` — but the "max 3 Harsh" constant may need
retuning for N≠7.)

**Preset interaction:** when a preset is active, the preset wins
verbatim — its authored terrains and sub-tiles are the snapshot
(`areas/presets/README.md`, `api/src/games/handlers.rs:61-66`). A
generated arena and an authored preset should be mutually exclusive
choices at creation ("generated" being a pseudo-preset id), not layers.
**Owner confirmed: mutually exclusive — a preset is an entire map.** If
bias is wanted later, the seam is: constrain the sector draw order
(e.g. seeded `choose` from a per-slot allowed-biome list) rather than
post-hoc rerolling, to keep the draw order stable.

---

## 4. STORAGE

### Findings

The arena snapshot mechanism already exists and is cheap:

- The resolved preset is serialized into the game row's `arena` field
  at creation (`api/src/games/handlers.rs:88-90`,
  `api/src/games/handlers.rs:226-227`; schema `schemas/game.surql:28-29`
  — `arena TYPE object DEFAULT {}` with `arena.*` wildcard, i.e. no
  schema churn to store any preset-shaped object).
- Round-tripping `ArenaPreset` through serde is already tested
  (`areas/src/preset.rs` `default_preset_round_trips_through_loader_format`,
  `every_registered_preset_round_trips`).
- Size: `classic_hex.json` is ~4 KB including all 49 sub-tiles. Seven
  slots with sub-tiles is the worst case. Cheap.
- Areas are instantiated from the snapshot (`arena.instantiate()` →
  `create_area`, `api/src/games/handlers.rs:148-170`) and the snapshot
  is what the game reads (`fn::get_full_game` returns `config` and the
  areas; the snapshot rides the row).

**Recommendation: generate once at game creation from the seed, then
snapshot the result exactly like a preset. The snapshot is the contract;
the seed rides along for provenance.** Do *not* regenerate at game start
from the seed. Reasons:

1. `SmallRng` reproducibility is explicitly not guaranteed across `rand`
   releases (rand's policy for `StdRng`/`SmallRng`: stream may change in
   any version). Regenerating later can silently produce a different
   arena than the one characters were rostered into.
2. The snapshot path is proven, round-trip tested, schema-supported, and
   a few KB. "Regenerate to save storage" would save nothing measurable.
3. The snapshot keeps spectator/UI code identical: a generated arena is
   indistinguishable from a preset downstream.

Cost check requested in scope: writing the snapshot is one JSON
object in the same UPSERT body the handler already builds
(`api/src/games/handlers.rs:72-93`) — effectively free.

---

## 5. PATHFINDING

### Findings

`AreaGraph` lives in the **characters** crate, not areas
(`characters/src/path.rs:27` — the spike's "areas::path" naming is off; the
module is `characters::path`).

Its v1 assumptions, with evidence:

- **Node set = `Area::iter()`** — `characters/src/path.rs:49`. Not taken
  from the areas slice; the `areas: Vec<Area>` field (`path.rs:31-32`)
  is populated from the enum. A generated arena with a different node
  set cannot be expressed.
- **Adjacency = `Area::neighbors()`** — `Graph::neighbors` delegates
  straight to it (`characters/src/path.rs:86-88`). The wheel shape is
  hardcoded one level down in the enum (`areas/src/lib.rs:104-124`).
- **Heuristic is constant zero** (`characters/src/path.rs:93-98`),
  degrading A* to Dijkstra. Admissible for *any* graph, so it survives
  generalization unchanged.
- **Cost model is data-driven already**: edge cost comes from per-area
  details lookup (`path.rs:62-80`), so terrain-driven costs follow
  whatever terrain the generator assigns. No change needed.
- The `Graph` trait itself (`world/src/pathfinding.rs:14-25`) is
  generic over node/cost types — the *contract* is fine; only the
  `AreaGraph` adapter hardwires the wheel.
- Closure semantics (closed areas as high-penalty last resort,
  `CLOSED_PENALTY` at `path.rs:22`, tests at `path.rs:157-186`) assume
  the ring offers detours around a closed Hub. Any generated topology
  must keep the ring connected enough that "routes around closed area
  when possible" remains true, or accept behavioral change — another
  reason v1 keeps the wheel.
- Sub-hex pathfinding (`areas::location::shortest_path`,
  `location.rs:192`) independently bakes the 7-area cluster via
  `all_locations()` (`location.rs:74-79`) — same seam story.

**Verdict:** v1 generated topology (dihedral rearrangements of the wheel)
is hex-compatible by construction — same nodes, same adjacency — and
needs **zero** pathfinding changes. Free graphs require `AreaGraph` to
take node set + adjacency as input instead of deriving them from the
enum; the trait underneath already supports that.

---

## 6. VERIFY — the PoC

Test-only module `#[cfg(test)] mod arena_generation_poc` appended to
`areas/src/preset.rs` (private `validate_layout`/`parse_preset` are
reused as-is — the generated arena is validated by the *existing* code,
not a reimplementation). This is a spike, **not the shipping API**: the
production module (if/when built) should live at `areas::generation`
with per-concern sub-RNGs and a golden test.

`generate(seed: u64) -> ArenaPreset` — seeded `SmallRng`, fixed draw
order (this order is the documented contract):

1. **Ring orientation** — rotation `0..6` + reflection flag (the 12
   dihedral rearrangements). Sector *i* lands at ring position
   `(±(i−1) + rotation) mod 6`. Hub stays at `(0,0)`.
2. **Terrains** — Hub via `random_safe`, six sectors via
   `TerrainType::random`, then `enforce_balance_constraint` (max 3
   Harsh; consumes a variable number of draws — see §2 debt).
3. **Sub-hexes** — 7 per slot via `TerrainType::random`, parent biome
   ignored (sub-tile divergence is the point, per `areas/presets/README.md`).
4. **Names** — `crate::generated_area_names(&mut rng)` (the existing
   14-name bank, seeded here — the public wrapper is unseeded).
5. **Assemble** — `ArenaSlot`s in label order (Hub first), authored
   names (so `instantiate()` needs no extra randomness), id
   `generated-poc-<seed>`.

Verify test `generated_arena_is_deterministic_for_seed`: same seed
twice → `assert_eq!` on the full `ArenaPreset` (PartialEq covers
everything); a different seed → `assert_ne!`. Companion test
`every_generated_arena_passes_existing_validation` runs a spread of
seeds through `validate_layout` + serde round-trip through the real
`parse_preset`.

---

## Phased implementation plan (post-spike, owner-gated)

- **P0 — this spike.** PoC test + findings. No production change.
- **P1 — `areas::generation` (production module).** `generate(seed) ->
  ArenaPreset` with per-concern sub-RNGs (terrain / orientation /
  sub-tiles / names each derived: `seed ⊕ DOMAIN`), golden test pinning
  one seed's full JSON, property test over 100 seeds (all validate,
  >1 distinct output, harsh-count ≤ 3). No wiring.
- **P2 — seed plumbing.** `CreateGame.seed: Option<u64>`; mint-and-store
  when absent; `seed` column on `game` (`schemas/game.surql` +
  migration); thread seed into both create handlers
  (`api/src/games/handlers.rs:61` path and `:215` quickstart).
- **P3 — creation wiring.** Arena choice gains `"generated"`; handler
  builds the preset via `areas::generation` and feeds the existing
  snapshot path unchanged. UI lists it beside registry presets.
- **P4 — preset interaction policy** (owner decision from §3): either
  close it as "mutually exclusive" (document) or implement seeded fill
  of unauthored slots under a preset's biome bias.
- **P5 — blocked on data-driven layouts work.** Free-graph generation:
  generalize `AreaGraph` node/adjacency input, relax `validate_layout`,
  teach `areas::location` the layout as input. The P1 generator's shape
  (seed → constraints → assemble → validate) is the template; only the
  node source changes.

## Risks

- **Draw-order fragility** — any change to the generator's draw order
  silently changes every seed's output. Mitigation: golden test (P1) +
  per-concern sub-RNGs.
- **RNG stream drift** — `SmallRng` output is not stable across `rand`
  upgrades. Mitigation: snapshot-at-creation (§4) makes this a
  one-time event, not a gameplay risk; golden tests catch it at CI.
- **Variety illusion** — 12 orientations × terrain/name permutations is
  real variance in look and feel but zero mechanical topology variance.
  If the owner expects topology-level surprise, this spike alone can't
  deliver it (enum is the wall); setting expectations matters.
- **Validation blind spot** — `validate_layout` never checks that q/r
  adjacency is consistent with `Area::neighbors`. The PoC respects it
  by construction (dihedral permutations), but a future free-form
  generator could emit coordinate-lying presets that pass validation.
  Worth a validation hardening item when P5 lands.
- **SurrealDB integer width** — u64 seeds may exceed SurrealDB int
  range; decide string vs int storage in P2 before the column lands.

## Top 3 questions for the owner — RESOLVED (owner answers on PR #62)

1. **Seed provenance — RESOLVED: explicit `seed` field, persisted on the
   game row.** Storage as string if u64 fidelity is unreliable in the
   database (decide the width when the column lands in P2).
2. **Variety ceiling — RESOLVED: cosmetic + terrain variance is fine for
   v1.** Topology variance waits for the data-driven layouts work.
3. **Preset interaction — RESOLVED: mutually exclusive.** A preset is an
   entire map; a game is either preset or generated, never layered. The
   gap-filling idea is dead.
