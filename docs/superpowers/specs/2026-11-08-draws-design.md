# Draws: agreed stop, mutual exhaustion, and rare "fingers crossed"

- **Status:** Draft (design complete, implemented behind tuning knobs)
- **Author:** klove (designed with Claude), implemented 2026-11-08
- **Related:** gobblin-games-cz9 (design,gameplay); supersedes the
  blood-judgment rule from gobblin-games-6wd for mutual exhaustion
- **Spec:** 2026-10-20 combat rework §6 (draw rule), this spec extends it

## Problem

Fights previously had one honest ending and one accidental one: someone
dies, or a rare blood-tie / round-cap draw. Design wants fights that can
*stop* — three ways.

## Design

1. **Agreed stop.** A fighter wants out when spent: zero stamina, or
   blood at/below the draw floor (`CombatTuning::draw_blood_floor`,
   default 100). When EVERY living member of BOTH sides wants out, the
   goblins agree to stop: the fight ends in a `CombatResult::Draw` and
   the log carries "The fight ends in a draw."
2. **Mutual exhaustion.** The same rule covers "both ran out of energy,
   neither can keep up": stamina-zero on both sides *is* the agreed stop.
   The old rule (judge the standoff on remaining blood, fresher side
   wins) is removed — cz9 supersedes it for this case. Tests pin the
   honest path with `fingers_crossed_chance = 0`.
3. **Fingers crossed.** On an agreed stop, the most desperate fighter
   (lowest blood) may cross its fingers and lunge instead
   (`CombatTuning::fingers_crossed_chance`, default 0.05): the sneak
   lands, the fight ends in that side's `Victory`, and the feed carries
   a `CombatBeat` with `BeatOutcome::FingersCrossed` and the line
   "{name} crosses their fingers and lunges instead of calling it off!"

## Measurement

`cargo run -p game --example lethality_sim -- 1000` (default format
3x8): outcome mix stays on target (62/26/12), median finish day 7,
99.7% on or before day 10, ~4.4 agreed-stop draws per game (~15% of
fights), fingers-crossed fires on ~5% of agreed stops. The round-loop
distribution test now bands draws at <30% (they are first-class
outcomes, not accidents).

## Knobs

| Knob | Default | Meaning |
|---|---|---|
| `draw_blood_floor` | 100 | blood at/below counts as "wants out" |
| `fingers_crossed_chance` | 0.05 | chance an agreed stop turns into a sneak |

## Follow-ups

- Failed-sneak variant (the lunge misses, the draw stands) — today the
  sneak always lands.
- Grovel-flavored stop before blood hits the floor (wounded, not just
  spent) if play-testing wants earlier stops.
