use super::*;
fn t(name: &str) -> CharacterRef {
    CharacterRef {
        identifier: format!("id-{name}").into(),
        name: name.into(),
    }
}

#[test]
fn phase_display_roundtrip() {
    for p in Phase::all() {
        let s = p.to_string();
        assert_eq!(s.parse::<Phase>().unwrap(), p);
    }
    assert_eq!(Phase::DAWN.to_string(), "06");
    assert_eq!(Phase::DAY.to_string(), "12");
    assert_eq!(Phase::DUSK.to_string(), "18");
    assert_eq!(Phase::NIGHT.to_string(), "00");
    assert!("noon".parse::<Phase>().is_err());
    assert!("07".parse::<Phase>().is_err(), "odd hours are not phases");
    assert!("24".parse::<Phase>().is_err());
    assert!("dawn".parse::<Phase>().is_err());
}

#[test]
fn phase_serde_hour_labels() {
    assert_eq!(serde_json::to_string(&Phase::DAWN).unwrap(), "\"06\"");
    assert_eq!(serde_json::to_string(&Phase::DAY).unwrap(), "\"12\"");
    assert_eq!(serde_json::to_string(&Phase::DUSK).unwrap(), "\"18\"");
    assert_eq!(serde_json::to_string(&Phase::NIGHT).unwrap(), "\"00\"");
    let p: Phase = serde_json::from_str("\"06\"").unwrap();
    assert_eq!(p, Phase::DAWN);
}

#[test]
fn phase_ord_and_next_canonical_cycle() {
    // Cycle position from the day start (06): 06→0 … 04→11.
    assert_eq!(Phase::DAWN.ord(), 0);
    assert_eq!(Phase::DAY.ord(), 3);
    assert_eq!(Phase::DUSK.ord(), 6);
    assert_eq!(Phase::NIGHT.ord(), 9);
    assert_eq!(Phase::DAY_END.ord(), 11);
    // Canonical cycle wraps 04 -> 06 so the engine can advance the
    // game-day at the boundary without special-casing the wire format.
    assert_eq!(Phase::DAWN.next(), Phase(8));
    assert_eq!(Phase::DAY_END.next(), Phase::DAWN);
    assert_eq!(Phase::NIGHT.next(), Phase(2));
    assert_eq!(
        Phase::all(),
        [
            Phase(6),
            Phase(8),
            Phase(10),
            Phase(12),
            Phase(14),
            Phase(16),
            Phase(18),
            Phase(20),
            Phase(22),
            Phase(0),
            Phase(2),
            Phase(4),
        ]
    );
    assert_eq!(Phase::all().len(), Phase::PHASES_PER_DAY as usize);

    // Ord sorts chronologically across the day boundary.
    let mut hours: Vec<Phase> = Phase::all().to_vec();
    hours.sort();
    assert_eq!(hours, Phase::all());
}

#[test]
fn phase_is_night_defaults() {
    // Night runs 20 -> 06 by default (wrap-around).
    assert!(Phase(20).is_night_default());
    assert!(Phase(0).is_night_default());
    assert!(Phase(4).is_night_default());
    assert!(!Phase(6).is_night_default());
    assert!(!Phase(12).is_night_default());
    assert!(!Phase(18).is_night_default());
}

#[test]
fn message_kind_serde_roundtrip() {
    for kind in [
        MessageKind::CharacterKilled,
        MessageKind::Combat,
        MessageKind::AllianceFormed,
        MessageKind::CharacterMoved,
        MessageKind::ItemFound,
        MessageKind::CharacterRested,
        MessageKind::CombatSwing,
    ] {
        let s = serde_json::to_string(&kind).unwrap();
        let back: MessageKind = serde_json::from_str(&s).unwrap();
        assert_eq!(kind, back);
    }
}

