use super::*;
use serial_test::serial;

#[test]
fn test_game_new() {
    let game = Game::new("Test Game");
    assert_eq!(game.name, "Test Game");
    assert_eq!(game.status, GameStatus::NotStarted);
    assert_eq!(game.day, None);
    assert_eq!(game.characters.len(), 0);
}

#[test]
fn game_has_empty_alliance_event_queue_on_new() {
    let g = Game::default();
    assert!(g.alliance_events.is_empty());
}

#[test]
fn test_game_start() {
    let mut game = Game::new("Test Game");
    game.start().expect("Failed to start game");
    assert_eq!(game.status, GameStatus::InProgress);
    assert_eq!(game.day, None);
}

#[test]
fn test_game_end() {
    let mut game = Game::new("Test Game");
    game.start().expect("Failed to start game");
    game.end();
    assert_eq!(game.status, GameStatus::Finished);
}

#[test]
fn test_living_and_recently_dead_characters() {
    let mut game = Game::new("Test Game");
    let t1 = Character::default();
    let t2 = Character::default();
    game.characters.push(t1);
    game.characters.push(t2);
    assert_eq!(game.living_characters().len(), 2);
    assert_eq!(game.recently_dead_characters().len(), 0);
    game.characters[0].status = CharacterStatus::RecentlyDead;
    assert_eq!(game.living_characters().len(), 1);
    assert_eq!(game.recently_dead_characters().len(), 1);
}

#[test]
fn test_game_winner() {
    let mut game = Game::new("Test Game");
    let t1 = Character::default();
    let t2 = Character::default();
    game.characters.push(t1);
    game.characters.push(t2.clone());
    game.start().expect("Failed to start game");
    assert_eq!(game.winner(), None);
    game.characters[0].status = CharacterStatus::Dead;
    assert_eq!(game.winner().unwrap().name, t2.name);
}

#[test]
fn initiative_order_prefers_higher_agility() {
    let mut rng = SmallRng::seed_from_u64(42);
    let s1 = initiative_score(100, &mut rng);
    let mut rng = SmallRng::seed_from_u64(42);
    let s2 = initiative_score(1, &mut rng);
    assert!(s1 > s2, "agi 100 should beat agi 1 with same seed");
}

#[test]
fn initiative_fuzz_can_flip_close_scores() {
    let mut lower_won = false;
    for seed in 0..1000 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let s1 = initiative_score(50, &mut rng);
        let s2 = initiative_score(55, &mut rng);
        if s1 > s2 {
            lower_won = true;
            break;
        }
    }
    assert!(
        lower_won,
        "fuzz should sometimes overcome 5-point agility gap"
    );
}

#[test]
fn initiative_liveness_gate_still_works() {
    let mut game = Game::new("Test Game");
    game.start().expect("Failed to start game");

    let mut killer = Character::new("Killer".to_string(), None, None);
    killer.blood = 1000;
    killer.attributes.strength = 50;
    killer.attributes.agility = 100;
    killer.area = Area::Hub;

    let mut victim = Character::new("Victim".to_string(), None, None);
    victim.blood = 10;
    victim.attributes.strength = 1;
    victim.attributes.defense = 1;
    victim.attributes.agility = 1;
    victim.area = Area::Hub;

    game.characters.clear();
    game.characters.push(killer);
    game.characters.push(victim);

    let mut rng = SmallRng::seed_from_u64(42);
    let agility_0 = game.characters[0].attributes.agility;
    let agility_1 = game.characters[1].attributes.agility;
    let s0 = initiative_score(agility_0, &mut rng);
    let mut rng = SmallRng::seed_from_u64(42);
    let s1 = initiative_score(agility_1, &mut rng);
    assert!(
        s0 >= s1,
        "high-agility character should have >= initiative of low-agility"
    );
}

#[test]
fn attributes_new_includes_agility() {
    let attrs = Attributes::new();
    assert!(
        attrs.agility >= 1 && attrs.agility <= 100,
        "agility should be in 1..=100 range, got {}",
        attrs.agility
    );
}

#[test]
fn test_random_open_area() {
    let mut game = Game::new("Test Game");
    let area1 = AreaDetails::new(Some("Lake".to_string()), Area::Sector1);
    let area2 = AreaDetails::new(Some("Forest".to_string()), Area::Sector4);
    game.areas.push(area1);
    game.areas.push(area2.clone());
    assert!(game.random_area().is_some());
    let mut rng = rand::rng();
    let event = AreaEvent::random(&mut rng);
    game.areas[0].events.push(event.clone());
    assert_eq!(game.random_open_area().unwrap(), area2);
}

