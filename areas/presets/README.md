# Arena Presets

An arena preset is a complete, human-editable description of the map a
game is played on. Game creators pick one when they create a game (the
JSON `POST /api/games` payload and the `/games/new` form both accept an
arena choice); the chosen preset is **snapshotted onto the game row** and
the game's areas are instantiated from that snapshot. Games created
without a choice (including Quickstart) use the default preset,
`classic-hex`, which reproduces the classic Hub-plus-six-Sectors layout.

Every preset here loads through the same loader path
(`areas::preset::load_preset`), including the default. There is no
generator and no out-of-repo source: a preset is a JSON file in this
directory plus one line in the registry in `areas/src/preset.rs`.

## File format

One JSON object per file. All fields are required unless marked
optional.

| Field | Type | Meaning |
|-------|------|---------|
| `id` | string | Stable machine id used in create-game payloads. Must match the id registered in `areas::preset::REGISTRY`. |
| `name` | string | Human-facing name shown in choice UIs. |
| `description` | string | One-paragraph human description shown next to the name. |
| `areas` | array | The arena's area slots, in label order: Hub first, then Sectors 1..6 clockwise from top-right. |

Each entry of `areas`:

| Field | Type | Meaning |
|-------|------|---------|
| `area` | string | Which engine area this slot instantiates: `"Hub"`, `"Sector 1"` … `"Sector 6"` (case-insensitive; `"Sector1"` also accepted). The engine's adjacency (`Area::neighbors`) is fixed, so every preset must cover all seven exactly once. |
| `q` | integer | Axial hex coordinate `q`. Presentation only — game logic uses area identity, not coordinates. |
| `r` | integer | Axial hex coordinate `r`. See `q`. |
| `name` | string (optional) | Fixed display name for this area. When omitted, each game draws a fresh themed name at creation. |
| `sub_hexes` | array (optional) | The seven sub-hexes this area hex divides into. See below. |

### Sub-hexes

Each big area hex divides into 7 sub-hexes — the smallest map units.
The big hex is the general biome; the sub-hexes are the actual
features inside it (a valley inside a mountain biome reads as its own
terrain). Today sub-hexes are **atmosphere**: they show on the map and
area cards and change nothing mechanical. Before 1.0 they become the
real interaction layer (combat, adjacency, travel costs) — this format
is the container that upgrade promotes, so author them like they
matter.

`sub_hexes` is an array of exactly 7 entries in `hex::SUB_SLOTS` order:
index 0 = center, 1..6 = ring clockwise from top-right (same numeric
order as the top-level layout). Omitting `sub_hexes` entirely is valid
(older presets keep loading).

Each entry of `sub_hexes`:

| Field | Type | Meaning |
|-------|------|---------|
| `terrain` | string | Base terrain of this sub-hex: `"Clearing"`, `"Forest"`, `"Desert"`, `"Tundra"`, `"Wetlands"`, `"Mountains"`, `"UrbanRuins"`, `"Jungle"`, `"Grasslands"`, `"Badlands"`, `"Highlands"`, `"Geothermal"`. May differ from the parent biome — that's the point (a `Wetlands` spring inside a `Mountains` sector). |
| `feature` | string (optional) | Short feature name shown on the map, e.g. `"Hidden Valley"`. |
| `flavor` | string (optional) | One-line flavor text shown next to the feature name. |

Example (three entries shown — the real list has seven):

```json
"sub_hexes": [
  { "terrain": "Mountains", "feature": "The High Rocks", "flavor": "Valleys nest between the peaks." },
  { "terrain": "Mountains", "feature": "Scree Slope" },
  { "terrain": "Wetlands", "feature": "Cold Spring", "flavor": "Meltwater gathers in a mountain bowl." }
]
```

Validation rules (enforced by the loader): valid JSON, `id` matches the
registry id, exactly the seven known areas each once, Hub first, no
two slots share the same `(q, r)`, and any slot that declares
`sub_hexes` declares exactly seven of them.

## Adding a preset

1. Copy `classic_hex.json`, change `id`/`name`/`description`, edit the
   `areas` list.
2. Register it in `areas/src/preset.rs` (`REGISTRY`).
3. `cargo test -p areas` — the loader tests cover the new file too.
