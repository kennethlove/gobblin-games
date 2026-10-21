//! Golden capture for the trait data migration (spec §9 "Migration: golden
//! test comparing alliance/betrayal/brain outputs before/after refactor on
//! fixed seeds").
//!
//! Recorded against UNMODIFIED main behavior (commit e40257d3). The trait
//! effect-data migration must leave every line here byte-identical: run
//! `GOLDEN_DUMP=1 cargo test -p characters trait_golden` and diff the dump.
//!
//! Intentional exceptions, each a pool-coverage change (bqs.6 precedent):
//! the `gen:` lines were re-recorded when Reckless and LoneWolf joined
//! `TRAIT_POOL` (they were unobtainable before, so generation output had
//! to change for them to appear), and re-recorded again when Builder and
//! ResourcefulForager moved from PENDING into `TRAIT_POOL` (weights 4
//! each). Every other line class must stay byte-identical.

use crate::brains::PersonalityThresholds;
use crate::brains::combat::CombatBrain;
use crate::combat_tuning::CombatTuning;
use crate::traits::{ALL_TRAITS, Trait, generate_traits, geometric_mean_affinity, threshold_of};
use rand::SeedableRng;
use rand::rngs::StdRng;

/// Deterministic actor fixture: entropy-rolled attributes are pinned so the
/// golden is byte-stable (attributes feed the outmatched-flee check).
fn fixture(name: &str) -> crate::Character {
    let mut c = crate::Character::new(name.to_string(), None, None);
    c.attributes.strength = 30;
    c.attributes.toughness = 50;
    c.attributes.bravery = 50;
    c.attributes.intelligence = 50;
    c.attributes.persuasion = 50;
    c.attributes.luck = 50;
    c.attributes.speed = 50;
    c.stamina = 100;
    c.max_stamina = 100;
    c
}

/// Trait sets exercised through every trait-sensitive consumer.
fn sets() -> Vec<(&'static str, Vec<Trait>)> {
    vec![
        ("empty", vec![]),
        ("friendly", vec![Trait::Friendly]),
        ("loyal_treacherous", vec![Trait::Loyal, Trait::Treacherous]),
        ("refusers", vec![Trait::Paranoid, Trait::LoneWolf]),
        ("agg_reckless", vec![Trait::Aggressive, Trait::Reckless]),
        ("cautious", vec![Trait::Cautious]),
        ("resilient_fragile", vec![Trait::Resilient, Trait::Fragile]),
        ("cunning_dim", vec![Trait::Cunning, Trait::Dim]),
        (
            "physical",
            vec![Trait::Asthmatic, Trait::Tough, Trait::Nearsighted],
        ),
        ("survival", vec![Trait::Builder, Trait::ResourcefulForager]),
        ("treacherous", vec![Trait::Treacherous]),
        ("aggressive", vec![Trait::Aggressive]),
        ("defensive", vec![Trait::Defensive]),
        ("reckless", vec![Trait::Reckless]),
        ("lone_wolf", vec![Trait::LoneWolf]),
        ("paranoid", vec![Trait::Paranoid]),
        ("loyal", vec![Trait::Loyal]),
        ("resilient", vec![Trait::Resilient]),
        ("fragile", vec![Trait::Fragile]),
        ("dim", vec![Trait::Dim]),
        (
            "kitchen_sink",
            vec![
                Trait::Loyal,
                Trait::Tough,
                Trait::Cunning,
                Trait::Friendly,
                Trait::Cautious,
            ],
        ),
    ]
}

