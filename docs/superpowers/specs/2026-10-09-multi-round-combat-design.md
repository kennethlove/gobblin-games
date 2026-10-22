# Multi-Round Combat — v1 Design

**Status:** Approved (brainstorming complete, awaiting implementation plan)
**Author:** klove
**Date:** 2026-10-09
**Related:**

- Supersedes the single-contest model in `characters/src/combat/resolve.rs` (`Character::attacks`)
- Builds on: `2026-05-03-stamina-combat-resource-design.md` (per-swing stamina, bands), `2026-05-03-break-mid-swing-design.md` (equipment breakage)
- Touches: `shared/src/combat_beat.rs`, `shared::messages` (`MessagePayload::CombatSwing`), `characters/src/combat`, `characters/src/brains` (new combat brain), `game::games` (cycle drives rounds), timeline UI

## 1. Problem

Combat resolves in a single contest: `Character::attacks(target)` runs one `attack_contest`, applies damage or a counter-attack, and the fight is over. One swing decides everything.

That collapses to a coin flip most of the time. Stamina bands, weapon quality, wounds, and afflictions barely matter when a whole fight is one roll. The spectator timeline shows a single `CombatBeat` — no arc, no drama, no story to watch. And brains make one decision ("attack") with no room to reconsider: flee, surrender, use an item, beg.

Expand combat into multiple rounds contained inside one phase. Each round, both combatants' combat brains pick an action, initiative decides who acts first, and the fight runs until a termination condition (§6).

## 2. Scope and non-goals

**In scope (v1):**

- Round loop inside `characters`, driven by the game cycle
- Per-round combat brain: Attack, Use Item, Flee, Cower (surrender)
- Initiative per round with modifiers
- Flee as a contested roll, available any round
- Cower + winner's spare/kill decision
- Redesigned `CombatBeat` / `CombatRound` / `Combat` wire types
- Termination: winner standing, one flees, or both stamina-empty (draw)
- Stamina cost tuning for fight-length encounters

**Out of scope (v2 / filed separately):**

- Phase-spanning combat (fights resume next phase) — **dropped 2026-10-22**:
  a fight runs until it is over, inside the phase it started in (bead
  gobblin-games-w9r closed)
