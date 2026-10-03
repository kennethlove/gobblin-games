use super::*;

#[test]
fn process_alliance_events_betrayal_removes_pair_on_victim_side() {
    let mut betrayer = Character::new("Betrayer".to_string(), Some(1), None);
    let mut victim = Character::new("Victim".to_string(), Some(2), None);
    victim.allies.push(betrayer.id);
    victim.blood = 1000;
    betrayer.blood = 1000;
    let bid = betrayer.id;
    let vid = victim.id;

    let mut game = create_test_game_with_characters(vec![betrayer, victim]);
    game.alliance_events.push(
        crate::characters::alliances::AllianceEvent::BetrayalRecorded {
            betrayer: bid,
            victim: vid,
        },
    );

    let mut rng = SmallRng::seed_from_u64(53);
    game.process_alliance_events(&mut rng);

    let v = game.characters.iter().find(|t| t.id == vid).unwrap();
    assert!(!v.allies.contains(&bid));
    assert!(v.pending_trust_shock);
    assert!(game.alliance_events.is_empty());
}

#[test]
fn process_alliance_events_betrayer_not_marked_for_trust_shock() {
    let betrayer = Character::new("Betrayer".to_string(), Some(1), None);
    let victim = Character::new("Victim".to_string(), Some(2), None);
    let bid = betrayer.id;
    let vid = victim.id;

    let mut game = create_test_game_with_characters(vec![betrayer, victim]);
    game.alliance_events.push(
        crate::characters::alliances::AllianceEvent::BetrayalRecorded {
            betrayer: bid,
            victim: vid,
        },
    );

    let mut rng = SmallRng::seed_from_u64(53);
    game.process_alliance_events(&mut rng);

    let b = game.characters.iter().find(|t| t.id == bid).unwrap();
    assert!(!b.pending_trust_shock, "betrayer must not roll trust-shock");
}

#[test]
fn process_alliance_events_death_removes_deceased_from_all_ally_lists() {
    let deceased = Character::new("Deceased".to_string(), Some(1), None);
    let mut a = Character::new("A".to_string(), Some(2), None);
    let mut b = Character::new("B".to_string(), Some(3), None);
    a.allies.push(deceased.id);
    b.allies.push(deceased.id);
    // Sanity is now derived from mental_conditions; full sanity by default (no conditions)
    // a and b start with 100 sanity via effective_sanity()

    let did = deceased.id;
    let mut game = create_test_game_with_characters(vec![deceased, a, b]);
    game.alliance_events
        .push(crate::characters::alliances::AllianceEvent::DeathRecorded {
            deceased: did,
            killer: None,
        });

    let mut rng = SmallRng::seed_from_u64(89);
    game.process_alliance_events(&mut rng);

    for t in game.characters.iter().filter(|t| t.id != did) {
        assert!(
            !t.allies.contains(&did),
            "character {} still lists deceased",
            t.name
        );
    }
    assert!(game.alliance_events.is_empty());
}

#[test]
fn run_character_cycle_drains_character_alliance_events_into_game_queue() {
    let mut character1 = create_character("Character1", true);
    let mut character2 = create_character("Character2", true);
    character2.allies.push(character1.id);
    let bid = character1.id;
    let vid = character2.id;
    character1.alliance_events.push(
        crate::characters::alliances::AllianceEvent::BetrayalRecorded {
            betrayer: bid,
            victim: vid,
        },
    );

    let mut game = create_test_game_with_characters(vec![character1.clone(), character2.clone()]);
    let area = AreaDetails::new(Some("Lake".to_string()), Area::Hub);
    game.areas.push(area);
    let closed_areas = game
        .areas
        .iter()
        .filter(|ad| ad.area.is_some() & !ad.is_open())
        .map(|ad| ad.area.unwrap())
        .collect::<Vec<Area>>();

    let mut rng = SmallRng::seed_from_u64(211);
    let _ = game.run_character_cycle(
        crate::messages::Phase::Day,
        &mut rng,
        closed_areas,
        vec![character1.clone(), character2.clone()],
        2,
    );

    assert!(game.alliance_events.is_empty(), "game queue must drain");
    for t in &game.characters {
        assert!(
            t.alliance_events.is_empty(),
            "character {} buffer must drain",
            t.name
        );
    }
    let v = game.characters.iter().find(|t| t.id == vid).unwrap();
    assert!(!v.allies.contains(&bid), "victim allies cleaned");
    assert!(v.pending_trust_shock, "victim flagged for trust shock");
}