/// Every captured line as `name -> value`. Byte-stable across migration.
pub fn captured_lines() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // 1. Creation generation across fixed seeds.
    for seed in 0..16u64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let rolled = generate_traits(&mut rng);
        out.push(format!(
            "gen:{seed} -> {:?}",
            rolled.iter().map(|t| t.label()).collect::<Vec<_>>()
        ));
    }

    // 2. Per-trait data reads: label, affinity, refusal, threshold delta.
    for t in ALL_TRAITS {
        out.push(format!(
            "trait:{t:?} -> label={} affinity={} refusal={} delta={:?}",
            t.label(),
            t.alliance_affinity(),
            crate::traits::refuses_alliance(*t),
            threshold_of(*t),
        ));
    }

    // 3. Geometric-mean affinity per set.
    for (name, set) in sets() {
        out.push(format!("geom:{name} -> {}", geometric_mean_affinity(&set)));
    }

    // 4. Derived brain thresholds (traits + +/-20% variance, fixed seeds).
    for (name, set) in sets() {
        for seed in [0u64, 1, 7] {
            let mut rng = StdRng::seed_from_u64(seed);
            let th = PersonalityThresholds::from_traits(&set, &mut rng);
            out.push(format!("thresholds:{name}:{seed} -> {th:?}"));
        }
    }

    // 5. Combat brain action matrix: states x trait sets.
    let tuning = CombatTuning::default();
    let brain = CombatBrain::new(tuning);
    let states: [(&str, u32, u32); 4] = [
        ("healthy", 1000, 100),
        ("wounded", 400, 100),
        ("exhausted", 1000, 0),
        ("hopeless", 100, 0),
    ];
    for (sname, blood, stamina) in states {
        for (name, set) in sets() {
            let mut actor = fixture(&format!("actor-{name}"));
            actor.traits = set.clone();
            actor.blood = blood;
            actor.stamina = stamina;
            let mut opponent = fixture(&format!("opp-{name}"));
            opponent.traits = set.clone();
            out.push(format!(
                "action:{sname}:{name} -> {:?}",
                brain.choose_action(&actor, &[&opponent])
            ));
        }
    }

    // 6. Spare/execute verdict matrix: relationships x temperaments.
    let relations: [(&str, bool, bool); 4] = [
        ("ally", true, false),
        ("team", false, true),
        ("both", true, true),
        ("stranger", false, false),
    ];
    for (rname, is_ally, same_team) in relations {
        for (name, set) in sets() {
            let mut actor = fixture(&format!("actor-{name}"));
            actor.traits = set.clone();
            let mut cowerer = fixture(&format!("cowerer-{name}"));
            if is_ally {
                cowerer.id = uuid::Uuid::new_v4();
                actor.allies.push(cowerer.id);
            }
            if same_team {
                actor.team = 1;
                cowerer.team = 1;
            }
            out.push(format!(
                "verdict:{rname}:{name} -> {:?}",
                brain.choose_verdict(&actor, &cowerer)
            ));
        }
    }

    // 7. Betrayal cadence constant (trait-driven interval).
    out.push(format!(
        "betrayal:interval -> {}",
        crate::alliances::TREACHEROUS_BETRAYAL_INTERVAL
    ));

    out
}

/// Run with `GOLDEN_DUMP=1` to (re)record; otherwise asserts against the
/// embedded golden captured pre-migration.
#[test]
fn trait_effect_migration_golden() {
    let actual = captured_lines();
    if std::env::var("GOLDEN_DUMP").is_ok() {
        let joined = actual.join("\n");
        std::fs::write("/tmp/trait_golden.txt", &joined).expect("write dump");
        eprintln!("dumped {} lines to /tmp/trait_golden.txt", actual.len());
    }
    assert_eq!(
        actual.join("\n"),
        TRAIT_GOLDEN,
        "golden capture drifted — trait behavior must be byte-identical across migration"
    );
}

