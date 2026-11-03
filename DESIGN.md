---
title: Gobblin Games Design System
version: 1.0.0
description: >
  A field-naturalist's ledger documenting goblin battle royale — aged paper,
  forest ink, scholarly apparatus over chaotic goblin trash energy.
  Covers the live game UI and marketing pages. Light and dark themes.
colors:
  # Every pair below was APCA-verified before shipping:
  # text Lc >= 75, large text Lc >= 45, non-text Lc >= 30 (WCAG ratio shown as sidecar).
  background:
    light: "oklch(0.96 0.015 85)"    # warm paper cream
    dark: "oklch(0.16 0.015 160)"    # deep green-black
  surface:
    light: "oklch(0.985 0.008 90)"   # raised leaf
    dark: "oklch(0.21 0.02 155)"
  surface-2:
    light: "oklch(0.93 0.018 85)"    # sunken well, hover fill
    dark: "oklch(0.26 0.025 155)"
  fg:
    light: "oklch(0.22 0.03 150)"    # green-black ink
    dark: "oklch(0.92 0.02 120)"     # light green-tinted ink
  muted:
    light: "oklch(0.45 0.035 150)"
    dark: "oklch(0.86 0.03 140)"
  border:
    light: "oklch(0.8 0.02 120)"
    dark: "oklch(0.32 0.02 155)"
  border-strong:
    light: "oklch(0.68 0.025 140)"
    dark: "oklch(0.45 0.025 155)"
  green:                              # brand: deep forest / moss
    light: "oklch(0.45 0.09 155)"
    dark: "oklch(0.72 0.1 155)"
  blue:                               # info, links, alliances, focus ring
    light: "oklch(0.45 0.1 250)"
    dark: "oklch(0.86 0.09 250)"
  gold:                               # aged brass — THE single CTA accent
    light: "oklch(0.48 0.09 80)"
    dark: "oklch(0.87 0.09 85)"
  gold-hover:
    light: "oklch(0.42 0.085 80)"
    dark: "oklch(0.92 0.08 85)"
  gold-active:
    light: "oklch(0.38 0.08 80)"
    dark: "oklch(0.8 0.09 85)"
  on-gold:                           # label on gold fills
    light: "oklch(0.96 0.015 85)"
    dark: "oklch(0.16 0.015 160)"
  # --- semantic: run status ---
  running:
    light: "oklch(0.44 0.11 150)"
    dark: "oklch(0.85 0.1 150)"
  waiting:
    light: "oklch(0.44 0.09 65)"
    dark: "oklch(0.86 0.1 85)"
  finished:
    light: "oklch(0.45 0.035 150)"   # = muted
    dark: "oklch(0.86 0.03 140)"     # = muted
  # --- semantic: outcomes ---
  success:
    light: "oklch(0.44 0.11 150)"
    dark: "oklch(0.85 0.1 150)"
  danger:
    light: "oklch(0.44 0.14 25)"
    dark: "oklch(0.89 0.09 25)"
  warning:
    light: "oklch(0.44 0.09 65)"
    dark: "oklch(0.86 0.1 85)"
  info:
    light: "oklch(0.44 0.11 250)"
    dark: "oklch(0.86 0.08 250)"
  # --- event-kind hues (commentary chips) + verified soft tints ---
  event-death:
    light: "oklch(0.44 0.14 25)"
    dark: "oklch(0.89 0.09 25)"
  event-death-soft:
    light: "oklch(0.95 0.025 25)"
    dark: "oklch(0.2 0.05 25)"
  event-alliance:
    light: "oklch(0.44 0.11 250)"
    dark: "oklch(0.86 0.08 250)"
  event-alliance-soft:
    light: "oklch(0.95 0.03 250)"
    dark: "oklch(0.22 0.05 250)"
  event-item:
    light: "oklch(0.45 0.08 80)"
    dark: "oklch(0.87 0.09 85)"
  event-item-soft:
    light: "oklch(0.95 0.03 85)"
    dark: "oklch(0.22 0.045 85)"
  event-affliction:
    light: "oklch(0.45 0.08 320)"
    dark: "oklch(0.86 0.07 320)"
  event-affliction-soft:
    light: "oklch(0.95 0.025 320)"
    dark: "oklch(0.23 0.05 320)"
  event-phobia:
    light: "oklch(0.45 0.07 200)"
    dark: "oklch(0.85 0.08 200)"
  event-phobia-soft:
    light: "oklch(0.95 0.025 200)"
    dark: "oklch(0.22 0.05 200)"
  event-trauma:
    light: "oklch(0.38 0.09 40)"     # oxblood
    dark: "oklch(0.89 0.08 40)"
  event-trauma-soft:
    light: "oklch(0.95 0.02 40)"
    dark: "oklch(0.2 0.045 40)"
  # --- severity levels (chip text on its event-soft tint) ---
  severity-mild:
    light: "oklch(0.45 0.035 150)"   # muted on gray-soft
    dark: "oklch(0.86 0.03 140)"
  severity-mild-soft:
    light: "oklch(0.95 0.01 90)"
    dark: "oklch(0.26 0.02 150)"
  severity-moderate:                 # = warning
    light: "oklch(0.44 0.09 65)"
    dark: "oklch(0.86 0.1 85)"
  severity-severe:                   # = danger
    light: "oklch(0.44 0.14 25)"
    dark: "oklch(0.89 0.09 25)"