#[test]
fn run_character_cycle_forms_alliance_between_compatible_same_area_characters() {
    use crate::characters::traits::Trait;
    let mut t1 = create_character("Snipsnout", true);
    let mut t2 = create_character("Pricklepaw", true);
    t1.clan = 1;
    t2.clan = 1;
    t1.traits = vec![Trait::Friendly];
    t2.traits = vec![Trait::Friendly];
    t1.area = Area::Hub;
    t2.area = Area::Hub;

    let id1 = t1.id;
    let id2 = t2.id;

    let mut game = create_test_game_with_characters(vec![t1.clone(), t2.clone()]);
    let area = AreaDetails::new(Some("Lake".to_string()), Area::Hub);
    game.areas.push(area);
    let closed_areas = game
        .areas
        .iter()
        .filter(|ad| ad.area.is_some() & !ad.is_open())
        .map(|ad| ad.area.unwrap())
        .collect::<Vec<Area>>();

    let mut formed = false;
    for seed in 0u64..400 {
        let mut g = game.clone();
        let mut rng = SmallRng::seed_from_u64(seed);
        let _ = g.run_character_cycle(
            crate::messages::Phase::Day,
            &mut rng,
            closed_areas.clone(),
            vec![t1.clone(), t2.clone()],
            2,
        );
        let a1 = g.characters.iter().find(|t| t.id == id1).unwrap();
        let a2 = g.characters.iter().find(|t| t.id == id2).unwrap();
        if a1.allies.contains(&id2) && a2.allies.contains(&id1) {
            formed = true;
            break;
        }
    }
    assert!(
        formed,
        "Friendly same-clan pair must form an alliance within a few cycles"
    );
}

#[test]
fn run_character_cycle_treacherous_character_betrays_same_area_ally_when_timer_elapses() {
    use crate::characters::alliances::TREACHEROUS_BETRAYAL_INTERVAL;
    use crate::characters::traits::Trait;

    let mut betrayer = create_character("Rendmaw", true);
    let mut victim = create_character("Gnawpaw", true);
    betrayer.traits = vec![Trait::Treacherous];
    victim.traits = vec![Trait::Tough];
    betrayer.allies.push(victim.id);
    victim.allies.push(betrayer.id);
    betrayer.turns_since_last_betrayal = TREACHEROUS_BETRAYAL_INTERVAL;
    betrayer.area = Area::Hub;
    victim.area = Area::Hub;
    betrayer.clan = 1;
    victim.clan = 2;

    let bid = betrayer.id;
    let vid = victim.id;

    let mut game = create_test_game_with_characters(vec![betrayer.clone(), victim.clone()]);
    let area = AreaDetails::new(Some("Lake".to_string()), Area::Hub);
    game.areas.push(area);
    let closed_areas = game
        .areas
        .iter()
        .filter(|ad| ad.area.is_some() & !ad.is_open())
        .map(|ad| ad.area.unwrap())
        .collect::<Vec<Area>>();

    let mut rng = SmallRng::seed_from_u64(313);
    let _ = game.run_character_cycle(
        crate::messages::Phase::Day,
        &mut rng,
        closed_areas,
        vec![betrayer.clone(), victim.clone()],
        2,
    );

    let v = game.characters.iter().find(|t| t.id == vid).unwrap();
    let b = game.characters.iter().find(|t| t.id == bid).unwrap();
    assert!(!v.allies.contains(&bid), "victim allies cleaned by event");
    assert!(v.pending_trust_shock, "victim flagged for trust shock");
    assert!(!b.allies.contains(&vid), "betrayer dropped victim locally");
    assert_eq!(
        b.turns_since_last_betrayal, 0,
        "betrayal resets the cooldown timer"
    );
}

