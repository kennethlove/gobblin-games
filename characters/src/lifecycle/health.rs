use crate::Character;
use crate::traits::{CharacterAttribute, Effect};
use crate::wounds;
use rand::RngExt;
use shared::messages::{CharacterRef, MessagePayload};
use shared::wounds::{BodyPart, Wound, WoundSeverity, WoundType};

/// Attribute maximums
const MAX_STRENGTH: u32 = 50;
const MAX_BRAVERY: u32 = 100;

/// Default healing amounts
const DEFAULT_HEAL: u32 = 5;
const DEFAULT_MENTAL_HEAL: u32 = 5;

/// `(base + flat) x percent`, floored at 1: every scale bottom is 1+.
/// A floor of 0 let a heavily wounded goblin's strength reach 0, and 0
/// strength means 0 damage on every hit — a fight it can never win by
/// striking (h26-era review catch).
///
/// Flat modifiers sum under the multiply; percentages accumulate additively
/// and apply to the flat-adjusted base. Exposed so the formula can be tested
/// without constructing a whole character.
pub fn apply_attribute_modifiers(base: u32, flat: i32, percent: f64) -> u32 {
    let value = (base as f64 + flat as f64) * (1.0 + percent / 100.0);
    value.max(1.0).round() as u32
}

/// Fresh stamina pool for a trait set: the base 100 bent by every
/// `MaxStaminaPercent` effect (creation-time coherence, mirroring
/// `Attributes::from_rng_with_traits` for the attribute axes).
pub fn max_stamina_for_traits(traits: &[crate::traits::Trait]) -> u32 {
    apply_attribute_modifiers(
        100,
        0,
        f64::from(crate::traits::stamina_max_percent_sum(traits)),
    )
}

/// Accumulate `(flat, percent)` from one effect list for one attribute.
/// Both permanent (trait) and temporary (affliction) layers funnel through
/// this, which is what makes them indistinguishable downstream (spec §4).
pub(crate) fn sum_attr_mods(effects: &[Effect], attr: CharacterAttribute) -> (i32, f64) {
    let mut flat = 0i32;
    let mut percent = 0.0f64;
    for e in effects {
        if let Effect::AttrMod {
            attr: effect_attr,
            flat: effect_flat,
            percent: effect_percent,
        } = e
            && *effect_attr == attr
        {
            flat += *effect_flat;
            percent += *effect_percent as f64;
        }
    }
    (flat, percent)
}

impl Character {
    /// Character is lonely/homesick/etc., loses some sanity.
    /// Sanity is now derived from mental conditions; this pushes a Despondent
    /// condition whose severity scales with bravery.
    pub(crate) fn misses_home(&mut self) {
        let loneliness = self.effective_attribute(CharacterAttribute::Bravery) as f64 / 100.0;

        if loneliness.round() < 0.25 {
            let severity = if self.effective_sanity() < 25 {
                shared::conditions::ConditionSeverity::Severe
            } else {
                shared::conditions::ConditionSeverity::Mild
            };
            self.mental_conditions
                .push(shared::conditions::MentalCondition::Despondent { severity });
        }
    }

    /// Reduces attack strength.
    pub(crate) fn reduce_strength(&mut self, amount: u32) {
        self.attributes.strength = self.attributes.strength.saturating_sub(amount).max(1);
    }

    /// Increases attack strength.
    pub(crate) fn increase_strength(&mut self, amount: u32) {
        self.attributes.strength = self
            .attributes
            .strength
            .saturating_add(amount)
            .min(MAX_STRENGTH);
    }

    /// Drains stamina — the single viability source. Used by afflictions
    /// (Sick, Frozen, Overheated, BrokenBone) that used to hamper travel by
    /// reducing the removed movement attribute.
    pub(crate) fn reduce_stamina(&mut self, amount: u32) {
        self.stamina = self.stamina.saturating_sub(amount);
    }

    /// Reduces intelligence which affects decision-making and hiding.
    pub(crate) fn reduce_intelligence(&mut self, amount: u32) {
        self.attributes.intelligence = self.attributes.intelligence.saturating_sub(amount).max(1);
    }

    /// Increases bravery which affects decision-making.
    pub(crate) fn increase_bravery(&mut self, amount: u32) {
        self.attributes.bravery = self
            .attributes
            .bravery
            .saturating_add(amount)
            .min(MAX_BRAVERY);
    }