#[test]
fn kind_lifecycle_variants_map_correctly() {
    let p = MessagePayload::CharacterKilled {
        victim: t("v"),
        killer: None,
        cause: crate::afflictions::DeathCause::Hazard(
            crate::afflictions::HazardKind::FallingDebris,
        ),
    };
    assert_eq!(p.kind(), MessageKind::CharacterKilled);

    let p = MessagePayload::CharacterWounded {
        victim: t("v"),
        attacker: None,
        hp_lost: 5,
    };
    assert_eq!(p.kind(), MessageKind::CharacterWounded);
}

#[test]
fn kind_combat_maps_to_combat() {
    let p = MessagePayload::Combat(CombatEngagement {
        attacker: t("a"),
        target: t("b"),
        outcome: CombatOutcome::Killed,
        detail_lines: vec![],
    });
    assert_eq!(p.kind(), MessageKind::Combat);
}

#[test]
fn kind_alliance_variants_map_correctly() {
    assert_eq!(
        MessagePayload::AllianceFormed {
            members: vec![t("a"), t("b")],
        }
        .kind(),
        MessageKind::AllianceFormed
    );
    assert_eq!(
        MessagePayload::AllianceProposed {
            proposer: t("a"),
            target: t("b"),
        }
        .kind(),
        MessageKind::AllianceProposed
    );
    assert_eq!(
        MessagePayload::AllianceDissolved {
            members: vec![t("a")],
            reason: "x".into(),
        }
        .kind(),
        MessageKind::AllianceDissolved
    );
    assert_eq!(
        MessagePayload::BetrayalTriggered {
            betrayer: t("a"),
            victim: t("b"),
        }
        .kind(),
        MessageKind::BetrayalTriggered
    );
    assert_eq!(
        MessagePayload::TrustShockBreak {
            character: t("a"),
            partner: t("b"),
        }
        .kind(),
        MessageKind::TrustShockBreak
    );
}

#[test]
fn kind_movement_variants_map_correctly() {
    let area = AreaRef {
        identifier: "a1".into(),
        name: "A".into(),
    };
    assert_eq!(
        MessagePayload::CharacterMoved {
            character: t("a"),
            from: area.clone(),
            to: area.clone(),
        }
        .kind(),
        MessageKind::CharacterMoved
    );
    assert_eq!(
        MessagePayload::CharacterHidden {
            character: t("a"),
            area: area.clone(),
        }
        .kind(),
        MessageKind::CharacterHidden
    );
    assert_eq!(
        MessagePayload::AreaClosed { area: area.clone() }.kind(),
        MessageKind::AreaClosed
    );
    assert_eq!(
        MessagePayload::AreaEvent {
            area: area.clone(),
            kind: AreaEventKind::Storm,
            description: "x".into(),
        }
        .kind(),
        MessageKind::AreaEvent
    );
}

#[test]
fn kind_item_variants_map_correctly() {
    let area = AreaRef {
        identifier: "a1".into(),
        name: "A".into(),
    };
    let item = ItemRef {
        identifier: "i1".into(),
        name: "I".into(),
    };
    assert_eq!(
        MessagePayload::ItemFound {
            character: t("a"),
            item: item.clone(),
            area: area.clone(),
        }
        .kind(),
        MessageKind::ItemFound
    );
    assert_eq!(
        MessagePayload::ItemUsed {
            character: t("a"),
            item: item.clone(),
        }
        .kind(),
        MessageKind::ItemUsed
    );
    assert_eq!(
        MessagePayload::ItemDropped {
            character: t("a"),
            item: item.clone(),
            area: area.clone(),
        }
        .kind(),
        MessageKind::ItemDropped
    );
    let patron = MessagePayload::PatronGift {
        recipient: t("a"),
        item: item.clone(),
        donor: "Aesthete".into(),
    };
    assert_eq!(patron.kind(), MessageKind::PatronGift);
}