typography:
  fonts:
    wordmark: "'Barrio', 'Comic Sans MS', cursive"
    display: "'Fraunces', Georgia, 'Times New Roman', serif"
    body: "'Source Serif 4', Georgia, 'Times New Roman', serif"
    ui: "'Atkinson Hyperlegible', system-ui, -apple-system, 'Segoe UI', Helvetica, Arial, sans-serif"
    mono: "'Atkinson Hyperlegible Mono', ui-monospace, 'SF Mono', Menlo, monospace"
    flavor: "'Caveat', 'Segoe Print', cursive"
  display-variation: "font-variation-settings: 'SOFT' 40, 'WONK' 1; /* goblin wonk; tune per surface */"
  roles:
    wordmark:
      family: wordmark
      sizes: "clamp(1.75rem, 4vw, 2.75rem)"
      use: "Gobblin Games logo ONLY. Graffiti tag face — never headlines, never UI, never below 1.5rem."
    display:
      family: display
      sizes: "clamp(2.75rem, 6vw, 4.5rem)"   # --fs-hero
      use: "hero headlines only"
    h1:
      family: display
      sizes: "clamp(2rem, 4vw, 3rem)"
    h2:
      family: display
      sizes: "clamp(1.5rem, 3vw, 2rem)"
    h3:
      family: ui
      sizes: "1.25rem"
      weight: 700
    lead:
      family: body
      sizes: "1.125rem"
    body:
      family: body
      sizes: "1rem"                        # 16px floor
      lineHeight: 1.6
    sm:
      family: ui
      sizes: "0.875rem"
    xs:
      family: mono                         # <=12px data always mono
      sizes: "0.75rem"
    meta:
      family: mono
      sizes: "0.8125rem"
    flavor:
      family: flavor
      sizes: "1.0625rem–1.5rem"
      use: "goblin names, Chronicler asides, scrawled notes. NEVER UI labels or paragraphs."
  numeric: "font-variant-numeric: tabular-nums; /* every HP, damage, count, timer */"

spacing:
  grid: 8
  gap:
    xs: "0.25rem"   # 4px half-step: icon/label inline gaps only
    sm: "0.5rem"    # 8px
    md: "1rem"      # 16px
    lg: "1.5rem"    # 24px
    xl: "2.5rem"    # 40px

rounded:
  sm: "4px"     # chips, inline code
  md: "8px"     # default: buttons, cards, inputs
  lg: "12px"    # large panels, modals
  pill: "999px" # status chips, dots containers

elevation:
  # Named levels only. No ad-hoc shadows. Depth via border + surface shift first;
  # shadow only when a surface genuinely floats (menu, modal).
  sm: "0 1px 2px oklch(0.22 0.03 150 / 0.08)"            # resting cards
  md: "0 4px 12px oklch(0.22 0.03 150 / 0.12)"           # dropdowns, popovers
  lg: "0 12px 32px oklch(0.22 0.03 150 / 0.18)"          # modals
  dark-strategy: >
    In dark theme, reduce shadow opacity ~40% and add a 1px border-strong
    outline. Never increase blur. Glow is banned in both themes.