    /// Restores stamina (consumables: trail mix, go-juice).
    pub(crate) fn restore_stamina(&mut self, amount: u32) {
        self.stamina = (self.stamina + amount).min(self.max_stamina);
    }

    /// Restores health (heals in old 0-100 scale, converted to blood 0-1000).
    pub(crate) fn heals(&mut self, health: u32) {
        if self.is_alive() {
            self.blood = self
                .blood
                .saturating_add(health * 10)
                .min(wounds::MAX_BLOOD);
        }
    }

    /// Restores mental health by removing mental conditions.
    /// Higher heal amounts remove more severe conditions.
    pub(crate) fn heals_mental_damage(&mut self, sanity: u32) {
        if self.mental_conditions.is_empty() {
            return;
        }
        // Remove conditions up to the heal budget
        let mut budget = sanity;
        self.mental_conditions.retain(|c| {
            if budget == 0 {
                return true;
            }
            let drain = c.severity().sanity_drain();
            if drain <= budget {
                budget -= drain;
                false // remove this condition
            } else {
                true // keep it
            }
        });
    }

    /// No-op since movement went away: short rest no longer has a pool to
    /// refill. Stamina recovery happens per-phase in `recover_stamina`.
    /// Kept because the travel-failure refund path in `act_move` calls it.
    pub(crate) fn short_rests(&mut self) {}

    /// Restores some health and sanity
    pub(crate) fn long_rests(&mut self) {
        self.short_rests();
        self.heals(DEFAULT_HEAL);
        self.heals_mental_damage(DEFAULT_MENTAL_HEAL);
    }

    // --- Wound-based blood system ---

    /// Creates a wound on this character. Adds the wound and drains blood
    /// immediately based on severity.
    pub(crate) fn create_wound(
        &mut self,
        wound_type: WoundType,
        severity: WoundSeverity,
        body_part: BodyPart,
    ) {
        let mut wound = Wound::new(wound_type, severity, body_part);
        wound.created_day = self.game_day;
        self.wounds.push(wound);
    }

    /// Drains blood from all active bleeding wounds. Returns the total blood
    /// lost this period. Called once per game cycle.
    pub(crate) fn drain_blood_from_wounds(&mut self) -> u32 {
        let mut total_loss = 0u32;
        for wound in &self.wounds {
            total_loss += wound.blood_loss_per_period();
        }
        self.blood = self.blood.saturating_sub(total_loss);
        total_loss
    }

    /// Natural wound healing pass. Each wound attempts to heal; this may stop
    /// bleeding or cause infection for Critical wounds.
    pub(crate) fn heal_wounds(
        &mut self,
        rng: &mut impl rand::Rng,
        events: &mut Vec<world::messages::TaggedEvent>,
    ) {
        let character_name = self.name.clone();
        let character_id = self.identifier.clone();

        for wound in &mut self.wounds {
            let was_infected = wound.infected;
            let was_bleeding = wound.bleeding;
            let roll: f64 = rng.random();
            wound.heals_naturally(roll);

            // Emit infection event
            if !was_infected && wound.infected {
                events.push(world::messages::TaggedEvent::new(
                    format!(
                        "{}'s {} wound became infected",
                        character_name, wound.body_part
                    ),
                    MessagePayload::WoundInfected {
                        character: CharacterRef {
                            identifier: character_id.clone().into(),
                            name: character_name.clone(),
                        },
                        body_part: wound.body_part.to_string(),
                    },
                ));
            }

            // Emit heal event (stopped bleeding, not infected)
            if was_bleeding && !wound.bleeding && !wound.infected {
                events.push(world::messages::TaggedEvent::new(
                    format!(
                        "{}'s {} wound stopped bleeding",
                        character_name, wound.body_part
                    ),
                    MessagePayload::WoundHealed {
                        character: CharacterRef {
                            identifier: character_id.clone().into(),
                            name: character_name.clone(),
                        },
                        body_part: wound.body_part.to_string(),
                    },
                ));
            }
        }
        // Remove fully healed wounds (no longer bleeding, not infected, Minor only)
        self.wounds
            .retain(|w| w.bleeding || w.infected || w.severity != WoundSeverity::Minor);
    }