#[test]
fn run_character_cycle_treacherous_no_betrayal_without_same_area_ally_resets_timer() {
    use crate::characters::alliances::TREACHEROUS_BETRAYAL_INTERVAL;
    use crate::characters::traits::Trait;

    let mut loner = create_character("Snoutrot", true);
    let mut other = create_character("Scabface", true);
    loner.traits = vec![Trait::Treacherous];
    other.traits = vec![Trait::Tough];
    loner.turns_since_last_betrayal = TREACHEROUS_BETRAYAL_INTERVAL;
    loner.area = Area::Sector1;
    other.area = Area::Sector4;
    loner.clan = 5;
    other.clan = 6;
    let lid = loner.id;

    let mut game = create_test_game_with_characters(vec![loner.clone(), other.clone()]);
    game.areas
        .push(AreaDetails::new(Some("Hill".to_string()), Area::Sector1));
    game.areas
        .push(AreaDetails::new(Some("Lake".to_string()), Area::Sector4));
    let closed_areas = game
        .areas
        .iter()
        .filter(|ad| ad.area.is_some() & !ad.is_open())
        .map(|ad| ad.area.unwrap())
        .collect::<Vec<Area>>();

    let mut rng = SmallRng::seed_from_u64(419);
    let _ = game.run_character_cycle(
        crate::messages::Phase::Day,
        &mut rng,
        closed_areas,
        vec![loner.clone(), other.clone()],
        2,
    );

    let l = game.characters.iter().find(|t| t.id == lid).unwrap();
    assert_eq!(
        l.turns_since_last_betrayal, 0,
        "missed opportunity also resets the timer per spec §7.4(b)"
    );
}

#[test]
fn run_character_cycle_enqueues_death_recorded_for_recently_dead_ally() {
    use crate::characters::traits::Trait;

    let mut deceased = create_character("Nib", true);
    let mut survivor = create_character("Snaggletooth", true);
    survivor.brain.thresholds.extreme_low_sanity = 50;
    survivor.traits = vec![Trait::Tough];
    deceased.traits = vec![Trait::Tough];
    survivor.allies.push(deceased.id);
    deceased.allies.push(survivor.id);
    deceased.blood = 0;
    deceased.status = CharacterStatus::RecentlyDead;
    deceased.area = Area::Hub;
    survivor.area = Area::Hub;
    deceased.clan = 11;
    survivor.clan = 12;

    let did = deceased.id;
    let sid = survivor.id;

    let mut game = create_test_game_with_characters(vec![deceased.clone(), survivor.clone()]);
    game.areas
        .push(AreaDetails::new(Some("Lake".to_string()), Area::Hub));
    let living = game.living_characters();
    let closed_areas: Vec<Area> = vec![];

    let mut rng = SmallRng::seed_from_u64(547);
    let _ = game.run_character_cycle(
        crate::messages::Phase::Day,
        &mut rng,
        closed_areas,
        living,
        1,
    );

    assert!(
        game.alliance_events.is_empty(),
        "queue must drain after cycle"
    );
    let d = game.characters.iter().find(|t| t.id == did).unwrap();
    assert_eq!(d.status, CharacterStatus::Dead);
    let s = game.characters.iter().find(|t| t.id == sid).unwrap();
    assert!(
        !s.allies.contains(&did),
        "survivor must not retain a dead ally edge"
    );
}

#[test]
fn run_character_cycle_three_way_preserves_existing_alliance() {
    use crate::characters::traits::Trait;

    let mut a = create_character("Snaggletooth", true);
    let mut b = create_character("Grubworm", true);
    let mut c = create_character("Rendmaw", true);
    a.traits = vec![Trait::Friendly];
    b.traits = vec![Trait::Loyal];
    c.traits = vec![Trait::LoneWolf];
    a.allies.push(b.id);
    b.allies.push(a.id);
    a.area = Area::Hub;
    b.area = Area::Hub;
    c.area = Area::Hub;
    a.clan = 1;
    b.clan = 2;
    c.clan = 3;

    let aid = a.id;
    let bid = b.id;
    let cid = c.id;

    let mut game = create_test_game_with_characters(vec![a.clone(), b.clone(), c.clone()]);
    game.areas
        .push(AreaDetails::new(Some("Lake".to_string()), Area::Hub));
    let living = game.living_characters();
    let closed_areas: Vec<Area> = vec![];

    let mut rng = SmallRng::seed_from_u64(547);
    let _ = game.run_character_cycle(
        crate::messages::Phase::Day,
        &mut rng,
        closed_areas,
        living,
        3,
    );

    let a2 = game.characters.iter().find(|t| t.id == aid).unwrap();
    let b2 = game.characters.iter().find(|t| t.id == bid).unwrap();
    let c2 = game.characters.iter().find(|t| t.id == cid).unwrap();
    assert!(a2.allies.contains(&bid), "A still allied with B");
    assert!(b2.allies.contains(&aid), "B still allied with A");
    assert!(!a2.allies.contains(&cid), "A did not bond with LoneWolf C");
    assert!(!b2.allies.contains(&cid), "B did not bond with LoneWolf C");
    assert!(c2.allies.is_empty(), "LoneWolf C remains unallied");
}

