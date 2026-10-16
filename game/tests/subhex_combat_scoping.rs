//! Sub-tile combat scoping (option A): fights start AND end in the same
//! sub-tile; only a successfully fleeing goblin moves to a new sub-tile.
//!
//! - same-sub targeting + defensive re-check at fight resolution
//! - cross-sub characters never fight
//! - a successful flee relocates the fleer into an adjacent sub-tile
//! - combat payloads carry the fight's location

use areas::Area;
use areas::hex::SubTriangle;
use areas::location::Location;
use characters::Character;
use game::games::Game;
use shared::messages::{MessagePayload, Phase};
use world::terrain::{BaseTerrain, TerrainType};

/// Game with a single Hub area so brains have no destinations to move to
/// and every fight stays put unless flee relocates someone.
fn single_hub_game() -> Game {
    let mut game = Game::new("subhex-combat-scoping");
    game.areas.push(areas::AreaDetails::new_with_terrain(
        Some("Hub".to_string()),
        Area::Hub,
        TerrainType::new(BaseTerrain::Clearing, vec![]).unwrap(),
    ));
    // WHY: with 2 characters (< low_character_threshold = 8)
    // constrain_areas attaches a uniform random event to the open Hub
    // each phase — ~8-10% Wildfire -> Burned -> -50 blood kills the
    // 50-blood defender before the deferred fight resolves (the suite
    // flake). Opt the tiny test cast out of random closing.
    game.config.low_character_threshold = 1;
    game
}

fn fighter(name: &str, game: &Game, team: u32, location: Location) -> Character {
    let mut c = Character::new(name.to_string(), Some(team), None);
    c.blood = 1000;
    c.statistics.game = game.identifier.clone();
    c.location = location;
    c
}

fn combat_messages(game: &Game) -> Vec<&shared::messages::GameMessage> {
    game.messages
        .iter()
        .filter(|m| {
            matches!(
                m.payload,
                MessagePayload::CombatRoundStart { .. }
                    | MessagePayload::CombatBeat(_)
                    | MessagePayload::CombatEnded { .. }
            )
        })
        .collect()
}

#[test]
fn same_sub_fight_starts_and_stamps_location_on_payloads() {
    let mut game = single_hub_game();
    let hub_sub0 = Location::new(Area::Hub, SubTriangle::new(0));
    let mut attacker = fighter("Attacker", &game, 1, hub_sub0);
    attacker.attributes.strength = 60;
    let mut defender = fighter("Defender", &game, 2, hub_sub0);
    defender.attributes.toughness = 0;
    defender.blood = 50;
    let target_id = defender.id;
    game.characters.push(attacker);
    game.characters.push(defender);

    // Force the deferred fight: act_attack normally sets this, but the
    // brain is free to pick anything else — the cycle must resolve a
    // pending attack regardless of what the brain chose this phase.
    game.characters[0].pending_attack_target = Some(target_id);

    game.run_phase(Phase::DAY).expect("day phase ran");

    let rounds: Vec<_> = game
        .messages
        .iter()
        .filter_map(|m| match &m.payload {
            MessagePayload::CombatRoundStart { location, .. } => Some(location.clone()),
            _ => None,
        })
        .collect();
    assert!(
        !rounds.is_empty(),
        "expected at least one CombatRoundStart for a same-sub pending attack"
    );
    for location in rounds {
        let location = location.expect("combat round payloads must carry a location");
        assert_eq!(location.area.name, "Hub");
        assert_eq!(location.sub, 0);
    }

    let ended = game.messages.iter().find_map(|m| match &m.payload {
        MessagePayload::CombatEnded { location, .. } => Some(location.clone()),
        _ => None,
    });
    let ended = ended.expect("fight must emit CombatEnded");
    let ended = ended.expect("CombatEnded must carry a location");
    assert_eq!(ended.area.name, "Hub");
    assert_eq!(ended.sub, 0);
}

#[test]
fn cross_sub_pending_attack_is_dropped_by_defensive_recheck() {
    let mut game = single_hub_game();
    let hub_sub0 = Location::new(Area::Hub, SubTriangle::new(0));
    let hub_sub3 = Location::new(Area::Hub, SubTriangle::new(3));
    let attacker = fighter("Attacker", &game, 1, hub_sub0);
    let defender = fighter("Defender", &game, 2, hub_sub3);
    let target_id = defender.id;
    game.characters.push(attacker);
    game.characters.push(defender);

    game.characters[0].pending_attack_target = Some(target_id);

    game.run_phase(Phase::DAY).expect("day phase ran");

    assert!(
        combat_messages(&game).is_empty(),
        "cross-sub pending attack must not resolve into a fight"
    );
}

#[test]
fn cross_sub_characters_never_start_a_fight() {
    let mut game = single_hub_game();
    let hub_sub0 = Location::new(Area::Hub, SubTriangle::new(0));
    let hub_sub3 = Location::new(Area::Hub, SubTriangle::new(3));
    let mut attacker = fighter("Attacker", &game, 1, hub_sub0);
    attacker.attributes.strength = 60;
    let mut defender = fighter("Defender", &game, 2, hub_sub3);
    defender.attributes.toughness = 0;
    defender.blood = 50;
    game.characters.push(attacker);
    game.characters.push(defender);

    // No pending-attack forcing: the brain must never even see the
    // cross-sub goblin as a target.
    for _ in 0..6 {
        game.run_phase(Phase::DAY).expect("day phase ran");
    }

    assert!(
        combat_messages(&game).is_empty(),
        "same area but different sub-tile must never fight"
    );
}

#[test]
fn successful_flee_relocates_fleer_into_adjacent_sub() {
    let mut game = single_hub_game();
    // Both combatants always choose Flee, and the fleer always wins the
    // contested roll — the fight ends in round 1 with a relocation.
    game.combat_tuning.brain_attack_base = -1_000;
    game.combat_tuning.brain_flee_base = 1_000;
    game.combat_tuning.flee_contest_modifier = 1_000;

    let hub_sub0 = Location::new(Area::Hub, SubTriangle::new(0));
    let attacker = fighter("Attacker", &game, 1, hub_sub0);
    let defender = fighter("Defender", &game, 2, hub_sub0);
    let target_id = defender.id;
    game.characters.push(attacker);
    game.characters.push(defender);

    game.characters[0].pending_attack_target = Some(target_id);

    game.run_phase(Phase::DAY).expect("day phase ran");

    let fled_result = game.messages.iter().any(|m| {
        matches!(
            m.payload,
            MessagePayload::CombatEnded {
                result: shared::combat_beat::CombatResult::Fled { .. },
                ..
            }
        )
    });
    assert!(fled_result, "tuned fight must end with a flee");

    let original = hub_sub0;
    let adjacent: Vec<Location> = areas::location::sub_neighbors(original)
        .into_iter()
        .flatten()
        .collect();

    let relocated: Vec<&Character> = game
        .characters
        .iter()
        .filter(|c| c.location != original)
        .collect();
    assert_eq!(
        relocated.len(),
        1,
        "exactly the fleer must move; locations: {:?}",
        game.characters
            .iter()
            .map(|c| (c.name.as_str(), c.location))
            .collect::<Vec<_>>()
    );
    let fleer = relocated[0];
    assert!(
        adjacent.contains(&fleer.location),
        "fleer moved to {:?}, which is not adjacent to {original:?}",
        fleer.location
    );
}
