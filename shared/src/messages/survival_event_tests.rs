use super::*;
fn tref() -> CharacterRef {
    CharacterRef {
        identifier: "t1".into(),
        name: "Rendmaw".into(),
    }
}
fn aref() -> AreaRef {
    AreaRef {
        identifier: "a1".into(),
        name: "Forest".into(),
    }
}
fn iref() -> ItemRef {
    ItemRef {
        identifier: "i1".into(),
        name: "Berries".into(),
    }
}

#[test]
fn shelter_sought_round_trip() {
    let p = MessagePayload::ShelterSought {
        character: tref(),
        area: aref(),
        success: true,
        roll: 2,
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: MessagePayload = serde_json::from_str(&json).unwrap();
    assert_eq!(format!("{:?}", p), format!("{:?}", back));
    assert_eq!(p.kind(), MessageKind::ShelterSought);
}

#[test]
fn band_change_payloads_round_trip() {
    let p = MessagePayload::HungerBandChanged {
        character: tref(),
        from: HungerBand::Sated,
        to: HungerBand::Hungry,
    };
    let back: MessagePayload = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(format!("{:?}", p), format!("{:?}", back));
    let p = MessagePayload::ThirstBandChanged {
        character: tref(),
        from: ThirstBand::Sated,
        to: ThirstBand::Parched,
    };
    let back: MessagePayload = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(format!("{:?}", p), format!("{:?}", back));
}

#[test]
fn stamina_band_change_round_trips_and_routes_to_state() {
    let p = MessagePayload::StaminaBandChanged {
        character: tref(),
        from: StaminaBand::Fresh,
        to: StaminaBand::Winded,
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: MessagePayload = serde_json::from_str(&json).unwrap();
    assert_eq!(format!("{:?}", p), format!("{:?}", back));
    assert_eq!(p.kind(), MessageKind::StaminaBandChanged);
    assert!(p.involves(tref().identifier.as_str()));
}

#[test]
fn stamina_band_enum_round_trips() {
    for band in [
        StaminaBand::Fresh,
        StaminaBand::Winded,
        StaminaBand::Exhausted,
    ] {
        let s = serde_json::to_string(&band).unwrap();
        let back: StaminaBand = serde_json::from_str(&s).unwrap();
        assert_eq!(band, back);
    }
}

#[test]
fn foraged_drank_ate_round_trip_and_kind() {
    let foraged = MessagePayload::Foraged {
        character: tref(),
        area: aref(),
        success: true,
        debt_recovered: 3,
    };
    let drank = MessagePayload::Drank {
        character: tref(),
        source: DrinkSource::Terrain { area: aref() },
        debt_recovered: 2,
    };
    let drank_item = MessagePayload::Drank {
        character: tref(),
        source: DrinkSource::Item { item: iref() },
        debt_recovered: 1,
    };
    let ate = MessagePayload::Ate {
        character: tref(),
        item: iref(),
        debt_recovered: 4,
    };
    let payloads = [
        (foraged, MessageKind::Foraged),
        (drank, MessageKind::Drank),
        (drank_item, MessageKind::Drank),
        (ate, MessageKind::Ate),
    ];
    for (p, expected) in payloads {
        let back: MessagePayload =
            serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(format!("{:?}", p), format!("{:?}", back));
        assert_eq!(p.kind(), expected);
    }
}

#[test]
fn survival_payloads_involve_character() {
    let p = MessagePayload::Ate {
        character: tref(),
        item: iref(),
        debt_recovered: 1,
    };
    assert!(p.involves("t1"));
    assert!(!p.involves("other"));
}

#[test]
fn wake_reason_serde_roundtrip_rested() {
    let r = WakeReason::Rested;
    let s = serde_json::to_string(&r).unwrap();
    let back: WakeReason = serde_json::from_str(&s).unwrap();
    assert_eq!(back, r);
}

#[test]
fn wake_reason_serde_roundtrip_interrupted_variants() {
    let cases = vec![
        WakeReason::Interrupted {
            event: InterruptionKind::Ambush {
                attacker: CharacterRef {
                    identifier: "a".into(),
                    name: "A".into(),
                },
            },
        },
        WakeReason::Interrupted {
            event: InterruptionKind::AreaEvent {
                kind: AreaEventKind::Fire,
            },
        },
        WakeReason::Interrupted {
            event: InterruptionKind::AllianceSummons {
                ally: CharacterRef {
                    identifier: "b".into(),
                    name: "B".into(),
                },
            },
        },
    ];
    for r in cases {
        let s = serde_json::to_string(&r).unwrap();
        let back: WakeReason = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }
}

#[test]
fn character_slept_woke_payload_kind_is_state() {
    let slept = MessagePayload::CharacterSlept {
        character: tref(),
        phase: Phase::Night,
        restored_stamina: 5,
        restored_hp: 2,
    };
    let woke = MessagePayload::CharacterWoke {
        character: tref(),
        phase: Phase::Dawn,
        reason: WakeReason::Rested,
    };
    assert_eq!(slept.kind(), MessageKind::CharacterSlept);
    assert_eq!(woke.kind(), MessageKind::CharacterWoke);
}

#[test]
fn character_slept_woke_involves_character() {
    let slept = MessagePayload::CharacterSlept {
        character: tref(),
        phase: Phase::Night,
        restored_stamina: 0,
        restored_hp: 0,
    };
    let woke = MessagePayload::CharacterWoke {
        character: tref(),
        phase: Phase::Dawn,
        reason: WakeReason::Rested,
    };
    assert!(slept.involves("t1"));
    assert!(!slept.involves("other"));
    assert!(woke.involves("t1"));
    assert!(!woke.involves("other"));
}

#[test]
fn phobia_acquired_round_trips_and_kind() {
    let p = MessagePayload::PhobiaAcquired {
        character: "t1".into(),
        trigger: "fire".into(),
        severity: "mild".into(),
        origin: "innate".into(),
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: MessagePayload = serde_json::from_str(&json).unwrap();
    assert_eq!(format!("{:?}", p), format!("{:?}", back));
    assert_eq!(p.kind(), MessageKind::PhobiaAcquired);
    assert!(p.involves("t1"));
    assert!(!p.involves("other"));
}

#[test]
fn phobia_triggered_round_trips_and_kind() {
    let p = MessagePayload::PhobiaTriggered {
        character: "t1".into(),
        trigger: "heights".into(),
        severity: "severe".into(),
        effect: PhobiaEffect::Freeze,
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: MessagePayload = serde_json::from_str(&json).unwrap();
    assert_eq!(format!("{:?}", p), format!("{:?}", back));
    assert_eq!(p.kind(), MessageKind::PhobiaTriggered);
    assert!(p.involves("t1"));
    assert!(!p.involves("other"));
}

#[test]
fn phobia_effect_serde_roundtrip() {
    for effect in [
        PhobiaEffect::Penalty,
        PhobiaEffect::Flee,
        PhobiaEffect::Freeze,
    ] {
        let s = serde_json::to_string(&effect).unwrap();
        let back: PhobiaEffect = serde_json::from_str(&s).unwrap();
        assert_eq!(effect, back);
    }
}
