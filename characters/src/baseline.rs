//! Cross-game baseline for the new-game character rebuild.
//!
//! A goblin's durable state — attributes, traits, lifetime fight record —
//! must survive game boundaries with one rule: whatever the goblin *earned*
//! during a game carries on; whatever merely *happened* to it does not.
//! Reset restores the pre-game baseline, then re-applies the earned deltas.
//!
//! The snapshot is only the **floor**. Trait intent comes from the
//! [`crate::advancement`] log stamped as changes happen: gains are replayed
//! onto the floor, logged losses stay lost, and unstamped trait changes fall
//! back to gains-persist (spec §7.3, §10.8). Attribute high-water and
//! record-increment rules still diff against the snapshot directly — the log
//! supplements them, it does not replace them yet.
//!
//! `Character::into_new_game` captures this snapshot at game entry and
//! settles it at the next entry. See the module docs on `GameBaseline` for
//! the exact carry rules.

use crate::{Attributes, Character, traits::Trait};
use serde::{Deserialize, Serialize};

/// One attribute's signed cross-game delta (0 = unchanged).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttributeDelta {
    pub movement: i64,
    pub strength: i64,
    pub toughness: i64,
    pub bravery: i64,
    pub intelligence: i64,
    pub persuasion: i64,
    pub luck: i64,
    pub speed: i64,
    pub perception: i64,
}