components:
  button-primary:
    background: gold
    label: on-gold
    radius: md
    states:
      default: { background: gold, label: on-gold }
      hover: { background: gold-hover }
      focus-visible: { outline: "2px solid blue", outline-offset: 2px }
      active: { background: gold-active }
      disabled: { background: surface-2, label: "oklch(0.55 0.02 150)", opacity: 0.7 }
  button-secondary:
    background: surface
    border: 1px border-strong
    label: fg
    states:
      default: { background: surface, label: fg, border: border-strong }
      hover: { background: surface-2 }
      focus-visible: { outline: "2px solid blue", outline-offset: 2px }
      active: { background: "surface-2, translateY(1px)" }
      disabled: { label: "oklch(0.55 0.02 150)", border: border, opacity: 0.7 }
  button-ghost:
    background: transparent
    label: muted
    states:
      default: { label: muted }
      hover: { background: surface-2, label: fg }
      focus-visible: { outline: "2px solid blue", outline-offset: 2px }
      active: { background: border }
      disabled: { label: "oklch(0.55 0.02 150)", opacity: 0.7 }
  card-panel:
    background: surface
    border: 1px border
    radius: md
    elevation: sm
    hover: "border: border-strong (interactive cards only; static cards never react)"
  commentary-row:
    layout: "grid: time (mono xs) | speaker (flavor, 1.125rem) | text (body sm) | chip"
    speaker: "goblin name in flavor face; Chronicler in display face italic"
    chip: "event-kind soft bg + event-kind text + severity suffix dot"
  status-dot:
    size: 8px
    running: running
    waiting: waiting
    finished: finished
    rule: "dot + text label always; never color alone"
  hex-tile:
    structure: "pointy-top hex split into 6 isosceles triangle cells — apex at center, base on edge; cell i faces axial direction i (matches areas/src/hex.rs SubTriangle 0..=5)"
    role: "hex = neutral container — movement never blocked (q7k), closures do not exist; hazards live on cells, never on the hex"
    open: { fill: surface, stroke: border, cells: "6 hairline spokes to center" }
    hover: { fill: surface-2, stroke: border-strong }
    focus-visible: { outline: "2px solid blue", outline-offset: 1px }
    occupancy: "goblin count/initial as centered text, ui 700 — data, not a tile state"
  hex-cell:
    clear: { fill: none, stroke: none }
    hazard: { fill: "event-kind-soft", stroke: "event-kind" }
    kind-hue:                     # game's AreaEventKind buckets
      Fire: event-death           # wildfire
      Flood: event-alliance
      Earthquake: event-trauma
      Storm: event-phobia         # blizzard, avalanche, sandstorm
      Hazard: event-item          # landslide, heatwave, drought, rockslide, sinkhole
      Other: muted
    secondary-channel: "a hazard shows as struck cell position + tint; clear cells stay empty — grayscale-readable by geometry alone. Severity rides the commentary chip dot, not the map"
    arrival: "struck cell tint draws in 250ms ease-out; prefers-reduced-motion fills instantly"
  map-layers:
    data-source: "biome = AreaDetails.terrain (per hex); terrain + feature + flavor = AreaDetails.sub_tiles[i] (per triangle — areas/presets/*.json)"
    biome-wash:                      # hex fill — background tier, low chroma
      grasslands: "oklch(0.95 0.025 115) / dark oklch(0.27 0.035 115)"
      wetlands: "oklch(0.925 0.03 235) / dark oklch(0.25 0.045 235)"
      forest: "oklch(0.93 0.03 150) / dark oklch(0.255 0.045 150)"
    terrain-texture: "cell = SubTriangle terrain as monochrome texture in muted (forest ticks, wetlands waves, grasslands tufts, urban-ruins grid, clearing blank). Color belongs to biome, texture to terrain; priority per cell: hazard tint > terrain texture > biome wash"
    detail-tiers:
      far: "default — biome wash + hex labels + occupancy + hazard cells"
      mid: "+ terrain textures in cells"
      near: "focused hex (viewBox 150 units) + SubTile.feature names + goblin dots (initial in a green-ringed circle, .goblin-mark) at cell median; flavor via SVG <title> tooltip; hex badge = initial (1 goblin) or count"
    tier-control: "one segmented control (Far · Terrain · Close); clicking a hex jumps to near focused on it; viewBox eases 200ms, instant under prefers-reduced-motion"
  error-banner:
    background: event-death-soft
    border-left: "3px event-death"
    icon + text: event-death
    rule: "icon + text label; color is reinforcement, never the signal alone"
  theme-switcher:
    control: segmented two-state (Light | Dark), ui face, radius md
    states: [default, hover, focus-visible, active, disabled]
    focus-visible: "2px solid blue"
  hour-picker:
    role: "day chronicle — pick the two-hour bucket of the active day to filter the feed"
    anatomy: "12 cells (buckets from the event log): count (mono xs, tabular) over density bar over hour label (mono xs)"
    states: [default, hover, focus-visible, pressed]
    pressed: "aria-pressed=true → surface-2 bg, border-strong, count/label fg 700, bar fill --green"
    focus-visible: "2px solid blue"
    data: "bar height = N/max as % of the --gap-lg track (--surface-2); percentage is data, not a token"
    semantics: "role=group; each cell is a button with aria-label '<HH>:00–<HH+2>:00, N events'"