#[test]
fn run_character_cycle_consumes_recently_killed_by_for_combat_death() {
    let mut deceased = create_character("Nib", true);
    let mut killer = create_character("Rendmaw", true);
    let mut survivor = create_character("Snaggletooth", true);

    let did = deceased.id;
    let kid = killer.id;
    let sid = survivor.id;

    survivor.allies.push(deceased.id);
    deceased.allies.push(survivor.id);

    deceased.blood = 0;
    deceased.status = CharacterStatus::RecentlyDead;
    deceased.recently_killed_by = Some(kid);

    deceased.area = Area::Hub;
    killer.area = Area::Hub;
    survivor.area = Area::Hub;
    deceased.clan = 11;
    killer.clan = 2;
    survivor.clan = 12;

    let mut game =
        create_test_game_with_characters(vec![deceased.clone(), killer.clone(), survivor.clone()]);
    game.areas
        .push(AreaDetails::new(Some("Lake".to_string()), Area::Hub));
    let living = game.living_characters();
    let closed_areas: Vec<Area> = vec![];

    let mut rng = SmallRng::seed_from_u64(547);
    let _ = game.run_character_cycle(
        crate::messages::Phase::Day,
        &mut rng,
        closed_areas,
        living,
        1,
    );

    assert!(
        game.alliance_events.is_empty(),
        "queue must drain after cycle"
    );
    let d = game.characters.iter().find(|t| t.id == did).unwrap();
    assert_eq!(d.status, CharacterStatus::Dead);
    assert!(
        d.recently_killed_by.is_none(),
        "cycle must take() the killer field after emitting DeathRecorded"
    );
    let s = game.characters.iter().find(|t| t.id == sid).unwrap();
    assert!(
        !s.allies.contains(&did),
        "survivor must not retain a dead ally edge"
    );
}

#[test]
fn run_character_cycle_environmental_death_emits_killer_none() {
    let mut deceased = create_character("Nib", true);
    let mut survivor = create_character("Snaggletooth", true);

    let did = deceased.id;
    let sid = survivor.id;

    survivor.allies.push(deceased.id);
    deceased.allies.push(survivor.id);

    deceased.blood = 0;
    deceased.status = CharacterStatus::RecentlyDead;
    assert!(deceased.recently_killed_by.is_none());

    deceased.area = Area::Hub;
    survivor.area = Area::Hub;
    deceased.clan = 11;
    survivor.clan = 12;

    let mut game = create_test_game_with_characters(vec![deceased.clone(), survivor.clone()]);
    game.areas
        .push(AreaDetails::new(Some("Lake".to_string()), Area::Hub));
    let living = game.living_characters();
    let closed_areas: Vec<Area> = vec![];

    let mut rng = SmallRng::seed_from_u64(547);
    let _ = game.run_character_cycle(
        crate::messages::Phase::Day,
        &mut rng,
        closed_areas,
        living,
        1,
    );

    let d = game.characters.iter().find(|t| t.id == did).unwrap();
    assert_eq!(d.status, CharacterStatus::Dead);
    assert!(
        d.recently_killed_by.is_none(),
        "environmental death keeps killer field None"
    );
    let s = game.characters.iter().find(|t| t.id == sid).unwrap();
    assert!(!s.allies.contains(&did));
}

