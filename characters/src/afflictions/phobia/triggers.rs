//! Trigger detection for phobia afflictions.
//!
//! Each `PhobiaTrigger` variant has a hand-written `is_present` rule that
//! checks whether the stimulus is active in the current cycle context.
//! Detection is boolean — severity drives reaction strength, not detection.
//!
//! See spec §4.

use crate::Character;
use areas::AreaDetails;
use areas::events::AreaEvent;
use shared::afflictions::PhobiaTrigger;
use world::terrain::BaseTerrain;

/// Minimal context needed for phobia trigger detection.
///
/// This is the ad-hoc context used until `CycleContext` (xp4x) lands.
/// Built inline in the run-character-cycle path from area details and
/// game state.
#[derive(Clone, Debug)]
pub struct PhobiaContext<'a> {
    /// The area the character is currently in.
    pub area: &'a AreaDetails,
    /// Whether the current phase is night.
    pub is_night: bool,
    /// All living characters in the same area (excluding self).
    pub other_characters_in_area: &'a [Character],
    /// Game messages from the current cycle (for recent-event checks).
    pub cycle_messages: &'a [shared::messages::GameMessage],
    /// Current cycle number.
    pub cycle: u32,
}

/// Returns true if `trigger`'s stimulus is present in the current cycle
/// context for the given character.
///
/// Detection rules per spec §4:
/// - **Fire**: area has Wildfire event
/// - **Water**: area has Flood event or water-dominant terrain
/// - **Dark**: night phase and character has no light source
/// - **Blood**: recent combat kill in cycle messages
/// - **Heights**: Mountains or Highlands terrain
/// - **Enclosed**: UrbanRuins terrain (closest v1 analogue to cave/bunker)
/// - **Open**: Desert, Grasslands, or Clearing terrain
/// - **Animal**: not yet modeled in v1 (always false)
/// - **Character**: any other character in the same area
/// - **TraitGroup**: any other character in the same area with any trait
pub fn is_present(trigger: &PhobiaTrigger, character: &Character, ctx: &PhobiaContext<'_>) -> bool {
    match trigger {
        PhobiaTrigger::Fire => is_fire_present(ctx),
        PhobiaTrigger::Water => is_water_present(ctx),
        PhobiaTrigger::Dark => is_dark_present(character, ctx),
        PhobiaTrigger::Blood => is_blood_present(ctx),
        PhobiaTrigger::Heights => is_heights_present(ctx),
        PhobiaTrigger::Enclosed => is_enclosed_present(ctx),
        PhobiaTrigger::Open => is_open_present(ctx),
        PhobiaTrigger::Animal => is_animal_present(ctx),
        PhobiaTrigger::Character => is_character_present(character, ctx),
        PhobiaTrigger::TraitGroup => is_trait_group_present(character, ctx),
    }
}

fn is_fire_present(ctx: &PhobiaContext<'_>) -> bool {
    ctx.area
        .events
        .iter()
        .any(|e| matches!(e, AreaEvent::Wildfire))
}

fn is_water_present(ctx: &PhobiaContext<'_>) -> bool {
    ctx.area
        .events
        .iter()
        .any(|e| matches!(e, AreaEvent::Flood))
        || matches!(ctx.area.terrain.base, BaseTerrain::Wetlands)
}

fn is_dark_present(character: &Character, ctx: &PhobiaContext<'_>) -> bool {
    ctx.is_night && !character_has_light_source(character)
}

fn is_blood_present(ctx: &PhobiaContext<'_>) -> bool {
    use shared::messages::MessagePayload;
    ctx.cycle_messages
        .iter()
        .any(|msg| matches!(msg.payload, MessagePayload::CharacterKilled { .. }))
}

fn is_heights_present(ctx: &PhobiaContext<'_>) -> bool {
    matches!(
        ctx.area.terrain.base,
        BaseTerrain::Mountains | BaseTerrain::Highlands
    )
}

fn is_enclosed_present(ctx: &PhobiaContext<'_>) -> bool {
    matches!(ctx.area.terrain.base, BaseTerrain::UrbanRuins)
}

fn is_open_present(ctx: &PhobiaContext<'_>) -> bool {
    matches!(
        ctx.area.terrain.base,
        BaseTerrain::Desert | BaseTerrain::Grasslands | BaseTerrain::Clearing
    )
}

fn is_animal_present(_ctx: &PhobiaContext<'_>) -> bool {
    // Animal threats not yet modeled in v1.
    false
}

