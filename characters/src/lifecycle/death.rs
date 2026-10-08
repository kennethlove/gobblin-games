use crate::Character;
use crate::statuses::CharacterStatus;
use rand::RngExt;
use rand::prelude::*;
use rand::rngs::SmallRng;

impl Character {
    /// Marks the character as dead and reveals them.
    pub fn dies(&mut self) {
        self.blood = 0;
        self.set_status(CharacterStatus::Dead);
        self.attributes.is_hidden = false;
        self.items.clear();
    }

    /// Does the character have health and an OK status?
    pub fn is_alive(&self) -> bool {
        self.blood > 0
            && self.status != CharacterStatus::Dead
            && self.status != CharacterStatus::RecentlyDead
    }

    /// Hides the character from view.
    pub(crate) fn hides(&mut self) -> bool {
        let mut rng = SmallRng::from_rng(&mut rand::rng());
        let hidden = rng.random_bool(self.attributes.intelligence as f64 / 100.0);
        self.attributes.is_hidden = hidden;
        hidden
    }

    /// Helper function to see if the character is hidden
    pub fn is_visible(&self) -> bool {
        !self.attributes.is_hidden
    }
}

#[cfg(test)]
mod tests {
    use crate::Character;
    use crate::statuses::CharacterStatus;
    use rstest::*;

    #[fixture]
    fn character() -> Character {
        Character::new("Snaggletooth".to_string(), None, None)
    }

    #[rstest]
    fn takes_no_physical_damage_when_dead(mut character: Character) {
        character.dies();
        // Blood is already 0 after dies(), saturating_sub keeps it at 0
        character.blood = character.blood.saturating_sub(100);
        assert_eq!(character.blood, 0);
    }

    #[rstest]
    fn does_not_heal_when_dead(mut character: Character) {
        character.dies();
        character.heals(10);
        assert_eq!(character.blood, 0);
    }

    #[rstest]
    fn dies(mut character: Character) {
        character.dies();
        assert_eq!(character.blood, 0);
        assert_eq!(character.status, CharacterStatus::Dead);
        assert!(!character.attributes.is_hidden);
        assert_eq!(character.items.len(), 0);
    }

    #[rstest]
    fn is_alive(mut character: Character) {
        assert!(character.is_alive());
        character.dies();
        assert!(!character.is_alive());
    }

    #[rstest]
    fn hides_success(mut character: Character) {
        character.attributes.intelligence = 100;
        let hidden = character.hides();
        assert!(hidden);
        assert!(character.attributes.is_hidden);
    }

    #[rstest]
    fn hides_fail(mut character: Character) {
        character.attributes.intelligence = 0;
        let hidden = character.hides();
        assert!(!hidden);
        assert!(!character.attributes.is_hidden);
    }

    #[rstest]
    fn is_visible(mut character: Character) {
        assert!(character.is_visible());
        character.attributes.is_hidden = true;
        assert!(!character.is_visible());
    }
}