    // --- Effective stats (single accessor path; spec §2/§4) ---

    /// Sum of every modifier carrying layer for one attribute: trait
    /// `AttrMod` effects and wound percentages. Returns `(flat, percent)`
    /// where percents accumulate additively (spec §4).
    fn attribute_modifiers(&self, attr: CharacterAttribute) -> (i32, f64) {
        let mut flat = 0i32;
        let mut percent = 0.0f64;

        for t in &self.traits {
            let (f, p) = sum_attr_mods(t.effects(), attr);
            flat += f;
            percent += p;
        }

        for w in &self.wounds {
            percent += wounds::attr_mod_percent(w.severity, w.body_part, attr);
        }

        (flat, percent)
    }

    /// `(base + flat) x percent`, floored at 1 (scales are 1+, not 0+ —
    /// see `apply_attribute_modifiers`). The one path
    /// every mechanic reads attributes through, so a permanent limp and a
    /// fresh sprain are indistinguishable to a consumer.
    pub fn effective_attribute(&self, attr: CharacterAttribute) -> u32 {
        let base = self.attributes.get(attr);
        let (flat, percent) = self.attribute_modifiers(attr);
        apply_attribute_modifiers(base, flat, percent)
    }

    /// Effective strength: wound penalties plus trait modifiers.
    pub fn effective_strength(&self) -> i32 {
        self.effective_attribute(CharacterAttribute::Strength) as i32
    }

    /// Effective toughness: wound penalties plus trait modifiers.
    pub fn effective_toughness(&self) -> i32 {
        self.effective_attribute(CharacterAttribute::Toughness) as i32
    }

    /// Effective health derived from blood pool. Normalizes 0-1000 blood to 0-100 scale.
    pub fn effective_health(&self) -> u32 {
        (self.blood / 10).min(100)
    }

    /// Current stamina as a 0-100 percentage of the pool. Brains read this
    /// wherever they used to read the (removed) Movement attribute — the
    /// homebody `low_movement` thresholds compare on this same 0-100 scale.
    /// A zero-sized pool counts as 0%.
    pub fn stamina_percent(&self) -> u32 {
        (self.stamina * 100)
            .checked_div(self.max_stamina)
            .unwrap_or(0)
    }

    /// Effective bravery: wound penalties plus trait modifiers.
    pub fn effective_bravery(&self) -> i32 {
        self.effective_attribute(CharacterAttribute::Bravery) as i32
    }

    /// Whether this character should trigger heroism (low blood bravery boost).
    pub fn should_heroism(&self) -> bool {
        let blood_ratio = self.blood as f64 / wounds::MAX_BLOOD as f64;
        blood_ratio <= wounds::HEROISM_BLOOD_THRESHOLD
    }

    /// Effective sanity derived from active mental conditions.
    /// No conditions -> full sanity (100). Each condition drains by its severity.
    pub fn effective_sanity(&self) -> u32 {
        if self.mental_conditions.is_empty() {
            100
        } else {
            100u32.saturating_sub(
                self.mental_conditions
                    .iter()
                    .map(|c| c.severity().sanity_drain())
                    .sum::<u32>(),
            )
        }
    }

