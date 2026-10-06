use crate::items::Item;
use crate::threats::animals::Animal;
use indefinite::indefinite;
use indefinite::indefinite_capitalized;
use shared::afflictions::TrapKind;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

// Collection on strings to be used as output for the game
pub enum GameOutput<'a> {
    GameDayStart(u32),
    GameDayEnd(u32),
    FirstDayStart,
    FeastDayStart,
    CharactersLeft(u32),
    GameNightStart(u32),
    GameNightEnd(u32),
    DailyDeathAnnouncement(u32),
    DeathAnnouncement(&'a str),
    NoOneWins,
    CharacterWins(&'a str),
    CharacterRest(&'a str),
    CharacterLongRest(&'a str),
    /// Character begins a multi-phase sleep (PR2c.1).
    CharacterSleeps(&'a str),
    /// Character wakes naturally after their planned sleep duration elapses.
    CharacterWakesRested(&'a str),
    /// Character is forcibly woken by an interruption (ambush, area event,
    /// alliance summons). PR2c.2.
    CharacterWakesInterrupted(&'a str),
    /// Character wakes due to a sleep incident (theft, animal, etc.). Description
    /// is pre-formatted by the incident system.
    CharacterWakesFromIncident(&'a str, &'a str),
    /// Non-waking incident flavor text (annoying only). Character stays asleep.
    CharacterSleepFlavor(&'a str, &'a str),
    CharacterHide(&'a str),
    CharacterTravel(&'a str, &'a str, &'a str),
    CharacterTakeItem(&'a str, &'a str),
    CharacterCannotUseItem(&'a str, &'a str),
    CharacterUseItem(&'a str, &'a Item),
    CharacterTravelTooTired(&'a str, &'a str),
    CharacterTravelExhausted(&'a str, &'a str),
    CharacterTravelAlreadyThere(&'a str, &'a str),
    CharacterTravelFollow(&'a str, &'a str),
    CharacterTravelStay(&'a str, &'a str),
    CharacterBleeds(&'a str),
    CharacterSick(&'a str),
    CharacterElectrocuted(&'a str),
    CharacterFrozen(&'a str),
    CharacterOverheated(&'a str),
    CharacterDehydrated(&'a str),
    CharacterStarving(&'a str),
    CharacterPoisoned(&'a str),
    CharacterDiedWhileTrapped {
        character_name: &'a str,
        kind: TrapKind,
    },
    CharacterMauled(&'a str, u32, &'a str, u32),
    CharacterBurned(&'a str),
    CharacterHorrified(&'a str, u32),
    CharacterSuffer(&'a str),
    CharacterSelfHarm(&'a str),
    CharacterSuicide(&'a str),
    CharacterAttackWin(&'a str, &'a str),
    CharacterAttackWinExtra(&'a str, &'a str),
    CharacterAttackWound(&'a str, &'a str),
    CharacterAttackLose(&'a str, &'a str),
    CharacterAttackLoseExtra(&'a str, &'a str),
    CharacterAttackMiss(&'a str, &'a str),
    CharacterAttackDied(&'a str, &'a str),
    CharacterAttackSuccessKill(&'a str, &'a str),
    CharacterAttackHidden(&'a str, &'a str),
    CharacterCriticalHit(&'a str, &'a str), // Natural 20 on attack
    CharacterCriticalFumble(&'a str),       // Natural 1 on attack
    CharacterPerfectBlock(&'a str, &'a str), // Natural 20 on defense
    CharacterDiesFromStatus(&'a str, &'a str),
    CharacterDiesFromAreaEvent(&'a str, &'a str), // Died in area
    CharacterDiesFromCharacterEvent(&'a str, &'a str),
    CharacterAlreadyDead(&'a str),
    CharacterDead(&'a str),
    WeaponBreak(&'a str, &'a str),
    WeaponWear(&'a str, &'a str),
    ShieldBreak(&'a str, &'a str),
    ShieldWear(&'a str, &'a str),
    /// Character name, weapon name, penalty (1..=4 absolute value).
    WeaponShattersMidSwing(&'a str, &'a str, u32),
    /// Character name, shield name, penalty (1..=4 absolute value).
    ShieldShattersMidBlock(&'a str, &'a str, u32),
    PatronGift(&'a str, &'a Item),
    AreaEvent(&'a str, &'a str),
    AreaClose(&'a str),
    AreaOpen(&'a str),
    TrappedInArea(&'a str, &'a str),
    DiedInArea(&'a str, &'a str),
    CharacterDeath(&'a str),
    CharacterTravelNoOptions(&'a str, &'a str),
    CharacterBetrayal(&'a str, &'a str),
    CharacterForcedBetrayal(&'a str, &'a str),
    NoOneToAttack(&'a str),
    AllAlone(&'a str),
    AllianceFormed(&'a str, &'a str, &'a str), // character_a, character_b, deciding factor
    BetrayalTriggered(&'a str, &'a str),       // betrayer, victim
    TrustShockBreak(&'a str),                  // shaken character
}

impl<'a> Display for GameOutput<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self {
            GameOutput::GameDayStart(day_number) => {
                write!(f, "=== ☀️ Day {} begins! ===", day_number)
            }
            GameOutput::GameDayEnd(day_number) => {
                write!(f, "=== ☀️ Day {} ends! ===", day_number)
            }
            GameOutput::FirstDayStart => {
                write!(f, "=== 🎉 The Gobblin' Games begin! 🎉 ===")
            }
            GameOutput::FeastDayStart => {
                write!(f, "=== 😋 Day 3: The Scramble ===")
            }
            GameOutput::CharactersLeft(character_count) => {
                write!(f, "=== 📌 Goblins still standing: {} ===", character_count)
            }
            GameOutput::GameNightStart(day_number) => {
                write!(f, "=== 🌙 Night {} begins ===", day_number)
            }
            GameOutput::GameNightEnd(day_number) => {
                write!(f, "=== 🌙 Night {} ends ===", day_number)
            }
            GameOutput::DailyDeathAnnouncement(death_count) => {
                write!(f, "=== 💀 Goblins downed: {} ===", death_count)
            }
            GameOutput::DeathAnnouncement(character) => {
                write!(f, "=== 🪦 {} is downed ===", character)
            }
            GameOutput::NoOneWins => {
                write!(f, "=== 🎭 No one wins! ===")
            }
            GameOutput::CharacterWins(character) => {
                write!(f, "=== 🏆 {} is the last goblin standing ===", character)
            }
            GameOutput::CharacterRest(character) => {
                write!(f, "😪 {} rests", character)
            }
            GameOutput::CharacterLongRest(character) => {
                write!(
                    f,
                    "💤 {} rests and recovers a little health and sanity",
                    character
                )
            }
            GameOutput::CharacterSleeps(character) => {
                write!(f, "😴 {} settles in to sleep", character)
            }
            GameOutput::CharacterWakesRested(character) => {
                write!(f, "🌅 {} wakes, well-rested", character)
            }
            GameOutput::CharacterWakesInterrupted(character) => {
                write!(f, "⚡ {} jolts awake", character)
            }
            GameOutput::CharacterWakesFromIncident(character, description) => {
                write!(f, "⚡ {} wakes abruptly — {}", character, description)
            }
            GameOutput::CharacterSleepFlavor(character, description) => {
                write!(f, "💤 {} stirs in their sleep — {}", character, description)
            }
            GameOutput::CharacterHide(character) => {
                write!(f, "🫥 {} tries to hide", character)
            }
            GameOutput::CharacterTravel(character, area_a, area_b) => {
                write!(f, "🚶 {} moves from {} to {}", character, area_a, area_b)
            }
            GameOutput::CharacterTakeItem(character, item) => {
                let object = indefinite(item);
                write!(f, "🔨 {} takes {}", character, object)
            }
            GameOutput::CharacterCannotUseItem(character, item) => {
                let object = indefinite(item);
                write!(f, "❌ {} cannot use {}", character, object)
            }
            GameOutput::CharacterUseItem(character, item) => {
                let object = indefinite(&item.name);
                write!(
                    f,
                    "💊 {} uses {}, gains {} {}",
                    character, object, item.effect, item.attribute
                )
            }
            GameOutput::CharacterTravelTooTired(character, area) => {
                write!(
                    f,
                    "😴 {} is too tired to move from {}, rests instead",
                    character, area
                )
            }
            GameOutput::CharacterTravelExhausted(character, area) => {
                write!(
                    f,
                    "🥵 {} is too exhausted to move from {}, lacks stamina",
                    character, area
                )
            }
            GameOutput::CharacterTravelAlreadyThere(character, area) => {
                write!(f, "🤔 {} is already in the {}, stays put", character, area)
            }
            GameOutput::CharacterTravelFollow(character, area) => {
                write!(f, "🫡 {} follows their team mate to {}", character, area)
            }
            GameOutput::CharacterTravelStay(character, area) => {
                write!(f, "🪑 {} stays in {}", character, area)
            }
            GameOutput::CharacterTravelNoOptions(character, area) => {
                write!(f, "📍 {} has nowhere to go, stays in {}", character, area)
            }
            GameOutput::CharacterBleeds(character) => {
                write!(f, "🩸 {} bleeds from their wounds.", character)
            }
            GameOutput::CharacterSick(character) => {
                write!(
                    f,
                    "🤒 {} contracts dysentery, loses strength and speed",
                    character
                )
            }
            GameOutput::CharacterElectrocuted(character) => {
                write!(f, "🌩️ {} is struck by lightning, loses health", character)
            }
            GameOutput::CharacterFrozen(character) => {
                write!(f, "🥶 {} suffers from hypothermia, loses speed.", character)
            }
            GameOutput::CharacterOverheated(character) => {
                write!(f, "🥵 {} suffers from heat stroke, loses speed.", character)
            }
            GameOutput::CharacterDehydrated(character) => {
                write!(f, "🌵 {} is severely dehydrated, loses strength", character)
            }
            GameOutput::CharacterStarving(character) => {
                write!(f, "🍴 {} is ravenously hungry, loses strength", character)
            }
            GameOutput::CharacterPoisoned(character) => {
                write!(f, "🧪 {} eats something poisonous, loses sanity", character)
            }
            GameOutput::CharacterMauled(character, count, animal, damage) => {
                let animal = Animal::from_str(animal).unwrap();
                write!(
                    f,
                    "🐾 {} is attacked by {} {}, takes {} damage!",
                    character,
                    count,
                    animal.plural(),
                    damage
                )
            }
            GameOutput::CharacterBurned(character) => {
                write!(f, "🔥 {} gets burned, loses health", character)
            }
            GameOutput::CharacterDiedWhileTrapped {
                character_name,
                kind,
            } => match kind {
                TrapKind::Drowning => {
                    write!(f, "💧 {} goes under, unconscious.", character_name)
                }
                TrapKind::Buried => {
                    write!(f, "🪦 {} is buried alive and passes out.", character_name)
                }
                TrapKind::Pitfall => {
                    write!(f, "🕳️ {} fell into a pitfall.", character_name)
                }
                TrapKind::SpikedPitfall => {
                    write!(f, "🕳️ {} impaled by a spiked pitfall.", character_name)
                }
                TrapKind::Snared => {
                    write!(f, "🪢 {} caught in a snare.", character_name)
                }
                TrapKind::Pinned => {
                    write!(f, "⛰️ {} pinned by debris.", character_name)
                }
            },
            GameOutput::CharacterHorrified(character, damage) => {
                write!(
                    f,
                    "😱 {} is horrified by the violence, loses {} sanity.",
                    character, damage
                )
            }
            GameOutput::CharacterSuffer(character) => {
                write!(f, "😭 {} suffers from loneliness and terror.", character)
            }
            GameOutput::CharacterSelfHarm(character) => {
                write!(f, "🤦 {} tries to attack themself!", character)
            }
            GameOutput::CharacterSuicide(character) => {
                write!(f, "🪒 {} attempts suicide.", character)
            }
            GameOutput::CharacterAttackWin(character, target) => {
                write!(f, "🔪 {} attacks {}, and wins!", character, target)
            }
            GameOutput::CharacterAttackWinExtra(character, target) => {
                write!(
                    f,
                    "🔪 {} attacks {}, and wins decisively!",
                    character, target
                )
            }
            GameOutput::CharacterAttackWound(character, target) => {
                write!(f, "🤕 {} wounds {}", character, target)
            }
            GameOutput::CharacterAttackLose(character, target) => {
                write!(f, "🤣 {} attacks {}, but loses!", character, target)
            }
            GameOutput::CharacterAttackLoseExtra(character, target) => {
                write!(
                    f,
                    "🤣 {} attacks {}, but loses decisively!",
                    character, target
                )
            }
            GameOutput::CharacterAttackMiss(character, target) => {
                write!(f, "😰 {} attacks {}, but misses!", character, target)
            }
            GameOutput::CharacterAttackDied(character, target) => {
                write!(f, "☠️ {} is downed by {}", character, target)
            }
            GameOutput::CharacterAttackSuccessKill(character, target) => {
                write!(f, "☠️ {} knocks out {}", character, target)
            }
            GameOutput::CharacterAttackHidden(character, target) => {
                write!(
                    f,
                    "🤔 {} can't attack {}, they're hidden",
                    character, target
                )
            }
            GameOutput::CharacterCriticalHit(character, target) => {
                write!(f, "💥 {} lands a CRITICAL HIT on {}!", character, target)
            }
            GameOutput::CharacterCriticalFumble(character) => {
                write!(
                    f,
                    "😵 {} fumbles their attack badly and hurts themself!",
                    character
                )
            }
            GameOutput::CharacterPerfectBlock(character, target) => {
                write!(
                    f,
                    "🛡️ {} perfectly blocks {}'s attack and counters!",
                    character, target
                )
            }
            GameOutput::CharacterDiesFromStatus(character, status) => {
                write!(f, "💀 {} is downed by {}", character, status)
            }
            GameOutput::CharacterDiesFromAreaEvent(character, area_event) => {
                write!(f, "🪦 {} is downed in the {}.", character, area_event)
            }
            GameOutput::CharacterDiesFromCharacterEvent(character, character_event) => {
                write!(f, "💀 {} is downed by {}", character, character_event)
            }
            GameOutput::CharacterAlreadyDead(character) => {
                write!(f, "‼️ {} is already downed!", character)
            }
            GameOutput::CharacterDead(character) => {
                write!(f, "❗️ {} is downed!", character)
            }
            GameOutput::WeaponBreak(character, weapon) => {
                write!(f, "🗡️ {} breaks their {}", character, weapon)
            }
            GameOutput::WeaponWear(character, weapon) => {
                write!(f, "🗡️ {}'s {} is showing signs of wear", character, weapon)
            }
            GameOutput::ShieldBreak(character, shield) => {
                write!(f, "🛡️ {} breaks their {}", character, shield)
            }
            GameOutput::ShieldWear(character, shield) => {
                write!(f, "🛡️ {}'s {} is showing signs of wear", character, shield)
            }
            GameOutput::WeaponShattersMidSwing(character, weapon, penalty) => {
                write!(
                    f,
                    "🗡️ {}'s {} shatters mid-swing! (-{} attack)",
                    character, weapon, penalty
                )
            }
            GameOutput::ShieldShattersMidBlock(character, shield, penalty) => {
                write!(
                    f,
                    "🛡️ {}'s {} shatters mid-block! (-{} defense)",
                    character, shield, penalty
                )
            }
            GameOutput::PatronGift(character, item) => {
                let object = indefinite(&item.name);
                write!(
                    f,
                    "🎁 {} receives {} (durability {}/{} {} +{})",
                    character,
                    object,
                    item.current_durability,
                    item.max_durability,
                    item.attribute,
                    item.effect
                )
            }
            GameOutput::AreaEvent(area_event, area) => {
                let area_name = area.replace("The ", "");
                let event = indefinite_capitalized(area_event);
                write!(f, "=== ⚠️ {} has occurred in the {} ===", event, area_name)
            }
            GameOutput::AreaClose(area) => {
                let area_name = area.replace("The ", "");
                write!(f, "=== 🔔 The {} is uninhabitable ===", area_name)
            }
            GameOutput::AreaOpen(area) => {
                let area_name = area.replace("The ", "");
                write!(f, "=== 🔔 The {} is habitable again ===", area_name)
            }
            GameOutput::TrappedInArea(character, area) => {
                let area_name = area.replace("The ", "");
                write!(f, "💥 {} is trapped in the {}.", character, area_name)
            }
            GameOutput::DiedInArea(character, area) => {
                let area_name = area.replace("The ", "");
                write!(f, "💥 {} is downed in the {}.", character, area_name)
            }
            GameOutput::CharacterDeath(character) => {
                write!(f, "⚰️ {} is downed.", character)
            }
            GameOutput::CharacterBetrayal(character, target) => {
                write!(f, "💔 {} betrays {}!", character, target)
            }
            GameOutput::CharacterForcedBetrayal(character, target) => {
                write!(f, "💔💔 {} is forced to betray {}!", character, target)
            }
            GameOutput::NoOneToAttack(character) => {
                write!(f, "🤷 {} has no one to attack!", character)
            }
            GameOutput::AllAlone(character) => {
                write!(f, "😢 {} is all alone!", character)
            }
            GameOutput::AllianceFormed(a, b, factor) => {
                write!(f, "{} and {} form an alliance ({}).", a, b, factor)
            }
            GameOutput::BetrayalTriggered(betrayer, victim) => {
                write!(
                    f,
                    "{} betrays {} — true to their treacherous nature.",
                    betrayer, victim
                )
            }
            GameOutput::TrustShockBreak(shaken) => {
                write!(
                    f,
                    "{} is shaken as their ally goes down and breaks the bond.",
                    shaken
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_alliance_formed_unchanged() {
        let s = GameOutput::AllianceFormed("Alice", "Bob", "trust").to_string();
        assert_eq!(s, "Alice and Bob form an alliance (trust).");
    }

    #[test]
    fn display_betrayal_triggered_unchanged() {
        let s = GameOutput::BetrayalTriggered("Rendmaw", "Gnawpaw").to_string();
        assert_eq!(
            s,
            "Rendmaw betrays Gnawpaw — true to their treacherous nature."
        );
    }

    #[test]
    fn display_trust_shock_break_unchanged() {
        let s = GameOutput::TrustShockBreak("Nib").to_string();
        assert_eq!(
            s,
            "Nib is shaken as their ally goes down and breaks the bond."
        );
    }

    #[test]
    fn display_character_wakes_from_incident() {
        let s = GameOutput::CharacterWakesFromIncident(
            "Snaggletooth",
            "Snaggletooth's sword is stolen while they sleep!",
        )
        .to_string();
        assert_eq!(
            s,
            "⚡ Snaggletooth wakes abruptly — Snaggletooth's sword is stolen while they sleep!"
        );
    }

    #[test]
    fn display_character_sleep_flavor() {
        let s =
            GameOutput::CharacterSleepFlavor("Grubworm", "a squirrel on their chest").to_string();
        assert_eq!(
            s,
            "💤 Grubworm stirs in their sleep — a squirrel on their chest"
        );
    }
}