#[test]
fn kind_state_variants_map_correctly() {
    assert_eq!(
        MessagePayload::CharacterRested {
            character: t("a"),
            hp_restored: 3,
        }
        .kind(),
        MessageKind::CharacterRested
    );
    assert_eq!(
        MessagePayload::CharacterStarved {
            character: t("a"),
            hp_lost: 1,
        }
        .kind(),
        MessageKind::CharacterStarved
    );
    assert_eq!(
        MessagePayload::CharacterDehydrated {
            character: t("a"),
            hp_lost: 2,
        }
        .kind(),
        MessageKind::CharacterDehydrated
    );
    assert_eq!(
        MessagePayload::SanityBreak { character: t("a") }.kind(),
        MessageKind::SanityBreak
    );
}

#[test]
fn unknown_payload_tag_hard_errors() {
    let raw = serde_json::json!({ "type": "DefinitelyNotAVariant" });
    let result: Result<MessagePayload, _> = serde_json::from_value(raw);
    assert!(result.is_err());
}

#[test]
fn game_message_new_populates_required_fields() {
    let msg = GameMessage::new(
        MessageSource::Game("g".into()),
        2,
        Phase::NIGHT,
        3,
        0,
        "subj".into(),
        "content".into(),
        MessagePayload::SanityBreak { character: t("a") },
    );
    assert_eq!(msg.game_day, 2);
    assert_eq!(msg.phase, Phase::NIGHT);
    assert_eq!(msg.tick, 3);
    assert_eq!(msg.emit_index, 0);
    assert_eq!(msg.payload.kind(), MessageKind::SanityBreak);
}

fn make_msg(day: u32, phase: Phase, payload: MessagePayload) -> GameMessage {
    GameMessage::new(
        MessageSource::Game("g".into()),
        day,
        phase,
        1,
        0,
        "subject".into(),
        "content".into(),
        payload,
    )
}

#[test]
fn summarize_empty_input_with_current_day_zero() {
    let result = summarize_periods(&[], (0, Phase::DAY));
    assert_eq!(
        result.len(),
        1,
        "current period (day 0, Day) should always be seeded"
    );
    assert_eq!(result[0].day, 0);
    assert_eq!(result[0].phase, Phase::DAY);
    assert!(result[0].is_current);
    assert_eq!(result[0].event_count, 0);
    assert_eq!(result[0].deaths, 0);
}

#[test]
fn summarize_groups_by_day_and_phase() {
    let tref = CharacterRef {
        identifier: "t".into(),
        name: "T".into(),
    };
    let killed = MessagePayload::CharacterKilled {
        victim: tref.clone(),
        killer: None,
        cause: crate::afflictions::DeathCause::Unknown,
    };
    let moved = MessagePayload::CharacterHidden {
        character: tref.clone(),
        area: AreaRef {
            identifier: "a".into(),
            name: "A".into(),
        },
    };

    let msgs = vec![
        make_msg(1, Phase::DAY, killed.clone()),
        make_msg(1, Phase::DAY, moved.clone()),
        make_msg(1, Phase::NIGHT, moved.clone()),
        make_msg(2, Phase::DAY, killed.clone()),
    ];
    let result = summarize_periods(&msgs, (2, Phase::DAY));
    // Day 1 runs the full 12-phase cycle; day 2 is in progress up to
    // the current phase (12, ord 3) → 12 + 4 = 16 periods.
    assert_eq!(result.len(), 16);
    assert_eq!(
        result[3],
        PeriodSummary {
            day: 1,
            phase: Phase::DAY,
            deaths: 1,
            event_count: 2,
            is_current: false
        }
    );
    assert_eq!(
        result[6],
        PeriodSummary {
            day: 1,
            phase: Phase::DUSK,
            deaths: 0,
            event_count: 0,
            is_current: false
        }
    );
    assert_eq!(
        result[9],
        PeriodSummary {
            day: 1,
            phase: Phase::NIGHT,
            deaths: 0,
            event_count: 1,
            is_current: false
        }
    );
    assert_eq!(
        result[12],
        PeriodSummary {
            day: 2,
            phase: Phase::DAWN,
            deaths: 0,
            event_count: 0,
            is_current: false
        }
    );
    assert_eq!(
        result[15],
        PeriodSummary {
            day: 2,
            phase: Phase::DAY,
            deaths: 1,
            event_count: 1,
            is_current: true
        }
    );
}