#[test]
fn alliance_formation_emits_message_with_alliance_formed_kind() {
    use crate::characters::traits::Trait;

    let mut t1 = create_character("Snipsnout", true);
    let mut t2 = create_character("Pricklepaw", true);
    t1.clan = 1;
    t2.clan = 1;
    t1.traits = vec![Trait::Friendly];
    t2.traits = vec![Trait::Friendly];
    t1.area = Area::Hub;
    t2.area = Area::Hub;

    let base = create_test_game_with_characters(vec![t1.clone(), t2.clone()]);
    let mut game_with_area = base.clone();
    game_with_area
        .areas
        .push(AreaDetails::new(Some("Lake".to_string()), Area::Hub));
    let closed_areas: Vec<Area> = vec![];

    let mut hit: Option<crate::messages::GameMessage> = None;
    for seed in 0u64..400 {
        let mut g = game_with_area.clone();
        let mut rng = SmallRng::seed_from_u64(seed);
        let _ = g.run_character_cycle(
            crate::messages::Phase::Day,
            &mut rng,
            closed_areas.clone(),
            vec![t1.clone(), t2.clone()],
            2,
        );
        if let Some(m) = g.messages.iter().find(|m| {
            matches!(
                m.payload,
                crate::messages::MessagePayload::AllianceFormed { .. }
            )
        }) {
            hit = Some(m.clone());
            break;
        }
    }
    let m = hit.expect("at least one cycle must emit AllianceFormed");
    assert!(
        matches!(
            m.payload,
            crate::messages::MessagePayload::AllianceFormed { .. }
        ),
        "expected AllianceFormed payload, got {:?}",
        m.payload
    );
    assert!(
        m.content.contains("form an alliance"),
        "content should match GameOutput::AllianceFormed display, got: {}",
        m.content
    );
}

#[test]
fn betrayal_emits_message_with_betrayal_triggered_kind() {
    use crate::characters::alliances::TREACHEROUS_BETRAYAL_INTERVAL;
    use crate::characters::traits::Trait;

    let mut betrayer = create_character("Rendmaw", true);
    let mut victim = create_character("Gnawpaw", true);
    betrayer.traits = vec![Trait::Treacherous];
    victim.traits = vec![Trait::Tough];
    betrayer.allies.push(victim.id);
    victim.allies.push(betrayer.id);
    betrayer.turns_since_last_betrayal = TREACHEROUS_BETRAYAL_INTERVAL;
    betrayer.area = Area::Hub;
    victim.area = Area::Hub;
    betrayer.clan = 1;
    victim.clan = 2;

    let mut game = create_test_game_with_characters(vec![betrayer.clone(), victim.clone()]);
    game.areas
        .push(AreaDetails::new(Some("Lake".to_string()), Area::Hub));
    let closed_areas: Vec<Area> = vec![];

    let mut rng = SmallRng::seed_from_u64(313);
    let _ = game.run_character_cycle(
        crate::messages::Phase::Day,
        &mut rng,
        closed_areas,
        vec![betrayer.clone(), victim.clone()],
        2,
    );

    let m = game
        .messages
        .iter()
        .find(|m| {
            matches!(
                m.payload,
                crate::messages::MessagePayload::BetrayalTriggered { .. }
            )
        })
        .expect("betrayal cycle must emit BetrayalTriggered message");
    assert_eq!(
        m.content,
        "Rendmaw betrays Gnawpaw — true to their treacherous nature."
    );
}

#[test]
fn alliance_formed_message_content_matches_game_event_display() {
    use crate::characters::traits::Trait;

    let mut t1 = create_character("Snipsnout", true);
    let mut t2 = create_character("Pricklepaw", true);
    t1.clan = 1;
    t2.clan = 1;
    t1.traits = vec![Trait::Friendly];
    t2.traits = vec![Trait::Friendly];
    t1.area = Area::Hub;
    t2.area = Area::Hub;

    let base = create_test_game_with_characters(vec![t1.clone(), t2.clone()]);
    let mut game_with_area = base.clone();
    game_with_area
        .areas
        .push(AreaDetails::new(Some("Lake".to_string()), Area::Hub));
    let closed_areas: Vec<Area> = vec![];

    let mut hit: Option<crate::messages::GameMessage> = None;
    for seed in 0u64..400 {
        let mut g = game_with_area.clone();
        let mut rng = SmallRng::seed_from_u64(seed);
        let _ = g.run_character_cycle(
            crate::messages::Phase::Day,
            &mut rng,
            closed_areas.clone(),
            vec![t1.clone(), t2.clone()],
            2,
        );
        if let Some(m) = g.messages.iter().find(|m| {
            matches!(
                m.payload,
                crate::messages::MessagePayload::AllianceFormed { .. }
            )
        }) {
            hit = Some(m.clone());
            break;
        }
    }
    let m = hit.expect("at least one cycle must emit AllianceFormed");

    let factor = m
        .content
        .rsplit_once('(')
        .and_then(|(_, rest)| rest.rsplit_once(')').map(|(f, _)| f.to_string()))
        .expect("rendered alliance message must contain a parenthesised factor");
    let candidates = [
        crate::events::GameEvent::AllianceFormed {
            character_a_id: t1.id,
            character_a_name: t1.name.clone(),
            character_b_id: t2.id,
            character_b_name: t2.name.clone(),
            factor: factor.clone(),
        },
        crate::events::GameEvent::AllianceFormed {
            character_a_id: t2.id,
            character_a_name: t2.name.clone(),
            character_b_id: t1.id,
            character_b_name: t1.name.clone(),
            factor,
        },
    ];
    assert!(
        candidates.iter().any(|ev| ev.to_string() == m.content),
        "GameMessage.content {:?} must match GameEvent::AllianceFormed Display \
         for one of {:?}",
        m.content,
        candidates.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
    );
}

