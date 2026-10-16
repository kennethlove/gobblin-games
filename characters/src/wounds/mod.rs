pub mod pain;

use shared::wounds::{BodyPart, WoundSeverity};

/// Maximum blood pool for a character.
pub const MAX_BLOOD: u32 = 1000;

/// Blood level at which a character dies.
pub const DEATH_THRESHOLD: u32 = 0;

/// Blood restored per period by natural clotting (applied to all wounds that
/// are still bleeding).
pub const NATURAL_CLOT_RATE: u32 = 5;

/// Infection chance per period for Critical wounds (0.0-1.0).
pub const CRITICAL_INFECTION_CHANCE: f64 = 0.25;

/// Blood restored per period from food/rest.
pub const REST_BLOOD_RESTORE: u32 = 20;

/// --- Stat penalties per wound severity ---
/// Wound penalties are **percentage-based** (spec §2/§10.1: wounds share the
/// `(base + flat) x percent` formula with trait effects, so no consumer
/// special-cases them). Each percentage is the old flat penalty expressed
/// against the attribute's reference maximum (strength/defense 50,
/// bravery 100): a character at reference max takes exactly the
/// old absolute hit, weaker characters take proportionally less.
///
/// Penalty to effective strength per wound severity tier.
pub fn strength_penalty_percent(severity: WoundSeverity) -> f64 {
    match severity {
        WoundSeverity::Minor => -2.0,     // was -1 of 50
        WoundSeverity::Moderate => -6.0,  // was -3 of 50
        WoundSeverity::Severe => -12.0,   // was -6 of 50
        WoundSeverity::Critical => -20.0, // was -10 of 50
    }
}

/// Penalty to effective defense per wound severity tier.
pub fn toughness_penalty_percent(severity: WoundSeverity) -> f64 {
    match severity {
        WoundSeverity::Minor => 0.0,      // was 0
        WoundSeverity::Moderate => -2.0,  // was -1 of 50
        WoundSeverity::Severe => -6.0,    // was -3 of 50
        WoundSeverity::Critical => -10.0, // was -5 of 50
    }
}

/// Penalty to effective bravery per wound severity tier.
pub fn bravery_penalty_percent(severity: WoundSeverity) -> f64 {
    match severity {
        WoundSeverity::Minor => 0.0,     // was 0
        WoundSeverity::Moderate => -1.0, // was -1 of 100
        WoundSeverity::Severe => -3.0,   // was -3 of 100
        WoundSeverity::Critical => -5.0, // was -5 of 100
    }
}

/// Penalty to effective perception per wound severity tier (reference max
/// 100, modest early, steep at Critical).
/// Head wounds amplify this further via the body-part multiplier.
pub fn perception_penalty_percent(severity: WoundSeverity) -> f64 {
    match severity {
        WoundSeverity::Minor => 0.0,
        WoundSeverity::Moderate => -2.0,
        WoundSeverity::Severe => -5.0,
        WoundSeverity::Critical => -10.0,
    }
}

/// Total percentage a wound of this severity/body part takes off one
/// attribute. Attributes with no wound model (intelligence, persuasion,
/// luck, speed) get `0.0`.
pub fn attr_mod_percent(
    severity: WoundSeverity,
    part: BodyPart,
    attr: crate::traits::CharacterAttribute,
) -> f64 {
    use crate::traits::CharacterAttribute as A;
    let base = match attr {
        A::Strength => strength_penalty_percent(severity),
        A::Toughness => toughness_penalty_percent(severity),
        A::Bravery => bravery_penalty_percent(severity),
        A::Perception => perception_penalty_percent(severity),
        A::Intelligence | A::Persuasion | A::Luck | A::Speed => 0.0,
    };
    base * body_part_penalty_multiplier(part)
}

/// Additional penalty multiplier for Head wounds (applied on top of severity).
pub const HEAD_WOUND_MULTIPLIER: f64 = 1.5;

/// Additional penalty multiplier for Torso wounds.
pub const TORSO_WOUND_MULTIPLIER: f64 = 1.2;

/// Limb that triggers amputation when Severe+ wound is on it.
pub const AMPUTATION_SEVERITY_THRESHOLD: WoundSeverity = WoundSeverity::Severe;

pub fn body_part_penalty_multiplier(part: BodyPart) -> f64 {
    match part {
        BodyPart::Head => HEAD_WOUND_MULTIPLIER,
        BodyPart::Torso => TORSO_WOUND_MULTIPLIER,
        BodyPart::LeftArm | BodyPart::RightArm | BodyPart::LeftLeg | BodyPart::RightLeg => 1.0,
    }
}

