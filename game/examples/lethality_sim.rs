//! 2qo lethality probe: N default-format matches (4 teams x 6 goblins)
//! run full-day until a winner or the day cap.
//!
//! Prints one line per run plus a summary, so damage/heal tuning can be
//! measured against the bead's target: most matches end by day 10.
//!
//! Usage: cargo run -p game --example lethality_sim [-- RUNS] (default 50)

use characters::Character;
use characters::statuses::CharacterStatus;
use game::games::Game;
use shared::GameStatus;
use shared::messages::MessagePayload;

const ROSTER: usize = 24;
const GOBLINS_PER_TEAM: u32 = shared::DEFAULT_GOBLINS_PER_TEAM;
const MAX_DAYS: u32 = 30;
const TARGET_DAY: u32 = 10;
const SOFT_DAY: u32 = 15;

fn alive(game: &Game) -> usize {
    game.characters.iter().filter(|c| c.is_alive()).count()
}

fn build_game(i: usize) -> Game {
    let mut game = Game::new(&format!("lethality-{i}"));
    game.start().expect("game must start");

    // Default preset areas so movement/adjacency behave like a real match.
    let preset = areas::preset::resolve(None).expect("default preset");
    for (n, (area, name, sub_tiles, axial, base)) in preset.instantiate().into_iter().enumerate() {
        let mut row = serde_json::json!({
            "identifier": format!("area-{n}"),
            "name": name,
            "area": area.to_string(),
            "axial": serde_json::to_value(axial).expect("axial"),
            "sub_tiles": serde_json::to_value(&sub_tiles).expect("sub tiles"),
        });
        if let Some(base) = base {
            let terrain = world::terrain::TerrainType::random_with_base(base, &mut rand::rng());
            row["terrain"] = serde_json::to_value(terrain).expect("terrain");
        }
        let detail: areas::AreaDetails = serde_json::from_value(row).expect("area row");
        game.areas.push(detail);
    }

    for i in 0..ROSTER {
        let mut character = Character::new(format!("Goblin {i:03}"), None, None);
        character.blood = 1000;
        character.status = CharacterStatus::Healthy;
        character.team = i as u32 / GOBLINS_PER_TEAM + 1;
        game.characters.push(character);
    }
    game
}