- Multi-fighter melee (3+ combatants); combat stays pairs — **shipped
  2026-10-22** (melee v2 design, PRs #85–#89)
- Call Ally action (combat brain keeps a reserved slot, not wired) —
  **shipped 2026-10-22** (PR #90)
- AI-controlled *other* combatants intervening in a fight they weren't invited to
- Surrender-then-capture (cower either ends in death or freedom, never
  captivity) — **dropped 2026-10-22**: outcomes stay victory, death, or
  escape (bead gobblin-games-n6o closed)

## 3. Combat flow

### 3.1 Entry

Outer brain picks a target and commits `Action::Attack(target)`. From that moment until the fight ends, the **combat brain** decides everything for that character. The outer brain does not re-enter mid-fight. The defenders' combat brain decides for them, as well.

A fight is a `Combat` between exactly two characters in the same area (unchanged pairing rules).

### 3.2 Round structure

All rounds of a fight live inside the current phase (a phase is 2 hours of game time; a fight never crosses a phase boundary — if the phase ends mid-fight the fight is force-terminated by the current phase-end behavior, deferred to plan if it needs care). Rounds only represent a half minute of game time, so crossing phases is unlikely. That would require combat to continue for 240 rounds straight.

One round, in order:

1. **Combat-brain pass** — each living, present combatant picks an action (§4). Actions are collected simultaneously.
2. **Initiative** — both combatants roll `d20 + modifiers` (§3.3). Higher total acts first. Tie: no advantage, both act in stable (deterministic) order.
3. **First actor's action resolves** — produces a `CombatBeat`.
4. **Second actor's action resolves** — produces a `CombatBeat`.
5. **Termination check** (§6) — if the fight ended, remaining beats of the round are skipped and the round closes.

### 3.3 Initiative modifiers

The initiative roll is `d20 + mod`, where `mod` combines:

| Modifier | Value |
| --- | --- |
| Speed attribute bonus | `(speed - 10) / 5`, signed |
| Stamina band | Fresh: +2, Winded: +0, Exhausted: -2 |
| Weapon speed | Weapon's existing speed/bonus value, halved |

Exact numbers are `CombatTuning` placeholders, tuned post-observability (§11). The shape matters, not the values: gear and fatigue shift who acts first, and momentum emerges from rolls rather than a fixed opener.

### 3.4 Actor/target within a swing

Within an initiative-ordered action, the acting character is the "actor" of that beat and the other is the "target", regardless of who initiated the overall fight. The existing `attack_contest` resolution (damage, counter-attack/PerfectBlock, fumble) applies unchanged per swing beat. Actions like "flee" don't have a target so that field is optional.

## 4. The combat brain

A small, dedicated brain that runs per round for each combatant. It is **not** the full utility pipeline — just combat-relevant options scored from local state:

```rust
pub enum CombatAction {
    Attack,
    UseItem(ItemRef),
    Flee,
    Cower,           // surrender
    // CallAlly reserved for v2
}
```

Scoring inputs (placeholder weights, tunable):

- **Attack** — baseline aggression; raised by stamina headroom, lowered when badly wounded.
- **Use Item** — heal/self-item when wounded or afflicted and an item is usable; otherwise not scored.
- **Flee** — raised by low HP, Exhausted band, wounded allies absent, opponent much stronger; lowered when already cornered. Goblins are somewhat cowardly: flee is always on the menu.
- **Cower** — raised when near-dead with no escape and the opponent is unrelenting; flavored by traits (a cowardly goblin cowers sooner).

The combat brain runs *after* the outer brain commits, every round, for both fighters.

## 5. Flee

- Available to any combatant in any round.
- Resolution: contested `d20 + speed` (fleer) vs `d20 + speed` (chaser), each with stamina-band modifiers. Fleer wins: they leave the area if they can; the fight ends with that combatant fled. Fleer wins but has no movement left: Same as losing since they can't escape the area. Fleer loses: the round continues normally (they lose their action beat to the failed escape).
- Fleeing is a beat: `CombatAction::Flee` with outcome `Fled` or `FailedFlee`.
- Stress applies as with any violence exposure (fleeing from a deadly fight is stressful but less than killing).

## 6. Termination

A fight ends when exactly one of:

| Ending | Result |
| --- | --- |
| One combatant dead | `CombatResult::Winner { character }` |
| One combatant flees successfully | `CombatResult::Fled { fled, remaining }` |
| Both stamina-empty with no kill possible | `CombatResult::Draw` |

- **Both stamina-empty** → draw: neither can keep swinging; they disengage. (Stamina band `Exhausted` and at zero: no attack actions available, so the loop detects no-progress and closes.)
- **Mid-round termination:** a kill or successful flee skips the remaining beats of the current round. Nothing to do once the enemy is dead or gone.
- An optional safety cap on rounds (e.g. 12) prevents pathological infinite stalemates; it terminates in a `Draw`. Value in `CombatTuning`. Every round drains stamina, however, so this cap should be evaluated.

## 7. Cower and the spare/kill decision

Cower is the cowering character's beat: they stop fighting and beg. The fight does **not** auto-resolve.

The opponent's next action is a forced decision beat — the combat brain's scoring is overridden by a spare/kill choice:

```rust
pub enum CowerOutcome {
    Spared,
    Executed,
}
```

Decision inputs (placeholder scoring):

- Own aggression/traits (some goblins always execute)
- Alliance / same-team / betrayal context with the cowerer (allies spare, betrayed execute)

Sequence in beats: Bob's `Cower` beat, then Alice's `Spare`/`Execute` beat. The timeline shows both — begging and verdict as separate dramatic beats. If Alice executes, that is a kill and ends the fight (§6); if she spares, the fight ends with both alive, result `CombatResult::Spared { winner, spared }`. A spared combatant doesn't have to flee to another area.

## 8. Equipment breakage per round

Break-mid-swing (`2026-05-03-break-mid-swing-design.md`) applies **per round**. Any round's swing can shatter a weapon or wear out a shield or armor; the existing penalty and `WearReport` mechanics carry into that round's beat unchanged. Durability drain accumulates across rounds, which makes long fights naturally more dangerous for gear.

## 9. Wire format

`shared/src/combat_beat.rs` is redesigned. The current beat was written for single-swing combat and is not satisfactory as-is.

```rust
/// One character's chosen action within a round.
pub enum CombatActionKind {
    Attack,
    UseItem { item: ItemRef },
    Flee,
    Cower,
    Spare,
    Execute,
}

/// Full record of one action in a round.
pub struct CombatBeat {
    pub round: u32,
    pub order: u8,               // position within the round (0 = initiative winner). Unnecessary if insert order is maintained for CombatBeats.
    pub actor: CharacterRef,     // who acted
    pub action: CombatActionKind,
    pub target: Option<CharacterRef>, // None for e.g. failed self-target edge cases
    pub outcome: BeatOutcome,    // per-action outcome enum
    pub wear: Vec<WearReport>,   // kept; equipment breakage per round (§8)
    pub stress: StressReport,    // kept
    pub stamina_costs: ...,      // kept, per-actor now
}

pub struct CombatRound {
    pub number: u32,
    pub beats: Vec<CombatBeat>,
}

/// The whole fight.
pub struct Combat {
    pub combatants: [CharacterRef; 2],
    pub rounds: Vec<CombatRound>,
    pub result: CombatResult,
}

pub enum CombatResult {
    Winner { character: CharacterRef },
    Fled { fled: CharacterRef, remaining: CharacterRef },
    Spared { winner: CharacterRef, spared: CharacterRef },
    Draw,
}
```

`BeatOutcome` is a per-action enum (miss/wound/kill/fumble stay as swing outcomes; flee gets `Fled`/`FailedFlee`; cower gets acknowledgment; spare/execute get their outcomes). Exact variant set is finalized during implementation — the shape above is the contract.

**Message shape:** `MessagePayload::CombatSwing(CombatBeat)` is replaced by round-granular emission: `MessagePayload::CombatRoundStart { combat_id, round }` and `MessagePayload::CombatBeat(CombatBeat)`, plus `MessagePayload::CombatEnded { combat_id, result }`. (Naming can shift at plan time; the beat/round split is the decision.)

`combat_id`: fights need an identifier so the UI can group beats into one timeline card. A UUID minted at fight start.

## 10. Persistence and timeline replay

The full `Combat` (all rounds, all beats) is serialized and kept in game history for timeline replay — users should be able to rewatch a fight.

To keep stored history bounded, beats carry a **significance** distinction (§3 in brainstorming: "record significant parts, log the rest"):

- **Significant** (always persisted with full detail): kills, cower, spare/execute, successful flee, equipment breaks, fight start/end.
- **Ordinary** (persisted as compact beat records, expanded only in the live message stream): routine swings, misses, counters.

Both kinds live in `Combat.rounds`; significance is a flag on the beat (`#[serde(default)] pub significant: bool` or an enum), letting storage/UI down-rank routine beats without dropping them.

## 11. Storage / code layout

- **Round loop lives in `characters/src/combat`** as a state machine: `Combat` owns rounds and exposes `advance_round(&mut self, rng) -> CombatRound` (and `is_over()`). The game cycle drives it — one `advance_round` per step per fight, deterministic order, emitting messages per round.
- The combat brain lives in `characters/src/brains/combat.rs` (new), called by the round loop.
- `Character::attacks()` remains as the single-swing primitive the round loop calls for `Attack` beats; its signature may grow to accept round context for wear/stress bookkeeping.
- Tuning: extend `CombatTuning` (`characters/src/combat_tuning.rs`) with initiative modifiers, flee contest values, combat-brain weights, round cap, stamina costs tuned for fight-length encounters (§12).

## 12. Stamina interaction

Per-swing stamina costs already exist. Multi-round fights make encounter length the dominant drain, so costs need retuning:

- Attacker and target both pay per beat (existing asymmetric design kept).
- Initiative modifier uses stamina band (§3.3), so fatigue shifts who acts first.
- Flee contest uses stamina band too — an Exhausted goblin rarely escapes.
- Draw-by-stamina (§6) makes stamina a real terminal resource, so cost values matter more. Placeholder costs ship first; tuning pass follows observability.

## 13. Testing strategy

Unit:

- Round loop: initiative winner acts first; tie breaks deterministically
- Termination: kill ends fight mid-round (remaining beats skipped); flee ends fight; both-empty produces `Draw`
- Combat brain: wounded goblin with items prefers heal; hopeless goblin cowers; cowardly traits raise flee weight
- Cower: winner's spare/execute beat follows the cower beat; alliance context biases spare
- Equipment: break can fire on any round; durability accumulates
- Serde round-trips for `CombatBeat`, `CombatRound`, `Combat`, `CombatResult`

Integration:

- Seeded fight runs to completion with a plausible beat stream (rounds in order, no beats after termination)
- Stamina-empty pair produces `Draw` and both leave standing
- Flee success/failure distribution is plausible over many seeds
- Persisted `Combat` replays identically through the timeline path

Snapshot (insta):

- Full beat stream for a seeded multi-round fight
- `Combat` serde with significance flags

## 14. Migration / PR split

1. **PR 1 — substrate:** new wire types (`CombatBeat` redesign, `CombatRound`, `Combat`, `CombatResult`) in `shared`; round loop state machine in `characters`; `attacks()` retained as swing primitive; game cycle drives rounds and emits new messages. `CombatSwing` deprecated then removed.
2. **PR 2 — combat brain:** per-round brain (attack/use-item/flee/cower), spare/execute decision, tuning placeholders.
3. **PR 3 — wire/UI:** timeline renders rounds as grouped cards, significance down-ranking, replay from persisted `Combat`.
4. **PR 4 — tuning:** initiative/flee/stamina cost tuning after observability on the earlier PRs.

PR 1 may pull minimal flee/cower stubs so the loop's termination paths are testable before the brain lands — confirm at plan-writing time.

## 15. Out of scope (v2 follow-ups)

- **Phase-spanning combat** — fight persists across a phase boundary; revisit after v1 ships.
- **Multi-fighter melee** — fights become area-scoped brawls; needs targeting, ally-joins, and a different round model.
- **Call ally** — combat brain action that summons an ally into the fight (promotes a pair fight into a melee; gated on melee work).
- **Intervening third parties** — a nearby non-combatant joining uninvited. Can be an ally (89%), another goblin (10%), or an animal (1%).
- **Captivity** — cower could lead to capture/tied-up states instead of spare/execute.
- **Ambush-window coupling** — ambush as a pre-combat opener feeding
  initiative advantage. **Decided 2026-10-22:** a free first swing
  against sleeping targets only (round-0 beat before round 1) — see
  melee v2 design §14; initiative-bonus and unaware-target variants
  rejected.
- **Combat replay UI beyond timeline** — a dedicated fight-replay view.

## 16. Spec self-review

- **Placeholders:** all numeric values (initiative modifiers, brain weights, flee contest, round cap, stamina costs) are declared tunable in `CombatTuning`; none block the shape of the design.
- **Internal consistency:** rounds inside a phase (§2) matches cycle driving (§11); termination (§6) covers every beat-outcome path including cower (§7); wire shape (§9) supports every beat described in §5–§7.
- **Scope check:** four PRs; PR 1 is the risky one (wire redesign + loop + cycle integration) but is a clean substrate the others hang off.
- **Ambiguity:** `BeatOutcome` variant set and exact message variant names are intentionally deferred to plan/implementation; the beat/round/combat structure and termination rules are fixed here.
