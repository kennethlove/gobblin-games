# One Player, One Team — Spike Notes

**Bead:** dww.11 (spike) · **Date:** 2026-10-13 · **Status:** findings, implementation not started

Scope per the bead's scope update: creator-multi already ships (creators field
as many owned goblins as they like, at creation and via later joins). This spike
asks whether a *guest* player can field a whole team, and whether any player's
goblins can be pinned to one team instead of scattering.

## Current state

- **Per-player limit:** `Game.max_goblins_per_player` (default
  `DEFAULT_MAX_GOBINS_PER_PLAYER = 1`), enforced in `join_game`
  (`api/src/characters.rs`) as `mine >= player_limit && !is_creator`. So guests
  can already bring multiple goblins when the creator raises the per-game
  allowance; the default just makes it one. A stale doc comment on
  `join_game` still says "the caller has no goblin in this game yet" — the
  code is the limit, not a hard one.
- **Join is per-goblin:** `POST /games/{id}/join` (HTML form and JSON API)
  enters exactly one character per call. Creation (`CreateGame.characters`)
  joins a chosen list in one request.
- **Team assignment is per-join:** `fewest_team` balances *each* join
  independently. A player joining three goblins lands them on roughly
  teams 1, 2, 1 — never one team. Same for the creator's post-creation joins
  and for the creation draft (`idx % team_count` round-robin in
  `routes/games.rs`).
- **Ready rule** (`schemas/game.surql`): the game is ready when the roster is
  exactly `team_count × goblins_per_team` AND all `team_count` teams are
  represented. `fewest_team`'s tie-to-lowest-slot behavior is what makes bot
  fill divide evenly.
- **Team lives on the character:** `character.team` (1..=8 slots), set at
  join time; `playing_in` edges carry no team of their own.

## Collisions (if one player's goblins must share a team)

1. **`fewest_team` scatters by design.** It has no notion of "who" is
   joining, only counts. Pinning requires an alternate assignment path plus a
   per-team capacity check — `fewest_team` cannot overfill only because it
   always picks the minimum; a pin deliberately targets one slot.
2. **Capacity check doesn't exist.** Nothing stops a pinned join from
   overfilling a team; the ready rule would simply never fire. A
   `members_of(team) < goblins_per_team` gate is required (added in the
   prototype).
3. **Creation draft round-robins.** A creator using the draft who wants "my
   goblins are team 3" gets teams 1..n instead. Draft would need a pin mode.
4. **Default limit of 1.** A guest can't field a team without the creator
   raising `max_goblins_per_player` to `goblins_per_team`. Either document
   "creator opts in" or introduce a distinct knob.
5. **Partial teams + strangers.** If a team-filler brings only 2 of 3 slots,
   `fewest_team` will route *other* players' joins to that team to fill it —
   silently breaking the "whole team = one player" story. Pinning and
   balancing fight each other; any real feature must decide which wins.
6. **Gameplay/balance (not explored in code):** one human steering a full
   team gets the in-team alliance bias for free — coordination advantage over
   scattered players. The bead already flags this; spike adds no numbers.

Nothing in *game* logic (cycle, alliances, combat, livery colors) cares who
steers a goblin — teams are color/livery groupings, so a pinned team is
invisible to simulation code.

## Prototype findings

Implemented in the spike workspace (`api` crate, ~60 lines):

- `join_game` gains `pinned_team: Option<u32>`: validates `1..=team_count`,
  counts current team members, rejects with 409 when full, else assigns the
  pin; `None` keeps `fewest_team` behavior unchanged.
- Form (`GameJoinForm.team: Option<String>`) and JSON (`JoinGame.team`) both
  carry the pin; the two auto-join paths (creation draft, bot fill) pass
  `None`.
- New test `test_pinned_team_fields_one_players_whole_team`: three pinned
  joins land on team 2, fourth pinned join → 409, out-of-range pin → 400,
  unpinned joins still balance. Full `cargo test -p api` green.

**What it proved:** the mechanism is small and mechanical. No schema change
(team already lives on the character), no game-logic change, one capacity
query. The hard part is not plumbing — it's the product semantics in
"Collisions" 5 and the limit default in 4.

## Estimate for a real implementation

Smallest version: the prototype plus

- `api`: optional pin already covered; add pin support to the HTML join form
  (a team `<select>` on the join UI) and the create-game draft UI;
  `~3 files`.
- `shared`: either document that "fielding a team" = creator raises
  `max_goblins_per_player`, or add `team_seats`-style knob. No schema
  migration either way (join-time writes only).
- `api/templates`: join form + game detail roster display changes (show which
  player owns which goblins — currently invisible).
- Tests: extend `join_tests.rs` (done in prototype for the core path).

Roughly 1–2 PRs; a day of work including UI, less without UI.

## Open questions (product)

1. **Strangers vs pins:** when a pinned team is partially filled, should
   `fewest_team` be allowed to fill the remainder with other players' joins
   (today's prototype behavior: yes, balancing wins), or should the team stay
   reserved for the filler to complete?
2. **Limit knob:** should "guest fields a team" be the default
   (`max_goblins_per_player` default follows `goblins_per_team`), a
   per-game creator opt-in (current default stays 1), or a separate "team
   seats" concept?
3. **Team full before all my goblins join:** pin order matters — should the
   UI let a player reserve a team with one join and fill it later, or be
   atomic (all-or-nothing multi-join)?
4. **Balance:** does one human + team alliance bias need a counterweight, or
   is the coordination advantage acceptable carnival chaos?
5. **Visibility:** should the roster show "these three goblins share a
   player" (it currently doesn't)?
