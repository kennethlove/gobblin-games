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
    // Two teams standing: no winner yet.
    game.characters[0].team = 1;
    game.characters[1].team = 2;
    assert_eq!(game.winning_team(), None);
    game.characters[0].status = CharacterStatus::Dead;
    // Sole surviving team wins with its member(s) still alive.
    assert_eq!(game.winning_team(), Some(2));
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
    killer.attributes.speed = 100;
    killer.location.area = Area::Hub;

    let mut victim = Character::new("Victim".to_string(), None, None);
    victim.blood = 10;
    victim.attributes.strength = 1;
    victim.attributes.toughness = 1;
    victim.attributes.speed = 1;
    victim.location.area = Area::Hub;

    game.characters.clear();
    game.characters.push(killer);
    game.characters.push(victim);

    let mut rng = SmallRng::seed_from_u64(42);
    let agility_0 = game.characters[0].attributes.speed;
    let agility_1 = game.characters[1].attributes.speed;
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
        attrs.speed >= 1 && attrs.speed <= 100,
        "agility should be in 1..=100 range, got {}",
        attrs.speed
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
    assert_eq!(game.winning_team(), Some(winner_character.team));

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
    assert!(game.winning_team().is_none());

    let _ = game.check_for_winner();

    assert_eq!(game.status, GameStatus::Finished);
}

#[test]
fn test_post_game_recovery_wakes_dead_and_restores_survivor() {
    let mut alive = create_character("Stinky", true);
    alive.team = 1;
    let mut dead1 = create_character("Billy Slick", false);
    dead1.team = 2;
    let mut dead2 = create_character("Snaggletooth", false);
    dead2.team = 3;
    let mut game = create_test_game_with_characters(vec![alive.clone(), dead1, dead2]);
    // Wound the survivor so the seam has something to heal.
    game.characters[0].blood = 123;

    assert_eq!(game.living_characters().len(), 1);

    game.post_game_recovery();

    // Everyone is alive again — the roll never leaves anyone dead.
    assert!(game.characters.iter().all(|c| c.is_alive()));
    assert_eq!(game.living_characters().len(), 3);
    // Three distinct teams stand after recovery — no winner, because
    // winner determination runs at the finish seam BEFORE recovery.
    assert!(game.winning_team().is_none());
    // The survivor is restored to full health (matches the fresh fixture:
    // default blood, healthy, no wounds).
    assert_eq!(game.characters[0], alive);
    // Revived characters wake clean: healthy, blood restored, wounds gone.
    for c in &game.characters[1..] {
        assert_eq!(c.status, CharacterStatus::Healthy);
        assert!(c.blood > 0);
        assert!(c.wounds.is_empty());
    }
}

#[test]
fn test_post_game_recovery_ticks_temporary_traits_at_the_boundary() {
    use std::collections::BTreeMap;
    // A survivor whose Tough boon ran its last game: the boundary tick
    // must remove the trait and its duration entry with no trace.
    let mut survivor = create_character("Stinky", true);
    survivor.team = 1;
    survivor
        .traits
        .retain(|t| *t != characters::traits::Trait::Tough);
    survivor.traits.push(characters::traits::Trait::Tough);
    survivor.temporary_traits = BTreeMap::from([(characters::traits::Trait::Tough, 1)]);

    let mut game = create_test_game_with_characters(vec![survivor]);
    game.post_game_recovery();

    let after = &game.characters[0];
    assert!(after.is_alive());
    assert!(!after.traits.contains(&characters::traits::Trait::Tough));
    assert!(after.temporary_traits.is_empty());
}

#[test]
fn test_sole_team_wins_with_multiple_survivors() {
    let stinky = create_character("Stinky", true);
    let slick = create_character("Billy Slick", true);
    let foe = create_character("Snaggletooth", false);
    let mut game = create_test_game_with_characters(vec![stinky, slick, foe]);
    game.characters[0].team = 5;
    game.characters[1].team = 5;
    game.characters[2].team = 2; // dead foe from another team

    assert_eq!(game.living_characters().len(), 2);
    // Last team standing wins with both members still alive.
    assert_eq!(game.winning_team(), Some(5));

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
    // Two survivors from different teams: nobody has won yet.
    game.characters[0].team = 1;
    game.characters[1].team = 2;

    assert_eq!(game.living_characters().len(), 2);
    assert!(game.winning_team().is_none());

    let _ = game.check_for_winner();

    assert_eq!(game.status, starting_state);
}