fn main() {
    let runs: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(50);

    let mut finish_days: Vec<u32> = Vec::new();
    let mut alive_at_finish: Vec<usize> = Vec::new();
    let mut alive_at_soft: Vec<usize> = Vec::new(); // snapshot at/after day 15
    let mut capped = 0usize;
    let mut killed = 0usize;
    let mut bled = 0usize;
    let mut killed_hazard = 0usize;
    let mut killed_fumble = 0usize;
    let mut wipe_killed = 0usize;
    let mut wipe_bled = 0usize;
    let mut wipe_fumble = 0usize;
    let mut wipe_day_hazard = 0usize;
    let mut wipe_day_combat = 0usize;
    let mut wipe_day_generic = 0usize;
    let mut wipe_examples = 0usize;

    for i in 0..runs {
        let mut game = build_game(i);
        if i == 0 {
            eprintln!(
                "area_count={} areas={:?}",
                game.areas.len(),
                game.areas
                    .iter()
                    .map(|a| a.name.clone())
                    .collect::<Vec<_>>()
            );
        }
        let mut soft_snapshot: Option<usize> = None;
        let mut finish: Option<u32> = None;

        while game.status != GameStatus::Finished && game.day.unwrap_or(0) < MAX_DAYS {
            game.run_full_day().expect("a day must run");
            let day = game.day.unwrap_or(0);
            if day >= SOFT_DAY && soft_snapshot.is_none() {
                soft_snapshot = Some(alive(&game));
            }
            if game.status == GameStatus::Finished {
                finish = Some(day);
            }
            if i == 0 {
                eprintln!("  day {day}: {} alive", alive(&game));
            }
        }

        for m in &game.messages {
            match m.payload {
                MessagePayload::CharacterKilled { .. } => {
                    killed += 1;
                    if m.content.contains("dies from the") || m.content.contains("instantly killed")
                    {
                        killed_hazard += 1;
                    }
                    if m.content.contains("themselves") {
                        killed_fumble += 1;
                    }
                }
                MessagePayload::CharacterBledOut { .. } => bled += 1,
                _ => {}
            }
        }

        match finish {
            Some(day) => {
                let a = alive(&game);
                if a == 0 {
                    let mut day_deaths: Vec<String> = Vec::new();
                    for m in &game.messages {
                        if m.game_day != day {
                            continue;
                        }
                        match m.payload {
                            MessagePayload::CharacterKilled { .. } => {
                                wipe_killed += 1;
                                if m.content.contains("themselves") {
                                    wipe_fumble += 1;
                                }
                                if m.content.contains("dies from the")
                                    || m.content.contains("instantly killed")
                                {
                                    wipe_day_hazard += 1;
                                } else if m.content.contains("dies from ") {
                                    wipe_day_generic += 1;
                                } else {
                                    wipe_day_combat += 1;
                                }
                                day_deaths.push(m.content.clone());
                            }
                            MessagePayload::CharacterBledOut { .. } => {
                                wipe_bled += 1;
                                day_deaths.push(m.content.clone());
                            }
                            _ => {}
                        }
                    }
                    wipe_examples += 1;
                    if wipe_examples <= 2 {
                        eprintln!("wipeout run {i} final day {day} deaths:");
                        for c in &day_deaths {
                            eprintln!("    {c}");
                        }
                    }
                }
                finish_days.push(day);
                alive_at_finish.push(a);
                alive_at_soft.push(soft_snapshot.unwrap_or(a));
                println!("run {i:3}: finished day {day:2}, {a} alive");
            }
            None => {
                capped += 1;
                let a = alive(&game);
                alive_at_soft.push(soft_snapshot.unwrap_or(a));
                println!("run {i:3}: NO FINISH by day {MAX_DAYS}, {a} alive");
            }
        }
    }

    let mut sorted = finish_days.clone();
    sorted.sort_unstable();
    let pct = |n: usize| 100.0 * n as f64 / runs as f64;
    let on_time = finish_days.iter().filter(|&&d| d <= TARGET_DAY).count();
    let median = if sorted.is_empty() {
        0
    } else {
        sorted[sorted.len() / 2]
    };
    let mean = |v: &[usize]| {
        if v.is_empty() {
            0.0
        } else {
            v.iter().sum::<usize>() as f64 / v.len() as f64
        }
    };
    let soft_fail = alive_at_soft.iter().filter(|&&a| a * 2 > ROSTER).count();
    let solo = alive_at_finish.iter().filter(|&&a| a == 1).count();
    let team = alive_at_finish.iter().filter(|&&a| a >= 2).count();
    let wipeout = alive_at_finish.iter().filter(|&&a| a == 0).count();

    println!(
        "\nsummary over {runs} runs:\n  finished: {} ({}%), capped at day {MAX_DAYS}: {capped} ({}%)\n  finish day: median {median}, on-or-before day {TARGET_DAY}: {} ({}%)\n  mean alive at finish: {:.1} of {ROSTER}\n  still >half roster alive at day {SOFT_DAY}: {soft_fail} runs ({}%)",
        finish_days.len(),
        pct(finish_days.len()),
        pct(capped),
        on_time,
        pct(on_time),
        mean(&alive_at_finish),
        pct(soft_fail),
    );
    let fin = finish_days.len().max(1);
    println!(
        "  outcome mix: solo winner (1 alive): {solo} ({}%), team survives (2+): {team} ({}%), total wipeout: {wipeout} ({}%)",
        100.0 * solo as f64 / fin as f64,
        100.0 * team as f64 / fin as f64,
        100.0 * wipeout as f64 / fin as f64,
    );
    println!(
        "  death causes across all runs: killed in combat: {killed} (hazard {killed_hazard}, fumble {killed_fumble}), bled out: {bled}"
    );
    println!(
        "  final-day deaths inside wipeout runs: killed {wipe_killed} (fumble {wipe_fumble}), bled out {wipe_bled}"
    );
    println!(
        "  wipeout final-day death mix: hazard {wipe_day_hazard}, combat {wipe_day_combat}, generic {wipe_day_generic}"
    );
}
