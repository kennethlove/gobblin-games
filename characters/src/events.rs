use rand::RngExt;
use rand::prelude::SmallRng;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::str::FromStr;
use world::threats::animals::Animal;

#[derive(Clone, Debug, PartialOrd, PartialEq, Serialize, Deserialize)]
pub enum CharacterEvent {
    AnimalAttack(Animal),
    Dysentery,
    LightningStrike,
    Hypothermia,
    HeatStroke,
    Dehydration,
    Starvation,
    Poisoning,
    BrokenBone,
    Infection,
    Drowning,
    Burn,
}

impl FromStr for CharacterEvent {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.to_lowercase().contains("animal attack") {
            return if let Some(animal_name) = s.split_once(':').map(|x| x.1) {
                if let Ok(animal) = Animal::from_str(animal_name.trim()) {
                    Ok(CharacterEvent::AnimalAttack(animal))
                } else {
                    Err(())
                }
            } else {
                Err(())
            };
        }

        match s {
            "dysentery" => Ok(CharacterEvent::Dysentery),
            "lightning strike" => Ok(CharacterEvent::LightningStrike),
            "hypothermia" => Ok(CharacterEvent::Hypothermia),
            "heat stroke" => Ok(CharacterEvent::HeatStroke),
            "dehydration" => Ok(CharacterEvent::Dehydration),
            "starvation" => Ok(CharacterEvent::Starvation),
            "poisoning" => Ok(CharacterEvent::Poisoning),
            "broken bone" => Ok(CharacterEvent::BrokenBone),
            "infection" => Ok(CharacterEvent::Infection),
            "drowning" => Ok(CharacterEvent::Drowning),
            "burn" => Ok(CharacterEvent::Burn),
            _ => Err(()),
        }
    }
}

impl Display for CharacterEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CharacterEvent::AnimalAttack(animal) => write!(f, "animal attack: {}", animal),
            CharacterEvent::Dysentery => write!(f, "dysentery"),
            CharacterEvent::LightningStrike => write!(f, "lightning strike"),
            CharacterEvent::Hypothermia => write!(f, "hypothermia"),
            CharacterEvent::HeatStroke => write!(f, "heat stroke"),
            CharacterEvent::Dehydration => write!(f, "dehydration"),
            CharacterEvent::Starvation => write!(f, "starvation"),
            CharacterEvent::Poisoning => write!(f, "poisoning"),
            CharacterEvent::BrokenBone => write!(f, "broken bone"),
            CharacterEvent::Infection => write!(f, "infection"),
            CharacterEvent::Drowning => write!(f, "drowning"),
            CharacterEvent::Burn => write!(f, "burn"),
        }
    }
}

impl CharacterEvent {
    pub fn random() -> CharacterEvent {
        let mut rng = SmallRng::from_rng(&mut rand::rng());
        let animal = Animal::random();
        let events = [
            CharacterEvent::AnimalAttack(animal),
            CharacterEvent::Dysentery,
            CharacterEvent::LightningStrike,
            CharacterEvent::Hypothermia,
            CharacterEvent::HeatStroke,
            CharacterEvent::Dehydration,
            CharacterEvent::Starvation,
            CharacterEvent::Poisoning,
            CharacterEvent::BrokenBone,
            CharacterEvent::Infection,
            CharacterEvent::Drowning,
            CharacterEvent::Burn,
        ];
        let index = rng.random_range(0..events.len());
        events[index].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(CharacterEvent::AnimalAttack(Animal::Wolf), "animal attack: wolf")]
    #[case(CharacterEvent::Dysentery, "dysentery")]
    #[case(CharacterEvent::LightningStrike, "lightning strike")]
    #[case(CharacterEvent::Hypothermia, "hypothermia")]
    #[case(CharacterEvent::HeatStroke, "heat stroke")]
    #[case(CharacterEvent::Dehydration, "dehydration")]
    #[case(CharacterEvent::Starvation, "starvation")]
    #[case(CharacterEvent::Poisoning, "poisoning")]
    #[case(CharacterEvent::BrokenBone, "broken bone")]
    #[case(CharacterEvent::Infection, "infection")]
    #[case(CharacterEvent::Drowning, "drowning")]
    #[case(CharacterEvent::Burn, "burn")]
    fn character_event_to_string(#[case] event: CharacterEvent, #[case] expected: &str) {
        assert_eq!(event.to_string(), expected);
    }

    #[rstest]
    #[case("animal attack: wolf", CharacterEvent::AnimalAttack(Animal::Wolf))]
    #[case("dysentery", CharacterEvent::Dysentery)]
    #[case("lightning strike", CharacterEvent::LightningStrike)]
    #[case("hypothermia", CharacterEvent::Hypothermia)]
    #[case("heat stroke", CharacterEvent::HeatStroke)]
    #[case("dehydration", CharacterEvent::Dehydration)]
    #[case("starvation", CharacterEvent::Starvation)]
    #[case("poisoning", CharacterEvent::Poisoning)]
    #[case("broken bone", CharacterEvent::BrokenBone)]
    #[case("infection", CharacterEvent::Infection)]
    #[case("drowning", CharacterEvent::Drowning)]
    #[case("burn", CharacterEvent::Burn)]
    fn character_event_from_str(#[case] input: &str, #[case] event: CharacterEvent) {
        assert_eq!(CharacterEvent::from_str(input).unwrap(), event);
    }

    #[test]
    fn random_character_event() {
        let te = CharacterEvent::random();
        assert_eq!(CharacterEvent::from_str(&te.to_string()).unwrap(), te);
    }

    #[test]
    fn character_event_from_str_invalid() {
        assert!(CharacterEvent::from_str("resurrection").is_err());
    }

    #[test]
    fn character_event_from_str_invalid_animal() {
        assert!(CharacterEvent::from_str("animal attack: dragon").is_err());
    }

    #[test]
    fn character_event_from_str_missing_separator() {
        assert!(CharacterEvent::from_str("animal attack coyote").is_err());
    }
}
