use crate::threats::animals::Animal;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::str::FromStr;
use strum::EnumIter;

#[derive(Clone, Debug, Default, Deserialize, EnumIter, Eq, PartialEq, Serialize)]
pub enum CharacterStatus {
    #[default]
    Healthy,
    RecentlyDead,
    Dead,
    Mauled(Animal),
}

impl FromStr for CharacterStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.to_lowercase().as_str().contains("mauled") {
            if let Some(animal_name) = s.split_once(':').map(|x| x.1) {
                if let Ok(animal) = Animal::from_str(animal_name.trim()) {
                    return Ok(Self::Mauled(animal));
                }
            } else {
                return Err(());
            }
        }
        match s.to_lowercase().as_str() {
            "healthy" => Ok(CharacterStatus::Healthy),
            "recently dead" => Ok(CharacterStatus::RecentlyDead),
            "dead" => Ok(CharacterStatus::Dead),
            _ => Err(()),
        }
    }
}

impl Display for CharacterStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CharacterStatus::Healthy => write!(f, "healthy"),
            CharacterStatus::RecentlyDead => write!(f, "recently dead"),
            CharacterStatus::Dead => write!(f, "dead"),
            CharacterStatus::Mauled(animal) => write!(f, "mauled: {}", animal),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(CharacterStatus::Healthy, "healthy")]
    #[case(CharacterStatus::RecentlyDead, "recently dead")]
    #[case(CharacterStatus::Dead, "dead")]
    #[case(CharacterStatus::Mauled(Animal::Bear), "mauled: bear")]
    fn character_status_to_string(#[case] status: CharacterStatus, #[case] expected: &str) {
        assert_eq!(status.to_string(), expected.to_string());
    }

    #[rstest]
    #[case("healthy", CharacterStatus::Healthy)]
    #[case("recently dead", CharacterStatus::RecentlyDead)]
    #[case("dead", CharacterStatus::Dead)]
    #[case("mauled: bear", CharacterStatus::Mauled(Animal::Bear))]
    fn character_status_from_str(#[case] input: &str, #[case] expected: CharacterStatus) {
        assert_eq!(CharacterStatus::from_str(input).unwrap(), expected);
    }

    #[test]
    fn character_status_from_str_invalid() {
        assert!(CharacterStatus::from_str("burping").is_err());
    }

    #[test]
    fn character_status_from_str_invalid_animal() {
        assert!(CharacterStatus::from_str("mauled: velociraptor").is_err());
    }

    #[test]
    fn character_status_from_str_invalid_mauled_str() {
        // Missing the ':'
        assert!(CharacterStatus::from_str("mauled velociraptor").is_err());
    }
}
