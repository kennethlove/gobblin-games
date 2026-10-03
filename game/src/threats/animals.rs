use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::str::FromStr;
use strum::{EnumIter, IntoEnumIterator};

#[derive(
    Clone, Debug, Default, Deserialize, EnumIter, Eq, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum Animal {
    #[default]
    Squirrel,
    Bear,
    Wolf,
    Panther,
    Boar,
    Snake,
    Monkey,
    Baboon,
    Hyena,
    Manticore,
    Warg,
    Elephant,
    Rhino,
    Hippo,
    BlightWasp,
}

impl Animal {
    pub fn plural(&self) -> String {
        match self {
            Animal::Wolf => "wolves".to_string(),
            _ => {
                format!("{}s", self)
            }
        }
    }

    pub fn random() -> Animal {
        let mut rng = SmallRng::from_rng(&mut rand::rng());
        Animal::iter().choose(&mut rng).unwrap()
    }

    pub fn damage(&self) -> u32 {
        match self {
            Animal::Squirrel => 1,
            Animal::Bear => 10,
            Animal::Wolf => 5,
            Animal::Panther => 5,
            Animal::Boar => 3,
            Animal::Snake => 2,
            Animal::Monkey => 3,
            Animal::Baboon => 5,
            Animal::Hyena => 5,
            Animal::Manticore => 10,
            Animal::Warg => 10,
            Animal::Elephant => 10,
            Animal::Rhino => 10,
            Animal::Hippo => 20,
            Animal::BlightWasp => 5,
        }
    }
}

impl FromStr for Animal {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "squirrel" => Ok(Animal::Squirrel),
            "bear" => Ok(Animal::Bear),
            "wolf" => Ok(Animal::Wolf),
            "panther" => Ok(Animal::Panther),
            "boar" => Ok(Animal::Boar),
            "snake" => Ok(Animal::Snake),
            "monkey" => Ok(Animal::Monkey),
            "baboon" => Ok(Animal::Baboon),
            "hyena" => Ok(Animal::Hyena),
            "manticore" => Ok(Animal::Manticore),
            "warg" => Ok(Animal::Warg),
            "elephant" => Ok(Animal::Elephant),
            "rhino" => Ok(Animal::Rhino),
            "hippo" => Ok(Animal::Hippo),
            "blight wasp" => Ok(Animal::BlightWasp),
            _ => Err(()),
        }
    }
}

impl Display for Animal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Animal::Squirrel => write!(f, "squirrel"),
            Animal::Bear => write!(f, "bear"),
            Animal::Wolf => write!(f, "wolf"),
            Animal::Panther => write!(f, "panther"),
            Animal::Boar => write!(f, "boar"),
            Animal::Snake => write!(f, "snake"),
            Animal::Monkey => write!(f, "monkey"),
            Animal::Baboon => write!(f, "baboon"),
            Animal::Hyena => write!(f, "hyena"),
            Animal::Manticore => write!(f, "manticore"),
            Animal::Warg => write!(f, "warg"),
            Animal::Elephant => write!(f, "elephant"),
            Animal::Rhino => write!(f, "rhino"),
            Animal::Hippo => write!(f, "hippo"),
            Animal::BlightWasp => write!(f, "blight wasp"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(Animal::Squirrel, "squirrel")]
    #[case(Animal::Bear, "bear")]
    #[case(Animal::Wolf, "wolf")]
    #[case(Animal::Panther, "panther")]
    #[case(Animal::Boar, "boar")]
    #[case(Animal::Snake, "snake")]
    #[case(Animal::Monkey, "monkey")]
    #[case(Animal::Baboon, "baboon")]
    #[case(Animal::Hyena, "hyena")]
    #[case(Animal::Manticore, "manticore")]
    #[case(Animal::Warg, "warg")]
    #[case(Animal::Elephant, "elephant")]
    #[case(Animal::Rhino, "rhino")]
    #[case(Animal::Hippo, "hippo")]
    #[case(Animal::BlightWasp, "blight wasp")]
    fn animal_to_string(#[case] animal: Animal, #[case] expected: &str) {
        assert_eq!(animal.to_string(), expected);
    }

    #[rstest]
    #[case("squirrel", Animal::Squirrel)]
    #[case("bear", Animal::Bear)]
    #[case("wolf", Animal::Wolf)]
    #[case("panther", Animal::Panther)]
    #[case("boar", Animal::Boar)]
    #[case("snake", Animal::Snake)]
    #[case("monkey", Animal::Monkey)]
    #[case("baboon", Animal::Baboon)]
    #[case("hyena", Animal::Hyena)]
    #[case("manticore", Animal::Manticore)]
    #[case("warg", Animal::Warg)]
    #[case("elephant", Animal::Elephant)]
    #[case("rhino", Animal::Rhino)]
    #[case("hippo", Animal::Hippo)]
    #[case("blight wasp", Animal::BlightWasp)]
    fn animal_from_str(#[case] input: &str, #[case] animal: Animal) {
        assert_eq!(animal, Animal::from_str(input).unwrap());
    }

    #[rstest]
    #[case(Animal::Squirrel, 1)]
    #[case(Animal::Bear, 10)]
    #[case(Animal::Wolf, 5)]
    #[case(Animal::Panther, 5)]
    #[case(Animal::Boar, 3)]
    #[case(Animal::Snake, 2)]
    #[case(Animal::Monkey, 3)]
    #[case(Animal::Baboon, 5)]
    #[case(Animal::Hyena, 5)]
    #[case(Animal::Manticore, 10)]
    #[case(Animal::Warg, 10)]
    #[case(Animal::Elephant, 10)]
    #[case(Animal::Rhino, 10)]
    #[case(Animal::Hippo, 20)]
    #[case(Animal::BlightWasp, 5)]
    fn animal_damage(#[case] animal: Animal, #[case] damage: u32) {
        assert_eq!(animal.damage(), damage);
    }

    #[test]
    fn random_animal() {
        let animal = Animal::random();
        assert_eq!(animal, Animal::from_str(&animal.to_string()).unwrap());
    }

    #[test]
    fn animal_plurals() {
        let tracker_jacker = Animal::BlightWasp;
        let wolf = Animal::Wolf;
        let panther = Animal::Panther;
        assert_eq!(tracker_jacker.plural(), "blight wasps");
        assert_eq!(wolf.plural(), "wolves");
        assert_eq!(panther.plural(), "panthers");
    }
}
