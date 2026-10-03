pub mod display;
pub mod types;
pub use types::GameEvent;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::{Attribute, Item, ItemRarity, ItemType};
    use crate::output::GameOutput;
    use crate::threats::animals::Animal;
    use uuid::Uuid;

    /// Stable UUIDs so test failures are easy to reason about.
    fn uid_a() -> Uuid {
        Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()
    }
    fn uid_b() -> Uuid {
        Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap()
    }

    fn sample_item() -> Item {
        Item {
            identifier: "33333333-3333-3333-3333-333333333333".to_string(),
            name: "elixir".to_string(),
            item_type: ItemType::Consumable,
            rarity: ItemRarity::Common,
            current_durability: 3,
            max_durability: 5,
            attribute: Attribute::Health,
            effect: 7,
        }
    }

    /// Single source of truth for the parity table. Each row pairs a
    /// constructed `GameEvent` with a `GameOutput` carrying the same data;
    /// the rendered strings must be byte-identical.
    fn parity_table() -> Vec<(GameEvent, GameOutput<'static>)> {
        let item = sample_item();
        // SAFETY: `Item` is owned, but `GameOutput` borrows. We leak the
        // sample item once for the test table so its references are 'static.
        // This is test-only code; the leak is bounded and intentional.
        let item_ref: &'static Item = Box::leak(Box::new(item.clone()));

        vec![
            (
                GameEvent::GameDayStart { day_number: 4 },
                GameOutput::GameDayStart(4),
            ),
            (
                GameEvent::GameDayEnd { day_number: 4 },
                GameOutput::GameDayEnd(4),
            ),
            (GameEvent::FirstDayStart, GameOutput::FirstDayStart),
            (GameEvent::FeastDayStart, GameOutput::FeastDayStart),
            (
                GameEvent::CharactersLeft {
                    character_count: 12,
                },
                GameOutput::CharactersLeft(12),
            ),
            (
                GameEvent::GameNightStart { day_number: 2 },
                GameOutput::GameNightStart(2),
            ),
            (
                GameEvent::GameNightEnd { day_number: 2 },
                GameOutput::GameNightEnd(2),
            ),
            (
                GameEvent::DailyDeathAnnouncement { death_count: 3 },
                GameOutput::DailyDeathAnnouncement(3),
            ),
            (
                GameEvent::DeathAnnouncement {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::DeathAnnouncement("Alice"),
            ),
            (GameEvent::NoOneWins, GameOutput::NoOneWins),
            (
                GameEvent::CharacterWins {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterWins("Alice"),
            ),
            (
                GameEvent::CharacterRest {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterRest("Alice"),
            ),
            (
                GameEvent::CharacterLongRest {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterLongRest("Alice"),
            ),
            (
                GameEvent::CharacterHide {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterHide("Alice"),
            ),
            (
                GameEvent::CharacterTravel {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    from_area: "Hub".into(),
                    to_area: "North".into(),
                },
                GameOutput::CharacterTravel("Alice", "Hub", "North"),
            ),
            (
                GameEvent::CharacterTakeItem {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    item_name: "elixir".into(),
                },
                GameOutput::CharacterTakeItem("Alice", "elixir"),
            ),
            (
                GameEvent::CharacterCannotUseItem {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    item_name: "elixir".into(),
                },
                GameOutput::CharacterCannotUseItem("Alice", "elixir"),
            ),
            (
                GameEvent::CharacterUseItem {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    item: item.clone(),
                },
                GameOutput::CharacterUseItem("Alice", item_ref),
            ),
            (
                GameEvent::CharacterTravelTooTired {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    area: "Forest".into(),
                },
                GameOutput::CharacterTravelTooTired("Alice", "Forest"),
            ),
            (
                GameEvent::CharacterTravelExhausted {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    area: "Forest".into(),
                },
                GameOutput::CharacterTravelExhausted("Alice", "Forest"),
            ),
            (
                GameEvent::CharacterTravelAlreadyThere {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    area: "Forest".into(),
                },
                GameOutput::CharacterTravelAlreadyThere("Alice", "Forest"),
            ),
            (
                GameEvent::CharacterTravelFollow {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    area: "Forest".into(),
                },
                GameOutput::CharacterTravelFollow("Alice", "Forest"),
            ),
            (
                GameEvent::CharacterTravelStay {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    area: "Forest".into(),
                },
                GameOutput::CharacterTravelStay("Alice", "Forest"),
            ),
            (
                GameEvent::CharacterTravelNoOptions {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    area: "Forest".into(),
                },
                GameOutput::CharacterTravelNoOptions("Alice", "Forest"),
            ),
            (
                GameEvent::CharacterBleeds {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterBleeds("Alice"),
            ),
            (
                GameEvent::CharacterSick {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterSick("Alice"),
            ),
            (
                GameEvent::CharacterElectrocuted {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterElectrocuted("Alice"),
            ),
            (
                GameEvent::CharacterFrozen {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterFrozen("Alice"),
            ),
            (
                GameEvent::CharacterOverheated {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterOverheated("Alice"),
            ),
            (
                GameEvent::CharacterDehydrated {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterDehydrated("Alice"),
            ),
            (
                GameEvent::CharacterStarving {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterStarving("Alice"),
            ),
            (
                GameEvent::CharacterPoisoned {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterPoisoned("Alice"),
            ),
            (
                GameEvent::CharacterMauled {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    animal_count: 3,
                    animal: Animal::Wolf,
                    damage: 12,
                },
                GameOutput::CharacterMauled("Alice", 3, "Wolf", 12),
            ),
            (
                GameEvent::CharacterBurned {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterBurned("Alice"),
            ),
            (
                GameEvent::CharacterHorrified {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    sanity_damage: 5,
                },
                GameOutput::CharacterHorrified("Alice", 5),
            ),
            (
                GameEvent::CharacterSuffer {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterSuffer("Alice"),
            ),
            (
                GameEvent::CharacterSelfHarm {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterSelfHarm("Alice"),
            ),
            (
                GameEvent::CharacterSuicide {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterSuicide("Alice"),
            ),
            (
                GameEvent::CharacterAttackWin {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterAttackWin("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterAttackWinExtra {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterAttackWinExtra("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterAttackWound {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterAttackWound("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterAttackLose {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterAttackLose("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterAttackLoseExtra {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterAttackLoseExtra("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterAttackMiss {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterAttackMiss("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterAttackDied {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterAttackDied("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterAttackSuccessKill {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterAttackSuccessKill("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterAttackHidden {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterAttackHidden("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterCriticalHit {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterCriticalHit("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterCriticalFumble {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterCriticalFumble("Alice"),
            ),
            (
                GameEvent::CharacterPerfectBlock {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterPerfectBlock("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterDiesFromStatus {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    status: "poison".into(),
                },
                GameOutput::CharacterDiesFromStatus("Alice", "poison"),
            ),
            (
                GameEvent::CharacterDiesFromAreaEvent {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    area_event: "wildfire".into(),
                },
                GameOutput::CharacterDiesFromAreaEvent("Alice", "wildfire"),
            ),
            (
                GameEvent::CharacterDiesFromCharacterEvent {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    character_event: "Bob".into(),
                },
                GameOutput::CharacterDiesFromCharacterEvent("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterAlreadyDead {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterAlreadyDead("Alice"),
            ),
            (
                GameEvent::CharacterDead {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterDead("Alice"),
            ),
            (
                GameEvent::CharacterDeath {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::CharacterDeath("Alice"),
            ),
            (
                GameEvent::WeaponBreak {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    weapon_name: "spear".into(),
                },
                GameOutput::WeaponBreak("Alice", "spear"),
            ),
            (
                GameEvent::WeaponWear {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    weapon_name: "spear".into(),
                },
                GameOutput::WeaponWear("Alice", "spear"),
            ),
            (
                GameEvent::ShieldBreak {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    shield_name: "buckler".into(),
                },
                GameOutput::ShieldBreak("Alice", "buckler"),
            ),
            (
                GameEvent::ShieldWear {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    shield_name: "buckler".into(),
                },
                GameOutput::ShieldWear("Alice", "buckler"),
            ),
            (
                GameEvent::PatronGift {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    item: item.clone(),
                },
                GameOutput::PatronGift("Alice", item_ref),
            ),
            (
                GameEvent::AreaEvent {
                    area_event: "earthquake".into(),
                    area_name: "The Forest".into(),
                },
                GameOutput::AreaEvent("earthquake", "The Forest"),
            ),
            (
                GameEvent::AreaClose {
                    area_name: "The Forest".into(),
                },
                GameOutput::AreaClose("The Forest"),
            ),
            (
                GameEvent::AreaOpen {
                    area_name: "The Forest".into(),
                },
                GameOutput::AreaOpen("The Forest"),
            ),
            (
                GameEvent::TrappedInArea {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    area_name: "The Forest".into(),
                },
                GameOutput::TrappedInArea("Alice", "The Forest"),
            ),
            (
                GameEvent::DiedInArea {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    area_name: "The Forest".into(),
                },
                GameOutput::DiedInArea("Alice", "The Forest"),
            ),
            (
                GameEvent::CharacterBetrayal {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterBetrayal("Alice", "Bob"),
            ),
            (
                GameEvent::CharacterForcedBetrayal {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                    target_id: uid_b(),
                    target_name: "Bob".into(),
                },
                GameOutput::CharacterForcedBetrayal("Alice", "Bob"),
            ),
            (
                GameEvent::NoOneToAttack {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::NoOneToAttack("Alice"),
            ),
            (
                GameEvent::AllAlone {
                    character_id: uid_a(),
                    character_name: "Alice".into(),
                },
                GameOutput::AllAlone("Alice"),
            ),
            (
                GameEvent::AllianceFormed {
                    character_a_id: uid_a(),
                    character_a_name: "Alice".into(),
                    character_b_id: uid_b(),
                    character_b_name: "Bob".into(),
                    factor: "trust".into(),
                },
                GameOutput::AllianceFormed("Alice", "Bob", "trust"),
            ),
            (
                GameEvent::BetrayalTriggered {
                    betrayer_id: uid_a(),
                    betrayer_name: "Rendmaw".into(),
                    victim_id: uid_b(),
                    victim_name: "Gnawpaw".into(),
                },
                GameOutput::BetrayalTriggered("Rendmaw", "Gnawpaw"),
            ),
            (
                GameEvent::TrustShockBreak {
                    character_id: uid_a(),
                    character_name: "Nib".into(),
                },
                GameOutput::TrustShockBreak("Nib"),
            ),
        ]
    }

    #[test]
    fn parity_table_covers_every_variant() {
        // Bumps any time a variant is added without a parity row.
        // 73 = current count of GameEvent variants in types.rs.
        assert_eq!(parity_table().len(), 73);
    }

    #[test]
    fn display_matches_game_output_for_every_variant() {
        for (event, output) in parity_table() {
            assert_eq!(
                event.to_string(),
                output.to_string(),
                "Display mismatch for {:?}",
                event
            );
        }
    }

    // ---------- Serde roundtrip coverage ----------
    // One assertion per data shape: unit, single-field, multi-field,
    // optional-field-via-Item.

    fn roundtrip(event: &GameEvent) {
        let json = serde_json::to_string(event).expect("serialize");
        let parsed: GameEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*event, parsed, "roundtrip mismatch: {}", json);
    }

    #[test]
    fn serde_roundtrip_unit_variant() {
        roundtrip(&GameEvent::FirstDayStart);
        roundtrip(&GameEvent::FeastDayStart);
        roundtrip(&GameEvent::NoOneWins);
    }

    #[test]
    fn serde_roundtrip_single_primitive_field() {
        roundtrip(&GameEvent::GameDayStart { day_number: 7 });
        roundtrip(&GameEvent::CharactersLeft {
            character_count: 11,
        });
    }

    #[test]
    fn serde_roundtrip_multi_field_with_uuid() {
        roundtrip(&GameEvent::AllianceFormed {
            character_a_id: uid_a(),
            character_a_name: "Alice".into(),
            character_b_id: uid_b(),
            character_b_name: "Bob".into(),
            factor: "shared clan".into(),
        });
    }

    #[test]
    fn serde_roundtrip_with_nested_item() {
        roundtrip(&GameEvent::PatronGift {
            character_id: uid_a(),
            character_name: "Alice".into(),
            item: sample_item(),
        });
        roundtrip(&GameEvent::CharacterUseItem {
            character_id: uid_a(),
            character_name: "Alice".into(),
            item: sample_item(),
        });
    }

    #[test]
    fn serde_roundtrip_with_animal_enum() {
        roundtrip(&GameEvent::CharacterMauled {
            character_id: uid_a(),
            character_name: "Alice".into(),
            animal_count: 4,
            animal: Animal::BlightWasp,
            damage: 9,
        });
    }
}