#[test]
fn betrayal_triggered_message_content_matches_game_event_display() {
    use crate::events::GameEvent;

    use crate::characters::alliances::TREACHEROUS_BETRAYAL_INTERVAL;
    use crate::characters::traits::Trait;

    let mut betrayer = create_character("Rendmaw", true);
    let mut victim = create_character("Gnawpaw", true);
    betrayer.traits = vec![Trait::Treacherous];
    victim.traits = vec![Trait::Tough];
    betrayer.allies.push(victim.id);
    victim.allies.push(betrayer.id);
    betrayer.turns_since_last_betrayal = TREACHEROUS_BETRAYAL_INTERVAL;
    betrayer.area = Area::Hub;
    victim.area = Area::Hub;
    betrayer.clan = 1;
    victim.clan = 2;

    let betrayer_id = betrayer.id;
    let victim_id = victim.id;
    let betrayer_name = betrayer.name.clone();
    let victim_name = victim.name.clone();

    let mut game = create_test_game_with_characters(vec![betrayer.clone(), victim.clone()]);
    game.areas
        .push(AreaDetails::new(Some("Lake".to_string()), Area::Hub));
    let closed_areas: Vec<Area> = vec![];

    let mut rng = SmallRng::seed_from_u64(313);
    let _ = game.run_character_cycle(
        crate::messages::Phase::Day,
        &mut rng,
        closed_areas,
        vec![betrayer.clone(), victim.clone()],
        2,
    );

    let m = game
        .messages
        .iter()
        .find(|m| {
            matches!(
                m.payload,
                crate::messages::MessagePayload::BetrayalTriggered { .. }
            )
        })
        .expect("betrayal cycle must emit BetrayalTriggered message");

    let event = GameEvent::BetrayalTriggered {
        betrayer_id,
        betrayer_name,
        victim_id,
        victim_name,
    };
    assert_eq!(
        m.content,
        event.to_string(),
        "GameMessage.content must equal GameEvent::BetrayalTriggered Display"
    );
}

#[test]
#[ignore = "scenario gap: needs game with populated areas + deterministic combat outcome; \
            see plan 2026-04-26-game-timeline-pr1-backend.md task 12. Asserts no \
            CharacterMoved for character B at a later (tick, emit_index) than B's \
            CharacterKilled within the same (game_day, phase)."]
fn dead_character_has_no_movement_event_after_death_in_same_period() {
    use crate::messages::MessagePayload;

    let character_a = create_character("A", true);
    let character_b = create_character("B", true);
    let mut game = create_test_game_with_characters(vec![character_a, character_b]);
    let _ = game.run_phase(crate::messages::Phase::Day);

    let b_killed = game.messages.iter().find(|m| {
        matches!(&m.payload,
            MessagePayload::CharacterKilled { victim, .. } if victim.name == "B")
    });
    let b_killed = b_killed.expect("B should have died");

    let later_b_move = game.messages.iter().find(|m| {
        m.game_day == b_killed.game_day
            && m.phase == b_killed.phase
            && (m.tick, m.emit_index) > (b_killed.tick, b_killed.emit_index)
            && matches!(&m.payload,
                MessagePayload::CharacterMoved { character, .. } if character.name == "B")
    });

    assert!(
        later_b_move.is_none(),
        "no CharacterMoved for B should appear after B's CharacterKilled in the same period"
    );
}
