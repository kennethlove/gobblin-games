# Spike: roguelike choice density in the run loop

Date: 2026-10-29 · Bead: gobblin-games-6bl · Status: model confirmed (klove), not scheduled

The arenas are already randomized — the roguelike evolves the **goblins**, all computer-controlled in-run. Everything below is decided; numbers are placeholders tuned with observability like every other odds table (spec §10.6 precedent).

## 1. Outcome tables at the finish seam (`post_game_recovery`)

A game ends two ways (`check_for_winner`): one team has living members while the other has zero, or nobody lives (draw). So the only alive goblins at finish are winners.

| table | who rolls | shape |
|---|---|---|
| **Survivor** | goblins alive at finish (winning side) | **65% permanent bonus trait**, else nothing |
| **Revival** | every dead goblin (losing side entirely + winners who died mid-game) | **nothing 65% / scar 25% / boon 10%** — retune of the existing table; the −1 stat-ache penalty row is dropped |

- Dead-carrying-winners roll revival, not the survivor table: dying is dying.
- Draw: nobody rolls survivor; all dead roll revival.
- Existing machinery stays: affliction→trait conversion (trauma→Fragile etc.) and the event boon/curse rolls at revival (temporary traits last N=2 games via `tick_temporary_traits`).
- Scar/boon grants use the bqs effect-bundle trait model; both tables live in `characters/src/revival.rs`, invoked once per game at the finish seam.

## 2. XP → level → owner choice (answers e1d's open questions)

- **All** goblins earn XP for each game played, plus a win bonus. (What earns = decided.)
- On level-up the goblin gets a **pending advancement**: the owner picks one permanent change from a small menu — bonus trait / +1 stat / cure a scar. Permanent; affects future games. (What buys = decided.)
- Enforcement = a gate in `join_game` alongside the existing ones: a goblin with a pending advancement cannot join its next game until the owner spends it. Harmless because entry is explicit only — nothing auto-enrolls goblins into new games, so no in-run wait and no backlog.
- The goblin stays computer-controlled in-run; the choice is meta-progression between games.

## 3. Creation couple

Two picks, creation stays ten seconds: **pick 1-of-3 offered starting traits**, and **pick which stat is your best** (one bump, the rest roll as today via `from_rng_with_traits`).

## Out of scope (as explored)

Player-facing spectator economy (patron spend), point-buy stat blocks beyond the best-stat pick, XP spend lists beyond the advancement menu. Nothing here is built until explicitly scheduled; likely slices = revival retune + survivor table first (smallest), XP/levels second, creation picks third.