---

# Gobblin Games — Design System

## Overview

Gobblin Games is a browser-based goblin battle royale: 24 autonomous goblins,
procedurally generated hex arenas, deterministic commentary narrated by
The Chronicler. The product surface is a dense live game UI — commentary
feed, goblin rosters, a 7-hex arena map, HP/combat numbers, status chips —
plus landing and marketing pages.

The visual identity is **a field-naturalist's ledger that documents goblins**:
aged paper, forest ink, scholarly apparatus (severity tags, event kinds,
specimen numbering) applied to chaotic goblin trash energy. Botanical ledger
meets goblin brawl.

Two complete themes ship from day one: a warm paper-cream light theme
(default) and a deep green-black dark theme. All color values are OKLCH.
Every foreground/background pair in this document was APCA-verified before
being written down (body text Lc ≥ 75, large text ≥ 45, non-text ≥ 30).

**Token names follow the existing app's CSS custom properties**
(`api/assets/src/main.css`, Tailwind + Maud). New tokens extend that
namespace; adoption in the app is a find-replace, not a redesign.

## Colors

### Foundations

| Token | Light | Dark | Role |
|---|---|---|---|
| `--bg` | `oklch(0.96 0.015 85)` | `oklch(0.16 0.015 160)` | page ground |
| `--surface` | `oklch(0.985 0.008 90)` | `oklch(0.21 0.02 155)` | raised panels, cards |
| `--surface-2` | `oklch(0.93 0.018 85)` | `oklch(0.26 0.025 155)` | wells, hover fills |
| `--fg` | `oklch(0.22 0.03 150)` | `oklch(0.92 0.02 120)` | body text, headings |
| `--muted` | `oklch(0.45 0.035 150)` | `oklch(0.86 0.03 140)` | meta, timestamps, disabled text |
| `--border` | `oklch(0.8 0.02 120)` | `oklch(0.32 0.02 155)` | hairlines, separators |
| `--border-strong` | `oklch(0.68 0.025 140)` | `oklch(0.45 0.025 155)` | input borders, emphasis |

Verified: `fg` on `bg` Lc 95.9 / −90.8 · `muted` on `bg` Lc 77.3 / −78.8.

### Brand and accent

| Token | Light | Dark | Role |
|---|---|---|---|
| `--green` | `oklch(0.45 0.09 155)` | `oklch(0.72 0.1 155)` | brand, section heads on light |
| `--blue` | `oklch(0.45 0.1 250)` | `oklch(0.86 0.09 250)` | links, info, alliances, focus ring |
| `--gold` | `oklch(0.48 0.09 80)` | `oklch(0.87 0.09 85)` | **the single CTA accent** — aged brass |

`--blue` as link text: Lc 77.4 / −78.0. `--gold` is a high-signal color:
one primary CTA per viewport, plus the `event-item` hazard-cell stroke.
Never a large wash, never body text.