#[test]
fn test_clean_up_recent_deaths() {
    let mut game = Game::new("Test Game");

    let mut character = Character::default();
    character.set_status(CharacterStatus::RecentlyDead);
    game.characters.push(character.clone());

    assert_eq!(game.recently_dead_characters().len(), 1);
    assert_eq!(game.recently_dead_characters()[0], character);

    game.clean_up_recent_deaths();
    assert_eq!(game.characters[0].status, CharacterStatus::Dead);
}

#[test]
fn test_check_game_state_winner_exists() {
    let winner_character = create_character("Winner", true);
    let loser_character = create_character("Loser", false);
    let mut game =
        create_test_game_with_characters(vec![winner_character.clone(), loser_character.clone()]);

    assert_eq!(game.living_characters().len(), 1);
    assert_eq!(game.winner(), Some(winner_character.clone()));

    let _ = game.check_for_winner();

    assert_eq!(game.status, GameStatus::Finished);
}

#[test]
fn test_check_game_state_no_survivors() {
    let loser_character = create_character("Loser", false);
    let loser2_character = create_character("Loser 2", false);
    let mut game =
        create_test_game_with_characters(vec![loser_character.clone(), loser2_character.clone()]);

    assert!(game.living_characters().is_empty());
    assert!(game.winner().is_none());

    let _ = game.check_for_winner();

    assert_eq!(game.status, GameStatus::Finished);
}

#[test]
fn test_check_game_state_continues() {
    let living_character1 = create_character("Living1", true);
    let living_character2 = create_character("Living2", true);
    let mut game = create_test_game_with_characters(vec![
        living_character1.clone(),
        living_character2.clone(),
    ]);
    let starting_state = game.status.clone();

    assert_eq!(game.living_characters().len(), 2);
    assert!(game.winner().is_none());

    let _ = game.check_for_winner();

    assert_eq!(game.status, starting_state);
}

#[test]
fn test_prepare_cycle() {
    use crate::messages::Phase;
    let mut game = Game::new("Test Game");
    let area = AreaDetails::new(Some("Lake".to_string()), Area::Sector1);
    let mut rng = rand::rng();
    let event = AreaEvent::random(&mut rng);
    game.day = Some(1);
    game.areas.push(area);
    game.areas[0].events.push(event.clone());
    let _ = game.prepare_cycle(Phase::Dawn);
    assert_eq!(game.day, Some(2));
    assert_eq!(game.areas[0].events.len(), 0);

    game.areas[0].events.push(event.clone());
    let _ = game.prepare_cycle(Phase::Night);
    assert_eq!(game.day, Some(2));
    assert_eq!(game.areas[0].events.len(), 0);
}

#[test]
fn test_trigger_cycle_events() {}

#[test]
fn test_constrain_areas() {
    let mut game = Game::new("Test Game");
    let area1 = AreaDetails::new(Some("Lake".to_string()), Area::Sector1);
    let area2 = AreaDetails::new(Some("Forest".to_string()), Area::Sector4);
    game.areas.push(area1);
    game.areas.push(area2);

    let character1 = create_character("Character1", true);
    let character2 = create_character("Character2", true);
    game.characters.push(character1.clone());
    game.characters.push(character2.clone());

    let mut rng = SmallRng::seed_from_u64(0);
    let _ = game.constrain_areas(&mut rng);

    assert!(game.random_open_area().is_some());
    assert_eq!(game.open_areas().len(), 1);
    assert_eq!(game.closed_areas().len(), 1);
}

#[test]
fn test_run_character_cycle() {
    let character1 = create_character("Character1", true);
    let character2 = create_character("Character2", true);

    let mut game = create_test_game_with_characters(vec![character1.clone(), character2.clone()]);
    let area = AreaDetails::new(Some("Lake".to_string()), Area::Hub);
    game.areas.push(area);
    let closed_areas = game
        .areas
        .iter()
        .filter(|ad| ad.area.is_some() & !ad.is_open())
        .map(|ad| ad.area.unwrap())
        .collect::<Vec<Area>>();

    let mut rng = SmallRng::seed_from_u64(42);
    let _ = game.run_character_cycle(
        crate::messages::Phase::Day,
        &mut rng,
        closed_areas,
        vec![character1.clone(), character2.clone()],
        2,
    );

    let new_character1 = game.characters[0].clone();
    let new_character2 = game.characters[1].clone();
    assert_ne!(character1, new_character1);
    assert_ne!(character2, new_character2);
}

#[test]
fn test_open_and_closed_areas() {
    let mut game = Game::new("Test Game");
    let area1 = AreaDetails::new(Some("Lake".to_string()), Area::Sector1);
    let area2 = AreaDetails::new(Some("Forest".to_string()), Area::Sector4);
    game.areas.push(area1);
    game.areas.push(area2);

    assert_eq!(game.open_areas().len(), 2);
    assert!(game.closed_areas().is_empty());

    let mut rng = rand::rng();
    game.areas[0].events.push(AreaEvent::random(&mut rng));

    assert_eq!(game.open_areas().len(), 1);
    assert_eq!(game.closed_areas().len(), 1);
}