/// Heroism trigger: when blood drops below this percentage, character gets a
/// temporary bravery boost.
pub const HEROISM_BLOOD_THRESHOLD: f64 = 0.25;

/// Bravery boost applied when heroism triggers.
pub const HEROISM_BRAVERY_BOOST: u32 = 20;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Character;
    use shared::wounds::WoundType;

    #[test]
    fn penalty_increases_with_severity() {
        let minor = strength_penalty_percent(WoundSeverity::Minor);
        let moderate = strength_penalty_percent(WoundSeverity::Moderate);
        let severe = strength_penalty_percent(WoundSeverity::Severe);
        let critical = strength_penalty_percent(WoundSeverity::Critical);
        // Percentages are negative; "more severe" means more negative.
        assert!(minor > moderate);
        assert!(moderate > severe);
        assert!(severe > critical);
    }

    #[test]
    fn head_wound_has_highest_multiplier() {
        let head = body_part_penalty_multiplier(BodyPart::Head);
        let torso = body_part_penalty_multiplier(BodyPart::Torso);
        let arm = body_part_penalty_multiplier(BodyPart::LeftArm);
        assert!(head > torso);
        assert!(torso >= arm);
    }

    #[test]
    fn attributes_without_a_wound_model_take_no_percent() {
        use crate::traits::CharacterAttribute as A;
        for attr in [A::Speed, A::Luck, A::Intelligence, A::Persuasion] {
            assert_eq!(
                attr_mod_percent(WoundSeverity::Critical, BodyPart::Head, attr),
                0.0,
                "{attr:?} has no wound model"
            );
        }
    }

    #[test]
    fn limb_wound_keeps_severity_percentage_head_scales_it() {
        use crate::traits::CharacterAttribute as A;
        let crit = WoundSeverity::Critical;
        assert_eq!(
            attr_mod_percent(crit, BodyPart::LeftArm, A::Strength),
            strength_penalty_percent(crit)
        );
        assert_eq!(
            attr_mod_percent(crit, BodyPart::Head, A::Strength),
            strength_penalty_percent(crit) * HEAD_WOUND_MULTIPLIER
        );
    }

    #[test]
    fn wound_blood_drain_per_period() {
        let mut character = Character::new("Test".to_string(), None, None);
        character.wounds.push(shared::wounds::Wound::new(
            shared::wounds::WoundType::Stab,
            WoundSeverity::Moderate,
            BodyPart::Torso,
        ));

        let initial_blood = character.blood;
        let lost = character.drain_blood_from_wounds();

        assert_eq!(lost, 15);
        assert_eq!(character.blood, initial_blood - 15);
    }

    #[test]
    fn multiple_wounds_stack_blood_loss() {
        let mut character = Character::new("Test".to_string(), None, None);
        character.wounds.push(shared::wounds::Wound::new(
            shared::wounds::WoundType::Cut,
            WoundSeverity::Minor,
            BodyPart::Torso,
        ));
        character.wounds.push(shared::wounds::Wound::new(
            shared::wounds::WoundType::Stab,
            WoundSeverity::Severe,
            BodyPart::LeftArm,
        ));

        let lost = character.drain_blood_from_wounds();
        assert_eq!(lost, 45);
    }

    #[test]
    fn stopped_wound_no_blood_loss() {
        let mut character = Character::new("Test".to_string(), None, None);
        let mut wound = shared::wounds::Wound::new(
            shared::wounds::WoundType::Cut,
            WoundSeverity::Minor,
            BodyPart::Torso,
        );
        wound.bleeding = false;
        character.wounds.push(wound);

        let lost = character.drain_blood_from_wounds();
        assert_eq!(lost, 0);
    }

    #[test]
    fn effective_strength_reduced_by_wounds() {
        let mut character = Character::new("Test".to_string(), None, None);
        character.attributes.strength = 30;

        let base = character.effective_strength();
        assert_eq!(base, 30);

        character.wounds.push(shared::wounds::Wound::new(
            shared::wounds::WoundType::Crush,
            WoundSeverity::Severe,
            BodyPart::Torso,
        ));

        let wounded = character.effective_strength();
        assert!(wounded < base, "wounds should reduce effective strength");
    }

    /// The percent tables were derived from the old flat penalty tables
    /// (spec §10.1). At each attribute's reference maximum the absolute hit
    /// must match the old flat value exactly — that is the parity contract
    /// the conversion was built on.
    #[test]
    fn percent_tables_match_old_flat_penalties_at_reference_max() {
        use crate::traits::CharacterAttribute as A;
        // (attr, reference max, old flat penalty per severity)
        let cases: [(A, u32, [f64; 4]); 3] = [
            // strength: -1/-3/-6/-10 on 50
            (A::Strength, 50, [-1.0, -3.0, -6.0, -10.0]),
            // defense: 0/-1/-3/-5 on 50
            (A::Toughness, 50, [0.0, -1.0, -3.0, -5.0]),
            // bravery: 0/-1/-3/-5 on 100
            (A::Bravery, 100, [0.0, -1.0, -3.0, -5.0]),
        ];
        let severities = [
            WoundSeverity::Minor,
            WoundSeverity::Moderate,
            WoundSeverity::Severe,
            WoundSeverity::Critical,
        ];

        for (attr, reference, old_flat) in cases {
            for (severity, old) in severities.iter().zip(old_flat) {
                let mut c = Character::new("Stinky".to_string(), None, None);
                // Isolate the wound tables: rolled traits (Tough's +20%
                // toughness, etc.) must not pollute parity assertions.
                c.traits.clear();
                match attr {
                    A::Strength => c.attributes.strength = reference,
                    A::Toughness => c.attributes.toughness = reference,
                    A::Bravery => c.attributes.bravery = reference,
                    // No wound model; skipped here.
                    _ => c.attributes.strength = reference,
                }
                c.wounds.push(shared::wounds::Wound::new(
                    WoundType::Cut,
                    *severity,
                    BodyPart::LeftArm,
                ));

                let effective = c.effective_attribute(attr) as f64;
                let expected = reference as f64 + old; // old flat table at max
                assert_eq!(
                    effective, expected,
                    "{attr:?} {severity:?}: percent table must match old flat penalty at reference max"
                );
            }
        }
    }

    /// Below reference max a percentage costs a weaker character less in
    /// absolute terms than the old flat table did. That is the one
    /// deliberate behavior change of the conversion; pin it so a future
    /// "retune" doesn't silently reintroduce flat scaling.
    #[test]
    fn below_reference_max_percentage_costs_less_than_the_old_flat_table() {
        let mut c = Character::new("Stinky".to_string(), None, None);
        c.attributes.strength = 20; // well under the 50 reference max
        c.wounds.push(shared::wounds::Wound::new(
            WoundType::Cut,
            WoundSeverity::Critical,
            BodyPart::LeftArm,
        ));

        // Old flat table: 20 - 10 = 10. Percent: 20 x 0.8 = 16.
        assert_eq!(c.effective_strength(), 16);
    }

    #[test]
    fn pain_condition_updates_from_wounds() {
        let mut character = Character::new("Test".to_string(), None, None);
        character.wounds.push(shared::wounds::Wound::new(
            shared::wounds::WoundType::Stab,
            WoundSeverity::Moderate,
            BodyPart::Torso,
        ));

        character.update_pain_condition();

        let has_pain = character
            .mental_conditions
            .iter()
            .any(|c| matches!(c, shared::conditions::MentalCondition::Pain { .. }));
        assert!(has_pain, "should have pain condition from wounds");
    }

    #[test]
    fn death_from_blood_loss() {
        let mut character = Character::new("Test".to_string(), None, None);
        character.blood = 10;
        character.wounds.push(shared::wounds::Wound::new(
            shared::wounds::WoundType::Stab,
            WoundSeverity::Critical,
            BodyPart::Torso,
        ));

        character.drain_blood_from_wounds();
        assert_eq!(character.blood, 0, "blood should be 0 after critical bleed");
    }

    #[test]
    fn perception_wound_model_scales_with_severity_and_head() {
        use crate::traits::CharacterAttribute as A;
        // Flat table of reference max (100).
        let cases = [
            (WoundSeverity::Minor, 0.0),
            (WoundSeverity::Moderate, -2.0),
            (WoundSeverity::Severe, -5.0),
            (WoundSeverity::Critical, -10.0),
        ];
        for (severity, expected) in cases {
            assert_eq!(
                perception_penalty_percent(severity),
                expected,
                "{severity:?} perception table"
            );
            let limb = attr_mod_percent(severity, BodyPart::LeftArm, A::Perception);
            assert_eq!(limb, expected, "limb multiplier is 1.0");
            let head = attr_mod_percent(severity, BodyPart::Head, A::Perception);
            assert_eq!(head, expected * 1.5, "head amplifies perception loss");
        }
    }
}