**CTA decision, made deliberately:** primary buttons are **gold fills with
ink labels** (`on-gold`: paper on brass, Lc −78.7 / gold on green-black,
≈ −80). Blue is reserved for links, info semantics, alliances, and the
focus ring. Rejected alternative: blue CTAs — they collide with the
link/alliance semantics that carry meaning in the commentary feed.

### Semantic — run status and outcomes

| Token | Light | Dark | Meaning |
|---|---|---|---|
| `--running` | `oklch(0.44 0.11 150)` | `oklch(0.85 0.1 150)` | match in progress |
| `--waiting` | `oklch(0.44 0.09 65)` | `oklch(0.86 0.1 85)` | lobby / not started |
| `--finished` | `= --muted` | `= --muted` | match complete |
| `--success` | `oklch(0.44 0.11 150)` | `oklch(0.85 0.1 150)` | positive outcome |
| `--danger` | `oklch(0.44 0.14 25)` | `oklch(0.89 0.09 25)` | death, elimination, errors |
| `--warning` | `oklch(0.44 0.09 65)` | `oklch(0.86 0.1 85)` | caution, moderate severity |
| `--info` | `oklch(0.44 0.11 250)` | `oklch(0.86 0.08 250)` | neutral notices |

### Event-kind hues (commentary chips)

Each event kind owns a hue plus a verified soft tint. Chip = soft
background + full-strength text (all pairs ≥ Lc 75 in both themes).

| Kind | Text (light / dark) | Soft bg (light / dark) |
|---|---|---|
| death | `oklch(0.44 0.14 25)` / `oklch(0.89 0.09 25)` | `oklch(0.95 0.025 25)` / `oklch(0.2 0.05 25)` |
| alliance | `oklch(0.44 0.11 250)` / `oklch(0.86 0.08 250)` | `oklch(0.95 0.03 250)` / `oklch(0.22 0.05 250)` |
| item | `oklch(0.45 0.08 80)` / `oklch(0.87 0.09 85)` | `oklch(0.95 0.03 85)` / `oklch(0.22 0.045 85)` |
| affliction | `oklch(0.45 0.08 320)` / `oklch(0.86 0.07 320)` | `oklch(0.95 0.025 320)` / `oklch(0.23 0.05 320)` |
| phobia | `oklch(0.45 0.07 200)` / `oklch(0.85 0.08 200)` | `oklch(0.95 0.025 200)` / `oklch(0.22 0.05 200)` |
| trauma | `oklch(0.38 0.09 40)` / `oklch(0.89 0.08 40)` | `oklch(0.95 0.02 40)` / `oklch(0.2 0.045 40)` |

The hues are desaturated on purpose. Nothing here fluoresces.

### Severity levels

Severity rides on the event-kind chip as a suffix dot or bold weight,
never a second full chip:

- **mild** — `oklch(0.45 0.035 150)` on `oklch(0.95 0.01 90)` (dark: `oklch(0.86 0.03 140)` on `oklch(0.26 0.02 150)`)
- **moderate** — warning pair
- **severe** — danger pair

### Contrast discipline

- Body text pairs: APCA Lc ≥ 75 both themes (all verified, WCAG ≥ 4.5:1).
- Large text (≥ 24px or ≥ 18.7px bold): Lc ≥ 45.
- Non-text (status dots, focus rings, hex-tile strokes): Lc ≥ 30.
- Disabled controls follow the WCAG 1.4.3 exemption: disabled labels sit at
  `oklch(0.55 0.02 …)` (≈ Lc 59 light) and stay legible but clearly inert.
- Never rely on hue alone: every status and severity carries a text label
  or shape cue.

## Typography

Six roles, one token each, all Google Fonts. Isolation is the point: any
role can be swapped without touching the others. The wordmark role is
that isolation proving itself: Fraunces carried logos until Barrio
arrived, and nothing else changed.

| Role | Font | App token | Use |
|---|---|---|---|
| wordmark | **Barrio** (single weight 400) | `--font-wordmark` | the Gobblin Games logo only. Graffiti tag face — goblin vandalism over the scholarly ledger. Never headlines, never UI, never below 1.5rem. |
| display | **Fraunces** (variable; `SOFT 40, WONK 1`) | `--font-display` | hero + section heads. Isolated token: a future dedicated wordmark face replaces it at logo sizes only, no other change. |
| body | **Source Serif 4** | `--font-body` | paragraphs, commentary text; screen-legible at 14–16px |
| ui | **Atkinson Hyperlegible** | `--font-mono`'s sibling: add `--font-ui` | buttons, labels, nav, tables |
| mono | **Atkinson Hyperlegible Mono** | `--font-mono` | all data ≤ 12px |
| flavor | **Caveat** | `--font-flavor` | goblin names, Chronicler asides, scrawls. **Never** UI labels, never paragraphs. |