#[test]
fn test_ensure_open_area() {
    let mut game = Game::new("Test Game");
    let area1 = AreaDetails::new(Some("Lake".to_string()), Area::Sector1);
    let area2 = AreaDetails::new(Some("Forest".to_string()), Area::Sector4);
    game.areas.push(area1);
    game.areas.push(area2);

    assert!(game.random_open_area().is_some());

    let mut rng = rand::rng();
    game.areas[0].events.push(AreaEvent::random(&mut rng));
    game.areas[1].events.push(AreaEvent::random(&mut rng));

    assert!(game.random_open_area().is_none());

    game.ensure_open_area();
    assert!(game.random_open_area().is_some());
}

#[test]
fn survival_tick_increments_hunger_and_thirst_per_phase() {
    let mut a = Character::new("A".to_string(), Some(1), None);
    let mut b = Character::new("B".to_string(), Some(2), None);
    for t in [&mut a, &mut b] {
        t.attributes.strength = 30;
        t.stamina = t.max_stamina / 2;
    }
    let mut game = create_test_game_with_characters(vec![a, b]);
    game.day = Some(1);
    let _ = game.run_phase(crate::messages::Phase::Day);
    for t in &game.characters {
        assert_eq!(t.hunger, 1, "{} hunger should be 1 after one tick", t.name);
        assert_eq!(t.thirst, 1, "{} thirst should be 1 after one tick", t.name);
    }
}

#[test]
fn survival_tick_routes_dehydration_death_through_character_killed() {
    use crate::messages::MessagePayload;
    use shared::afflictions::DeathCause;
    let mut a = Character::new("Doomed".to_string(), Some(1), None);
    a.thirst = 4;
    a.dehydration_drain_step = 5;
    a.blood = 10;
    let mut game = create_test_game_with_characters(vec![a]);
    game.day = Some(1);
    let _ = game.run_phase(crate::messages::Phase::Day);
    let killed = game.messages.iter().any(|m| {
        matches!(&m.payload,
            MessagePayload::CharacterKilled { cause, .. } if *cause == DeathCause::Dehydration)
    });
    assert!(killed, "expected a CharacterKilled with cause=dehydration");
}

#[test]
#[serial]
fn sleeping_character_naturally_wakes_after_duration_emits_character_woke() {
    use crate::messages::{MessagePayload, Phase};
    let mut t = create_character("Sleeper", true);
    t.sleeping = true;
    t.sleep_remaining = 1;
    t.cycles_awake = 9;
    t.stamina = 50;
    let mut game = create_test_game_with_characters(vec![t.clone()]);
    let area = AreaDetails::new(Some("Lake".to_string()), Area::Hub);
    game.areas.push(area);
    let mut rng = SmallRng::seed_from_u64(42);
    let _ = game.run_character_cycle(Phase::Night, &mut rng, vec![], vec![t], 1);

    let woken = &game.characters[0];
    assert!(!woken.sleeping, "character should be awake");
    assert_eq!(woken.sleep_remaining, 0);
    assert_eq!(woken.cycles_awake, 0, "natural wake resets cycles_awake");

    let woke = game.messages.iter().any(|m| {
        matches!(
            &m.payload,
            MessagePayload::CharacterWoke {
                reason: shared::messages::WakeReason::Rested,
                ..
            }
        )
    });
    assert!(woke, "expected a CharacterWoke{{Rested}} message");
}

#[test]
#[serial]
fn sleeping_character_regenerates_stamina_each_phase() {
    use crate::messages::Phase;
    let mut t = create_character("Sleeper", true);
    t.sleeping = true;
    t.sleep_remaining = 3;
    t.stamina = 10;
    t.max_stamina = 100;
    let prior = t.stamina;
    let mut game = create_test_game_with_characters(vec![t.clone()]);
    let area = AreaDetails::new(Some("Lake".to_string()), Area::Hub);
    game.areas.push(area);
    let mut rng = SmallRng::seed_from_u64(42);
    let _ = game.run_character_cycle(Phase::Night, &mut rng, vec![], vec![t], 1);

    let after = &game.characters[0];
    assert!(after.sleeping, "still mid-sleep");
    assert_eq!(after.sleep_remaining, 2);
    assert!(after.stamina > prior, "stamina should regen");
}