/// Recorded from unmodified main (e40257d3) before the effect-data
/// migration; `gen:` lines re-recorded for the Reckless/LoneWolf pool
/// addition and again for the Builder/ResourcefulForager pool addition
/// (see module doc).
const TRAIT_GOLDEN: &str = r#"gen:0 -> ["fragile", "defensive", "nearsighted", "a builder", "loyal", "cautious"]
gen:1 -> ["dim", "reckless", "aggressive", "resilient", "tough", "loyal"]
gen:2 -> ["tough", "a lone wolf"]
gen:3 -> ["loyal", "resilient", "tough", "cunning", "nearsighted"]
gen:4 -> ["defensive", "loyal", "fragile", "asthmatic", "friendly"]
gen:5 -> ["cunning", "aggressive"]
gen:6 -> ["cunning", "fragile"]
gen:7 -> ["loyal", "tough", "resilient", "aggressive"]
gen:8 -> ["cunning", "paranoid", "reckless", "treacherous", "resilient"]
gen:9 -> ["cautious", "nearsighted", "loyal", "fragile"]
gen:10 -> ["aggressive", "resilient", "tough"]
gen:11 -> ["tough", "friendly", "nearsighted"]
gen:12 -> ["defensive", "asthmatic", "friendly", "dim"]
gen:13 -> ["friendly", "cautious", "tough", "cunning"]
gen:14 -> ["friendly", "tough", "a resourceful forager", "cunning", "asthmatic", "loyal"]
gen:15 -> ["cautious", "a builder", "tough", "friendly", "dim", "a lone wolf"]
trait:Aggressive -> label=aggressive affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: -5, mid_health_limit: -10, low_sanity_limit: -2, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 2 }
trait:Defensive -> label=defensive affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: 10, mid_health_limit: 10, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: -2 }
trait:Cautious -> label=cautious affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: 15, mid_health_limit: 15, low_sanity_limit: 10, mid_sanity_limit: 10, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: -3 }
trait:Reckless -> label=reckless affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: -10, mid_health_limit: 0, low_sanity_limit: -10, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 4 }
trait:Friendly -> label=friendly affinity=1.5 refusal=false delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 0 }
trait:Loyal -> label=loyal affinity=1.4 refusal=false delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 0 }
trait:Paranoid -> label=paranoid affinity=0.5 refusal=true delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 0 }
trait:LoneWolf -> label=a lone wolf affinity=0.6 refusal=true delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 0 }
trait:Treacherous -> label=treacherous affinity=1.2 refusal=false delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 0 }
trait:Resilient -> label=resilient affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: -3, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: -3 }
trait:Fragile -> label=fragile affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 5, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 3 }
trait:Cunning -> label=cunning affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: -5, high_intelligence_limit: -5, psychotic_break_threshold: 0 }
trait:Dim -> label=dim affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 10, high_intelligence_limit: 5, psychotic_break_threshold: 0 }
trait:Asthmatic -> label=asthmatic affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 0 }
trait:Nearsighted -> label=nearsighted affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 0 }
trait:Tough -> label=tough affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 0 }
trait:Builder -> label=a builder affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 0 }
trait:ResourcefulForager -> label=a resourceful forager affinity=1 refusal=false delta=ThresholdDelta { low_health_limit: 0, mid_health_limit: 0, low_sanity_limit: 0, mid_sanity_limit: 0, high_sanity_limit: 0, movement_limit: 0, low_intelligence_limit: 0, high_intelligence_limit: 0, psychotic_break_threshold: 0 }
geom:empty -> 1
geom:friendly -> 1.5
geom:loyal_treacherous -> 1.296148139681572
geom:refusers -> 0.5477225575051661
geom:agg_reckless -> 1
geom:cautious -> 1
geom:resilient_fragile -> 1
geom:cunning_dim -> 1
geom:physical -> 1
geom:survival -> 1
geom:treacherous -> 1.2
geom:aggressive -> 1
geom:defensive -> 1
geom:reckless -> 1
geom:lone_wolf -> 0.6
geom:paranoid -> 0.5
geom:loyal -> 1.4
geom:resilient -> 1
geom:fragile -> 1
geom:dim -> 1
geom:kitchen_sink -> 1.1599622586540013
thresholds:empty:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 9 }
thresholds:empty:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 8 }
thresholds:empty:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 8 }
thresholds:friendly:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 9 }
thresholds:friendly:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 8 }
thresholds:friendly:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 8 }
thresholds:loyal_treacherous:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 9 }
thresholds:loyal_treacherous:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 8 }
thresholds:loyal_treacherous:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 8 }
thresholds:refusers:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 9 }
thresholds:refusers:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 8 }
thresholds:refusers:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 8 }
thresholds:agg_reckless:0 -> PersonalityThresholds { low_health: 5, mid_health: 32, extreme_low_sanity: 1, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 16 }
thresholds:agg_reckless:1 -> PersonalityThresholds { low_health: 5, mid_health: 35, extreme_low_sanity: 1, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 15 }
thresholds:agg_reckless:7 -> PersonalityThresholds { low_health: 4, mid_health: 24, extreme_low_sanity: 1, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 14 }
thresholds:cautious:0 -> PersonalityThresholds { low_health: 39, mid_health: 60, extreme_low_sanity: 20, low_sanity: 33, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 5 }
thresholds:cautious:1 -> PersonalityThresholds { low_health: 39, mid_health: 65, extreme_low_sanity: 22, low_sanity: 32, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 5 }
thresholds:cautious:7 -> PersonalityThresholds { low_health: 33, mid_health: 44, extreme_low_sanity: 17, low_sanity: 27, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 5 }
thresholds:resilient_fragile:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 12, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 9 }
thresholds:resilient_fragile:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 13, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 8 }
thresholds:resilient_fragile:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 10, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 8 }
thresholds:cunning_dim:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 87, psychotic_break_threshold: 9 }
thresholds:cunning_dim:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 73, psychotic_break_threshold: 8 }
thresholds:cunning_dim:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 86, psychotic_break_threshold: 8 }
thresholds:physical:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 9 }
thresholds:physical:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 8 }
thresholds:physical:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 8 }
thresholds:survival:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 9 }
thresholds:survival:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 8 }
thresholds:survival:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 8 }
thresholds:treacherous:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 9 }
thresholds:treacherous:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 8 }
thresholds:treacherous:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 8 }
thresholds:aggressive:0 -> PersonalityThresholds { low_health: 16, mid_health: 32, extreme_low_sanity: 8, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 11 }
thresholds:aggressive:1 -> PersonalityThresholds { low_health: 16, mid_health: 35, extreme_low_sanity: 8, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 11 }
thresholds:aggressive:7 -> PersonalityThresholds { low_health: 14, mid_health: 24, extreme_low_sanity: 6, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 10 }
thresholds:defensive:0 -> PersonalityThresholds { low_health: 33, mid_health: 54, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 6 }
thresholds:defensive:1 -> PersonalityThresholds { low_health: 33, mid_health: 59, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 6 }
thresholds:defensive:7 -> PersonalityThresholds { low_health: 28, mid_health: 40, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 6 }
thresholds:reckless:0 -> PersonalityThresholds { low_health: 11, mid_health: 43, extreme_low_sanity: 1, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 13 }
thresholds:reckless:1 -> PersonalityThresholds { low_health: 11, mid_health: 47, extreme_low_sanity: 1, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 13 }
thresholds:reckless:7 -> PersonalityThresholds { low_health: 9, mid_health: 32, extreme_low_sanity: 1, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 12 }
thresholds:lone_wolf:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 9 }
thresholds:lone_wolf:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 8 }
thresholds:lone_wolf:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 8 }
thresholds:paranoid:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 9 }
thresholds:paranoid:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 8 }
thresholds:paranoid:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 8 }
thresholds:loyal:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 9 }
thresholds:loyal:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 8 }
thresholds:loyal:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 8 }
thresholds:resilient:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 7, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 5 }
thresholds:resilient:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 7, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 5 }
thresholds:resilient:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 5, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 5 }
thresholds:fragile:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 15, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 37, low_intelligence: 82, psychotic_break_threshold: 12 }
thresholds:fragile:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 16, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 31, low_intelligence: 69, psychotic_break_threshold: 12 }
thresholds:fragile:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 12, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 37, low_intelligence: 81, psychotic_break_threshold: 11 }
thresholds:dim:0 -> PersonalityThresholds { low_health: 22, mid_health: 43, extreme_low_sanity: 10, low_sanity: 22, mid_sanity: 39, low_movement: 8, high_intelligence: 42, low_intelligence: 93, psychotic_break_threshold: 9 }
thresholds:dim:1 -> PersonalityThresholds { low_health: 22, mid_health: 47, extreme_low_sanity: 11, low_sanity: 21, mid_sanity: 30, low_movement: 9, high_intelligence: 35, low_intelligence: 78, psychotic_break_threshold: 8 }
thresholds:dim:7 -> PersonalityThresholds { low_health: 19, mid_health: 32, extreme_low_sanity: 8, low_sanity: 18, mid_sanity: 28, low_movement: 8, high_intelligence: 42, low_intelligence: 91, psychotic_break_threshold: 8 }
thresholds:kitchen_sink:0 -> PersonalityThresholds { low_health: 39, mid_health: 60, extreme_low_sanity: 20, low_sanity: 33, mid_sanity: 39, low_movement: 8, high_intelligence: 32, low_intelligence: 77, psychotic_break_threshold: 5 }
thresholds:kitchen_sink:1 -> PersonalityThresholds { low_health: 39, mid_health: 65, extreme_low_sanity: 22, low_sanity: 32, mid_sanity: 30, low_movement: 9, high_intelligence: 26, low_intelligence: 65, psychotic_break_threshold: 5 }
thresholds:kitchen_sink:7 -> PersonalityThresholds { low_health: 33, mid_health: 44, extreme_low_sanity: 17, low_sanity: 27, mid_sanity: 28, low_movement: 8, high_intelligence: 31, low_intelligence: 76, psychotic_break_threshold: 5 }
action:healthy:empty -> Attack
action:healthy:friendly -> Attack
action:healthy:loyal_treacherous -> Attack
action:healthy:refusers -> Attack
action:healthy:agg_reckless -> Attack
action:healthy:cautious -> Attack
action:healthy:resilient_fragile -> Attack
action:healthy:cunning_dim -> Attack
action:healthy:physical -> Attack
action:healthy:survival -> Attack
action:healthy:treacherous -> Attack
action:healthy:aggressive -> Attack
action:healthy:defensive -> Attack
action:healthy:reckless -> Attack
action:healthy:lone_wolf -> Attack
action:healthy:paranoid -> Attack
action:healthy:loyal -> Attack
action:healthy:resilient -> Attack
action:healthy:fragile -> Attack
action:healthy:dim -> Attack
action:healthy:kitchen_sink -> Attack
action:wounded:empty -> Attack
action:wounded:friendly -> Attack
action:wounded:loyal_treacherous -> Attack
action:wounded:refusers -> Attack
action:wounded:agg_reckless -> Attack
action:wounded:cautious -> Flee
action:wounded:resilient_fragile -> Attack
action:wounded:cunning_dim -> Attack
action:wounded:physical -> Attack
action:wounded:survival -> Attack
action:wounded:treacherous -> Attack
action:wounded:aggressive -> Attack
action:wounded:defensive -> Attack
action:wounded:reckless -> Attack
action:wounded:lone_wolf -> Attack
action:wounded:paranoid -> Attack
action:wounded:loyal -> Attack
action:wounded:resilient -> Attack
action:wounded:fragile -> Attack
action:wounded:dim -> Attack
action:wounded:kitchen_sink -> Flee
action:exhausted:empty -> Attack
action:exhausted:friendly -> Attack
action:exhausted:loyal_treacherous -> Attack
action:exhausted:refusers -> Attack
action:exhausted:agg_reckless -> Attack
action:exhausted:cautious -> Attack
action:exhausted:resilient_fragile -> Attack
action:exhausted:cunning_dim -> Attack
action:exhausted:physical -> Attack
action:exhausted:survival -> Attack
action:exhausted:treacherous -> Attack
action:exhausted:aggressive -> Attack
action:exhausted:defensive -> Attack
action:exhausted:reckless -> Attack
action:exhausted:lone_wolf -> Attack
action:exhausted:paranoid -> Attack
action:exhausted:loyal -> Attack
action:exhausted:resilient -> Attack
action:exhausted:fragile -> Attack
action:exhausted:dim -> Attack
action:exhausted:kitchen_sink -> Attack
action:hopeless:empty -> Cower
action:hopeless:friendly -> Cower
action:hopeless:loyal_treacherous -> Cower
action:hopeless:refusers -> Cower
action:hopeless:agg_reckless -> Cower
action:hopeless:cautious -> Cower
action:hopeless:resilient_fragile -> Cower
action:hopeless:cunning_dim -> Cower
action:hopeless:physical -> Cower
action:hopeless:survival -> Cower
action:hopeless:treacherous -> Cower
action:hopeless:aggressive -> Cower
action:hopeless:defensive -> Cower
action:hopeless:reckless -> Cower
action:hopeless:lone_wolf -> Cower
action:hopeless:paranoid -> Cower
action:hopeless:loyal -> Cower
action:hopeless:resilient -> Cower
action:hopeless:fragile -> Cower
action:hopeless:dim -> Cower
action:hopeless:kitchen_sink -> Cower
verdict:ally:empty -> Spare
verdict:ally:friendly -> Spare
verdict:ally:loyal_treacherous -> Spare
verdict:ally:refusers -> Spare
verdict:ally:agg_reckless -> Spare
verdict:ally:cautious -> Spare
verdict:ally:resilient_fragile -> Spare
verdict:ally:cunning_dim -> Spare
verdict:ally:physical -> Spare
verdict:ally:survival -> Spare
verdict:ally:treacherous -> Spare
verdict:ally:aggressive -> Spare
verdict:ally:defensive -> Spare
verdict:ally:reckless -> Spare
verdict:ally:lone_wolf -> Spare
verdict:ally:paranoid -> Spare
verdict:ally:loyal -> Spare
verdict:ally:resilient -> Spare
verdict:ally:fragile -> Spare
verdict:ally:dim -> Spare
verdict:ally:kitchen_sink -> Spare
verdict:team:empty -> Spare
verdict:team:friendly -> Spare
verdict:team:loyal_treacherous -> Spare
verdict:team:refusers -> Spare
verdict:team:agg_reckless -> Spare
verdict:team:cautious -> Spare
verdict:team:resilient_fragile -> Spare
verdict:team:cunning_dim -> Spare
verdict:team:physical -> Spare
verdict:team:survival -> Spare
verdict:team:treacherous -> Execute
verdict:team:aggressive -> Spare
verdict:team:defensive -> Spare
verdict:team:reckless -> Spare
verdict:team:lone_wolf -> Spare
verdict:team:paranoid -> Spare
verdict:team:loyal -> Spare
verdict:team:resilient -> Spare
verdict:team:fragile -> Spare
verdict:team:dim -> Spare
verdict:team:kitchen_sink -> Spare
verdict:both:empty -> Spare
verdict:both:friendly -> Spare
verdict:both:loyal_treacherous -> Spare
verdict:both:refusers -> Spare
verdict:both:agg_reckless -> Spare
verdict:both:cautious -> Spare
verdict:both:resilient_fragile -> Spare
verdict:both:cunning_dim -> Spare
verdict:both:physical -> Spare
verdict:both:survival -> Spare
verdict:both:treacherous -> Spare
verdict:both:aggressive -> Spare
verdict:both:defensive -> Spare
verdict:both:reckless -> Spare
verdict:both:lone_wolf -> Spare
verdict:both:paranoid -> Spare
verdict:both:loyal -> Spare
verdict:both:resilient -> Spare
verdict:both:fragile -> Spare
verdict:both:dim -> Spare
verdict:both:kitchen_sink -> Spare
verdict:stranger:empty -> Execute
verdict:stranger:friendly -> Spare
verdict:stranger:loyal_treacherous -> Execute
verdict:stranger:refusers -> Execute
verdict:stranger:agg_reckless -> Execute
verdict:stranger:cautious -> Execute
verdict:stranger:resilient_fragile -> Execute
verdict:stranger:cunning_dim -> Execute
verdict:stranger:physical -> Execute
verdict:stranger:survival -> Execute
verdict:stranger:treacherous -> Execute
verdict:stranger:aggressive -> Execute
verdict:stranger:defensive -> Execute
verdict:stranger:reckless -> Execute
verdict:stranger:lone_wolf -> Execute
verdict:stranger:paranoid -> Execute
verdict:stranger:loyal -> Spare
verdict:stranger:resilient -> Execute
verdict:stranger:fragile -> Execute
verdict:stranger:dim -> Execute
verdict:stranger:kitchen_sink -> Spare
betrayal:interval -> 5"#;