### Scale (modular 1.25, mapped to existing `--fs-*` names)

| Token | Value | Face |
|---|---|---|
| `--fs-hero` | `clamp(2.75rem, 6vw, 4.5rem)` | display, line-height 1.05, letter-spacing −0.02em |
| `--fs-h1` | `clamp(2rem, 4vw, 3rem)` | display, 1.1 |
| `--fs-h2` | `clamp(1.5rem, 3vw, 2rem)` | display, 1.15 |
| `--fs-h3` | `1.25rem` | ui 700, 1.3 |
| `--fs-lead` | `1.125rem` | body, 1.6 |
| `--fs-body` | `1rem` (16px floor) | body, 1.6, max 65–75ch desktop / 35–60ch mobile |
| `--fs-sm` | `0.875rem` | ui, 1.45 |
| `--fs-xs` | `0.75rem` | **mono** — combat numbers, coordinates |
| `--fs-meta` | `0.8125rem` | mono — timestamps, seed hashes, specimen numbers |

### Numbers

Every HP value, damage figure, timer, count, and roster stat gets
`font-variant-numeric: tabular-nums`. Numbers ≤ 12px use mono. Combat
numbers in feed rows are mono `--fs-xs` so digits never reflow mid-fight.

### Flavor discipline

Caveat is the goblin's handwriting. It appears on: goblin names in the
commentary feed, Chronicler asides, roster nicknames, empty-state scrawls.
It never labels a button, never sets a paragraph, never carries
information the user must act on.

## Layout

- **8px baseline grid.** Spacing only from `--gap-*`: xs 4 (inline half-step), sm 8, md 16, lg 24, xl 40.
- **Live game UI** is a dense three-region layout: commentary feed (scroll column, newest at top, mono timestamps), arena map (7 hexes, center + ring), roster strip (24 goblins, specimen-numbered `No. 01–24`, HP bars with tabular numbers). Regions separate by hairline `--border`, not floating cards.
- **Marketing pages** use generous ledger margins, ruled horizontal lines instead of card grids, hero = Fraunces `--fs-hero` with one gold CTA.
- **Ledger apparatus** as structure: specimen numbering, seed hash in `--fs-meta`, ruled rows with even heights, metadata columns at fixed x-positions.
- Rows beat card grids for repeated content (rosters, match lists, event logs).
- Radius scale: `--radius-sm` 4px (chips), `--radius` 8px (default — buttons, cards, inputs), `--radius-md` 12px (panels, modals), pill 999px (status chips).
- Safe area: key info and CTAs within the central 70–80% of the canvas, ≥ 50px from edges on marketing surfaces.

## Elevation

Three named levels. Nothing else. No ad-hoc shadow values anywhere.

| Level | Value | Use |
|---|---|---|
| `--shadow-sm` | `0 1px 2px oklch(0.22 0.03 150 / 0.08)` | resting cards |
| `--shadow-md` | `0 4px 12px oklch(0.22 0.03 150 / 0.12)` | dropdowns, popovers |
| `--shadow-lg` | `0 12px 32px oklch(0.22 0.03 150 / 0.18)` | modals |

Depth order is fixed: page < card < dropdown < modal. A modal is nearer
than a card and stays nearer, whoever edited last.

Separate with whitespace → background shift (`--surface` on `--bg`) →
elevation, in that order. Borders are the last resort, hairline
`--border` only. In dark theme: reduce shadow opacity ~40% and outline
with `--border-strong`; never raise blur. **No glow, ever.**

## Components

### Buttons

Primary / secondary / ghost. All states declared; focus ring is always
`2px solid --blue`, offset 2px — the ring is the one place blue acts
alone, and it is a non-text graphic (Lc ≥ 30 verified).