#[test]
#[serial]
fn sleeping_wounded_character_does_not_regen_hp() {
    use crate::messages::Phase;
    use shared::afflictions::{Affliction, AfflictionKind, AfflictionSource, Severity};
    let mut t = create_character("Sleeper", true);
    t.sleeping = true;
    t.sleep_remaining = 3;
    t.blood = 400;
    t.afflictions.insert(
        (AfflictionKind::Wounded, None),
        Affliction {
            kind: AfflictionKind::Wounded,
            body_part: None,
            severity: Severity::Moderate,
            source: AfflictionSource::Combat {
                attacker_id: String::new(),
            },
            acquired_cycle: 0,
            last_progressed_cycle: 0,
            trauma_metadata: None,
            phobia_metadata: None,
            fixation_metadata: None,
            addiction_metadata: None,
            trapped_metadata: None,
        },
    );
    let prior_hp = t.effective_health();
    let mut game = create_test_game_with_characters(vec![t.clone()]);
    let area = AreaDetails::new(Some("Lake".to_string()), Area::Hub);
    game.areas.push(area);
    let mut rng = SmallRng::seed_from_u64(42);
    let _ = game.run_character_cycle(Phase::Night, &mut rng, vec![], vec![t], 1);
    assert_eq!(
        game.characters[0].effective_health(),
        prior_hp,
        "wounded characters do not heal while sleeping"
    );
}

#[test]
#[serial]
fn cycles_awake_does_not_increment_while_sleeping() {
    use crate::messages::Phase;
    let mut t = create_character("Sleeper", true);
    t.sleeping = true;
    t.sleep_remaining = 3;
    t.cycles_awake = 4;
    let mut game = create_test_game_with_characters(vec![t.clone()]);
    let area = AreaDetails::new(Some("Lake".to_string()), Area::Hub);
    game.areas.push(area);
    let mut rng = SmallRng::seed_from_u64(42);
    let _ = game.run_character_cycle(Phase::Night, &mut rng, vec![], vec![t], 1);
    assert_eq!(game.characters[0].cycles_awake, 4);
}

#[test]
fn area_event_interrupts_sleeping_character() {
    use crate::areas::events::AreaEvent;
    use crate::messages::{MessagePayload, Phase};
    let mut t = create_character("Sleeper", true);
    t.sleeping = true;
    t.sleep_remaining = 5;
    t.cycles_awake = 9;
    t.stamina = 10;
    let prior_stamina = t.stamina;
    let mut game = create_test_game_with_characters(vec![t.clone()]);
    let mut area = AreaDetails::new(Some("Lake".to_string()), Area::Hub);
    area.events.push(AreaEvent::Wildfire);
    game.areas.push(area);
    let mut rng = SmallRng::seed_from_u64(42);
    let _ = game.run_character_cycle(Phase::Night, &mut rng, vec![], vec![t], 1);

    let woken = &game.characters[0];
    assert!(!woken.sleeping, "area-event should wake sleeper");
    assert_eq!(woken.sleep_remaining, 0);
    assert_eq!(woken.cycles_awake, 0);
    assert!(
        woken.stamina < prior_stamina + SLEEP_STAMINA_PER_PHASE,
        "sleep-tick regen must be skipped on interruption (stamina={})",
        woken.stamina
    );

    let woke = game.messages.iter().any(|m| {
        matches!(
            &m.payload,
            MessagePayload::CharacterWoke {
                reason: shared::messages::WakeReason::Interrupted {
                    event: shared::messages::InterruptionKind::AreaEvent {
                        kind: shared::messages::AreaEventKind::Fire,
                    },
                },
                ..
            }
        )
    });
    assert!(woke, "expected CharacterWoke{{Interrupted/AreaEvent/Fire}}");
}

#[test]
fn alliance_summons_wakes_sleeping_target() {
    use crate::messages::MessagePayload;
    let mut summoner = create_character("Snipsnout", true);
    let mut target = create_character("Snaggletooth", true);
    target.sleeping = true;
    target.sleep_remaining = 3;
    target.cycles_awake = 6;
    let summoner_id = summoner.id;
    let target_id = target.id;
    summoner.allies.push(target_id);
    target.allies.push(summoner_id);

    let mut game = create_test_game_with_characters(vec![summoner, target]);
    game.alliance_events.push(
        crate::characters::alliances::AllianceEvent::AllianceSummons {
            summoner: summoner_id,
            target: target_id,
        },
    );
    let mut rng = SmallRng::seed_from_u64(42);
    game.process_alliance_events(&mut rng);

    let woken = &game.characters[1];
    assert!(!woken.sleeping, "summons should wake sleeping ally");
    assert_eq!(woken.sleep_remaining, 0);
    assert_eq!(woken.cycles_awake, 0);

    let woke = game.messages.iter().any(|m| {
        matches!(
            &m.payload,
            MessagePayload::CharacterWoke {
                reason: shared::messages::WakeReason::Interrupted {
                    event: shared::messages::InterruptionKind::AllianceSummons { .. },
                },
                ..
            }
        )
    });
    assert!(
        woke,
        "expected CharacterWoke{{Interrupted/AllianceSummons}}"
    );
}
