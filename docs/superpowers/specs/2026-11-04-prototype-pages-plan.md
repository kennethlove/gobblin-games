# Plan: build out pages from `art/gobblin-games-prototype`

Authority: repo-root `DESIGN.md` (tokens, type, components, motion — source of truth,
shipped in PR #145). Composition source: `art/gobblin-games-prototype/` (13 screens,
Oct 13–15 2026). The prototype's own `DESIGN.md` fonts (Grenze Gotisch / JSL
Blackletter) are superseded history — do not use them.

Bead epic: `gobblin-games-uqw`. Audit `uqw.1` complete — verdicts below are confirmed
unless marked open.

## Inventory (audited)

| Prototype | Current template (mtime) | Route | Verdict |
|---|---|---|---|
| `home.html` | `home.html` (Oct 25): quickstart button + inline hx create form + 4 stats cards | `GET /` | **Update (cluster D)** — prototype adds hero arena map, public-games list, how-it-works, CTA band; logged-in variant has quickstart map, start panel, games + goblins sections. Current has none of that. |
| `home-logged-in.html` | same `home.html` | `GET /` | **Update (cluster D)** — needs logged-in state per prototype |
| `auth-screen.html` (login/register/forgot/sent views) | `auth.html` (Oct 25): login/register/reset forms; `check_email.html` (Nov 3) separate | `/auth` routes | **Update (cluster A)** — roughly aligned; verify + polish |
| — | `email_verified.html`, `reset_form.html` (Oct 25), `account.html` (Oct 7), `account_settings.html` (Oct 3) | auth/account routes | **Update (cluster A)** |
| `game-list.html` (summary strip, filter/sort toolbar, delete dialog) | `games_list.html` (Nov 2) | `GET /games` | **Verify only** — post-system; note prototype's filter/sort + delete-confirm as possible feature gaps, not styling work |
| `game-detail.html` (status-strip/overview/roster/players/rail) | `game_detail.html` (Nov 2) — **conflates info + live**: status, join form, winner splash, hex_map, roster, SSE feed all one page | `GET /games/{id}` | **Split confirmed → `uqw.3`** |
| `in-game.html` (timeline, roster panel, map stage, day-timeline) | same `game_detail.html` + sub-routes | `/areas /log /timeline /fights /characters` | **Split confirmed → `uqw.3`** — DESIGN.md: game detail and in-game page are not the same page. Live side keeps SSE (`hx-ext="sse"` = established pattern). |
| `join-game.html` (game facts, pick panel, entry rail, decline-invite) | join form embedded in `game_detail.html` (NotStarted + joinable_goblins) | POST `/games/{id}/join` | **Prototype = dedicated page** — decide extraction as part of `uqw.3` scope |
| `create-game-v2.html` (3-step Setup/Arena/Draft wizard, JS `hidden` steps) | `create_game.html` (Nov 4, newest) single form | POST `/games/new` | **Verify against wizard; no-JS constraint** — prototype wizard toggles `hidden` sections (JS). Any step UI must work without JS: single scrollable form with step anchors, or server-side step posts. |
| `create-game.html` | superseded by v2 | — | Reference only |
| `goblin-stable.html` (summary strip, filter/sort toolbar) | `my_goblins.html` (Oct 29, 0 inline styles, create + edit forms inline) | `GET /my-goblins` | **Update (cluster B)** |
| `create-goblin.html` (identity/avatar/character/record blocks + summary rail, `?edit=1`) | inline form in `my_goblins.html`; **no dedicated page** | POST `/my-goblins` | **Missing surface — decide in cluster B**: dedicated route per prototype, or keep inline. Prototype favors dedicated page. |
| `goblin-detail.html` (stats/record/boons/history + rail) | `character_detail.html` (Oct 29) — rendered by `games.rs:1121` = **game** character detail, not "my goblin" | `GET /games/{id}/characters/{character_id}` | **Update (cluster C)** — grouping corrected: it is a game sub-page |
| `faq.html` (4 topic sections + nav rail) | **none** | **none** | **Missing → `uqw.2`** |
| — | `characters.html` (Oct 3, `games.rs:628` = game roster page) | `GET /games/{id}/characters` | **Update (cluster C)** — grouping corrected from B |
| — | `areas.html`, `trait_list.html` (Oct 15, partial via `game_detail.rs`), `log.html` (Oct 22), `timeline.html`, `replay.html`, `combat_round.html` (Oct 24, partial) | game sub-routes/partials | **Update (cluster C)** |
| — | `base.html` + card partials (`event_card`, `day_summary_card`, `commentary_card`) | layout | Post-system (Nov 2) — verify only |

## Findings (audit evidence)

1. **Legacy token drift:** `main.css` still defines `--broad-*` (152 refs, old blue
   palette, hue 260). `timeline.html` (6 uses, 15 inline styles), `replay.html`
   (5 uses, 14 inline styles) still use them. Others with inline `style=`:
   `trait_list` 7, `character_detail` 4, `characters`/`areas`/`log` 2 each,
   `account` 1. Current palette = `--bg/--accent/--gold/--border` etc. (DESIGN.md
   mapping table at line 640 = mechanical find-replace).
2. **`error.html` is dead** — zero references anywhere in the repo (not even tests).
   Delete (cluster D), or wire to a real 500 handler first if one is wanted.
3. **`game_detail.html` has one off-system error box**
   (`bg-red-900/30 border-red-700 text-red-300`) — violates token rule; use the
   DESIGN.md error-banner component during the split.
4. **Prototype is 100% static** — zero `hx-*` attributes in all 13 files. It is a
   composition reference only; the app keeps its established htmx (home quickstart,
   game SSE, check-email resend) and adds no new JS.
5. **Current home posts via htmx** to `/api/games/quickstart` and `/api/games`
   (hx-swap=none) — existing pattern, keep when rebuilding home.

## Gates (every page)

1. Tokens only from root `DESIGN.md` (no off-system hex/shadows/type sizes; kill
   `--broad-*` and inline `style=` in touched files).
2. No JavaScript added — forms/links/server-rendered only; htmx only where the
   template already uses it (user rule: if you can do it without JS, do).
3. `ux_audit` passes strict before the bead closes (contrast, tokens, states,
   reduced-motion).
4. Numbers `tabular-nums`; status = dot + label; single gold CTA accent.
5. `cargo check`/`fmt` clean if Rust touched; no template test framework exists —
   spot-check rendered pages via local api.

## Beads

| Bead | Scope | Blocked by |
|---|---|---|
| `uqw` | epic/parent — this plan | — |
| `uqw.1` | audit — **done, verdicts recorded above** | — |
| `uqw.2` | faq: new `faq.html` + route from prototype | `uqw.1` |
| `uqw.3` | split game-detail / in-game (+ decide join-page extraction) | `uqw.1` |
| `uqw.4` | cluster A: auth/account templates | `uqw.1` |
| `uqw.5` | cluster B: `my_goblins.html` + create-goblin surface decision | `uqw.1` |
| `uqw.6` | cluster C: game sub-pages (`characters`, `character_detail`, `trait_list`, `areas`, `log`, `timeline`, `replay`, `combat_round`) | `uqw.1` |
| `uqw.7` | cluster D: shell/static (home both states, not_found, delete dead `error.html`) | `uqw.1` |
