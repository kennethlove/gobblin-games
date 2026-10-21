# Multi-Round Combat — v2 Design (Multi-Fighter Melee)

**Status:** Draft (design complete, awaiting implementation plan)
**Author:** klove
**Date:** 2026-10-20
**Related:**

- Bead: `gobblin-games-3y1` (this work), blocks `gobblin-games-wpe` (Call Ally)
- Follow-up to: `2026-10-09-multi-round-combat-design.md` (v1, §15)
- Builds on: `2026-04-25-character-alliances-design.md` (allies graph), `2026-10-15-subhex-interaction-spike.md` (same-sub combat scoping)
- Touches: `shared/src/combat_beat.rs`, `characters/src/combat/round_loop.rs`, `characters/src/brains/combat.rs`, `game/src/games/cycle.rs`, timeline/history UI

## 1. Problem

v1 combat is strictly pairs: `CombatRoundLoop` holds exactly two `&mut Character`,
`Combat.combatants` is `[CharacterRef; 2]`, initiative resolves two actors, and the
combat brain scores one opponent. An ally standing right there can never join the
fight, so "Call Ally", uninvited third parties, and any brawl-shaped play are
blocked.

Expand fights into area-scoped brawls with 3+ combatants: two sides, targeting
rules across multiple opponents, allies joining mid-fight, and a round model that
orders N actors instead of two.

## 2. Scope and non-goals

**In scope (3y1):**

- Two-side model for every fight (a 1v1 is a melee with one combatant per side)
- Generalized round model: N actors, initiative-ordered, one beat each per round
- Target selection across an opposing side
- Round-boundary **ally auto-join** (ally in the fight's sub-tile joins their
  side) plus the general **join hook** that later beads call
- Generalized wire types (`Combat`, `CombatResult`, join messages) with
  backwards-compatible deserialization of v1 persisted history
- Round loop restructured for N mutable combatants
- Cower/verdict and flee rules generalized to N

**Out of scope (separate beads under epic `gobblin-games-9vm`):**

- **Call Ally action** (`wpe`) — combat brain action that summons an *absent*
  ally into the fight; calls the join hook with a pre-assigned side
- **Intervening third parties** (`ck7`) — uninvited joins (ally 89%, goblin 10%,
  animal 1%); calls the same join hook with a side derived from their target
- **Phase-spanning fights** (`w9r`), **captivity** (`n6o`), **ambush coupling**
  (`7si`), **replay view** (`vt9`)
- Side-switching (betrayal mid-fight) — a side is fixed at join time
- Free-for-all (no sides) — rejected, see §3

## 3. The side model

Every fight has exactly two sides. Side is assigned once and never changes:

- **Fight start:** attacker's side = A, defender's side = B (today's pair, now
  expressed as `A = [attacker]`, `B = [defender]`).
- **Joining:** a joiner is placed by one rule:
  - *Joined to an existing combatant* (ally auto-join, Call Ally) → that
    combatant's side.
  - *Uninvited* (third parties, later beads) → side **opposite their chosen
    target**: a bully who hates Bob joins the side fighting Bob.
- **Ambiguous allies** (allied to members of *both* sides): the fight sits them
  out — they don't join. `CombatTuning` placeholder; revisit if it matters.

Why sides instead of free-for-all: termination, verdicts, and flee all need a
notion of "who won". With no sides the only endings are last-standing or
stalemate, and cower's spare/execute has no answer for "which opponent decides".
Sides keep the v1 answer: the other side decides. The join hook is still open —
two sides do not block a nosy goblin jumping in, they only fix which side they
land on.

**Capacity:** `CombatTuning::max_combatants` (placeholder, e.g. 8 total) caps
joins; later joiners wait out the fight (or the bead calling the hook retries
next round).

## 4. Combat flow

### 4.1 Entry and round boundaries

Unchanged from v1: outer brain commits `Action::Attack`, the deferred fight
resolves in the cycle after `act_attack`, combat brain runs from then on.

New at each **round boundary** (before the brain pass):