| State | Primary | Secondary | Ghost |
|---|---|---|---|
| default | gold bg, on-gold label | surface bg, border-strong, fg label | transparent, muted label |
| hover | gold-hover | surface-2 bg | surface-2 bg, fg label |
| focus-visible | 2px blue ring | 2px blue ring | 2px blue ring |
| active | gold-active | surface-2, 1px press | border bg |
| disabled | surface-2 bg, muted label, 70% opacity, cursor not-allowed | same | same |

Disabled labels use the WCAG-disabled exemption (see Colors → contrast
discipline) and must also change cursor and suppress hover styles.

### Cards / panels

`--surface` on `--bg`, `1px solid --border`, `--radius` 8px, `--shadow-sm`.
Static cards never react to hover. Interactive cards strengthen the border
to `--border-strong` on hover. No colored left-border accent callouts, no
shadow stacks.

### Commentary feed rows

Grid: mono timestamp (xs) · speaker name · event text · severity chip.
- Speaker: goblin name in **Caveat**; Chronicler lines in **Fraunces italic**.
- Event text: Source Serif 4 `--fs-sm`, tabular-nums on any combat figure.
- Chip: event-kind soft bg + event-kind text + severity suffix (dot or bold).
- Rows separate with hairline `--border`; equal row heights; newest row
  tinted `--surface-2` while the match runs.
- Feed scrolls inside a `60vh` viewport so one bucket of a 300+ event day
  never stretches the page; nothing is dropped, the cap is the scrollbar.

### Hour picker

Strip above the feed: 12 buttons, one per two-hour bucket of the day.
- Anatomy per cell: event count (mono xs, tabular) → density bar → hour
  label (mono xs). Bar = `N/max` height inside a `--gap-lg` `--surface-2`
  track; resting fill `--muted`, pressed fill `--green`.
- States: default (transparent border) / hover (`--surface-2` +
  `--border-strong`) / focus-visible (2px blue ring, offset 2) /
  pressed (`aria-pressed=true`, same as hover + accent bar + `fg 700` label).
- The bar is a data visualization: labels always show the count, color
  never carries the number alone.
- Selecting a bucket re-renders the feed below; the count row in the panel
  head shows day total, the cells show per-bucket counts.

### Status dots

8px dot + text label, always both. Running `--running`, waiting
`--waiting`, finished `--finished`. Never color alone.

### Hex-arena tiles

Each hex is a pointy-top tile divided into six isosceles triangle cells —
apex at the hex center, base spanning the full edge — matching the game's
`SubTriangle` geometry (cell index = edge direction, clockwise from
top-right). The hex is a neutral container: hazards never block movement
or penalise pathing (q7k), and closures do not exist — a hazard is data
on one cell, not a state of the hex.

| Hex | Fill | Stroke | Content |
|---|---|---|---|
| open | surface | border, 6 hairline cell spokes | goblin count/initial, ui 700 |
| hover | surface-2 | border-strong | — |
| focus-visible | — | 2px blue ring, offset 1px | — |

| Cell | Fill | Stroke | Content |
|---|---|---|---|
| clear | — | — | — |
| hazard | event-kind-soft | event-kind | — |

Hazard kind maps to hue by the game's `AreaEventKind` buckets: Fire →
`--event-death`, Flood → `--event-alliance`, Earthquake →
`--event-trauma`, Storm (blizzard/avalanche/sandstorm) →
`--event-phobia`, Hazard (landslide/heatwave/drought/rockslide/sinkhole)
→ `--event-item`, Other → `--muted`. Severity rides the commentary chip's
suffix dot, not the map.

Color never carries the map alone: a hazard shows as its struck cell —
position plus tint — while clear cells stay empty. Grayscale-readable by
cell geometry alone; the hues are reinforcement.

Arrival: the struck cell's tint draws in 250ms ease-out and ships a
`prefers-reduced-motion` fallback that fills instantly. Hazard scoping to
cells lands with `bd gobblin-games-q6k`; today the sim keeps hazards
area-wide.

### Biome, terrain, and the detail layers

Two data levels, two channels — never two color palettes fighting:

- **Biome (hex) = color.** `AreaDetails.terrain` washes the hex base:
  background tier, one low-chroma tint per `BaseTerrain`.
