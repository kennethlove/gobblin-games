//! Tuning harness: runs N seeded fights between default-rolled characters and
//! prints outcome/round/initiative/flee/stamina distributions. Used to set
//! `CombatTuning` values empirically; committed as a repeatable measuring tool.
//!
//! Run: `cargo run --example tuning_harness -- 2000`

use characters::Character;
use characters::brains::combat::CombatBrain;
use characters::combat::round_loop::{CombatActionProvider, CombatRoundLoop};
use characters::combat_tuning::CombatTuning;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use shared::combat_beat::CombatActionKind;
use shared::combat_beat::CombatResult;

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

    for seed in 0..n as u64 {
        let mut rng = SmallRng::seed_from_u64(seed);
        // Default-rolled characters: realistic attribute spread.
        let mut a = Character::new("Alice".into(), None, None);
        let mut b = Character::new("Bob".into(), None, None);
        a.traits.clear();
        b.traits.clear();
        a.items.clear();
        b.items.clear();

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

        if rounds <= 1 {
            let kind = match &result {
                Some(CombatResult::Winner { .. }) => "winner",
                Some(CombatResult::Fled { .. }) => "fled",
                Some(CombatResult::Spared { .. }) => "spared",
                Some(CombatResult::Draw) => "draw",
                None => "none",
            };
            *one_round.entry(kind).or_insert(0) += 1;
            if kind == "winner" {
                for ev in &events {
                    if let shared::messages::MessagePayload::CombatBeat(b) = &ev.payload {
                        if let shared::combat_beat::BeatOutcome::Swing { outcome, .. } = &b.outcome
                        {
                            if let shared::combat_beat::SwingOutcome::Kill { damage } = outcome {
                                sample_kill.push(format!(
                                    "{} -> {} dmg {damage} (round {})",
                                    b.actor.name,
                                    b.target.as_ref().map(|t| t.name.as_str()).unwrap_or("?"),
                                    b.round
                                ));
                            }
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
            Some(CombatResult::Winner { .. }) => winner += 1,
            Some(CombatResult::Fled { .. }) => fled += 1,
            Some(CombatResult::Spared { .. }) => spared += 1,
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
        // (order 1) win initiative? Alice acts first ⇒ order-0 beat has her name.
        if let Some(first_beat) = events.iter().find_map(|e| match &e.payload {
            shared::messages::MessagePayload::CombatBeat(b) if b.round == 1 && b.order == 0 => {
                Some(b)
            }
            _ => None,
        }) {
            if first_beat.actor.name == "Alice" {
                init_first += 1;
            }
        }

        // Flee stats from beats.
        for ev in &events {
            if let shared::messages::MessagePayload::CombatBeat(b) = &ev.payload {
                match b.action {
                    CombatActionKind::Flee => {
                        flee_attempts += 1;
                        if matches!(b.outcome, shared::combat_beat::BeatOutcome::Fled) {
                            flee_success += 1;
                        }
                    }
                    _ => {}
                }
            }
        }

        if a.stamina == 0 || b.stamina == 0 {
            exhausted_end += 1;
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
}