#[test]
fn summarize_includes_empty_reached_periods() {
    let result = summarize_periods(&[], (2, Phase::DAY));
    // Day 1: full 12-phase cycle + day 2 up to the current phase (12).
    assert_eq!(result.len(), 16);
    assert_eq!(result[0].day, 1);
    assert_eq!(result[0].phase, Phase::DAWN);
    assert_eq!(result[6].phase, Phase::DUSK);
    assert_eq!(result[9].phase, Phase::NIGHT);
    assert_eq!(result[11].phase, Phase::DAY_END);
    assert_eq!(result[12].day, 2);
    assert_eq!(result[12].phase, Phase::DAWN);
    assert_eq!(result[15].phase, Phase::DAY);
    assert!(result[15].is_current);
}

#[test]
fn summarize_counts_combat_kills_as_deaths() {
    let combat_kill = MessagePayload::Combat(CombatEngagement {
        attacker: t("a"),
        target: t("b"),
        outcome: CombatOutcome::Killed,
        detail_lines: vec![],
    });
    let combat_wound = MessagePayload::Combat(CombatEngagement {
        attacker: t("a"),
        target: t("b"),
        outcome: CombatOutcome::Wounded,
        detail_lines: vec![],
    });
    let msgs = vec![
        make_msg(1, Phase::DAY, combat_kill.clone()),
        make_msg(1, Phase::DAY, combat_wound),
        make_msg(1, Phase::DAY, combat_kill),
    ];
    let result = summarize_periods(&msgs, (1, Phase::DAY));
    // Day 1 in progress up to the current phase (12): 06, 08, 10, 12.
    assert_eq!(result.len(), 4);
    assert_eq!(result[3].deaths, 2);
    assert_eq!(result[3].event_count, 3);
    assert!(result[3].is_current);
    assert!(
        result[..3]
            .iter()
            .all(|p| p.deaths == 0 && p.event_count == 0)
    );
}

#[test]
fn summarize_is_current_flag_set_correctly() {
    let tref = CharacterRef {
        identifier: "t".into(),
        name: "T".into(),
    };
    let p = MessagePayload::CharacterRested {
        character: tref,
        hp_restored: 1,
    };
    let msgs = vec![make_msg(2, Phase::NIGHT, p.clone())];
    let result = summarize_periods(&msgs, (2, Phase::NIGHT));
    let current: Vec<_> = result.iter().filter(|s| s.is_current).collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].day, 2);
    assert_eq!(current[0].phase, Phase::NIGHT);
}

