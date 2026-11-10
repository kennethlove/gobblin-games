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

## Unverified provenance

These predate the icon-pipeline work and were already in the repository
history; source set unknown:

- `ui/delete.svg`, `ui/edit.svg`, `ui/lock_closed.svg`, `ui/lock_open.svg`,
  `ui/map_pin.svg`
- ~~mockingjay trio~~ (removed per review — not on-brand)

## Authored here

- `ui/arrow-left.svg`, `ui/list.svg` — simple geometric glyphs drawn for
  this set (no external source).
