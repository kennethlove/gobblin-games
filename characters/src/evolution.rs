//! Cross-game trait evolution — the trigger layer (`dww.10`).
//!
//! `gain_trait`/`lose_trait` and baseline settle already carry trait intent
//! across game boundaries; this module decides *when* a goblin's lifetime
//! record earns a durable change. Spec §10.5 deferred drift triggers "until
//! an event list exists" — this is that list: a small deterministic table of
//! lifetime-stat thresholds, evaluated once per game entry
//! ([`Character::into_new_game`]) between settle and baseline capture, so an
//! earned trait lands in the new game's baseline and brain derivation.
//!
//! Rules:
//! - **Deterministic** — no RNG. A lifetime record earns exactly the changes
//!   it crosses; every trigger is idempotent (a gain the goblin already has
//!   is skipped by `gain_trait`, a loss of an absent trait is a no-op).
//! - **Caps** — gains stop at [`MAX_TRAITS`] total traits; loss triggers
//!   never strip below [`MIN_TRAITS`] (pity floor: a long-losing goblin
//!   keeps its personality).
//! - **Conflicts** — `gain_trait` enforces `traits::CONFLICTS`; a trigger
//!   whose trait conflicts with one held is skipped, never forced.
//! - Reachable stats are the lifetime fight record (kills, wins, defeats,
//!   draws), the lifetime revival counter (`times_revived`), and the two
//!   lifetime betrayal counters (`betrayals_suffered`,
//!   `betrayals_committed`).

use crate::Character;
use crate::traits::Trait;

/// Trait-count ceiling for evolution gains (creation rolls 2–6; evolution
/// can add a little, never unlimited).
pub const MAX_TRAITS: usize = 8;

/// Loss-trigger floor: evolution never strips a goblin below this many
/// traits.
pub const MIN_TRAITS: usize = 2;

/// Which lifetime stat a trigger watches. All are lifetime totals carried
/// by settle (`Statistics`, dww.1 decision 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stat {
    Kills,
    Wins,
    Defeats,
    Revivals,
    BetrayalsSuffered,
    BetrayalsCommitted,
}

/// What crossing a trigger's threshold does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Gain(Trait),
    Lose(Trait),
}

/// One lifetime-stat threshold and its durable trait change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trigger {
    pub stat: Stat,
    pub threshold: u32,
    pub effect: Effect,
}

/// The trigger table. Flavor reads off the stats: a hardened killer turns
/// Aggressive, a champion turns Resilient, the long-defeated shed Reckless
/// and mellow into Cautious, the serially-victorious lose their Paranoid
/// edge. Tuned like every other table — placeholders until play says
/// otherwise (spec §10.6).
pub const TRIGGERS: &[Trigger] = &[
    Trigger {
        stat: Stat::Kills,
        threshold: 5,
        effect: Effect::Gain(Trait::Aggressive),
    },
    Trigger {
        stat: Stat::Wins,
        threshold: 10,
        effect: Effect::Gain(Trait::Resilient),
    },
    Trigger {
        stat: Stat::Defeats,
        threshold: 10,
        effect: Effect::Lose(Trait::Reckless),
    },
    Trigger {
        stat: Stat::Defeats,
        threshold: 15,
        effect: Effect::Gain(Trait::Cautious),
    },
    Trigger {
        stat: Stat::Wins,
        threshold: 25,
        effect: Effect::Lose(Trait::Paranoid),
    },
    Trigger {
        stat: Stat::Revivals,
        threshold: 2,
        effect: Effect::Gain(Trait::Tough),
    },
    Trigger {
        stat: Stat::BetrayalsSuffered,
        threshold: 2,
        effect: Effect::Gain(Trait::Paranoid),
    },
    Trigger {
        stat: Stat::BetrayalsCommitted,
        threshold: 3,
        effect: Effect::Lose(Trait::Loyal),
    },
];

fn lifetime(character: &Character, stat: Stat) -> u32 {
    match stat {
        Stat::Kills => character.statistics.kills,
        Stat::Wins => character.statistics.wins,
        Stat::Defeats => character.statistics.defeats,
        Stat::Revivals => character.statistics.times_revived,
        Stat::BetrayalsSuffered => character.statistics.betrayals_suffered,
        Stat::BetrayalsCommitted => character.statistics.betrayals_committed,
    }
}

