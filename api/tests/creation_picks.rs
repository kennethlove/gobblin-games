//! Creation-couple picks (6bl §3): offer validation + apply.
//! Pure helpers — no database.

use api::AppError;
use api::characters::{ATTRIBUTE_NAMES, apply_creation_picks};
use characters::traits::Trait;

fn pinned_character() -> characters::Character {
    // Struct-update init (clippy: field_reassign_with_default), pinned
    // values to defeat roll randomness.
    characters::Character {
        traits: vec![Trait::Tough],
        attributes: characters::Attributes {
            luck: 10,
            strength: 10,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn happy_path_applies_trait_and_stat() {
    let mut c = pinned_character();
    apply_creation_picks(
        &mut c,
        "Aggressive,Defensive,Friendly",
        "Aggressive",
        "strength",
    )
    .expect("valid offer");
    assert!(c.traits.contains(&Trait::Aggressive));
    assert_eq!(c.attributes.strength, 11);
    assert_eq!(c.attributes.luck, 10, "no fallback on a clean apply");
    assert!(c
        .advancement_log
        .iter()
        .any(|e| matches!(e, characters::advancement::AdvancementEvent::TraitGained { trait_ } if *trait_ == Trait::Aggressive)));
}

#[test]
fn chosen_must_come_from_the_offer() {
    let mut c = pinned_character();
    let err = apply_creation_picks(&mut c, "Aggressive,Defensive,Friendly", "Paranoid", "luck")
        .expect_err("off-pool pick");
    assert!(matches!(err, AppError::ValidationError(_)));
}

#[test]
fn offer_must_be_three_distinct_real_traits() {
    let mut c = pinned_character();
    assert!(apply_creation_picks(&mut c, "Aggressive,Defensive", "Aggressive", "luck").is_err());
    assert!(
        apply_creation_picks(
            &mut c,
            "Aggressive,Aggressive,Defensive",
            "Aggressive",
            "luck"
        )
        .is_err()
    );
    assert!(
        apply_creation_picks(
            &mut c,
            "Aggressive,Defensive,NotATrait",
            "Aggressive",
            "luck"
        )
        .is_err()
    );
}

#[test]
fn roll_collision_falls_back_to_luck() {
    // Friendly vs Paranoid is a real conflict (traits::conflict_symmetry).
    // Keep best_stat OFF luck here so the fallback and the bump are
    // observable separately.
    let mut c = characters::Character {
        traits: vec![Trait::Friendly],
        attributes: characters::Attributes {
            luck: 10,
            strength: 10,
            ..Default::default()
        },
        ..Default::default()
    };
    apply_creation_picks(&mut c, "Friendly,Paranoid,Fragile", "Paranoid", "strength")
        .expect("offer is well-formed");
    assert!(!c.traits.contains(&Trait::Paranoid));
    assert_eq!(c.attributes.luck, 11, "conflict falls back to +1 luck");
    assert_eq!(c.attributes.strength, 11, "best-stat bump lands separately");
}

#[test]
fn unknown_best_stat_is_rejected() {
    let mut c = pinned_character();
    let err = apply_creation_picks(
        &mut c,
        "Aggressive,Defensive,Friendly",
        "Aggressive",
        "vibes",
    )
    .expect_err("unknown stat");
    assert!(matches!(err, AppError::ValidationError(_)));
    assert_eq!(ATTRIBUTE_NAMES.len(), 8);
}