- **Terrain (cell) = texture.** Each triangle's terrain draws as a
  monochrome texture in `--muted` — forest ticks, wetlands waves,
  grasslands tufts, urban-ruins grid, clearing blank. Texture reads over
  any wash, survives grayscale, and leaves hue free for hazards:
  priority per cell is hazard tint > terrain texture > biome wash.
- **Feature = text.** `SubTile.feature` names appear only in the
  close-up tier; flavor rides an SVG `<title>` tooltip.

Biome tokens: `--biome-grasslands` `oklch(0.95 0.025 115)` (dark
`0.27 0.035 115`), `--biome-wetlands` `oklch(0.925 0.03 235)` (dark
`0.25 0.045 235`), `--biome-forest` `oklch(0.93 0.03 150)` (dark
`0.255 0.045 150`). Add the remaining `BaseTerrain` washes on the same
rules when the map needs them: L 0.92–0.95 / C 0.02–0.04 light, L
0.24–0.28 dark, hue = the obvious family.

Detail tiers (zoom-driven):

| Tier | Reveals |
|---|---|
| Far | biome wash, hex labels, occupancy, hazard cells |
| Mid | + terrain textures per cell |
| Near | + `SubTile.feature` names and goblin position dots (initial in a
  green-ringed circle) in the focused hex; flavor tooltip |

One segmented control (Far · Terrain · Close) sets the tier; clicking a
hex jumps to Near focused on it. The viewBox eases 200ms (instant under
`prefers-reduced-motion`). Same rule as hazards: never color alone —
biome is wash, terrain is shape, text is text.

### Error banners

`--event-death-soft` background, 3px `--event-death` left rule, icon +
text in `--event-death`. Direct copy, no apologies. Always icon + label —
color reinforces, never carries alone.

### Theme switcher

Segmented two-state control (Light | Dark), ui face, `--radius` 8px,
full state set with the blue focus ring. Persist choice; respect
`prefers-color-scheme` on first visit.

### Motion rules (all components)

Any transition: 150–300ms, ease-out in, ease-in out at ~65% of entrance
duration. List stagger 30–50ms max. Every transition or animation ships a
`prefers-reduced-motion: reduce` fallback that preserves the end state and
the information, not just the silence.

## Do's and Don'ts

**Do**
- Use gold for the one primary CTA per viewport; use blue for links, info, alliances, and focus.
- Give every number tabular-nums; drop every ≤12px datum to mono.
- Give every status, severity, and event kind a text or shape label beside its hue.
- Keep Caveat on goblin names and asides only.
- Keep Barrio on the wordmark only — never below 1.5rem, never in UI.
- Separate with whitespace → background shift → elevation, in that order.
- Ship both themes; audit dark independently (all pairs above are separately verified).
- Tag commentary with event kind + severity — the scholarly apparatus is the brand.

**Don't**
- No neon, no fluorescent or safety yellow, no lime. Brass and parchment gold only.
- No pure `#fff` or `#000` — every neutral is tinted by the paper/green hue.
- No purple-indigo gradients, no gradient washes of any kind.
- No glow, no `backdrop-filter` glassmorphism.
- No ad-hoc shadows — named elevation levels only.
- No Caveat on UI chrome or paragraphs.
- No mono for prose — mono is for data.
- No second CTA accent. Gold won; blue is spoken for by links and alliances.
- No color-only signals anywhere in the live UI.

### App token mapping (mechanical adoption)

| This system | Existing app var |
|---|---|
| `--bg` `--surface` `--muted` `--border` `--border-strong` | same names, same meaning |
| `--fg` | same name |
| `--green` (brand) | add new |
| `--blue` | add new (info/links/focus) |
| `--gold` | replaces ad-hoc gold use; retire stray `--accent` blues |
| `--running` `--waiting` `--finished` `--success` `--danger` `--warning` `--info` | same names |
| `--event-*` `--severity-*` | add new (chip system) |
| `--font-display` `--font-body` `--font-mono` | same names; add `--font-ui`, `--font-flavor`, `--font-wordmark` |
| `--fs-*` `--gap-*` `--radius-sm` `--radius` `--radius-md` | same names and values |
| `--shadow-sm/md/lg` | add new; delete ad-hoc box-shadows |
