use super::*;
use crate::characters::Attributes;

pub(crate) fn create_test_game_with_characters(characters: Vec<Character>) -> Game {
    Game {
        identifier: "test-game".to_string(),
        name: "Test Game".to_string(),
        status: GameStatus::InProgress,
        day: Some(1),
        areas: vec![],
        characters,
        private: true,
        config: Default::default(),
        messages: vec![],
        alliance_events: vec![],
        tick_counter: TickCounter::default(),
        current_phase: crate::messages::Phase::Day,
        emit_index: 0,
        combat_tuning: crate::characters::combat_tuning::CombatTuning::default(),
        patrons: vec![],
    }
}

pub(crate) fn create_character(name: &str, is_alive: bool) -> Character {
    let mut character = Character::new(name.to_string(), None, None);
    if is_alive {
        character.blood = 1000;
        character.status = CharacterStatus::Healthy;
    } else {
        character.blood = 0;
        character.status = CharacterStatus::Dead;
    }
    character
}

mod alliances;
mod messaging;
mod survival;
