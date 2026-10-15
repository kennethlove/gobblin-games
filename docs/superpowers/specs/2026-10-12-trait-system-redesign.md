# Trait System Redesign — v1 Design

**Status:** Approved (design decisions complete — §10 — awaiting implementation plan)
**Author:** klove
**Date:** 2026-10-12
**Related:**
- Supersedes the trait mechanics in `characters/src/traits.rs` (unit enum + hand-listed conflict pairs + scattered consumers)
- Builds on: `2026-05-03-stamina-combat-resource-design.md`, `2026-05-04-addiction-design.md`, `2026-05-04-trauma-design.md`
- Touches: `characters` (traits, attributes, brains, revival, afflictions), `game` (cycle, combat), XP work, character-advancements baseline work, trait-evolution
- Mental model: RimWorld — pawns have base capabilities, health states that can scar or become permanent, and behavioral traits that shape decisions.

## 1. Problem

The current trait system works but doesn't express the game's actual model:

1. **"Trait" means one narrow thing.** Eighteen unit variants, mostly behavioral. Acquired physical changes (a limp, scarred lungs) and personality (Friendly) are different systems bolted together — revival pushes scars into the same enum, but nothing carries data (`limp` would need `-25% movement`, which a unit variant can't say).
2. **Effects are hard-coded per consumer.** Traits carry `ThresholdDelta`s into brain derivation; alliance affinity is a `match`; betrayal cadence checks `Treacherous` by name; combat brain hard-codes trait weights. Adding one trait's effect means finding every consumer.
3. **Dead and missing traits.** `Reckless` and `LoneWolf` are fully wired but absent from `TRAIT_POOL` (unobtainable). `Builder` and `ResourcefulForager` have zero mechanics. `Asthmatic`, `Nearsighted`, `Tough` are wired to almost nothing. No invariant test catches pool/variant drift.
4. **No trait-change API.** Conflicts and duplicates are enforced only inside creation generation and revival's picker. Any other path can push conflicting traits. There is no central place to answer "can this character gain this trait?"
5. **Attributes are fixed and opaque.** Eight flat stats rolled at creation, no relationship to traits, and names that don't match how mechanics read them (`agility` doubles as initiative speed; `defense` is about to become `toughness`).

## 2. The model

Three layers, one direction of dependency:

```
effective value = (base attribute + flat modifiers) × percent modifiers     [floor: 0]
```

| Layer | Lifetime | Examples |
|---|---|---|
| **Base attributes** | Rolled at creation, permanent (XP may raise them) | movement 36, strength, perception |
| **Traits** | Acquired/lost; cross games | limp (−25% movement), Cautious (flee +84), scarred lungs |
| **Afflictions** | Per-game temporary conditions | poison gas lung, wounds, hangover |

Rules:

- **Floor 0.** A trait can reduce an attribute to 0. No negative effective values.
- **Single path.** Every consumer reads effective values through one accessor (`effective_movement()`, `effective_strength()`, …). Raw attributes are never read by mechanics directly. Percent-type modifiers stack multiplicatively; flat modifiers stack additively under the multiply.
- **Wounds are percentage-based** (decided, §10.1) — same formula as traits, no consumer special case.

## 3. Traits: unified, effect-carrying

One `Trait` type for everything — behavioral, physical, boons, curses. A trait is a **named bundle of typed effects**:

```rust
pub struct Trait {
    pub id: TraitId,           // stable key for serde + tables
    pub label: String,         // display ("a limp", "a lone wolf")
    pub kind: TraitKind,       // Boon | Curse | Personality (display/tuning only)
    pub effects: Vec<Effect>,
}

pub enum Effect {
    AttrMod { attr: Attribute, flat: i32, percent: i32 },  // percent: -25 = ×0.75
    BehaviorWeight { axis: BehaviorAxis, delta: i32 },      // flee, spare, betray, alliance…
    Unlock { action: ActionKind },                          // future-proofing (Call Ally…)
}
```

- `BehaviorAxis` enumerates the brain/alliance/betrayal weights traits currently touch — the existing hard-coded matches (`alliance_affinity`, combat-brain weights, `ThresholdDelta` brain derivation, betrayal cadence) become data reads from the trait's effects.
- Personality traits are just traits whose effects are all `BehaviorWeight`. **Friendly → Cautious drift is trait loss + trait gain**, same mechanism as a limp.
- Conflict rules move from hand-listed trait pairs to **effect-level rules**: at most one `AttrMod { attr, percent < 0 }` source per attribute from curses of the same family, no duplicate `TraitId`, plus a small explicit incompatibility table for flavor pairs (Resilient/Fragile) that have no mechanical overlap.

### Acquisition and loss

Four paths, one API — `Character::gain_trait(trait) / lose_trait(id)`, enforcing conflicts:

1. **Creation roll** — 2–6 traits, weighted pool, coherent with attributes (§6).
2. **Affliction conversion (revival)** — **design A: consume.** Each affliction kind has a fixed per-kind conversion chance to a permanent trait form (table in `CombatTuning`-style config struct, plain numbers, not per-instance tunable). Roll at revival: success removes the affliction and grants its trait; failure the affliction is gone with nothing. Mental afflictions convert too — trauma may harden into a phobia-trait. The affliction never crosses the game boundary; only the trait does.
3. **Event boons/curses** — in-game events grant trait candidates with a persistence chance at revival. Example: Boblin enters with an injured lung, heals it with a potion, dies to a cliff — revival rolls two new traits: boon `healed lungs`, curse `sprained ankle`. "Everything about the character should evolve between pre- and post-game."
4. **Mid-game drift** — boundaries are more likely, but mid-game changes are possible: a brutal betrayal can make someone Treacherous before the game ends. Drama first; mechanics are just `gain_trait`/`lose_trait` at event time.

### XP purchases — not traits

XP purchases are **guaranteed and immediate** (a +10% HP purchase applies now). They change base attributes/HP directly, not through the trait layer. Traits are for *conditional, chance-driven, or flavor-carrying* changes; XP is a straight purchase. (XP system itself: `e1d`.)

## 4. Effective-value accessors

```rust
fn effective_attribute(&self, attr: Attribute) -> u32 {
    let base = self.attributes.get(attr);                     // e.g. 36
    let flat: i32 = mods.flat_for(attr).sum();                // e.g. +0
    let mult = 1.0 + mods.percent_for(attr).sum() / 100.0;    // limp: 0.75
    (((base + flat) as f64) * mult).max(0.0) as u32            // floor 0
}
```

- All consumers (pathing, combat initiative, brain scoring, survival) go through accessors. No scattered arithmetic.
- Percent modifiers apply to the summed flat-adjusted base — `(base + flat) × percent`.
- Affliction-derived effects use the same path (a temporary affliction contributes `AttrMod`s while active), so no consumer knows the difference between a limp and a sprained ankle during the game.

## 5. Affliction → trait conversion table

Per-affliction-kind fixed odds, plain constants in one config struct (not per-instance tunable — overboard at this stage). Shape:

| Affliction | Permanent trait | Odds (placeholder) |
|---|---|---|
| Poisoned lungs | Scarred lungs (−% max stamina) | 50% |
| Frozen | Chilblains (−% movement at night) | 40% |
| Cut/wound | Scar (flat −toughness) | 30% |
| Trauma | Phobia-trait | 60% |
| Broken bone | Limp variant | 70% |

Numbers are placeholders tuned with the rest (mirrors existing tuning patterns). Table covers only kinds that *have* a permanent form; others convert at 0%.

## 6. Attributes

Current set: `movement, strength, defense, bravery, intelligence, persuasion, luck, agility` + `is_hidden` flag. Decisions:

- **`agility` → `speed`** — it's read as reaction/initiative speed (combat rounds already use it); name it what it does.
- **`defense` → `toughness`** — absorbs damage math; name reflects body resilience.
- **`+ perception`** — new axis: sight range, ambush detection, trap spotting. Gives `Nearsighted` a real home (−perception).
- **Sanity** already exists as a derived value (`effective_sanity()` from mental conditions) — not an attribute; leave derived.
- **Not set in stone** — further axes may emerge (§10).
- **Creation coherence: point-buy/reroll-with-traits.** Attributes and traits are rolled together so they're consistent (a goblin with `limp` rolls low movement; a `Tough` goblin trends toward high toughness). One generation pass, not two independent rolls.

Keep single `movement` for now; model `Attribute` as an enum so adding `Swim` (etc.) later is additive (**decided, §10.2**). No new axes beyond `perception` in this redesign (**§10.3**); skills/stealth parked as out of scope (**§10.4**).

## 7. Persistence and baseline (with `7hw`, implemented)

`7hw` landed the baseline model (`characters/src/baseline.rs`, `GameBaseline` persisted on `Character`):

- Reset at entry is both **settlement** (restore baseline → re-apply earned deltas) and **refresh** (capture fresh baseline for the new game).
- Carry rules as implemented: trait **gains** persist, trait **losses** don't; attributes use a **high-water rule** (positive deltas = earned, negative deltas = per-game debuffs, dropped); lifetime fight record = baseline + increments.
- Derivation order: settle traits → re-derive brain → per-game clears.

Implications this redesign must honor:

1. **Negative advancements must be trait effects.** Raw negative attribute mutations are indistinguishable from debuffs and get dropped by settle. A genuine curse reducing movement is an `AttrMod` on a trait — which the effect model already provides. State this as a hard invariant: *attributes only ever move up outside of trait/affliction effects.*
2. **XP purchases (`e1d`) must be non-negative** under the high-water rule, or the delta mechanism needs an explicit advancement registry. Recommendation: non-negative for v1; the registry only if a purchase ever needs a cost-like negative.
3. **Intentional trait loss is invisible to snapshot-diffing.** Settle currently treats a missing trait as "lost, restore it" — so mid-game drift (§3, Friendly → Cautious) would be undone at the next settle. **Decided (§10.8): the design adopts an explicit advancement-event log** — `gain_trait`/`lose_trait` stamps an advancement event as it happens; the log is the delta, the snapshot is only the floor. XP purchases and conversion events host here too.
4. Cut in game 1 → scar in game 2 is "conversion succeeded at revival, trait advanced into the baseline".

## 8. Migration

1. Inventory current 18 variants → new `TraitId`s; effects data-written to match today's behavior exactly (alliance affinities, threshold deltas, betrayal cadence, combat-brain weights, phobia/fixation modifiers become `Effect` rows). Characterize as **behavior-preserving refactor first**.
2. Fix pool drift: `Reckless`, `LoneWolf` join `TRAIT_POOL`; wire-or-drop `Builder`, `ResourcefulForager`; give `Asthmatic`/`Nearsighted`/`Tough` real effects (Asthmatic ↔ stamina bands; Nearsighted ↔ perception; Tough ↔ toughness) — or drop them.
3. Serde: old persisted `traits: ["Friendly", …]` arrays deserialize into trait ids (`#[serde(untagged)]` shim or one-time mapping table). PascalCase aliases keep old rows loading.
4. Introduce `effective_*` accessors and migrate consumers off raw attribute reads (mechanics grep: `attributes.movement`, `.strength`, etc.).
5. `gain_trait`/`lose_trait` become the only mutation paths; `generate_traits` and revival `pick_eligible` route through them.

PR split at plan time: (1) trait model + effect data + accessors behavior-preserving, (2) acquisition/conversion/drift paths + revival table, (3) attribute renames + perception + creation coherence, (4) dead-variant cleanup.

## 9. Testing strategy

- **Invariant:** every `TraitId` reachable at creation OR documented as event/revival-only; effect-level conflicts enforced by `gain_trait` (property test: random gain/lose sequences never violate conflicts, never duplicate ids).
- **Accessors:** floor-0 (multi-curse stack to 0, not underflow); `(base + flat) × percent` order; affliction and equivalent trait produce identical effective values.
- **Conversion:** seeded revival rolls per kind produce table-conformant rates over N seeds; consumed afflictions gone, traits present; failed conversion leaves neither.
- **Drift:** mid-game betrayal → Treacherous with immediate behavior change; boundary drift persists, mid-game drift persists if it happened before death.
- **Migration:** old serialized trait arrays load; behavior parity — golden test comparing alliance/betrayal/brain outputs before/after refactor on fixed seeds.
- **Pool coverage:** the missing invariant test (§1.3) — `TRAIT_POOL` ∪ explicit-event-only set == all variants.

## 10. Decisions (resolved with the author)

1. **Wounds: percentage-based** — served by the same formula as traits; no flat special case.
2. **Multiple movement types: deferred** — single `movement` now, `Attribute` enum so `Swim` and friends are additive later; flood/swim hazard design gets its own pass.
3. **Attribute axes: perception ships, nothing else yet** — revisit after perception lands.
4. **Skills: parked, out of scope** — stealth is an effective-attribute behavior until a learn-by-doing system exists.
5. **Mid-game drift: boundary-only by default** — mechanics support mid-game changes (they add drama), but no drift triggers ship until an event list exists.
6. **Conversion odds: fixed placeholders fine** — tuned with observability like every other table.
7. **TraitKind display: Boon/Curse show in UI** (character-sheet flavor), `Personality` tag internal-only for now.
8. **Advancement-event log adopted** (§7.3) — explicit gain/lose stamps; hosts XP purchases and conversion events; fixes loss-intent vs settle.
9. **High-water invariant confirmed** (§7.1) — raw attributes only ever move up outside trait/affliction effects; curses always travel as `AttrMod` trait effects; XP purchases non-negative.

## 11. Out of scope

- **XP system mechanics** (`e1d`) — only the boundary (purchases ≠ traits) is defined here.
- **Skills / stealth / learn-by-doing** — future system.
- **Multiple movement types** — parked pending §10.2.
- **Trait UI presentation** (character sheet rendering of effect bundles) — small once the model exists; separate if it grows.
