use super::GameEvent;
use std::fmt::{Display, Formatter};

use indefinite::{indefinite, indefinite_capitalized};

impl Display for GameEvent {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            GameEvent::GameDayStart { day_number } => {
                write!(f, "=== ☀️ Day {} begins! ===", day_number)
            }
            GameEvent::GameDayEnd { day_number } => {
                write!(f, "=== ☀️ Day {} ends! ===", day_number)
            }
            GameEvent::FirstDayStart => {
                write!(f, "=== 🎉 The Gobblin' Games begin! 🎉 ===")
            }
            GameEvent::FeastDayStart => {
                write!(f, "=== 😋 Day 3: The Scramble ===")
            }
            GameEvent::CharactersLeft { character_count } => {
                write!(f, "=== 📌 Goblins alive: {} ===", character_count)
            }
            GameEvent::GameNightStart { day_number } => {
                write!(f, "=== 🌙 Night {} begins ===", day_number)
            }
            GameEvent::GameNightEnd { day_number } => {
                write!(f, "=== 🌙 Night {} ends ===", day_number)
            }
            GameEvent::DailyDeathAnnouncement { death_count } => {
                write!(f, "=== 💀 Goblins dead: {} ===", death_count)
            }
            GameEvent::DeathAnnouncement { character_name, .. } => {
                write!(f, "=== 🪦 {} has died ===", character_name)
            }
            GameEvent::NoOneWins => {
                write!(f, "=== 🎭 No one wins! ===")
            }
            GameEvent::CharacterWins { character_name, .. } => {
                write!(
                    f,
                    "=== 🏆 {} is the last goblin standing ===",
                    character_name
                )
            }
            GameEvent::CharacterRest { character_name, .. } => {
                write!(f, "😪 {} rests", character_name)
            }
            GameEvent::CharacterLongRest { character_name, .. } => {
                write!(
                    f,
                    "💤 {} rests and recovers a little health and sanity",
                    character_name
                )
            }
            GameEvent::CharacterHide { character_name, .. } => {
                write!(f, "🫥 {} tries to hide", character_name)
            }
            GameEvent::CharacterTravel {
                character_name,
                from_area,
                to_area,
                ..
            } => {
                write!(
                    f,
                    "🚶 {} moves from {} to {}",
                    character_name, from_area, to_area
                )
            }
            GameEvent::CharacterTakeItem {
                character_name,
                item_name,
                ..
            } => {
                let object = indefinite(item_name);
                write!(f, "🔨 {} takes {}", character_name, object)
            }
            GameEvent::CharacterCannotUseItem {
                character_name,
                item_name,
                ..
            } => {
                let object = indefinite(item_name);
                write!(f, "❌ {} cannot use {}", character_name, object)
            }
            GameEvent::CharacterUseItem {
                character_name,
                item,
                ..
            } => {
                let object = indefinite(&item.name);
                write!(
                    f,
                    "💊 {} uses {}, gains {} {}",
                    character_name, object, item.effect, item.attribute
                )
            }
            GameEvent::CharacterTravelTooTired {
                character_name,
                area,
                ..
            } => {
                write!(
                    f,
                    "😴 {} is too tired to move from {}, rests instead",
                    character_name, area
                )
            }
            GameEvent::CharacterTravelExhausted {
                character_name,
                area,
                ..
            } => {
                write!(
                    f,
                    "🥵 {} is too exhausted to move from {}, lacks stamina",
                    character_name, area
                )
            }
            GameEvent::CharacterTravelAlreadyThere {
                character_name,
                area,
                ..
            } => {
                write!(
                    f,
                    "🤔 {} is already in the {}, stays put",
                    character_name, area
                )
            }
            GameEvent::CharacterTravelFollow {
                character_name,
                area,
                ..
            } => {
                write!(
                    f,
                    "🫡 {} follows their clan mate to {}",
                    character_name, area
                )
            }
            GameEvent::CharacterTravelStay {
                character_name,
                area,
                ..
            } => {
                write!(f, "🪑 {} stays in {}", character_name, area)
            }
            GameEvent::CharacterTravelNoOptions {
                character_name,
                area,
                ..
            } => {
                write!(
                    f,
                    "📍 {} has nowhere to go, stays in {}",
                    character_name, area
                )
            }
            GameEvent::CharacterBleeds { character_name, .. } => {
                write!(f, "🩸 {} bleeds from their wounds.", character_name)
            }
            GameEvent::CharacterSick { character_name, .. } => {
                write!(
                    f,
                    "🤒 {} contracts dysentery, loses strength and speed",
                    character_name
                )
            }
            GameEvent::CharacterElectrocuted { character_name, .. } => {
                write!(
                    f,
                    "🌩️ {} is struck by lightning, loses health",
                    character_name
                )
            }
            GameEvent::CharacterFrozen { character_name, .. } => {
                write!(
                    f,
                    "🥶 {} suffers from hypothermia, loses speed.",
                    character_name
                )
            }
            GameEvent::CharacterOverheated { character_name, .. } => {
                write!(
                    f,
                    "🥵 {} suffers from heat stroke, loses speed.",
                    character_name
                )
            }
            GameEvent::CharacterDehydrated { character_name, .. } => {
                write!(
                    f,
                    "🌵 {} is severely dehydrated, loses strength",
                    character_name
                )
            }
            GameEvent::CharacterStarving { character_name, .. } => {
                write!(
                    f,
                    "🍴 {} is ravenously hungry, loses strength",
                    character_name
                )
            }
            GameEvent::CharacterPoisoned { character_name, .. } => {
                write!(
                    f,
                    "🧪 {} eats something poisonous, loses sanity",
                    character_name
                )
            }
            GameEvent::CharacterMauled {
                character_name,
                animal_count,
                animal,
                damage,
                ..
            } => {
                write!(
                    f,
                    "🐾 {} is attacked by {} {}, takes {} damage!",
                    character_name,
                    animal_count,
                    animal.plural(),
                    damage
                )
            }
            GameEvent::CharacterBurned { character_name, .. } => {
                write!(f, "🔥 {} gets burned, loses health", character_name)
            }
            GameEvent::CharacterHorrified {
                character_name,
                sanity_damage,
                ..
            } => {
                write!(
                    f,
                    "😱 {} is horrified by the violence, loses {} sanity.",
                    character_name, sanity_damage
                )
            }
            GameEvent::CharacterSuffer { character_name, .. } => {
                write!(
                    f,
                    "😭 {} suffers from loneliness and terror.",
                    character_name
                )
            }
            GameEvent::CharacterSelfHarm { character_name, .. } => {
                write!(f, "🤦 {} tries to attack themself!", character_name)
            }
            GameEvent::CharacterSuicide { character_name, .. } => {
                write!(f, "🪒 {} attempts suicide.", character_name)
            }
            GameEvent::CharacterAttackWin {
                character_name,
                target_name,
                ..
            } => {
                write!(
                    f,
                    "🔪 {} attacks {}, and wins!",
                    character_name, target_name
                )
            }
            GameEvent::CharacterAttackWinExtra {
                character_name,
                target_name,
                ..
            } => {
                write!(
                    f,
                    "🔪 {} attacks {}, and wins decisively!",
                    character_name, target_name
                )
            }
            GameEvent::CharacterAttackWound {
                character_name,
                target_name,
                ..
            } => {
                write!(f, "🤕 {} wounds {}", character_name, target_name)
            }
            GameEvent::CharacterAttackLose {
                character_name,
                target_name,
                ..
            } => {
                write!(
                    f,
                    "🤣 {} attacks {}, but loses!",
                    character_name, target_name
                )
            }
            GameEvent::CharacterAttackLoseExtra {
                character_name,
                target_name,
                ..
            } => {
                write!(
                    f,
                    "🤣 {} attacks {}, but loses decisively!",
                    character_name, target_name
                )
            }
            GameEvent::CharacterAttackMiss {
                character_name,
                target_name,
                ..
            } => {
                write!(
                    f,
                    "😰 {} attacks {}, but misses!",
                    character_name, target_name
                )
            }
            GameEvent::CharacterAttackDied {
                character_name,
                target_name,
                ..
            } => {
                write!(f, "☠️ {} is killed by {}", character_name, target_name)
            }
            GameEvent::CharacterAttackSuccessKill {
                character_name,
                target_name,
                ..
            } => {
                write!(
                    f,
                    "☠️ {} successfully kills {}",
                    character_name, target_name
                )
            }
            GameEvent::CharacterAttackHidden {
                character_name,
                target_name,
                ..
            } => {
                write!(
                    f,
                    "🤔 {} can't attack {}, they're hidden",
                    character_name, target_name
                )
            }
            GameEvent::CharacterCriticalHit {
                character_name,
                target_name,
                ..
            } => {
                write!(
                    f,
                    "💥 {} lands a CRITICAL HIT on {}!",
                    character_name, target_name
                )
            }
            GameEvent::CharacterCriticalFumble { character_name, .. } => {
                write!(
                    f,
                    "😵 {} fumbles their attack badly and hurts themself!",
                    character_name
                )
            }
            GameEvent::CharacterPerfectBlock {
                character_name,
                target_name,
                ..
            } => {
                write!(
                    f,
                    "🛡️ {} perfectly blocks {}'s attack and counters!",
                    character_name, target_name
                )
            }
            GameEvent::CharacterDiesFromStatus {
                character_name,
                status,
                ..
            } => {
                write!(f, "💀 {} dies from {}", character_name, status)
            }
            GameEvent::CharacterDiesFromAreaEvent {
                character_name,
                area_event,
                ..
            } => {
                write!(f, "🪦 {} died in the {}.", character_name, area_event)
            }
            GameEvent::CharacterDiesFromCharacterEvent {
                character_name,
                character_event,
                ..
            } => {
                write!(f, "💀 {} dies by {}", character_name, character_event)
            }
            GameEvent::CharacterAlreadyDead { character_name, .. } => {
                write!(f, "‼️ {} is already dead!", character_name)
            }
            GameEvent::CharacterDead { character_name, .. } => {
                write!(f, "❗️ {} is dead!", character_name)
            }
            GameEvent::WeaponBreak {
                character_name,
                weapon_name,
                ..
            } => {
                write!(f, "🗡️ {} breaks their {}", character_name, weapon_name)
            }
            GameEvent::WeaponWear {
                character_name,
                weapon_name,
                ..
            } => {
                write!(
                    f,
                    "🗡️ {}'s {} is showing signs of wear",
                    character_name, weapon_name
                )
            }
            GameEvent::ShieldBreak {
                character_name,
                shield_name,
                ..
            } => {
                write!(f, "🛡️ {} breaks their {}", character_name, shield_name)
            }
            GameEvent::ShieldWear {
                character_name,
                shield_name,
                ..
            } => {
                write!(
                    f,
                    "🛡️ {}'s {} is showing signs of wear",
                    character_name, shield_name
                )
            }
            GameEvent::PatronGift {
                character_name,
                item,
                ..
            } => {
                let object = indefinite(&item.name);
                write!(
                    f,
                    "🎁 {} receives {} (durability {}/{} {} +{})",
                    character_name,
                    object,
                    item.current_durability,
                    item.max_durability,
                    item.attribute,
                    item.effect
                )
            }
            GameEvent::AreaEvent {
                area_event,
                area_name,
            } => {
                let area_short = area_name.replace("The ", "");
                let event = indefinite_capitalized(area_event);
                write!(f, "=== ⚠️ {} has occurred in the {} ===", event, area_short)
            }
            GameEvent::AreaClose { area_name } => {
                let area_short = area_name.replace("The ", "");
                write!(f, "=== 🔔 The {} is uninhabitable ===", area_short)
            }
            GameEvent::AreaOpen { area_name } => {
                let area_short = area_name.replace("The ", "");
                write!(f, "=== 🔔 The {} is habitable again ===", area_short)
            }
            GameEvent::TrappedInArea {
                character_name,
                area_name,
                ..
            } => {
                let area_short = area_name.replace("The ", "");
                write!(f, "💥 {} is trapped in the {}.", character_name, area_short)
            }
            GameEvent::DiedInArea {
                character_name,
                area_name,
                ..
            } => {
                let area_short = area_name.replace("The ", "");
                write!(f, "💥 {} died in the {}.", character_name, area_short)
            }
            GameEvent::CharacterDeath { character_name, .. } => {
                write!(f, "⚰️ {} has died.", character_name)
            }
            GameEvent::CharacterBetrayal {
                character_name,
                target_name,
                ..
            } => {
                write!(f, "💔 {} betrays {}!", character_name, target_name)
            }
            GameEvent::CharacterForcedBetrayal {
                character_name,
                target_name,
                ..
            } => {
                write!(
                    f,
                    "💔💔 {} is forced to betray {}!",
                    character_name, target_name
                )
            }
            GameEvent::NoOneToAttack { character_name, .. } => {
                write!(f, "🤷 {} has no one to attack!", character_name)
            }
            GameEvent::AllAlone { character_name, .. } => {
                write!(f, "😢 {} is all alone!", character_name)
            }
            GameEvent::AllianceFormed {
                character_a_name,
                character_b_name,
                factor,
                ..
            } => {
                write!(
                    f,
                    "{} and {} form an alliance ({}).",
                    character_a_name, character_b_name, factor
                )
            }
            GameEvent::BetrayalTriggered {
                betrayer_name,
                victim_name,
                ..
            } => {
                write!(
                    f,
                    "{} betrays {} — true to their treacherous nature.",
                    betrayer_name, victim_name
                )
            }
            GameEvent::TrustShockBreak { character_name, .. } => {
                write!(
                    f,
                    "{} is shaken by their ally's death and breaks the bond.",
                    character_name
                )
            }
            GameEvent::CombatSwing { beat } => {
                use crate::characters::combat_beat::CombatBeatExt;
                let lines = beat.to_log_lines();
                write!(f, "{}", lines.join(" "))
            }
        }
    }
}
