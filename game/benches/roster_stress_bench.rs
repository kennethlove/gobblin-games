//! Criterion benches for the 128-goblin roster ceiling (16 teams x 8).
//!
//! Re-run: `just bench-stress` (or
//! `cargo bench -p game --bench roster_stress_bench`).
//! Compare against a saved baseline with `--save-baseline <name>` /
//! `--baseline <name>` when checking a change.

use characters::Character;
use characters::statuses::CharacterStatus;
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use game::games::Game;
use std::hint::black_box;

const GOBLINS_PER_TEAM: u32 = 8;

fn full_roster_game(roster: usize, name: &str) -> Game {
    let mut game = Game::new(name);
    game.start().expect("game must start");
    for i in 0..roster {
        let mut character = Character::new(format!("Goblin {i:04}"), None, None);
        character.blood = 1000;
        character.status = CharacterStatus::Healthy;
        character.team = i as u32 / GOBLINS_PER_TEAM + 1;
        game.characters.push(character);
    }
    game
}

/// Processing: one full day (all phases, 128 characters) vs the
/// historical default of 24.
fn bench_full_day(c: &mut Criterion) {
    let mut group = c.benchmark_group("roster_stress/run_full_day");
    group.sample_size(10);
    for roster in [24, 128] {
        group.bench_with_input(
            BenchmarkId::from_parameter(roster),
            &roster,
            |b, &roster| {
                b.iter_batched(
                    || full_roster_game(roster, "bench-day"),
                    |mut game| game.run_full_day().expect("full day"),
                    BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

/// Storage: serialize a fresh 128-goblin game (the per-cycle save_game
/// payload) and deserialize it back (the read-path hydrate).
fn bench_persistence_codec(c: &mut Criterion) {
    let game = full_roster_game(128, "bench-storage");
    let json = serde_json::to_string(&game).expect("serialize");
    println!("roster_stress serialized game: {} bytes", json.len());

    let mut group = c.benchmark_group("roster_stress/persistence_codec");
    group.sample_size(20);
    group.bench_function("serialize_128", |b| {
        b.iter(|| serde_json::to_string(black_box(&game)).expect("serialize"));
    });
    group.bench_function("deserialize_128", |b| {
        b.iter(|| serde_json::from_str::<Game>(black_box(&json)).expect("deserialize"));
    });
    group.finish();
}

criterion_group!(benches, bench_full_day, bench_persistence_codec);
criterion_main!(benches);