fn is_character_present(_character: &Character, ctx: &PhobiaContext<'_>) -> bool {
    !ctx.other_characters_in_area.is_empty()
}

fn is_trait_group_present(character: &Character, ctx: &PhobiaContext<'_>) -> bool {
    ctx.other_characters_in_area
        .iter()
        .any(|t| t.id != character.id && !t.traits.is_empty())
}

/// Checks if a character carries a light source item.
///
/// V1 approximation: checks for items with "torch", "lamp", or "light"
/// in the name. Proper item-type gating arrives with the consumable spec.
fn character_has_light_source(character: &Character) -> bool {
    character.items.iter().any(|item| {
        let name_lower = item.name.to_lowercase();
        name_lower.contains("torch") || name_lower.contains("lamp") || name_lower.contains("light")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use areas::Area;
    use shared::messages::{CharacterRef, GameMessage, MessagePayload, MessageSource, Phase};
    use world::items::Item;
    use world::terrain::TerrainType;

    fn make_area_details(terrain: BaseTerrain, events: Vec<AreaEvent>) -> AreaDetails {
        let mut area = AreaDetails::new(None, Area::Hub);
        area.terrain = TerrainType::new(terrain, vec![]).unwrap();
        area.events = events;
        area
    }

    fn make_context<'a>(
        area: &'a AreaDetails,
        is_night: bool,
        other_characters: &'a [Character],
        messages: &'a [GameMessage],
    ) -> PhobiaContext<'a> {
        PhobiaContext {
            area,
            is_night,
            other_characters_in_area: other_characters,
            cycle_messages: messages,
            cycle: 1,
        }
    }

    fn make_character(name: &str) -> Character {
        let mut t = Character::new(name.to_string(), None, None);
        t.traits.clear();
        t
    }

    #[test]
    fn fire_present_when_wildfire_event() {
        let area = make_area_details(BaseTerrain::Forest, vec![AreaEvent::Wildfire]);
        let character = make_character("Snaggletooth");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(is_present(&PhobiaTrigger::Fire, &character, &ctx));
    }

    #[test]
    fn fire_not_present_without_wildfire() {
        let area = make_area_details(BaseTerrain::Forest, vec![]);
        let character = make_character("Snaggletooth");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(!is_present(&PhobiaTrigger::Fire, &character, &ctx));
    }

    #[test]
    fn water_present_when_flood_event() {
        let area = make_area_details(BaseTerrain::Clearing, vec![AreaEvent::Flood]);
        let character = make_character("Grubworm");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(is_present(&PhobiaTrigger::Water, &character, &ctx));
    }

    #[test]
    fn water_present_on_wetlands_terrain() {
        let area = make_area_details(BaseTerrain::Wetlands, vec![]);
        let character = make_character("Grubworm");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(is_present(&PhobiaTrigger::Water, &character, &ctx));
    }

    #[test]
    fn dark_present_at_night_without_light() {
        let area = make_area_details(BaseTerrain::Forest, vec![]);
        let character = make_character("Nib");
        let ctx = make_context(&area, true, &[], &[]);
        assert!(is_present(&PhobiaTrigger::Dark, &character, &ctx));
    }

    #[test]
    fn dark_not_present_at_day() {
        let area = make_area_details(BaseTerrain::Forest, vec![]);
        let character = make_character("Nib");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(!is_present(&PhobiaTrigger::Dark, &character, &ctx));
    }

    #[test]
    fn dark_not_present_with_light_source() {
        let area = make_area_details(BaseTerrain::Forest, vec![]);
        let mut character = make_character("Nib");
        let torch = Item {
            name: "Torch".to_string(),
            ..Default::default()
        };
        character.items.push(torch);
        let ctx = make_context(&area, true, &[], &[]);
        assert!(!is_present(&PhobiaTrigger::Dark, &character, &ctx));
    }

    #[test]
    fn blood_present_when_kill_message() {
        let area = make_area_details(BaseTerrain::Clearing, vec![]);
        let character = make_character("Rendmaw");
        let messages = vec![GameMessage {
            identifier: "msg-1".to_string(),
            source: MessageSource::Game("game-1".to_string()),
            game_day: 1,
            phase: Phase::DAY,
            tick: 1,
            emit_index: 0,
            subject: String::new(),
            timestamp: chrono::Utc::now(),
            content: String::new(),
            payload: MessagePayload::CharacterKilled {
                victim: CharacterRef {
                    identifier: "victim".into(),
                    name: "Victim".to_string(),
                },
                killer: None,
                cause: shared::afflictions::DeathCause::Combat,
            },
        }];
        let ctx = make_context(&area, false, &[], &messages);
        assert!(is_present(&PhobiaTrigger::Blood, &character, &ctx));
    }

    #[test]
    fn blood_not_present_without_kill() {
        let area = make_area_details(BaseTerrain::Clearing, vec![]);
        let character = make_character("Rendmaw");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(!is_present(&PhobiaTrigger::Blood, &character, &ctx));
    }

    #[test]
    fn heights_present_on_mountains() {
        let area = make_area_details(BaseTerrain::Mountains, vec![]);
        let character = make_character("Thresh");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(is_present(&PhobiaTrigger::Heights, &character, &ctx));
    }

    #[test]
    fn heights_present_on_highlands() {
        let area = make_area_details(BaseTerrain::Highlands, vec![]);
        let character = make_character("Thresh");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(is_present(&PhobiaTrigger::Heights, &character, &ctx));
    }

    #[test]
    fn heights_not_present_on_clearing() {
        let area = make_area_details(BaseTerrain::Clearing, vec![]);
        let character = make_character("Thresh");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(!is_present(&PhobiaTrigger::Heights, &character, &ctx));
    }

    #[test]
    fn enclosed_present_on_urban_ruins() {
        let area = make_area_details(BaseTerrain::UrbanRuins, vec![]);
        let character = make_character("Snoutrot");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(is_present(&PhobiaTrigger::Enclosed, &character, &ctx));
    }

    #[test]
    fn enclosed_not_present_on_forest() {
        let area = make_area_details(BaseTerrain::Forest, vec![]);
        let character = make_character("Snoutrot");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(!is_present(&PhobiaTrigger::Enclosed, &character, &ctx));
    }

    #[test]
    fn open_present_on_desert() {
        let area = make_area_details(BaseTerrain::Desert, vec![]);
        let character = make_character("Gnawpaw");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(is_present(&PhobiaTrigger::Open, &character, &ctx));
    }

    #[test]
    fn open_present_on_grasslands() {
        let area = make_area_details(BaseTerrain::Grasslands, vec![]);
        let character = make_character("Gnawpaw");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(is_present(&PhobiaTrigger::Open, &character, &ctx));
    }

    #[test]
    fn open_present_on_clearing() {
        let area = make_area_details(BaseTerrain::Clearing, vec![]);
        let character = make_character("Gnawpaw");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(is_present(&PhobiaTrigger::Open, &character, &ctx));
    }

    #[test]
    fn open_not_present_on_forest() {
        let area = make_area_details(BaseTerrain::Forest, vec![]);
        let character = make_character("Gnawpaw");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(!is_present(&PhobiaTrigger::Open, &character, &ctx));
    }

    #[test]
    fn animal_trigger_always_false_v1() {
        let area = make_area_details(BaseTerrain::Forest, vec![]);
        let character = make_character("Scabface");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(!is_present(&PhobiaTrigger::Animal, &character, &ctx));
    }

    #[test]
    fn character_present_when_others_in_area() {
        let area = make_area_details(BaseTerrain::Clearing, vec![]);
        let character = make_character("Clove");
        let other = make_character("Rendmaw");
        let others = [other];
        let ctx = make_context(&area, false, &others, &[]);
        assert!(is_present(&PhobiaTrigger::Character, &character, &ctx));
    }

    #[test]
    fn character_not_present_when_alone() {
        let area = make_area_details(BaseTerrain::Clearing, vec![]);
        let character = make_character("Clove");
        let ctx = make_context(&area, false, &[], &[]);
        assert!(!is_present(&PhobiaTrigger::Character, &character, &ctx));
    }

    #[test]
    fn trait_group_present_when_others_have_traits() {
        use crate::traits::Trait;
        let area = make_area_details(BaseTerrain::Clearing, vec![]);
        let character = make_character("Clove");
        let mut other = make_character("Rendmaw");
        other.traits.push(Trait::Aggressive);
        let others = [other];
        let ctx = make_context(&area, false, &others, &[]);
        assert!(is_present(&PhobiaTrigger::TraitGroup, &character, &ctx));
    }

    #[test]
    fn trait_group_not_present_when_others_traitless() {
        let area = make_area_details(BaseTerrain::Clearing, vec![]);
        let character = make_character("Clove");
        let other = make_character("Rendmaw"); // no traits
        let others = [other];
        let ctx = make_context(&area, false, &others, &[]);
        assert!(!is_present(&PhobiaTrigger::TraitGroup, &character, &ctx));
    }
}