1. **Join scan** — for each living character not in the fight, in the fight's
   sub-tile, who is an ally of a current combatant and the sides aren't at
   capacity → they join (their ally's side). Emits `CombatJoined` (§9).
2. **Exit sweep** — dead and fled combatants are dropped from the active set
   (their beats remain in the recorded rounds).

### 4.2 Round structure for N

1. **Initiative** — every active combatant rolls `d20 + modifiers` (v1 §3.3
   unchanged). Sort descending; ties keep **join order** (deterministic, and for
   a pair reproduces v1's stable first-then-second).
2. **Act in order** — one beat per combatant:
   - pick action *and target* at resolve time (see note below), resolve, append
     beat;
   - skip beats of combatants who died, fled, or cowered earlier in the round;
   - a kill or exit mid-round skips only the *deceased/gone* actor's beat —
     everyone else keeps acting;
   - cower forces the verdict rule (§7).
3. **Termination check** (§6).

*Note on simultaneity:* v1's spec said actions are collected simultaneously, but
the implementation picks each action at resolve time (the second actor sees the
first beat). v2 keeps the **implementation's lazy pick** — it's less state, it's
what ships today, and with N actors "simultaneous" would mean a full extra pass
for no observed benefit. Recorded here so the two specs don't silently disagree.

### 4.3 Targeting rules

The combat brain picks a target among **living members of the opposing side**.
Scoring (placeholder weights in `CombatTuning`):

- **Threat** — who damaged me most recently / most this fight (they're on me).
- **Finishing** — wounded targets score up (goblins finish the weak).
- **Traits** — Aggressive/Reckless push toward the strongest target; cowardly
  traits push toward the weakest.
- **Tie-break** — deterministic (join order), same as initiative.

Hard rules: never target your own side; never target a cowerer (they're waiting
on a verdict, §7); a combatant whose entire opposing side is gone has no target
and the fight is over anyway (§6).

## 5. Flee

Flee stays a per-beat contested roll, generalized:

- The fleer contests against the **fastest living opponent on the opposing
  side** (single defender keeps the v1 shape; "fastest" is deterministic —
  placeholder, could become side-average later).
- Success: the fleer **exits the fight** and relocates per existing sub-tile
  flee rules (unchanged). Their side keeps fighting if it still has members.
- Failure: the beat is lost as in v1.
- The fight ends only when a whole side is gone (§6) — one goblin running never
  ends a 3v3.

## 6. Termination

| Ending | Result |
| --- | --- |
| One side has no living combatants | `Victory { victors }` = the other side's survivors |
| One side fully exited (all fled/spared-out) while the other stands | `Victory { victors }` |
| Both sides stamina-empty, no kill possible | `Draw` |
| Round cap reached (`max_rounds`) | `Draw` |

- Deaths, flees, and spared cowerers all reduce a side's active set; the
  side-empty check runs after every beat and at round end.
- A side can also win by attrition-without-death: everyone on side B runs,
  side A wins. (v1's `Fled`/`Winner` pair results fold into this.)
- Mid-round termination skips only beats of actors already out; a 2v2 where one
  dies in beat 0 still resolves beat 1.

## 7. Cower and verdict in a melee

- A cowerer stops acting (beats suppressed, as v1) and waits for a verdict.
- The verdict falls to the **next opposing-side combatant to act in initiative
  order** after the cower beat; their action is replaced by Spare/Execute. If
  the initiative order has no remaining opposing actor this round, the verdict
  resolves at their first action next round.
- Multiple cowerers each get their own verdict from the next eligible opposing
  actor (one verdict per cowerer per fight, not per round).
- **Execute** = kill → side check.
- **Spare** = the cowerer **exits the fight alive** (frees them from the fight;
  they stay in the area — captivity is `n6o`, not here). The fight continues if
  both sides still have members. v1's "spare ends the fight" becomes "spare ends
  *their* fight", which is the same thing at 1v1.

## 8. Round loop architecture (the borrow problem)

v1's `CombatRoundLoop` holds `first`/`second: &'a mut Character` — impossible for
N combatants at scattered indices in `game.characters`.

**Decision:** the loop takes `&mut Vec<Character>` plus the fight's identity set
(a stable `Vec<Uuid>` of combatants with side tags), not individual references.

- Every actor/target pair is borrowed **on demand**: a `two_at(chars, i, j)`
  helper (sort indices, `split_at_mut`) yields `(&mut Character, &mut Character)`
  for one beat, then drops.
- The brain pass reads immutably; joins scan the vec directly; the flee
  relocation mutates in place.
- Indices stay valid for the whole fight: the cycle runs each fight to
  completion within one phase and nothing else mutates the vec meanwhile. Stale
  ids (target died earlier) resolve to "skip", as today.

A pair fight is just N=2; there is no separate pair code path — v1 tests must
pass against the generalized loop unchanged.

## 9. Wire format

```rust
pub struct Combat {
    pub combat_id: String,
    /// All combatants, in join order. v1's `[CharacterRef; 2]` deserializes
    /// into this as-is (JSON array → Vec).
    pub combatants: Vec<CharacterRef>,
    /// Side membership. `None` for v1 history (implicit [[first], [second]]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sides: Option<[Vec<CharacterRef>; 2]>,
    pub rounds: Vec<CombatRound>,
    pub result: Option<CombatResult>,
}

pub enum CombatResult {
    /// The opposing side is gone; these combatants stand.
    Victory { victors: Vec<CharacterRef> },
    Draw,
}
```

`CombatRound` and `CombatBeat` gain no shape changes: a round already holds
`Vec<CombatBeat>` (comment "at most two" becomes "one per active combatant"),
`CombatBeat.target` is already `Option`, and `order` is the position in the
initiative order (0 = initiative winner).

**Message payloads:**

- New: `MessagePayload::CombatJoined { combat_id, character, side, round }` —
  emitted at the round-boundary join. Separate payload rather than a fake beat
  so join never distorts initiative ordering or `order` semantics.
- `CombatRoundStart` / `CombatBeat` / `CombatEnded { combat_id, result }`
  unchanged in shape; `result` carries the new `CombatResult`.

**History migration:** old persisted `CombatResult` variants (`Winner`, `Fled`,
`Spared`, `Draw`) are decoded via a custom `Deserialize` into the new shape:

| Old | New |
| --- | --- |
| `Winner { character }` | `Victory { victors: [character] }` |
| `Fled { fled, remaining }` | `Victory { victors: [remaining] }` |
| `Spared { winner, spared }` | `Victory { victors: [winner] }` |
| `Draw` | `Draw` |

Tradeoff (declared): old fled/spared games replay as a plain "Victory" — the
fled/spared flavor lives in the significant beats (they were recorded), and UIs
can derive it from the beat stream. The alternative — keeping all four old
variants forever — forks every consumer's match arm permanently. Custom decode
(~30 lines, one place) wins.

Consumers to update: `announcers/src/history.rs` (matches `Winner`),
`api/src/templates/game_detail.rs` (payload filter), timeline UI.

## 10. Tuning additions (`CombatTuning`)

- `max_combatants` — total fight capacity (join cap)
- Target-scoring weights: `brain_target_threat_bonus`,
  `brain_target_wounded_bonus`, `brain_target_strong_bonus`, etc.
- Flee defender rule constant if it becomes configurable (fastest-opponent
  selection is a rule, not a number — likely no new field)
- All placeholders; a tuning pass follows observability (v1 §14 PR 4 pattern).

## 11. Testing strategy

**Unit:**

- Round model: 3-combatant initiative orders by roll, tie falls to join order;
  pair fight under N-loop reproduces v1 beat order exactly
- Targeting: never targets own side or a cowerer; threat bonus picks the recent
  attacker; deterministic tie-break
- Joins: ally in same sub-tile joins at boundary and gets a beat in the *next*
  round; ally in a different sub-tile doesn't; capacity blocks; ambiguous ally
  (both sides) sits out
- Termination: side wiped mid-round → `Victory`, other side's beats after the
  kill still resolve; last member flees → `Victory` for the other side;
  both-sides-empty stamina → `Draw`
- Cower: verdict goes to next opposing actor in initiative order; spare removes
  the cowerer but the fight continues; executed cowerer triggers side check
- Migration: v1 `Combat` JSON (with old `CombatResult`) decodes into the new
  types with correct `Victory` mapping; new `Combat` round-trips

**Integration:**

- Seeded 2v2 brawl runs to a plausible beat stream (joins, targets, termination
  in order, no beats after termination)
- Ally auto-join fires in a real phase run (cycle path, same-sub)
- Existing v1 fight tests pass unchanged (N-loop compatibility)

**Snapshot (insta):**

- Seeded 3-fighter beat stream incl. a join message
- `Combat` serde: new melee + decoded v1 history

## 12. Migration / PR split

1. **PR 1 — substrate:** `two_at` borrow helper + N-fighter `CombatRoundLoop`
   (identity set, side tags); wire types (`Combat.combatants` → `Vec`, `sides`,
   `CombatResult` → `Victory`/`Draw` + custom v1 decode); cycle passes the vec
   instead of two refs. Pair tests green unchanged. No behavior change visible.
2. **PR 2 — targeting + verdict:** combat brain target selection across a side;
   N-cower verdict routing; `CombatActionProvider` signature grows to
   `(actor, opposing_side) -> (action, Option<target>)`; tuning placeholders.
3. **PR 3 — joins:** round-boundary join scan (ally auto-join) + public join
   hook, `CombatJoined` message, capacity cap. `wpe`/`ck7` become thin callers.
4. **PR 4 — consumers + tuning:** history announcer, API template, timeline UI
   (join event, side badges, generalized result), `max_combatants`/target
   weights tuned.

PR 1 is the risky one (borrow restructure + wire change), same shape as v1's PR 1.

## 13. Spec self-review

- **Placeholders:** join capacity, target weights, ambiguous-ally rule are all
  `CombatTuning`/declared-choices; none block the shape.
- **Internal consistency:** §4.2 skip rules match §6 side-empty checks after
  every beat; §7 spare-exit feeds §6's "fully exited" ending; §8 index-stability
  assumption is backed by "fight runs to completion within one phase" (v1 §2).
- **Scope check:** four PRs; `wpe` and `ck7` reduce to calling the §4.1 join
  hook — this spec deliberately makes them thin.
- **Ambiguity:** flee-defender choice (fastest opponent) and ambiguous-ally
  sit-out are decisions made here, flagged as tunable; exact target-score
  formulas deferred to plan/implementation as v1 did with `BeatOutcome`.
