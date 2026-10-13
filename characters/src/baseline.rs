//! Cross-game baseline for per-game reset.
//!
//! A goblin's durable state — attributes, traits, lifetime fight record —
//! must survive game boundaries with one rule: whatever the goblin *earned*
//! during a game carries on; whatever merely *happened* to it does not.
//! Reset restores the pre-game baseline, then re-applies the earned deltas.
//!
//! `Character::reset_for_new_game` captures this snapshot at game entry and
//! settles it at the next entry. See the module docs on `GameBaseline` for
//! the exact carry rules.

use crate::{Attributes, Character, traits::Trait};
use serde::{Deserialize, Serialize};

/// One attribute's signed cross-game delta (0 = unchanged).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttributeDelta {
    pub movement: i64,
    pub strength: i64,
    pub defense: i64,
    pub bravery: i64,
    pub intelligence: i64,
    pub persuasion: i64,
    pub luck: i64,
    pub agility: i64,
}

/// Durable state captured at game entry, used to settle advancements at the
/// next entry.
///
/// Carry rules, per field family:
/// - **Traits:** gains persist, losses do not. Traits only enter mid-game
///   through advancement paths (revival scars and bonuses), so any trait
///   present now but absent from the baseline is an advancement.
/// - **Attributes:** the field-wise high-water mark persists. Positive
///   deltas are advancements (e.g. revival's `+1 luck` fallback); negative
///   deltas are per-game debuffs (wound penalties from
///   `lifecycle::health`) and are dropped by the restore.
/// - **Lifetime fight record** (`kills`/`wins`/`defeats`/`draws`):
///   increments persist; per-game record fields (`day_killed`,
///   `killed_by`, `game`) are cleared by the reset itself.
///
/// Not captured: identity, name, team, clan, avatar, `brain` (re-derived
/// from traits), `ever_addicted_to` (deliberate cross-game carry, see its
/// field docs), and everything per-game (wounds, items, allies, …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameBaseline {
    pub attributes: Attributes,
    pub traits: Vec<Trait>,
    pub kills: u32,
    pub wins: u32,
    pub defeats: u32,
    pub draws: u32,
}

impl GameBaseline {
    /// Snapshot the durable state of `character` as its next baseline.
    pub fn capture(character: &Character) -> Self {
        Self {
            attributes: character.attributes.clone(),
            traits: character.traits.clone(),
            kills: character.statistics.kills,
            wins: character.statistics.wins,
            defeats: character.statistics.defeats,
            draws: character.statistics.draws,
        }
    }

    /// Advance `character`'s durable state to `baseline + earned`: restore
    /// the baseline, then keep only the advancements earned since it was
    /// captured (trait gains, attribute high-water deltas, record
    /// increments).
    pub fn settle_into(&self, character: &mut Character) {
        let gained_traits: Vec<Trait> = character
            .traits
            .iter()
            .filter(|t| !self.traits.contains(t))
            .copied()
            .collect();

        // Attributes: field-wise max — positive deltas persist, debuffs drop.
        let now = &character.attributes;
        let base = &self.attributes;
        character.attributes = Attributes {
            movement: base.movement.max(now.movement),
            strength: base.strength.max(now.strength),
            defense: base.defense.max(now.defense),
            bravery: base.bravery.max(now.bravery),
            intelligence: base.intelligence.max(now.intelligence),
            persuasion: base.persuasion.max(now.persuasion),
            luck: base.luck.max(now.luck),
            agility: base.agility.max(now.agility),
            is_hidden: base.is_hidden,
        };

        // Traits: baseline plus whatever was gained mid-game.
        character.traits = self.traits.clone();
        for trait_ in gained_traits {
            if !character.traits.contains(&trait_) {
                character.traits.push(trait_);
            }
        }

        // Lifetime fight record: baseline plus increments earned this game.
        character.statistics.kills = self
            .kills
            .saturating_add(character.statistics.kills.saturating_sub(self.kills));
        character.statistics.wins = self
            .wins
            .saturating_add(character.statistics.wins.saturating_sub(self.wins));
        character.statistics.defeats = self
            .defeats
            .saturating_add(character.statistics.defeats.saturating_sub(self.defeats));
        character.statistics.draws = self
            .draws
            .saturating_add(character.statistics.draws.saturating_sub(self.draws));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Trait;

    fn clean_goblin(name: &str) -> Character {
        let mut c = Character::new(name.to_string(), None, None);
        c.traits.clear();
        c.attributes.movement = 36;
        c.attributes.luck = 50;
        c
    }

    #[test]
    fn settle_restores_baseline_plus_trait_gains() {
        let mut c = clean_goblin("Boblin");
        // A durable trait at capture time: must survive even if the game
        // "loses" it mid-game (trait losses are not advancements).
        c.traits.push(Trait::Tough);
        let baseline = GameBaseline::capture(&c);

        c.traits.retain(|t| *t != Trait::Tough); // lost mid-game: restored
        c.traits.push(Trait::Resilient); // earned mid-game: persists
        c.traits.push(Trait::Fragile); // curse-shaped gain: persists too
        // (boons AND curses travel as traits;
        // debuff-dropping is attribute-only)

        let mut restored = c.clone();
        baseline.settle_into(&mut restored);

        assert!(restored.traits.contains(&Trait::Resilient));
        assert!(restored.traits.contains(&Trait::Fragile));
        assert!(restored.traits.contains(&Trait::Tough));
        assert_eq!(restored.traits, {
            let mut expected = baseline.traits.clone();
            expected.push(Trait::Resilient);
            expected.push(Trait::Fragile);
            expected
        });
    }

    #[test]
    fn settle_keeps_attribute_gains_drops_debuffs() {
        let mut c = clean_goblin("Boblin");
        let baseline = GameBaseline::capture(&c);

        c.attributes.luck = baseline.attributes.luck + 1; // revival +1 luck
        c.attributes.strength = baseline.attributes.strength.saturating_sub(20); // wound debuff
        c.attributes.movement = 0; // floor case

        let mut restored = c.clone();
        baseline.settle_into(&mut restored);

        assert_eq!(restored.attributes.luck, baseline.attributes.luck + 1);
        assert_eq!(restored.attributes.strength, baseline.attributes.strength);
        assert_eq!(restored.attributes.movement, baseline.attributes.movement);
    }

    #[test]
    fn settle_keeps_lifetime_record_increments() {
        let mut c = clean_goblin("Boblin");
        let baseline = GameBaseline::capture(&c);

        c.statistics.kills += 1;
        c.statistics.wins += 1;

        let mut restored = c.clone();
        baseline.settle_into(&mut restored);
        assert_eq!(restored.statistics.kills, baseline.kills + 1);
        assert_eq!(restored.statistics.wins, baseline.wins + 1);
    }

    #[test]
    fn baseline_serde_roundtrips() {
        let c = clean_goblin("Boblin");
        let baseline = GameBaseline::capture(&c);
        let json = serde_json::to_string(&baseline).unwrap();
        let back: GameBaseline = serde_json::from_str(&json).unwrap();
        assert_eq!(baseline, back);
    }
}
