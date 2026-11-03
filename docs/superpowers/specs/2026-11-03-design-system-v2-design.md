# Gobblin Games — Design System (v2)

**Date:** 2026-11-03
**Status:** Supersedes [v1](2026-05-04-design-system-v1-design.md) (dated files stay, same convention as combat v1/v2). Token values live only in repo-root `DESIGN.md` — this doc records direction and changes, never copies hex.

## 1. What changed vs v1, and why

| Area | v1 (2026-05-04) | v2 (current) | Why |
|---|---|---|---|
| Tone mix | 80 broadcast / 15 capitol-glam / 5 pun | **65% sports-broadcast / 25% humor / 10% fantasy; capitol-glam dropped** | Approved direction, `docs/DESIGN.md` (a8f59f9f, 2026-10-13). Glam read costume-y; humor earns its share through copy. |
| Display face | Bebas Neue | **Fraunces** (soft 40 / wonk 1) | v1 asked for broadcast-condensed; Fraunces carries the ledger-italic voice without shouting. |
| Wordmark | — (Bebas used everywhere) | **Barrio** | Logo face separate from UI, per root `DESIGN.md` frontmatter. |
| Body face | Source Sans 3 | **Source Serif 4** | Ledger reads as print; serif body is the identity. |
| UI / mono | IBM Plex Mono | **Atkinson Hyperlegible** (+ Mono) | Accessibility as a visible brand trait; mono kept for data. |
| Logo face | — | **JSL Blackletter** (proposed 2026-10-13) → **Barrio** (final, root `DESIGN.md`) | Conflict flagged: the Oct 13 direction proposed Grenze Gotisch + JSL Blackletter; nothing ever shipped with either (no `DESIGN.md` revision in history contains them). Root `DESIGN.md` (Oct 30, shipped in PR #145) settled on Fraunces + Barrio. Root wins — it is the token source of truth PR #145 built against. |
| Palette | D2 Velvet (`#19121A`, cyan `#00E5FF`) / L3 Capitol Cream | **Green-leaning near-black / near-white** botanical-ledger tokens | Redesigned in bead `959`, shipped PR #133; values in `DESIGN.md` only. Floor/ceiling discipline: no absolute black/white (`#010101` / `#EFEFEF`). |
| Theme count | theme1/2/3 removal plan | **Light + dark**, system default, manual toggle | v1's removal plan executed; two themes, not three. |
| Components | Top bar, scoreboard, sidebar HUD, ticker, event card… | v1 inventory absorbed by §3 layout inventory below | The page inventory in `docs/DESIGN.md` names what actually exists; v1's speculative HUD elements (kill-cam, victor card) remain out of scope. |
| Platform | Dioxus `web/src/components/` | **Axum + Askama + Tailwind** (`api/templates/`, `api/assets/src/main.css`) | v1 migration plan points at a stack the project no longer uses. |

Unchanged from v1: numerals rule (numbers outside prose set in mono — now Atkinson Hyperlegible Mono, `tabular-nums`), 8px spacing grid, two density modes, color+pattern two-feature identification for in-game entities, no absolute black/white.

## 2. Motion

- Transitions 150–300ms, ease-out. Motion answers an action or confirms a state; no ambient animation.
- **`prefers-reduced-motion: reduce` fallback is required on every transition/animation** — collapse to final state, no information carried by motion alone.

## 3. Layout inventory (absorbs v1 §6)

Every page: top bar (wordmark `Barrio`, nav, auth links, start-game) + footer.

- **Auth flow** — signup, login, reset, email-confirmation pages.
- **Game list** — filterable/searchable; card per game (map, title, info, view); owner delete with modal confirm; create-game.
- **Game detail** vs **in-game page** are distinct: detail is metadata; in-game is live — name + summary strip, event timeline (refreshes on combat rounds), large arena map (zoom/pan, detail tiers Far·Terrain·Close, q6k hazard cell styling), roster with select → quick-stats pane → goblin detail link, day-timeline event cards (collapsed/expanded), winner splash.
- **Goblin list** — filterable/sortable cards; create.
- **Goblin detail** — name, attributes, stats, portrait, boons/penalties, highlights, edit, submit-to-game.
- **Game creation** — name, teams, goblins/team, private flag, arena presets/random, draft flow (auto-draft, clear, randomize, fill-with-new-random).
- **Goblin creation** — name, avatar upload or pre-made, one coherent point-buy/reroll step per trait (per trait redesign).
- **Private join** — URL page: game details, pick goblins, decline.

Utility pages (not-found, error, log, replay) inherit tokens; no bespoke spec.

Live-game three-region layout (feed / map / roster), hour-picker (12 two-hour buckets), commentary feed rows (60vh), buttons/cards/inputs/error-banners: all defined in root `DESIGN.md` — components follow it, this inventory only says where they appear.

## 4. Out of scope (carried from v1 §9)

Iconography set (bead `wh4`), arena-map interaction beyond detail tiers, kill-cam / victor card, data-viz beyond kind hues, mobile pass, portrait treatment.