/// Apply every met trigger to `character` via the sanctioned mutation paths
/// (`gain_trait`/`lose_trait`, so changes stamp the advancement log).
/// Gains above [`MAX_TRAITS`] and losses below [`MIN_TRAITS`] are skipped;
/// conflicting gains are skipped by `gain_trait`. Called at the game-entry
/// seam after settle, before baseline capture.
pub fn apply_evolution(character: &mut Character) {
    for trigger in TRIGGERS {
        if lifetime(character, trigger.stat) < trigger.threshold {
            continue;
        }
        match trigger.effect {
            Effect::Gain(trait_) => {
                if character.traits.len() >= MAX_TRAITS {
                    continue;
                }
                let _ = character.gain_trait(trait_);
            }
            Effect::Lose(trait_) => {
                if character.traits.len() <= MIN_TRAITS {
                    continue;
                }
                character.lose_trait(trait_);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goblin() -> Character {
        let mut c = Character::new("Boblin".to_string(), None, None);
        c.traits.clear();
        c
    }

    #[test]
    fn gains_fire_when_threshold_met() {
        let mut c = goblin();
        c.statistics.kills = 5;
        apply_evolution(&mut c);
        assert!(c.traits.contains(&Trait::Aggressive));
    }

    #[test]
    fn below_threshold_is_noop() {
        let mut c = goblin();
        c.statistics.kills = 4;
        apply_evolution(&mut c);
        assert!(!c.traits.contains(&Trait::Aggressive));
    }

    #[test]
    fn gains_are_idempotent_across_entries() {
        let mut c = goblin();
        c.statistics.kills = 5;
        apply_evolution(&mut c);
        apply_evolution(&mut c);
        assert_eq!(
            c.traits.iter().filter(|t| **t == Trait::Aggressive).count(),
            1
        );
    }

    #[test]
    fn conflicting_gain_is_skipped_not_forced() {
        let mut c = goblin();
        c.traits.push(Trait::Cautious); // conflicts with Aggressive
        c.statistics.kills = 5;
        apply_evolution(&mut c);
        assert!(!c.traits.contains(&Trait::Aggressive));
        assert!(c.traits.contains(&Trait::Cautious));
    }

    #[test]
    fn gains_stop_at_max_traits() {
        let mut c = goblin();
        // Eight held traits = at cap; the Aggressive gain must not land.
        c.traits = vec![
            Trait::Defensive,
            Trait::Loyal,
            Trait::Friendly,
            Trait::Tough,
            Trait::Cunning,
            Trait::Builder,
            Trait::ResourcefulForager,
            Trait::LoneWolf,
        ];
        c.statistics.kills = 5;
        apply_evolution(&mut c);
        assert!(!c.traits.contains(&Trait::Aggressive));
    }

    #[test]
    fn loss_respects_pity_floor() {
        let mut c = goblin();
        c.traits.push(Trait::Reckless);
        c.traits.push(Trait::Aggressive); // at MIN_TRAITS
        c.statistics.defeats = 10;
        apply_evolution(&mut c);
        assert!(c.traits.contains(&Trait::Reckless), "floor protects it");
    }

    #[test]
    fn loss_strips_when_above_floor() {
        let mut c = goblin();
        c.traits.push(Trait::Reckless);
        c.traits.push(Trait::Aggressive);
        c.traits.push(Trait::Cunning); // above the floor
        c.statistics.defeats = 10;
        apply_evolution(&mut c);
        assert!(!c.traits.contains(&Trait::Reckless));
    }

    #[test]
    fn loss_stamps_advancement_log() {
        let mut c = goblin();
        c.traits.push(Trait::Reckless);
        c.traits.push(Trait::Aggressive);
        c.traits.push(Trait::Cunning);
        c.statistics.defeats = 10;
        apply_evolution(&mut c);
        assert!(
            c.advancement_log
                .iter()
                .any(|e| matches!(e, crate::advancement::AdvancementEvent::TraitLost { trait_ } if *trait_ == Trait::Reckless)),
            "evolution losses ride the sanctioned lose_trait path"
        );
    }

    #[test]
    fn revivals_trigger_fires_at_two_not_one() {
        let mut c = goblin();
        c.statistics.times_revived = 1;
        apply_evolution(&mut c);
        assert!(
            !c.traits.contains(&Trait::Tough),
            "one revival is not yet toughening"
        );

        c.statistics.times_revived = 2;
        apply_evolution(&mut c);
        assert!(c.traits.contains(&Trait::Tough));
    }

    #[test]
    fn betrayals_suffered_trigger_fires_at_two_not_one() {
        let mut c = goblin();
        c.statistics.betrayals_suffered = 1;
        apply_evolution(&mut c);
        assert!(
            !c.traits.contains(&Trait::Paranoid),
            "one suffered betrayal is not paranoia yet"
        );

        c.statistics.betrayals_suffered = 2;
        apply_evolution(&mut c);
        assert!(c.traits.contains(&Trait::Paranoid));
    }

    #[test]
    fn betrayals_committed_trigger_fires_at_three_not_two() {
        let mut c = goblin();
        c.traits.push(Trait::Loyal);
        c.traits.push(Trait::Cunning); // above the MIN_TRAITS loss floor
        c.traits.push(Trait::Tough);
        c.statistics.betrayals_committed = 2;
        apply_evolution(&mut c);
        assert!(
            c.traits.contains(&Trait::Loyal),
            "two committed betrayals do not shed loyalty yet"
        );

        c.statistics.betrayals_committed = 3;
        apply_evolution(&mut c);
        assert!(!c.traits.contains(&Trait::Loyal));
    }

    #[test]
    fn evolution_settles_into_the_next_baseline() {
        let mut c = goblin();
        c.statistics.kills = 5;
        // Entry seam order: settle, evolve, capture — the gain must be in
        // the captured baseline (i.e. `into_new_game` output keeps it).
        let entered = c.into_new_game();
        assert!(entered.traits.contains(&Trait::Aggressive));
        assert!(
            entered
                .game_baseline
                .as_ref()
                .expect("entry captures a baseline")
                .traits
                .contains(&Trait::Aggressive),
            "evolved trait is durable baseline, not a per-game fluke"
        );
    }
}
