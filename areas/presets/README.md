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

Validation rules (enforced by the loader): valid JSON, `id` matches the
registry id, exactly the seven known areas each once, Hub first, and no
two slots share the same `(q, r)`.

## Adding a preset

1. Copy `classic_hex.json`, change `id`/`name`/`description`, edit the
   `areas` list.
2. Register it in `areas/src/preset.rs` (`REGISTRY`).
3. `cargo test -p areas` — the loader tests cover the new file too.