/// The decision table: every `MessagePayload` variant's persistence tier,
/// asserted explicitly. New variants must be classified here (and in
/// `MessagePayload::importance`, which is exhaustive at compile time).
#[test]
fn importance_tiers_match_decision() {
    use crate::afflictions::{DeathCause, Severity, TrapKind};
    use crate::combat_beat::{CombatBeat, StressReport, SwingOutcome};
    use Importance::{Persist, ServerLogOnly};
    use MessagePayload::*;

    let area = AreaRef {
        identifier: "a".into(),
        name: "A".into(),
    };
    let item = ItemRef {
        identifier: "i".into(),
        name: "I".into(),
    };
    let beat = |outcome| CombatBeat {
        attacker: t("a"),
        target: t("b"),
        weapon: None,
        shield: None,
        wear: vec![],
        outcome,
        stress: StressReport {
            stress_damage: 0,
            stressed: None,
        },
        attacker_stamina_cost: 1,
        target_stamina_cost: 1,
    };
    let engagement = |outcome| CombatEngagement {
        attacker: t("a"),
        target: t("b"),
        outcome,
        detail_lines: vec![],
    };

    let cases: Vec<(MessagePayload, Importance)> = vec![
        (Generic, ServerLogOnly),
        (
            CharacterKilled {
                victim: t("v"),
                killer: None,
                cause: DeathCause::Combat,
            },
            Persist,
        ),
        (
            CharacterWounded {
                victim: t("v"),
                attacker: None,
                hp_lost: 10,
            },
            Persist,
        ),
        (
            CharacterAttacked {
                victim: t("v"),
                attacker: None,
            },
            ServerLogOnly, // outcome surfaces via Wounded/Combat
        ),
        (Combat(engagement(CombatOutcome::Killed)), Persist),
        (Combat(engagement(CombatOutcome::Wounded)), Persist),
        (Combat(engagement(CombatOutcome::TargetFled)), Persist),
        (Combat(engagement(CombatOutcome::AttackerFled)), Persist),
        (Combat(engagement(CombatOutcome::Stalemate)), ServerLogOnly),
        (CombatSwing(beat(SwingOutcome::Miss)), ServerLogOnly),
        (
            CombatSwing(beat(SwingOutcome::Wound { damage: 5 })),
            Persist,
        ),
        (
            AllianceFormed {
                members: vec![t("a"), t("b")],
            },
            Persist,
        ),
        (
            AllianceProposed {
                proposer: t("a"),
                target: t("b"),
            },
            Persist,
        ),
        (
            AllianceDissolved {
                members: vec![t("a")],
                reason: "r".into(),
            },
            Persist,
        ),
        (
            BetrayalTriggered {
                betrayer: t("a"),
                victim: t("b"),
            },
            Persist,
        ),
        (
            TrustShockBreak {
                character: t("a"),
                partner: t("b"),
            },
            Persist,
        ),
        (
            CharacterMoved {
                character: t("c"),
                from: area.clone(),
                to: area.clone(),
            },
            ServerLogOnly, // position derivable; from/to goes to server logs
        ),
        (
            CharacterHidden {
                character: t("c"),
                area: area.clone(),
            },
            ServerLogOnly, // hidden flag lives on the character
        ),
        (AreaClosed { area: area.clone() }, Persist),
        (
            AreaEvent {
                area: area.clone(),
                kind: AreaEventKind::Hazard,
                description: "d".into(),
            },
            Persist,
        ),
        (
            ItemFound {
                character: t("c"),
                item: item.clone(),
                area: area.clone(),
            },
            Persist,
        ),
        (
            ItemUsed {
                character: t("c"),
                item: item.clone(),
            },
            Persist,
        ),
        (
            ItemDropped {
                character: t("c"),
                item: item.clone(),
                area: area.clone(),
            },
            ServerLogOnly,
        ),
        (
            PatronGift {
                recipient: t("c"),
                item: item.clone(),
                donor: "donor".into(),
            },
            Persist,
        ),
        (
            CharacterRested {
                character: t("c"),
                hp_restored: 5,
            },
            ServerLogOnly, // hp derivable
        ),
        (
            CharacterStarved {
                character: t("c"),
                hp_lost: 20,
            },
            ServerLogOnly, // per-tick hp drain
        ),
        (
            CharacterDehydrated {
                character: t("c"),
                hp_lost: 20,
            },
            ServerLogOnly, // per-tick hp drain
        ),
        (SanityBreak { character: t("c") }, ServerLogOnly), // sanity derivable
        (CharacterBledOut { character: t("c") }, Persist),
        (
            WoundInfected {
                character: t("c"),
                body_part: "arm".into(),
            },
            Persist,
        ),
        (
            WoundHealed {
                character: t("c"),
                body_part: "arm".into(),
            },
            Persist,
        ),
        (
            WoundInflicted {
                character: t("c"),
                wound_type: "cut".into(),
                severity: "Mild".into(),
                body_part: "arm".into(),
            },
            Persist,
        ),
        (
            WoundBled {
                character: t("c"),
                blood_lost: 10,
            },
            ServerLogOnly, // per-tick drain, derivable from `blood`
        ),
        (
            WoundTreated {
                character: t("c"),
                body_part: "arm".into(),
            },
            Persist,
        ),
        (
            WoundAmputated {
                character: t("c"),
                body_part: "arm".into(),
            },
            Persist,
        ),
        (
            ConditionAcquired {
                character: t("c"),
                condition: "x".into(),
                severity: "Mild".into(),
            },
            Persist,
        ),
        (
            ConditionResolved {
                character: t("c"),
                condition: "x".into(),
            },
            Persist,
        ),
        (
            CharacterDesperate {
                character: t("c"),
                reason: "low hp".into(),
            },
            ServerLogOnly, // threshold derivable from blood/sanity
        ),
        (
            HungerBandChanged {
                character: t("c"),
                from: HungerBand::Sated,
                to: HungerBand::Starving,
            },
            ServerLogOnly,
        ),
        (
            ThirstBandChanged {
                character: t("c"),
                from: ThirstBand::Sated,
                to: ThirstBand::Dehydrated,
            },
            ServerLogOnly,
        ),
        (
            StaminaBandChanged {
                character: t("c"),
                from: StaminaBand::Fresh,
                to: StaminaBand::Exhausted,
            },
            ServerLogOnly,
        ),
        (
            ShelterSought {
                character: t("c"),
                area: area.clone(),
                success: true,
                roll: 10,
            },
            ServerLogOnly, // shelter state derivable
        ),
        (
            Foraged {
                character: t("c"),
                area: area.clone(),
                success: true,
                debt_recovered: 1,
            },
            ServerLogOnly, // hunger derivable; finds surface via ItemFound
        ),
        (
            Drank {
                character: t("c"),
                source: DrinkSource::Terrain { area: area.clone() },
                debt_recovered: 1,
            },
            ServerLogOnly,
        ),
        (
            Ate {
                character: t("c"),
                item: item.clone(),
                debt_recovered: 1,
            },
            ServerLogOnly, // consumption surfaces via ItemUsed
        ),
        (
            CycleStart {
                day: 1,
                phase: Phase::DAY_START,
            },
            Persist,
        ),
        (
            CycleStart {
                day: 1,
                phase: Phase::from_hour(Phase::NIGHTFALL_HOUR).unwrap(),
            },
            Persist,
        ),
        (
            CycleStart {
                day: 1,
                phase: Phase::from_hour(12).unwrap(),
            },
            ServerLogOnly, // 10 other phase starts are clock bookkeeping
        ),
        (
            CycleEnd {
                day: 1,
                phase: Phase::DAY,
            },
            ServerLogOnly,
        ),
        (
            PhaseStarted {
                day: 1,
                phase: Phase::DAY,
                weather_summary: None,
            },
            ServerLogOnly,
        ),
        (
            PhaseEnded {
                day: 1,
                phase: Phase::DAY,
            },
            ServerLogOnly,
        ),
        (
            CharacterSlept {
                character: t("c"),
                phase: Phase::NIGHT,
                restored_stamina: 1,
                restored_hp: 0,
            },
            ServerLogOnly,
        ),
        (
            CharacterWoke {
                character: t("c"),
                phase: Phase::NIGHT,
                reason: WakeReason::Rested,
            },
            ServerLogOnly,
        ),
        (
            CharacterWoke {
                character: t("c"),
                phase: Phase::NIGHT,
                reason: WakeReason::Interrupted {
                    event: InterruptionKind::Ambush { attacker: t("a") },
                },
            },
            Persist, // something hit them
        ),
        (
            SleepIncident {
                character: t("c"),
                kind: SleepIncidentKind::Annoying,
                description: "d".into(),
            },
            Persist,
        ),
        (
            GameEnded {
                winner: None,
                winning_team: None,
            },
            Persist,
        ),
        (
            AfflictionAcquired {
                character_id: "c".into(),
                affliction: "flu".into(),
                severity: "Mild".into(),
            },
            Persist,
        ),
        (
            AfflictionProgressed {
                character_id: "c".into(),
                affliction: "flu".into(),
                from_severity: "Mild".into(),
                to_severity: "Severe".into(),
            },
            Persist,
        ),
        (
            AfflictionHealed {
                character_id: "c".into(),
                affliction: "flu".into(),
            },
            ServerLogOnly, // afflictions field shows current state
        ),
        (
            AfflictionCascaded {
                character_id: "c".into(),
                from_affliction: "wound".into(),
                to_affliction: "infection".into(),
            },
            Persist,
        ),
        (
            TraumaAcquired {
                character: "c".into(),
                severity: "Mild".into(),
                source: "death".into(),
            },
            Persist,
        ),
        (
            TraumaReinforced {
                character: "c".into(),
                from_severity: "Mild".into(),
                to_severity: "Moderate".into(),
                floor_bumped: true,
            },
            Persist,
        ),
        (
            PhobiaAcquired {
                character: "c".into(),
                trigger: "fire".into(),
                severity: "Mild".into(),
                origin: "trauma".into(),
            },
            Persist,
        ),
        (
            PhobiaTriggered {
                character: "c".into(),
                trigger: "fire".into(),
                severity: "Mild".into(),
                effect: PhobiaEffect::Penalty,
            },
            Persist,
        ),
        (
            PhobiaEscalated {
                character: "c".into(),
                trigger: "fire".into(),
                from_severity: "Mild".into(),
                to_severity: "Severe".into(),
            },
            Persist,
        ),
        (
            PhobiaHabituated {
                character: "c".into(),
                trigger: "fire".into(),
                from_severity: "Severe".into(),
                to_severity: None,
            },
            ServerLogOnly, // decay
        ),
        (
            PhobiaObserved {
                observer: "o".into(),
                subject: "s".into(),
                trigger: "fire".into(),
            },
            ServerLogOnly,
        ),
        (
            PhobiaForgotten {
                observer: "o".into(),
                subject: "s".into(),
                trigger: "fire".into(),
            },
            ServerLogOnly,
        ),
        (
            TraumaEscalated {
                character: "c".into(),
                from_severity: "Mild".into(),
                to_severity: "Moderate".into(),
            },
            Persist,
        ),
        (
            TraumaFlashback {
                character: "c".into(),
                severity: "Severe".into(),
                source: "death".into(),
            },
            Persist,
        ),
        (
            TraumaAvoidance {
                character: "c".into(),
                source: "fire".into(),
                prevented_action: "move".into(),
            },
            ServerLogOnly,
        ),
        (
            TraumaObserved {
                observer: "o".into(),
                subject: "s".into(),
                source: "death".into(),
            },
            ServerLogOnly,
        ),
        (
            TraumaForgotten {
                observer: "o".into(),
                subject: "s".into(),
                source: "death".into(),
            },
            ServerLogOnly,
        ),
        (
            TraumaHabituated {
                character: "c".into(),
                from_severity: "Severe".into(),
                to_severity: None,
            },
            ServerLogOnly,
        ),
        (
            FixationAcquired {
                character_id: "c".into(),
                target: "t".into(),
                severity: "Mild".into(),
                origin: "x".into(),
            },
            Persist,
        ),
        (
            FixationEscalated {
                character_id: "c".into(),
                target: "t".into(),
                old_severity: "Mild".into(),
                new_severity: "Severe".into(),
            },
            Persist,
        ),
        (
            FixationFired {
                character_id: "c".into(),
                target: "t".into(),
                severity: "Mild".into(),
                action: "move".into(),
            },
            Persist,
        ),
        (
            FixationConsummated {
                character_id: "c".into(),
                target: "t".into(),
            },
            Persist,
        ),
        (
            FixationThwarted {
                character_id: "c".into(),
                target: "t".into(),
                reason: "gone".into(),
            },
            Persist,
        ),
        (
            FixationFaded {
                character_id: "c".into(),
                target: "t".into(),
            },
            ServerLogOnly,
        ),
        (
            SubstanceUsed {
                character: "c".into(),
                item: "i".into(),
                substance: "s".into(),
            },
            Persist,
        ),
        (
            AddictionAcquired {
                character: "c".into(),
                substance: "s".into(),
                severity: "Mild".into(),
                use_count: 1,
            },
            Persist,
        ),
        (
            AddictionReinforced {
                character: "c".into(),
                substance: "s".into(),
                severity: "Mild".into(),
            },
            Persist,
        ),
        (
            AddictionEscalated {
                character: "c".into(),
                substance: "s".into(),
                from_severity: "Mild".into(),
                to_severity: "Moderate".into(),
            },
            Persist,
        ),
        (
            AddictionResisted {
                character: "c".into(),
                substance: "s".into(),
                reason: "cap".into(),
            },
            ServerLogOnly, // nothing changed
        ),
        (
            AddictionRelapse {
                character: "c".into(),
                substance: "s".into(),
                prior_uses: 3,
            },
            Persist,
        ),
        (
            AddictionCraving {
                character: "c".into(),
                substance: "s".into(),
                severity: "Mild".into(),
            },
            ServerLogOnly,
        ),
        (
            AddictionObserved {
                observer: "o".into(),
                subject: "s".into(),
                substance: "x".into(),
            },
            ServerLogOnly,
        ),
        (
            AddictionForgotten {
                observer: "o".into(),
                subject: "s".into(),
                substance: "x".into(),
            },
            ServerLogOnly,
        ),
        (
            AddictionHabituated {
                character: "c".into(),
                substance: "s".into(),
                from_severity: "Severe".into(),
                to_severity: None,
            },
            ServerLogOnly,
        ),
        (
            CharacterTrapped {
                character: "c".into(),
                kind: TrapKind::Drowning,
                severity: Severity::Severe,
            },
            Persist,
        ),
        (
            Struggling {
                character: "c".into(),
                kind: TrapKind::Drowning,
                severity: Severity::Severe,
                cycles_trapped: 1,
            },
            ServerLogOnly, // cycles_trapped lives on the affliction
        ),
        (
            TrappedEscaped {
                character: "c".into(),
                kind: TrapKind::Drowning,
                cycles_trapped: 2,
                rescued_by: vec![],
            },
            Persist,
        ),
        (
            CharacterDiedWhileTrapped {
                character: "c".into(),
                kind: TrapKind::Drowning,
            },
            Persist,
        ),
        (
            TrapSet {
                character: t("c"),
                trap_kind: "pit".into(),
            },
            Persist,
        ),
        (
            TrapTriggered {
                victim: t("c"),
                trap_kind: "pit".into(),
            },
            Persist,
        ),
        (
            RescueAttempted {
                rescuer: "r".into(),
                target: "t".into(),
                kind: TrapKind::Drowning,
                severity: Severity::Severe,
                bonus: 1.0,
            },
            Persist,
        ),
        (
            PartialRescueProgress {
                rescuer: "r".into(),
                target: "t".into(),
                kind: TrapKind::Drowning,
                severity: Severity::Severe,
                bonus: 1.0,
                progress: 1,
                threshold: 2,
            },
            ServerLogOnly, // progress lives on the rescue state
        ),
    ];

    // Every payload variant appears in the table (dupes allowed for
    // outcome-dependent tiers; distinct kinds must cover the enum).
    let mut kinds: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (payload, _) in &cases {
        kinds.insert(format!("{:?}", payload.kind()));
    }
    assert_eq!(
        kinds.len(),
        90,
        "table must cover every MessagePayload variant"
    );
    assert!(cases.len() >= 90, "table rows: {}", cases.len());

    for (payload, want) in cases {
        assert_eq!(
            payload.importance(),
            want,
            "tier mismatch for {:?}",
            payload.kind()
        );
    }
}
