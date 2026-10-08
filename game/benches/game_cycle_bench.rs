use characters::Character;
use characters::statuses::CharacterStatus;
use criterion::{Criterion, criterion_group, criterion_main};
use game::games::Game;
use std::hint::black_box;

fn create_test_game(character_count: usize) -> Game {
    let mut game = Game::new("bench-game");
    let _ = game.start();

    for i in 0..character_count {
        let mut character = Character::new(format!("Character {}", i), None, None);
        character.blood = 1000;
        character.status = CharacterStatus::Healthy;
        game.characters.push(character);
    }

    game
}

fn bench_living_characters_full(c: &mut Criterion) {
    let game = create_test_game(24);

    c.bench_function("living_characters_count (24 alive)", |b| {
        b.iter(|| black_box(game.living_characters_count()))
    });
}

fn bench_living_characters_half(c: &mut Criterion) {
    let mut game = create_test_game(24);

    // Kill half the characters
    for i in 0..12 {
        game.characters[i].blood = 0;
        game.characters[i].status = CharacterStatus::Dead;
    }

    c.bench_function("living_characters_count (12 alive)", |b| {
        b.iter(|| black_box(game.living_characters_count()))
    });
}

fn bench_living_characters_few(c: &mut Criterion) {
    let mut game = create_test_game(24);

    // Kill all but 2 characters
    for i in 0..22 {
        game.characters[i].blood = 0;
        game.characters[i].status = CharacterStatus::Dead;
    }

    c.bench_function("living_characters_count (2 alive)", |b| {
        b.iter(|| black_box(game.living_characters_count()))
    });
}

criterion_group!(
    benches,
    bench_living_characters_full,
    bench_living_characters_half,
    bench_living_characters_few
);
criterion_main!(benches);
