# Icon attributions

Sprites under `api/assets/icons/` are built from sources in
`api/assets/icons/src/{ui,narrative}/` by `api/build.rs`.

## game-icons.net (CC BY 3.0)

The 35 narrative icons and several UI icons come from
[game-icons.net](https://game-icons.net/), converted from the set that
shipped with the old Dioxus `web` crate (removed in #279).

Fetched fresh for this set:

| File | Original | Author |
|------|----------|--------|
| `ui/ghost.svg` | lorc/ghost | Lorc |
| `ui/swords.svg` | lorc/crossed-swords | Lorc |
| `ui/mail.svg` | lorc/envelope | Lorc |
| `ui/skull.svg` | lorc/dread-skull | Lorc |
| `ui/sword.svg` | delapouite/ancient-sword | Delapouite |
| `ui/users.svg` | delapouite/three-friends | Delapouite |
| `ui/backpack.svg` | delapouite/backpack | Delapouite |

Per-icon author and license page: <https://game-icons.net/licenses.html>
(CC BY 3.0 — attribution required; this file is that attribution).

## Hangry Games icon pack (`art/hg - icons/`)

97 narrative icons were converted from the pack's `png/` exports
(64×64 black line-art) with `mkbitmap -s8` + `potrace`, normalized to a
512-unit `currentColor` viewBox — same pipeline each time, no hand edits.
The pack's own license page ships with the art at
`art/hg - icons/license/license.html` ("User License"); the art directory
is gitignored, only the traced SVGs live in this repository.

Of the pack's 115 files, 18 were not imported: 12 subjects the existing
set already covers (`swords`, `sword`, `mace`, `blade`, `knife`, `spear`,
`axe`, `broken`, `rocks`, `shield`, `location`, `poison`) and 6 alternate
drawings of an imported subject (`desert-1`, `desert-2`, `grenade-1`,
`grass-1`, `pill-1`, `drone-1`).

## mycollection pack (`~/Downloads/mycollection`)

65 narrative icons converted from the second pack the same way —
`mkbitmap -s8 -t0.4` + `potrace`, potrace's transform kept, `currentColor`
512 shell. The pack ships its own `license/license.html` ("User License")
alongside the `png/` exports.

Of the pack's 72 files, 7 were not imported: 4 subjects already in the
set (`ghost`, `toxic`, `broken-bone`, `skull`), `heart` (the sprite
already has `hearts`), and 2 near-duplicates of existing subjects
(`injured` = `wounded`, `fracture` = `broken-bone`).

## Unverified provenance

These predate the icon-pipeline work and were already in the repository
history; source set unknown:

- `ui/delete.svg`, `ui/edit.svg`, `ui/lock_closed.svg`, `ui/lock_open.svg`,
  `ui/map_pin.svg`
- ~~mockingjay trio~~ (removed per review — not on-brand)

## Authored here

- `ui/arrow-left.svg`, `ui/list.svg` — simple geometric glyphs drawn for
  this set (no external source).
