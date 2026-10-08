use super::*;

#[test]
fn test_announce_cycle_start() {
    let character1 = create_character("Character1", true);
    let character2 = create_character("Character2", true);
    let mut game = create_test_game_with_characters(vec![character1.clone(), character2.clone()]);
    game.day = Some(1);
    let _ = game.announce_cycle_start(shared::messages::Phase::Day);
    assert_eq!(game.messages.len(), 2);
}

#[test]
fn test_announce_cycle_end() {
    let character1 = create_character("Character1", true);
    let mut character2 = create_character("Character2", false);
    character2.set_status(CharacterStatus::RecentlyDead);
    let mut game = create_test_game_with_characters(vec![character1.clone(), character2.clone()]);
    game.day = Some(1);
    let _ = game.announce_cycle_end(shared::messages::Phase::Day);
    assert_eq!(game.messages.len(), 2);
}

#[test]
fn test_announce_area_events() {
    let mut game = Game::new("Test Game");
    let mut area = AreaDetails::new(Some("Lake".to_string()), Area::Hub);
    let mut rng = rand::rng();
    area.events.push(AreaEvent::random(&mut rng));
    area.events.push(AreaEvent::random(&mut rng));
    game.areas.push(area);

    assert!(!game.areas[0].is_open());
    let _ = game.announce_area_events();

    assert_eq!(game.messages.len(), 3);
    let area_name = Area::Hub.to_string();
    for msg in &game.messages {
        assert_eq!(
            msg.source,
            shared::messages::MessageSource::Area(area_name.clone())
        );
        assert_eq!(
            msg.subject,
            format!("{}:area:{}", game.identifier, area_name)
        );
    }
}

#[test]
fn message_subjects_are_prefixed_with_game_id() {
    let mut game = Game::new("Subject Prefix Test");
    game.log(
        shared::messages::MessageSource::Game(game.identifier.clone()),
        format!("game:{}", game.identifier),
        "hello".to_string(),
    );
    game.log(
        shared::messages::MessageSource::Area("Hub".to_string()),
        "area:Hub".to_string(),
        "boom".to_string(),
    );
    game.log(
        shared::messages::MessageSource::Character("trib-id".to_string()),
        "character:trib-id".to_string(),
        "ouch".to_string(),
    );
    let prefix = format!("{}:", game.identifier);
    for msg in &game.messages {
        assert!(
            msg.subject.starts_with(&prefix),
            "subject {:?} missing game-id prefix {:?}",
            msg.subject,
            prefix
        );
    }
    let count_before = game.messages.len();
    let already_prefixed = format!("{}:area:Other", game.identifier);
    game.log(
        shared::messages::MessageSource::Area("Other".to_string()),
        already_prefixed.clone(),
        "ok".to_string(),
    );
    assert_eq!(
        game.messages[count_before].subject, already_prefixed,
        "subject already prefixed should not be double-prefixed"
    );
}

#[test]
fn spawn_patrons_creates_six_with_loyalist_team() {
    let mut game = Game::default();
    let mut rng = SmallRng::seed_from_u64(42);
    game.spawn_patrons(&mut rng);

    assert_eq!(game.patrons.len(), 6);
    let loyalist = game
        .patrons
        .iter()
        .find(|s| s.archetype == shared::patrons::ArchetypeId::Loyalist)
        .expect("Loyalist must spawn");
    let team = loyalist.bound_team.expect("Loyalist gets a team");
    assert!((1u8..=8).contains(&team));
}

#[test]
fn spawn_patrons_is_idempotent() {
    let mut game = Game::default();
    let mut rng = SmallRng::seed_from_u64(1);
    game.spawn_patrons(&mut rng);
    game.spawn_patrons(&mut rng);
    assert_eq!(game.patrons.len(), 6);
}

#[test]
fn budget_falls_inside_archetype_band() {
    let mut game = Game::default();
    let mut rng = SmallRng::seed_from_u64(7);
    game.spawn_patrons(&mut rng);
    for s in &game.patrons {
        let band = shared::patrons::archetype(s.archetype).budget_band;
        assert!(s.budget_remaining >= band.0 && s.budget_remaining <= band.1);
    }
}
