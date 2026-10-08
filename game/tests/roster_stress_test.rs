//! Deterministic 128-goblin roster stress (worst case: 16 teams × 8).
//!
//! This is the repeatable regression gate — it runs with `just test`.
//! Perf numbers come from the criterion bench (`just bench-stress`);
//! API/DB paths live in `api/tests/roster_stress_test.rs`.

use characters::Character;
use characters::statuses::CharacterStatus;
use game::games::Game;

const TEAM_COUNT: u32 = 16;
const GOBLINS_PER_TEAM: u32 = 8;
const ROSTER: usize = (TEAM_COUNT * GOBLINS_PER_TEAM) as usize; // 128

/// Serialized-size ceiling for a fresh 128-goblin game (bytes).
/// Baseline measured at 142 KB; the budget keeps ~2.8x headroom so
/// runaway item/message growth trips this test instead of the DB.
const STORAGE_BUDGET_BYTES: usize = 400_000;

fn full_roster_game(name: &str) -> Game {
    let mut game = Game::new(name);
    game.start().expect("game must start");
    for i in 0..ROSTER {
        let mut character = Character::new(format!("Goblin {i:03}"), None, None);
        character.blood = 1000;
        character.status = CharacterStatus::Healthy;
        character.team = i as u32 / GOBLINS_PER_TEAM + 1;
        game.characters.push(character);
    }
    game
}

#[test]
fn full_day_at_128_completes() {
    let mut game = full_roster_game("stress-day");
    assert_eq!(game.characters.len(), ROSTER);

    game.run_full_day()
        .expect("a full day must run at 128 goblins");

    assert_eq!(game.day, Some(1), "day counter advances");
    assert_eq!(game.characters.len(), ROSTER, "roster size unchanged");
    assert!(
        !game.messages.is_empty(),
        "phase produced messages (combat/survival traffic)"
    );
}

#[test]
fn storage_budget_and_roundtrip_at_128() {
    let game = full_roster_game("stress-storage");

    let json = serde_json::to_string(&game).expect("game must serialize");
    assert!(
        json.len() <= STORAGE_BUDGET_BYTES,
        "serialized game is {} bytes, over the {} byte budget",
        json.len(),
        STORAGE_BUDGET_BYTES
    );

    let back: Game = serde_json::from_str(&json).expect("game must deserialize");
    assert_eq!(back.characters.len(), ROSTER);
    let teams: std::collections::HashSet<u32> = back.characters.iter().map(|c| c.team).collect();
    assert_eq!(teams.len(), TEAM_COUNT as usize, "all 16 teams survive");
}