#[test]
fn test_prepare_cycle() {
    use shared::messages::Phase;
    let mut game = Game::new("Test Game");
    let area = AreaDetails::new(Some("Lake".to_string()), Area::Sector1);
    let mut rng = rand::rng();
    let event = AreaEvent::random(&mut rng);
    game.day = Some(1);
    game.areas.push(area);
    game.areas[0].events.push(event.clone());
    let _ = game.prepare_cycle(Phase::DAWN);
    assert_eq!(game.day, Some(2));
    assert_eq!(game.areas[0].events.len(), 0);

    game.areas[0].events.push(event.clone());
    let _ = game.prepare_cycle(Phase::NIGHT);
    assert_eq!(game.day, Some(2));
    assert_eq!(game.areas[0].events.len(), 0);
}

#[test]
fn test_trigger_cycle_events() {}

/// The event driver must take the night band and the dawn/dusk anchors
/// from the game config: hour 18 is a plain night phase under shifted
/// hours (day 08 / night 18) but the silent dusk anchor under the
/// defaults (06/20), and the anchors move with the configured hours.
/// The band probabilities come from the config too — pinned to 1.0 / 0.0
/// per band here so the band classification asserts as exact counts.
#[test]
fn smoke_night_event_frequency_uses_configured_hours() {
    use shared::messages::Phase;

    let game_with_hours = |day_start_hour: u8, nightfall_hour: u8| {
        let mut game = Game::new("Test Game");
        game.status = GameStatus::InProgress;
        game.day = Some(2);
        game.config.day_start_hour = day_start_hour;
        game.config.nightfall_hour = nightfall_hour;
        for i in 0..128 {
            game.areas
                .push(AreaDetails::new(Some(format!("Lake{i}")), Area::Sector1));
        }
        game
    };
    let areas_that_fired = |game: &Game| game.areas.iter().filter(|a| !a.events.is_empty()).count();
    let mut rng = SmallRng::seed_from_u64(0);

    // Default hours: 18 is the dusk anchor — the substrate stays silent.
    let mut default_dusk = game_with_hours(6, 20);
    default_dusk
        .trigger_cycle_events(Phase::from_hour(18).unwrap(), &mut rng)
        .unwrap();
    assert_eq!(
        areas_that_fired(&default_dusk),
        0,
        "default dusk anchor stays silent"
    );

    // Shifted hours: hour 18 is a plain night phase, so the configured
    // night frequency applies (1.0) and the day frequency (0.0) never
    // does — "every area fires" is exact and seed-independent.
    let mut shifted_night = game_with_hours(8, 18);
    shifted_night.config.night_event_frequency = 1.0;
    shifted_night.config.day_event_frequency = 0.0;
    shifted_night
        .trigger_cycle_events(Phase::from_hour(18).unwrap(), &mut rng)
        .unwrap();
    assert_eq!(
        areas_that_fired(&shifted_night),
        128,
        "shifted night hour fires via the configured night frequency"
    );

    // Shifted hours: the dawn anchor moves to hour 8 — silent there ...
    let mut shifted_dawn = game_with_hours(8, 18);
    shifted_dawn
        .trigger_cycle_events(Phase::from_hour(8).unwrap(), &mut rng)
        .unwrap();
    assert_eq!(
        areas_that_fired(&shifted_dawn),
        0,
        "shifted dawn anchor stays silent"
    );

    // ... while hour 8 is an ordinary day phase under the defaults, fired
    // through the configured day frequency.
    let mut default_day = game_with_hours(6, 20);
    default_day.config.day_event_frequency = 1.0;
    default_day.config.night_event_frequency = 0.0;
    default_day
        .trigger_cycle_events(Phase::from_hour(8).unwrap(), &mut rng)
        .unwrap();
    assert_eq!(
        areas_that_fired(&default_day),
        128,
        "default day hour fires via the configured day frequency"
    );
}

/// The configured frequency is a per-area per-cycle probability: 1.0
/// fires every area, 0.0 fires none, and the default night rate (1/8,
/// from `GameConfig::default()`) fires a strict non-empty subset — all
/// deterministic.
#[test]
fn smoke_cycle_event_frequency_drives_cadence() {
    use shared::messages::Phase;

    let night_game = |night_event_frequency: f64| {
        let mut game = Game::new("Test Game");
        game.status = GameStatus::InProgress;
        game.day = Some(2);
        game.config.night_event_frequency = night_event_frequency;
        for i in 0..128 {
            game.areas
                .push(AreaDetails::new(Some(format!("Lake{i}")), Area::Sector1));
        }
        game
    };
    let areas_that_fired = |game: &Game| game.areas.iter().filter(|a| !a.events.is_empty()).count();
    // Hour 22 is an ordinary night phase under the default hours.
    let night = Phase::from_hour(22).unwrap();

    let mut always = night_game(1.0);
    let mut rng = SmallRng::seed_from_u64(0);
    always.trigger_cycle_events(night, &mut rng).unwrap();
    assert_eq!(
        areas_that_fired(&always),
        128,
        "frequency 1.0 fires every area"
    );

    let mut never = night_game(0.0);
    let mut rng = SmallRng::seed_from_u64(0);
    never.trigger_cycle_events(night, &mut rng).unwrap();
    assert_eq!(areas_that_fired(&never), 0, "frequency 0.0 fires no area");

    // The default night rate straight from GameConfig::default(): a
    // non-empty proper subset of the 128 areas fires under the pinned
    // seed (a silent or all-firing outcome would be a ~0 fluke).
    let default_rate = world::config::GameConfig::default().night_event_frequency;
    let mut partial = night_game(default_rate);
    let mut rng = SmallRng::seed_from_u64(0);
    partial.trigger_cycle_events(night, &mut rng).unwrap();
    let fired = areas_that_fired(&partial);
    assert!(
        fired > 0 && fired < 128,
        "default night rate fires a proper subset, fired={fired}"
    );
}

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
        shared::messages::Phase::DAY,
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
    let _ = game.run_phase(shared::messages::Phase::DAY);
    for t in &game.characters {
        assert_eq!(t.hunger, 1, "{} hunger should be 1 after one tick", t.name);
        assert_eq!(t.thirst, 1, "{} thirst should be 1 after one tick", t.name);
    }
}

