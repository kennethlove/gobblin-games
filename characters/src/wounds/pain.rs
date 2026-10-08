use shared::conditions::{ConditionSeverity, MentalCondition};
use shared::wounds::WoundSeverity;

use crate::Character;

/// Calculates pain severity from the character's current wounds.
///
/// Pain scales with total wound count and severity:
/// - 1 Minor wound → Mild
/// - 2+ Minor or 1 Moderate → Moderate
/// - 2+ Moderate or 1 Severe → Severe
/// - 2+ Severe or any Critical → Critical
fn pain_severity_from_wounds(wounds: &[shared::wounds::Wound]) -> ConditionSeverity {
    if wounds.is_empty() {
        return ConditionSeverity::Mild;
    }

    let severe_plus = wounds
        .iter()
        .filter(|w| matches!(w.severity, WoundSeverity::Severe | WoundSeverity::Critical))
        .count();
    let moderate_plus = wounds
        .iter()
        .filter(|w| {
            matches!(
                w.severity,
                WoundSeverity::Moderate | WoundSeverity::Severe | WoundSeverity::Critical
            )
        })
        .count();

    if severe_plus >= 2 || wounds.iter().any(|w| w.severity == WoundSeverity::Critical) {
        ConditionSeverity::Critical
    } else if severe_plus >= 1 || moderate_plus >= 2 {
        ConditionSeverity::Severe
    } else if moderate_plus >= 1 || wounds.len() >= 2 {
        ConditionSeverity::Moderate
    } else {
        ConditionSeverity::Mild
    }
}

impl Character {
    /// Recalculates the Pain condition from current wounds.
    /// Removes existing Pain condition, then adds a new one if wounds are present.
    pub(crate) fn update_pain_condition(&mut self) {
        // Remove existing Pain condition
        self.mental_conditions
            .retain(|c| !matches!(c, MentalCondition::Pain { .. }));

        if self.wounds.is_empty() {
            return;
        }

        let severity = pain_severity_from_wounds(&self.wounds);
        self.mental_conditions
            .push(MentalCondition::Pain { severity });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::wounds::{BodyPart, Wound, WoundType};

    #[test]
    fn no_wounds_no_pain() {
        let mut character = crate::Character::new("Test".to_string(), None, None);
        character.update_pain_condition();
        assert!(
            character
                .mental_conditions
                .iter()
                .all(|c| !matches!(c, MentalCondition::Pain { .. }))
        );
    }

    #[test]
    fn single_minor_wound_mild_pain() {
        let mut character = crate::Character::new("Test".to_string(), None, None);
        character.wounds.push(Wound::new(
            WoundType::Cut,
            WoundSeverity::Minor,
            BodyPart::Torso,
        ));
        character.update_pain_condition();
        let pain = character
            .mental_conditions
            .iter()
            .find(|c| matches!(c, MentalCondition::Pain { .. }));
        assert!(pain.is_some());
        assert_eq!(pain.unwrap().severity(), ConditionSeverity::Mild);
    }

    #[test]
    fn two_moderate_wounds_severe_pain() {
        let mut character = crate::Character::new("Test".to_string(), None, None);
        for _ in 0..2 {
            character.wounds.push(Wound::new(
                WoundType::Stab,
                WoundSeverity::Moderate,
                BodyPart::Torso,
            ));
        }
        character.update_pain_condition();
        let pain = character
            .mental_conditions
            .iter()
            .find(|c| matches!(c, MentalCondition::Pain { .. }));
        assert_eq!(pain.unwrap().severity(), ConditionSeverity::Severe);
    }

    #[test]
    fn critical_wound_critical_pain() {
        let mut character = crate::Character::new("Test".to_string(), None, None);
        character.wounds.push(Wound::new(
            WoundType::Crush,
            WoundSeverity::Critical,
            BodyPart::Head,
        ));
        character.update_pain_condition();
        let pain = character
            .mental_conditions
            .iter()
            .find(|c| matches!(c, MentalCondition::Pain { .. }));
        assert_eq!(pain.unwrap().severity(), ConditionSeverity::Critical);
    }

    #[test]
    fn pain_replaces_old_pain() {
        let mut character = crate::Character::new("Test".to_string(), None, None);
        character.wounds.push(Wound::new(
            WoundType::Cut,
            WoundSeverity::Minor,
            BodyPart::Torso,
        ));
        character.update_pain_condition();
        // Wounds heal, recalculate
        character.wounds.clear();
        character.update_pain_condition();
        let pain_count = character
            .mental_conditions
            .iter()
            .filter(|c| matches!(c, MentalCondition::Pain { .. }))
            .count();
        assert_eq!(pain_count, 0);
    }
}
