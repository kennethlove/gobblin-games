//! Tuning harness: runs N seeded fights between default-rolled characters and
//! prints outcome/round/initiative/flee/stamina distributions. Used to set
//! `CombatTuning` values empirically; committed as a repeatable measuring tool.
//!
//! Run: `cargo run --example tuning_harness -- 2000`
//! Gear mode (second arg `gear`): fighters carry weapon/shield/armor and the
//! harness reports wear consumption and break rates — the instrument for
//! tuning the weapon/shield/armor wear knobs:
//!
//! Run: `cargo run --example tuning_harness -- 2000 gear`

use characters::Character;
use characters::brains::combat::CombatBrain;
use characters::combat::round_loop::{CombatActionProvider, CombatRoundLoop};
use characters::combat_tuning::CombatTuning;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use shared::combat_beat::CombatActionKind;
use shared::combat_beat::CombatResult;
use world::items::OwnsItems;

/// Brain wrapper so the loop uses real per-round decisions.
struct BrainProvider(CombatBrain);

impl CombatActionProvider for BrainProvider {
    fn action(&mut self, actor: &Character, opponent: &Character) -> CombatActionKind {
        self.0.action(actor, opponent)
    }
    fn verdict(&mut self, actor: &Character, cowerer: &Character) -> CombatActionKind {
        self.0.verdict(actor, cowerer)
    }
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(2000);
    // Gear mode: equip both fighters so the wear knobs actually fire.
    let gear = std::env::args().any(|a| a == "gear");
    let tuning = CombatTuning::default();

    let mut rounds_hist: Vec<u32> = Vec::new();
    let mut winner = 0usize;
    let mut fled = 0usize;
    let mut spared = 0usize;
    let mut draw = 0usize;
    let mut init_first = 0usize;
    let mut flee_attempts = 0usize;
    let mut flee_success = 0usize;
    let mut stamina_empty_draws = 0usize;
    let mut cap_draws = 0usize;
    let mut exhausted_end = 0usize;
    let mut one_round: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut one_round_kinds: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    let mut sample_kill: Vec<String> = Vec::new();
    // Wear accounting (gear mode): points consumed per slot and fights that
    // ended with at least one item of that slot broken (durability 0 or gone).
    let mut wear_points: [u32; 3] = [0, 0, 0]; // weapon, shield, armor
    let mut slot_breaks: [usize; 3] = [0, 0, 0];
    let mut any_break_fights = 0usize;

    for seed in 0..n as u64 {
        let mut rng = SmallRng::seed_from_u64(seed);
        // Default-rolled characters: realistic attribute spread.
        let mut a = Character::new("Alice".into(), None, None);
        let mut b = Character::new("Bob".into(), None, None);
        a.traits.clear();
        b.traits.clear();
        a.items.clear();
        b.items.clear();
        if gear {
            for c in [&mut a, &mut b] {
                c.add_item(world::items::Item::new(
                    "Sidearm",
                    world::items::ItemType::Weapon,
                    world::items::ItemRarity::Common,
                    4,
                    world::items::Attribute::Strength,
                    3,
                ));
                c.add_item(world::items::Item::new(
                    "Oak Shield",
                    world::items::ItemType::Shield,
                    world::items::ItemRarity::Common,
                    5,
                    world::items::Attribute::Defense,
                    4,
                ));
                c.add_item(world::items::Item::new(
                    "Armor Plate",
                    world::items::ItemType::Armor,
                    world::items::ItemRarity::Common,
                    5,
                    world::items::Attribute::Defense,
                    4,
                ));
            }
        }

        let mut events = Vec::new();
        let mut combat = CombatRoundLoop::with_provider(
            &mut a,
            &mut b,
            format!("c{seed}"),
            tuning.clone(),
            Box::new(BrainProvider(CombatBrain::new(tuning.clone()))),
        );

        let mut rounds = 0u32;
        while !combat.is_over() {
            rounds += 1;
            combat.advance_round(&mut rng, &mut events);
        }
        rounds_hist.push(rounds);

        let result = combat.result().cloned();
        drop(combat); // release &mut a/b before reading stamina

        // Fight flavor (flee/spare) now lives in the beats, not the result.
        let fight_fled = events.iter().any(|ev| {
            matches!(&ev.payload, shared::messages::MessagePayload::CombatBeat(b)
                if matches!(b.outcome, shared::combat_beat::BeatOutcome::Fled))
        });
        let fight_spared = events.iter().any(|ev| {
            matches!(&ev.payload, shared::messages::MessagePayload::CombatBeat(b)
                if matches!(b.outcome, shared::combat_beat::BeatOutcome::Spared))
        });

        if rounds <= 1 {
            let kind = match &result {
                Some(CombatResult::Victory { .. }) if fight_fled => "fled",
                Some(CombatResult::Victory { .. }) if fight_spared => "spared",
                Some(CombatResult::Victory { .. }) => "winner",
                Some(CombatResult::Draw) => "draw",
                None => "none",
            };
            *one_round.entry(kind).or_insert(0) += 1;
            if kind == "winner" {
                for ev in &events {
                    if let shared::messages::MessagePayload::CombatBeat(b) = &ev.payload {
                        if let shared::combat_beat::BeatOutcome::Swing { outcome, .. } = &b.outcome
                            && let shared::combat_beat::SwingOutcome::Kill { damage } = outcome
                        {
                            sample_kill.push(format!(
                                "{} -> {} dmg {damage} (round {})",
                                b.actor.name,
                                b.target.as_ref().map(|t| t.name.as_str()).unwrap_or("?"),
                                b.round
                            ));
                        }
                        let key: &str = match &b.outcome {
                            shared::combat_beat::BeatOutcome::Swing { outcome, .. } => {
                                match outcome {
                                    shared::combat_beat::SwingOutcome::Kill { .. } => "kill",
                                    shared::combat_beat::SwingOutcome::CriticalHitWound {
                                        ..
                                    } => "critwound",
                                    shared::combat_beat::SwingOutcome::Wound { .. } => "wound",
                                    shared::combat_beat::SwingOutcome::Miss => "miss",
                                    shared::combat_beat::SwingOutcome::FumbleSurvive { .. } => {
                                        "fumblesurvive"
                                    }
                                    _ => "other",
                                }
                            }
                            _ => "nonswing",
                        };
                        *one_round_kinds.entry(key).or_insert(0) += 1;
                    }
                }
            }
        }

        match result {
            Some(CombatResult::Victory { .. }) if fight_fled => fled += 1,
            Some(CombatResult::Victory { .. }) if fight_spared => spared += 1,
            Some(CombatResult::Victory { .. }) => winner += 1,
            Some(CombatResult::Draw) => {
                draw += 1;
                if a.stamina == 0 && b.stamina == 0 {
                    stamina_empty_draws += 1;
                } else {
                    cap_draws += 1;
                }
            }
            None => unreachable!("fight must terminate"),
        }

        // Initiative spread: in round 1, did Alice (order 0's actor) or Bob
        // (order 1) win initiative? Alice acts first => order-0 beat has her name.
        if let Some(first_beat) = events.iter().find_map(|e| match &e.payload {
            shared::messages::MessagePayload::CombatBeat(b) if b.round == 1 && b.order == 0 => {
                Some(b)
            }
            _ => None,
        }) && first_beat.actor.name == "Alice"
        {
            init_first += 1;
        }

        // Flee stats from beats.
        for ev in &events {
            if let shared::messages::MessagePayload::CombatBeat(b) = &ev.payload
                && matches!(b.action, CombatActionKind::Flee)
            {
                flee_attempts += 1;
                if matches!(b.outcome, shared::combat_beat::BeatOutcome::Fled) {
                    flee_success += 1;
                }
            }
        }

        if a.stamina == 0 || b.stamina == 0 {
            exhausted_end += 1;
        }

        if gear {
            // Slots: weapon, shield, armor. Absent = broken mid-fight = all 10
            // points consumed.
            let names = ["Sidearm", "Oak Shield", "Armor Plate"];
            let mut broke_this_fight = false;
            for (slot, name) in names.iter().enumerate() {
                let start = match slot {
                    0 => 4,
                    _ => 5,
                };
                for c in [&a, &b] {
                    let dur = c
                        .items
                        .iter()
                        .find(|i| i.name == *name)
                        .map(|i| i.current_durability)
                        .unwrap_or(0);
                    wear_points[slot] += start - dur.min(start);
                    if dur == 0 {
                        slot_breaks[slot] += 1;
                        broke_this_fight = true;
                    }
                }
            }
            if broke_this_fight {
                any_break_fights += 1;
            }
        }
    }