/// Durable state captured at game entry, used to settle advancements at the
/// next entry.
///
/// Carry rules, per field family:
/// - **Traits:** the baseline is the floor and the pending
///   [`crate::advancement`] log is the delta: `TraitGained` events are
///   replayed, `TraitLost` events stay removed (an intentional loss is no
///   longer restored as if it were a debuff), and trait changes that never
///   got stamped (legacy rows, pre-log direct pushes) still count as gains.
/// - **Attributes:** the field-wise high-water mark persists. Positive
///   deltas are advancements (e.g. revival's `+1 luck` fallback); negative
///   deltas are per-game debuffs (wound penalties from
///   `lifecycle::health`) and are dropped by the restore.
/// - **Lifetime fight record** (`kills`/`wins`/`defeats`/`draws`):
///   increments persist; per-game record fields (`day_killed`,
///   `killed_by`, `game`) are cleared when the character is rebuilt for
///   the new game.
///
/// Not captured: identity, name, team, clan, avatar, `brain` (re-derived
/// from traits), `ever_addicted_to` (deliberate cross-game carry, see its
/// field docs), and everything per-game (wounds, items, allies, ...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameBaseline {
    pub attributes: Attributes,
    pub traits: Vec<Trait>,
    pub kills: u32,
    pub wins: u32,
    pub defeats: u32,
    pub draws: u32,
    /// Lifetime revivals (additive with a serde default so legacy rows load).
    #[serde(default)]
    pub times_revived: u32,
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
            times_revived: character.statistics.times_revived,
        }
    }

    /// Advance `character`'s durable state to `baseline + earned`, consuming
    /// the pending advancement log: restore the baseline floor, replay the
    /// log's explicit trait intent in order, keep attribute high-water
    /// deltas and record increments as before. The log is taken (not
    /// copied), so the events describe only the game just settled.
    pub fn settle_into(&self, character: &mut Character) {
        use crate::advancement::AdvancementEvent;

        // The delta for the game just played; consumed here.
        let log = std::mem::take(&mut character.advancement_log);
        // Ending trait state, before the floor overwrites it.
        let ending_traits = std::mem::take(&mut character.traits);

        // Attributes: field-wise max — positive deltas persist, debuffs drop.
        let now = &character.attributes;
        let base = &self.attributes;
        character.attributes = Attributes {
            strength: base.strength.max(now.strength),
            toughness: base.toughness.max(now.toughness),
            bravery: base.bravery.max(now.bravery),
            intelligence: base.intelligence.max(now.intelligence),
            persuasion: base.persuasion.max(now.persuasion),
            luck: base.luck.max(now.luck),
            speed: base.speed.max(now.speed),
            perception: base.perception.max(now.perception),
            is_hidden: base.is_hidden,
        };

        // Traits: baseline floor, then explicit intent from the log, then
        // the unstamped-gain fallback. Losses are log-driven ONLY — that is
        // the fix: `lose_trait`'s stamp keeps an intentional loss gone,
        // where snapshot-diffing used to restore it. A logged loss wins even
        // over a direct push of the same trait (the sanctioned path for
        // re-acquiring is `gain_trait`, which stamps its own gain after).
        let mut traits = self.traits.clone();
        for event in &log {
            match event {
                AdvancementEvent::TraitGained { trait_ } => {
                    if !traits.contains(trait_) {
                        traits.push(*trait_);
                    }
                }
                AdvancementEvent::TraitLost { trait_ } => {
                    traits.retain(|t| t != trait_);
                }
                // Recorded, consumed, NOT replayed: XP purchases apply
                // their state effect at purchase time (spec §10.9); the
                // durable effect rides the attribute high-water rule.
                AdvancementEvent::XpPurchase { .. } => {}
            }
        }
        for t in &ending_traits {
            if !traits.contains(t) && !self.traits.contains(t) {
                // Unstamped gain (legacy row or a pre-log direct push,
                // e.g. revival's scar/bonus paths): still an advancement.
                traits.push(*t);
            }
        }
        character.traits = traits;

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
        character.statistics.times_revived = self.times_revived.saturating_add(
            character
                .statistics
                .times_revived
                .saturating_sub(self.times_revived),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Trait;

    fn clean_goblin(name: &str) -> Character {
        let mut c = Character::new(name.to_string(), None, None);
        c.traits.clear();
        c.attributes.speed = 36;
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
        c.attributes.speed = 0; // floor case

        let mut restored = c.clone();
        baseline.settle_into(&mut restored);

        assert_eq!(restored.attributes.luck, baseline.attributes.luck + 1);
        assert_eq!(restored.attributes.strength, baseline.attributes.strength);
        assert_eq!(restored.attributes.speed, baseline.attributes.speed);
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
    fn settle_keeps_revival_count_increments() {
        let mut c = clean_goblin("Boblin");
        let baseline = GameBaseline::capture(&c);

        c.statistics.times_revived += 1;

        let mut restored = c.clone();
        baseline.settle_into(&mut restored);
        assert_eq!(
            restored.statistics.times_revived,
            baseline.times_revived + 1
        );
    }

    #[test]
    fn revival_count_survives_into_new_game() {
        // Entry seam: settle carries the increment, then the new baseline
        // captures the carried total (same seam as evolution settle).
        let mut c = clean_goblin("Boblin");
        c.statistics.times_revived = 1;

        let entered = c.into_new_game();
        assert_eq!(entered.statistics.times_revived, 1);
        assert_eq!(
            entered
                .game_baseline
                .as_ref()
                .expect("entry captures a baseline")
                .times_revived,
            1
        );
    }

    #[test]
    fn settle_honors_intentional_trait_loss() {
        // The regression the log exists for: a trait lost on purpose mid-game
        // used to be restored by snapshot-diffing, because a diff cannot tell
        // "lost intentionally" from "was never there".
        let mut c = clean_goblin("Boblin");
        c.gain_trait(Trait::Friendly).expect("friendly");
        let baseline = GameBaseline::capture(&c);

        assert!(c.lose_trait(Trait::Friendly));
        assert!(c.advancement_log.len() >= 2, "gain and loss both stamped");

        let mut restored = c.clone();
        baseline.settle_into(&mut restored);
        assert!(
            !restored.traits.contains(&Trait::Friendly),
            "intentional loss must survive settle"
        );
    }

    #[test]
    fn settle_replays_logged_gain_events() {
        // A logged gain replays even when the ending state no longer carries
        // the trait (e.g. a legacy path removed it without stamping a loss):
        // the log, not the diff, is the delta.
        let mut c = clean_goblin("Boblin");
        let baseline = GameBaseline::capture(&c);

        c.gain_trait(Trait::Resilient).expect("resilient");
        c.traits.retain(|t| *t != Trait::Resilient); // unstamped removal

        let mut restored = c.clone();
        baseline.settle_into(&mut restored);
        assert!(restored.traits.contains(&Trait::Resilient));
    }

    #[test]
    fn settle_consumes_the_advancement_log() {
        let mut c = clean_goblin("Boblin");
        let baseline = GameBaseline::capture(&c);

        c.gain_trait(Trait::Resilient).expect("resilient");
        assert!(!c.advancement_log.is_empty());

        let mut restored = c.clone();
        baseline.settle_into(&mut restored);
        assert!(
            restored.advancement_log.is_empty(),
            "settle consumes the events — they described only the settled game"
        );
    }

    #[test]
    fn repeated_settles_do_not_double_count() {
        let mut c = clean_goblin("Boblin");
        c.gain_trait(Trait::Tough).expect("tough");
        let baseline = GameBaseline::capture(&c);

        c.gain_trait(Trait::Resilient).expect("resilient");
        c.lose_trait(Trait::Tough);
        c.statistics.kills += 1;

        let mut restored = c.clone();
        baseline.settle_into(&mut restored);
        let after_first = restored.clone();

        // Second game, no further advancements: settle again off the fresh
        // floor — nothing may grow or resurrect.
        let baseline2 = GameBaseline::capture(&after_first);
        let mut settled = after_first.clone();
        baseline2.settle_into(&mut settled);

        assert_eq!(settled.traits, after_first.traits);
        assert_eq!(settled.statistics.kills, after_first.statistics.kills);
        assert!(settled.advancement_log.is_empty());
    }

    #[test]
    fn xp_purchase_event_slot_is_recorded_and_consumed_without_replay() {
        // Slot for the XP system (e1d): recorded for the durable-change
        // record, consumed at settle, replays nothing — purchases apply
        // immediately (spec §10.9), and the durable effect rides the
        // attribute high-water rule below.
        use crate::advancement::{AdvancementEvent, push_event};

        let mut c = clean_goblin("Boblin");
        let baseline = GameBaseline::capture(&c);

        push_event(
            &mut c.advancement_log,
            AdvancementEvent::XpPurchase {
                detail: "+1 luck".into(),
                amount: 10,
            },
        );
        c.attributes.luck = baseline.attributes.luck + 1; // applied at purchase

        let mut restored = c.clone();
        baseline.settle_into(&mut restored);

        assert!(restored.advancement_log.is_empty());
        assert_eq!(restored.attributes.luck, baseline.attributes.luck + 1);
        assert!(restored.traits.is_empty(), "nothing to replay");
    }

    #[test]
    fn advancement_log_serde_roundtrips_on_character() {
        use crate::advancement::AdvancementEvent;

        let mut c = clean_goblin("Boblin");
        c.advancement_log.push(AdvancementEvent::TraitGained {
            trait_: Trait::Resilient,
        });

        let json = serde_json::to_string(&c).expect("serialize");
        let back: Character = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.advancement_log, c.advancement_log);

        // Legacy rows without the field deserialize to an empty log.
        let mut value: serde_json::Value = serde_json::from_str(&json).expect("value");
        value
            .as_object_mut()
            .expect("object")
            .remove("advancement_log");
        let legacy: Character = serde_json::from_value(value).expect("legacy");
        assert!(legacy.advancement_log.is_empty());
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