    /// Ticks all mental conditions: advances durations and removes expired conditions.
    pub(crate) fn tick_mental_conditions(&mut self) {
        for condition in &mut self.mental_conditions {
            condition.tick();
        }
        self.mental_conditions.retain(|c| !c.is_expired());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Character;
    use rstest::*;

    #[fixture]
    fn character() -> Character {
        Character::new("Snaggletooth".to_string(), None, None)
    }

    #[rstest]
    fn saturating_sub_health(mut character: Character) {
        let health = character.effective_health();
        character.blood = character.blood.saturating_sub(100);
        assert_eq!(character.effective_health(), health - 10);
    }

    #[rstest]
    fn heals(mut character: Character) {
        character.blood = 500;
        character.heals(10);
        assert_eq!(character.effective_health(), 60);
    }

    #[rstest]
    fn heals_mental_damage(mut character: Character) {
        use shared::conditions::{ConditionSeverity, MentalCondition};
        character
            .mental_conditions
            .push(MentalCondition::Horrified {
                severity: ConditionSeverity::Mild,
                remaining_periods: 5,
            });
        assert_eq!(character.effective_sanity(), 99);
        character.heals_mental_damage(5);
        assert_eq!(character.effective_sanity(), 100);
    }

    #[rstest]
    fn short_rests_noop(mut character: Character) {
        // Movement pool is gone; short rest must not touch stamina or health.
        character.stamina = 37;
        character.blood = 500;
        character.short_rests();
        assert_eq!(character.stamina, 37);
        assert_eq!(character.effective_health(), 50);
    }

    #[rstest]
    fn long_rests(mut character: Character) {
        character.stamina = 0;
        character.blood = 500;
        character.long_rests();
        assert_eq!(character.effective_health(), 55);
    }

    #[rstest]
    fn misses_home(mut character: Character) {
        character.attributes.bravery = 20;
        // With bravery 20, loneliness = 0.2, rounds to 0 < 0.25 -> pushes condition
        assert!(character.mental_conditions.is_empty());
        character.misses_home();
        assert!(!character.mental_conditions.is_empty());
    }

    // --- Effective-value accessors (trait redesign §2/§4) ---

    #[test]
    fn modifiers_apply_as_base_plus_flat_times_percent() {
        // (36 + 10) x 1.0 = 46, then x 0.75 = 34.5 -> 35 (round)
        assert_eq!(apply_attribute_modifiers(36, 10, -25.0), 35);
        // Percent applies to the flat-adjusted base, not the raw base:
        // 36 x 0.75 = 27 would be wrong if flat were applied after.
        assert_eq!(apply_attribute_modifiers(36, 12, -25.0), 36);
    }

    #[test]
    fn effective_value_floors_at_zero() {
        assert_eq!(apply_attribute_modifiers(10, -30, 0.0), 1);
        assert_eq!(apply_attribute_modifiers(10, 0, -150.0), 1);
        assert_eq!(apply_attribute_modifiers(0, 0, 0.0), 1);
    }

    #[test]
    fn percents_and_flats_accumulate_across_sources() {
        // Two sources of -25% plus a flat -10 on base 100 -> (90) x 0.5 = 45.
        assert_eq!(apply_attribute_modifiers(100, -10, -50.0), 45);
    }

    #[test]
    fn wounds_reach_the_accessors_as_percentages() {
        let mut c = Character::new("Stinky".to_string(), None, None);
        c.attributes.strength = 50;
        assert_eq!(c.effective_strength(), 50, "no wounds, no modifiers");

        c.wounds.push(Wound::new(
            WoundType::Cut,
            WoundSeverity::Critical,
            BodyPart::LeftArm,
        ));
        // Critical -20% on a limb (x1.0) -> 50 x 0.8 = 40.
        assert_eq!(c.effective_strength(), 40);
        assert_eq!(
            c.effective_strength() as u32,
            c.effective_attribute(CharacterAttribute::Strength)
        );
    }

    #[test]
    fn temporary_and_permanent_sources_are_indistinguishable() {
        // A wound (temporary) and an effect list (how a trait or an
        // affliction contributes) carrying the same percentage must land on
        // the same effective value — no consumer special-cases either.
        let mut wounded = Character::new("Billy Slick".to_string(), None, None);
        wounded.attributes.strength = 50;
        wounded.wounds.push(Wound::new(
            WoundType::Cut,
            WoundSeverity::Critical,
            BodyPart::LeftArm,
        ));

        let base = 50u32;
        let (flat, percent) = sum_attr_mods(
            &[Effect::AttrMod {
                attr: CharacterAttribute::Strength,
                flat: 0,
                percent: -20,
            }],
            CharacterAttribute::Strength,
        );
        let effect_sourced = apply_attribute_modifiers(base, flat, percent);

        assert_eq!(wounded.effective_strength(), effect_sourced as i32);
    }

    #[test]
    fn stack_of_wounds_can_floor_an_attribute() {
        let mut c = Character::new("Stinky".to_string(), None, None);
        c.attributes.strength = 10;
        for _ in 0..6 {
            c.wounds.push(Wound::new(
                WoundType::Cut,
                WoundSeverity::Critical,
                BodyPart::LeftArm,
            ));
        }
        // 6 x -20% = -120% -> floored at 1, never 0 or negative.
        assert_eq!(c.effective_strength(), 1);
        assert_eq!(c.effective_attribute(CharacterAttribute::Strength), 1);
    }
}