    rounds_hist.sort_unstable();
    let pct = |x: usize| 100.0 * x as f64 / n as f64;
    let p = |q: f64| rounds_hist[((n as f64 * q) as usize).min(n - 1)];

    println!("fights: {n}");
    println!(
        "outcomes: winner {pct:5.1}%  fled {pct2:5.1}%  spared {pct3:5.1}%  draw {pct4:5.1}%",
        pct = pct(winner),
        pct2 = pct(fled),
        pct3 = pct(spared),
        pct4 = pct(draw)
    );
    println!(
        "rounds: p10 {} p25 {} p50 {} p75 {} p90 {} p99 {} max {}",
        p(0.10),
        p(0.25),
        p(0.50),
        p(0.75),
        p(0.90),
        p(0.99),
        rounds_hist[n - 1]
    );
    println!(
        "rounds 2-6: {pct:5.1}%",
        pct = 100.0
            * rounds_hist
                .iter()
                .filter(|&&r| (2..=6).contains(&r))
                .count() as f64
            / n as f64
    );
    println!(
        "draw split: stamina-empty {} ({p1:5.1}%)  round-cap {p2:5.1}%",
        stamina_empty_draws,
        p1 = pct(stamina_empty_draws),
        p2 = pct(cap_draws)
    );
    println!(
        "first-beat init: order-0 share {pct:5.1}%  (50% = fair)",
        pct = pct(init_first)
    );
    println!(
        "flee: attempts {} ({pa:5.1}% of fights)  success {ps:5.1}% of attempts",
        flee_attempts,
        pa = pct(flee_attempts),
        ps = if flee_attempts > 0 {
            100.0 * flee_success as f64 / flee_attempts as f64
        } else {
            0.0
        }
    );
    println!(
        "someone stamina-empty at end: {pct:5.1}%",
        pct = pct(exhausted_end)
    );
    println!("one-round fights: {one_round:?}");
    println!("one-round beat kinds: {one_round_kinds:?}");
    for s in sample_kill.iter().take(5) {
        println!("kill sample: {s}");
    }
    if gear {
        let names = ["weapon", "shield", "armor"];
        println!("gear wear (per 100 fights):");
        for (slot, label) in names.iter().enumerate() {
            let points = 100.0 * wear_points[slot] as f64 / n as f64;
            let breaks = 100.0 * slot_breaks[slot] as f64 / n as f64;
            println!("  {label}: {points:6.1} pts  breaks {breaks:5.1}%");
        }
        println!(
            "  fights with any break: {pct:5.1}%",
            pct = pct(any_break_fights)
        );
    }
}