#[test]
fn survival_tick_routes_dehydration_death_through_character_killed() {
    use shared::afflictions::DeathCause;
    use shared::messages::MessagePayload;
    let mut a = Character::new("Doomed".to_string(), Some(1), None);
    a.thirst = 4;
    a.blood = 10;
    let mut game = create_test_game_with_characters(vec![a]);
    game.day = Some(1);
    let _ = game.run_phase(shared::messages::Phase::DAY);
    let killed = game
        .messages
        .iter()
        .filter(|m| {
            matches!(&m.payload,
                MessagePayload::CharacterKilled { cause, .. } if *cause == DeathCause::Dehydration)
        })
        .count();
    assert_eq!(
        killed, 1,
        "exactly one CharacterKilled with cause=dehydration"
    );
}

#[test]
fn status_death_emits_exactly_one_character_killed() {
    use characters::AfflictionDraft;
    use shared::afflictions::{AfflictionKind, AfflictionSource, Severity};
    use shared::messages::MessagePayload;
    let mut a = Character::new("Doomed".to_string(), Some(1), None);
    a.blood = 5;
    a.try_acquire_affliction(AfflictionDraft {
        kind: AfflictionKind::Wounded,
        body_part: None,
        severity: Severity::Moderate,
        source: AfflictionSource::Environmental,
        trapped_metadata: None,
    });
    let mut game = create_test_game_with_characters(vec![a]);
    // Without an area record the character's turn (and process_status,
    // where the death announcement lives) never runs.
    game.areas
        .push(AreaDetails::new(Some("Hub".to_string()), Area::Hub));
    game.day = Some(1);
    let _ = game.run_phase(shared::messages::Phase::DAY);
    // process_status announces the status death and promotes to RecentlyDead;
    // the period-end sweep must not announce it a second time.
    let killed = game
        .messages
        .iter()
        .filter(|m| matches!(&m.payload, MessagePayload::CharacterKilled { .. }))
        .count();
    assert_eq!(
        killed,
        1,
        "one status death -> exactly one CharacterKilled; state: blood={} status={:?} aff={:?}",
        game.characters[0].blood,
        game.characters[0].status,
        game.characters[0].afflictions.keys().collect::<Vec<_>>()
    );
}

#[test]
fn bleed_out_death_emits_one_bled_out_and_one_killed() {
    use shared::messages::MessagePayload;
    use shared::wounds::{BodyPart, Wound, WoundSeverity, WoundType};
    let mut a = Character::new("Bleedy".to_string(), Some(1), None);
    a.blood = 10;
    // A critical bleeding wound drains blood to 0 in the wounds block,
    // not in process_status — this is the bleed-out death path.
    a.wounds.push(Wound::new(
        WoundType::Stab,
        WoundSeverity::Critical,
        BodyPart::Torso,
    ));
    let mut game = create_test_game_with_characters(vec![a]);
    // WHY: the default low_character_threshold (8) makes constrain_areas
    // close the lone open area with a random event (e.g. Wildfire -> Severe
    // Burned), which can zero blood in process_status before the wound drain
    // runs and announces an Unknown death instead of the bleed-out.
    game.config.low_character_threshold = 1;
    // Without an area record the character's turn (where the wound drain
    // lives) never runs.
    game.areas
        .push(AreaDetails::new(Some("Hub".to_string()), Area::Hub));
    game.day = Some(1);
    let _ = game.run_phase(shared::messages::Phase::DAY);
    let bled_out = game
        .messages
        .iter()
        .filter(|m| matches!(&m.payload, MessagePayload::CharacterBledOut { .. }))
        .count();
    assert_eq!(bled_out, 1, "one bleed-out -> exactly one CharacterBledOut");
    let killed: Vec<_> = game
        .messages
        .iter()
        .filter(|m| matches!(&m.payload, MessagePayload::CharacterKilled { .. }))
        .collect();
    assert_eq!(
        killed.len(),
        1,
        "one bleed-out -> exactly one CharacterKilled; state: blood={} status={:?}",
        game.characters[0].blood,
        game.characters[0].status
    );
    if let MessagePayload::CharacterKilled { killer, cause, .. } = &killed[0].payload {
        assert!(killer.is_none(), "bleed-out has no character killer");
        assert_eq!(
            *cause,
            shared::afflictions::DeathCause::Affliction(
                shared::afflictions::AfflictionKind::Wounded
            ),
            "bleed-out death cause"
        );
    } else {
        panic!("expected a CharacterKilled payload");
    }
}

#[test]
fn bleed_out_death_reaches_cycle_sweep_and_records_day_killed() {
    use shared::messages::MessagePayload;
    use shared::wounds::{BodyPart, Wound, WoundSeverity, WoundType};
    let mut a = Character::new("Bleedy".to_string(), Some(1), None);
    a.blood = 10;
    a.wounds.push(Wound::new(
        WoundType::Stab,
        WoundSeverity::Critical,
        BodyPart::Torso,
    ));
    let mut game = create_test_game_with_characters(vec![a]);
    game.areas
        .push(AreaDetails::new(Some("Hub".to_string()), Area::Hub));
    game.day = Some(1);
    let _ = game.run_phase(shared::messages::Phase::DAY);
    // A second phase proves the dead character is swept and stays swept.
    let _ = game.run_phase(shared::messages::Phase::DAY);
    let character = &game.characters[0];
    assert_eq!(
        character.status,
        CharacterStatus::Dead,
        "bleed-out death promotes through the cycle to Dead"
    );
    assert_eq!(
        character.statistics.day_killed,
        Some(1),
        "bleed-out death records day_killed"
    );
    let killed = game
        .messages
        .iter()
        .filter(|m| matches!(&m.payload, MessagePayload::CharacterKilled { .. }))
        .count();
    assert_eq!(killed, 1, "second phase must not re-announce the death");
}

#[test]
#[serial]
fn sleeping_character_naturally_wakes_after_duration_emits_character_woke() {
    use shared::messages::{MessagePayload, Phase};
    let mut t = create_character("Sleeper", true);
    t.sleeping = true;
    t.sleep_remaining = 1;
    t.cycles_awake = 9;
    t.stamina = 50;
    let mut game = create_test_game_with_characters(vec![t.clone()]);
    let area = AreaDetails::new(Some("Lake".to_string()), Area::Hub);
    game.areas.push(area);
    let mut rng = SmallRng::seed_from_u64(42);
    let _ = game.run_character_cycle(Phase::NIGHT, &mut rng, vec![], vec![t], 1);

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
    use shared::messages::Phase;
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
    let _ = game.run_character_cycle(Phase::NIGHT, &mut rng, vec![], vec![t], 1);

    let after = &game.characters[0];
    assert!(after.sleeping, "still mid-sleep");
    assert_eq!(after.sleep_remaining, 2);
    assert!(after.stamina > prior, "stamina should regen");
}

#[test]
#[serial]
fn sleeping_wounded_character_does_not_regen_hp() {
    use shared::afflictions::{Affliction, AfflictionKind, AfflictionSource, Severity};
    use shared::messages::Phase;
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
    let _ = game.run_character_cycle(Phase::NIGHT, &mut rng, vec![], vec![t], 1);
    assert_eq!(
        game.characters[0].effective_health(),
        prior_hp,
        "wounded characters do not heal while sleeping"
    );
}

#[test]
#[serial]
fn cycles_awake_does_not_increment_while_sleeping() {
    use shared::messages::Phase;
    let mut t = create_character("Sleeper", true);
    t.sleeping = true;
    t.sleep_remaining = 3;
    t.cycles_awake = 4;
    let mut game = create_test_game_with_characters(vec![t.clone()]);
    let area = AreaDetails::new(Some("Lake".to_string()), Area::Hub);
    game.areas.push(area);
    let mut rng = SmallRng::seed_from_u64(42);
    let _ = game.run_character_cycle(Phase::NIGHT, &mut rng, vec![], vec![t], 1);
    assert_eq!(game.characters[0].cycles_awake, 4);
}

#[test]
fn area_event_interrupts_sleeping_character() {
    use areas::events::AreaEvent;
    use shared::messages::{MessagePayload, Phase};
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
    let _ = game.run_character_cycle(Phase::NIGHT, &mut rng, vec![], vec![t], 1);

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
    use shared::messages::MessagePayload;
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
    game.alliance_events
        .push(characters::alliances::AllianceEvent::AllianceSummons {
            summoner: summoner_id,
            target: target_id,
        });
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
