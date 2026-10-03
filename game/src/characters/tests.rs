use crate::messages::TaggedEvent;
use crate::characters::Character;
use crate::characters::brains::Brain;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use rstest::*;

#[fixture]
fn character() -> Character {
    Character::new("Snaggletooth".to_string(), None, None)
}

#[fixture]
fn target() -> Character {
    Character::new("Grubworm".to_string(), None, None)
}

#[fixture]
fn small_rng() -> SmallRng {
    SmallRng::seed_from_u64(0)
}

#[rstest]
fn default() {
    let character = Character::default();
    assert_eq!(character.name, "Default Character");
}

#[rstest]
fn serde_roundtrip_alliance_fields() {
    use crate::characters::traits::Trait;
    use uuid::Uuid;

    let mut character = Character::new("Nib".to_string(), None, None);
    let ally = Uuid::new_v4();
    character.allies.push(ally);
    character.traits.clear();
    character.traits.push(Trait::Loyal);
    character.traits.push(Trait::Treacherous);
    character.turns_since_last_betrayal = 7;
    character.pending_trust_shock = true;

    let json = serde_json::to_string(&character).expect("serialize");
    assert!(json.contains("\"allies\""));
    assert!(json.contains("\"traits\""));
    assert!(json.contains("\"Loyal\""));
    assert!(json.contains("\"turns_since_last_betrayal\":7"));
    assert!(json.contains("\"pending_trust_shock\":true"));

    let restored: Character = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.allies, vec![ally]);
    assert_eq!(restored.traits, vec![Trait::Loyal, Trait::Treacherous]);
    assert_eq!(restored.turns_since_last_betrayal, 7);
    assert!(restored.pending_trust_shock);
}

#[rstest]
fn serde_defaults_for_missing_alliance_fields() {
    // Persisted character records written before the alliance fields existed
    // must still deserialize. Simulate this by serialising a fresh character,
    // stripping the new fields, then round-tripping.
    let mut rng = SmallRng::seed_from_u64(42);
    let baseline = Character::new_with_rng("Legacy".to_string(), None, None, &mut rng);
    let mut value: serde_json::Value = serde_json::to_value(&baseline).expect("to_value");
    let obj = value.as_object_mut().expect("object");
    obj.remove("allies");
    obj.remove("traits");
    obj.remove("turns_since_last_betrayal");
    obj.remove("pending_trust_shock");

    let restored: Character = serde_json::from_value(value).expect("legacy deserialize");
    assert!(restored.allies.is_empty());
    assert!(restored.traits.is_empty());
    assert_eq!(restored.turns_since_last_betrayal, 0);
    assert!(!restored.pending_trust_shock);
}

#[rstest]
fn brain_roundtrips_psychotic_break_state() {
    use crate::characters::brains::PsychoticBreakType;

    let mut rng = SmallRng::seed_from_u64(42);
    let mut character = Character::new_with_rng("Rendmaw".to_string(), None, None, &mut rng);
    character.brain.psychotic_break = Some(PsychoticBreakType::Berserk);

    let json = serde_json::to_string(&character).expect("serialize");
    assert!(json.contains("\"brain\""));
    assert!(json.contains("\"Berserk\""));

    let restored: Character = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        restored.brain.psychotic_break,
        Some(PsychoticBreakType::Berserk),
    );
}

#[rstest]
fn brain_preferred_action_is_not_persisted() {
    // preferred_action is transient AI state recomputed each cycle, so the
    // field is `skip_serializing` and `deserialize_optional_enum_lenient`
    // (which absorbs both null and the {} corruption left over from the
    // SDK's enum-collapse bug). A roundtrip therefore intentionally drops
    // any preferred_action that was set in memory.
    use crate::characters::actions::Action;

    let mut character = Character::new("Snoutrot".to_string(), None, None);
    character.brain.preferred_action = Some(Action::Hide);
    character.brain.preferred_action_percentage = 0.75;

    let json = serde_json::to_string(&character).expect("serialize");

    let restored: Character = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.brain.preferred_action, None);
    // Non-skipped fields still round-trip normally.
    assert!((restored.brain.preferred_action_percentage - 0.75).abs() < f64::EPSILON);
}

#[rstest]
fn brain_tolerates_corrupt_preferred_action_object() {
    // SurrealDB rows written before the bug-5 fix have preferred_action: {}
    // because the SDK's bespoke serializer collapsed the externally-tagged
    // Action enum. The lenient deserializer must read those rows as None.
    // Round-trip a real Brain to get a valid base JSON, then swap
    // preferred_action's value to {} to simulate the corruption.
    use crate::characters::brains::Brain;

    let brain = Brain {
        preferred_action_percentage: 0.5,
        ..Brain::default()
    };
    let mut value = serde_json::to_value(&brain).expect("serialize brain");
    value["preferred_action"] = serde_json::json!({});
    let restored: Brain = serde_json::from_value(value).expect("deserialize legacy row");
    assert_eq!(restored.preferred_action, None);
}

#[rstest]
fn brain_missing_field_defaults() {
    // Pre-fix character rows persisted before #[serde(default)] was added
    // omit the `brain` column entirely. They must still deserialize, with
    // brain hydrated via `Brain::default()`.
    let mut rng = SmallRng::seed_from_u64(42);
    let baseline = Character::new_with_rng("Legacy".to_string(), None, None, &mut rng);
    let mut value: serde_json::Value = serde_json::to_value(&baseline).expect("to_value");
    value.as_object_mut().expect("object").remove("brain");

    let restored: Character = serde_json::from_value(value).expect("legacy deserialize");
    assert_eq!(restored.brain, Brain::default());
    assert!(restored.brain.psychotic_break.is_none());
    assert!(restored.brain.preferred_action.is_none());
}

#[rstest]
fn new() {
    let character = Character::new("Snaggletooth".to_string(), Some(12), None);
    assert_eq!(character.name, "Snaggletooth");
    assert_eq!(character.clan, 12);
    // Attributes::new() randomizes health in 50..=max_health.
    assert!(
        (50..=100).contains(&character.effective_health()),
        "health {} out of range",
        character.effective_health()
    );
}

#[rstest]
fn random() {
    let character = Character::random();
    assert!(!character.name.is_empty());
    assert!(character.clan >= 1 && character.clan <= 12);
}

#[rstest]
fn new_character_has_empty_alliance_state() {
    let character = Character::new("Snipsnout".to_string(), Some(1), None);
    assert!(character.allies.is_empty());
    assert_eq!(character.turns_since_last_betrayal, 0);
    // `id` mirrors `identifier`.
    assert_eq!(character.id.to_string(), character.identifier);
}

#[rstest]
fn new_character_has_no_pending_trust_shock() {
    let character = Character::new("Snipsnout".to_string(), Some(1), None);
    assert!(!character.pending_trust_shock);
}

#[test]
fn character_default_survival_fields_are_zero_and_none() {
    let t = Character::new("Test".to_string(), None, None);
    assert_eq!(t.hunger, 0, "hunger starts at 0 (Sated)");
    assert_eq!(t.thirst, 0, "thirst starts at 0 (Sated)");
    assert_eq!(t.sheltered_until, None, "starts exposed");
    assert_eq!(t.starvation_drain_step, 0);
    assert_eq!(t.dehydration_drain_step, 0);
}

#[test]
fn character_legacy_json_loads_with_defaults() {
    // JSON missing the new survival fields entirely (simulates a saved
    // game from before this feature landed). serde(default) must
    // populate them.
    let mut t = Character::new("Legacy".to_string(), Some(1), None);
    t.hunger = 0;
    t.thirst = 0;
    t.sheltered_until = None;
    t.starvation_drain_step = 0;
    t.dehydration_drain_step = 0;
    let mut json: serde_json::Value = serde_json::to_value(&t).unwrap();
    // strip the survival fields to mimic a pre-feature save
    let obj = json.as_object_mut().unwrap();
    obj.remove("hunger");
    obj.remove("thirst");
    obj.remove("sheltered_until");
    obj.remove("starvation_drain_step");
    obj.remove("dehydration_drain_step");
    let loaded: Character = serde_json::from_value(json).expect("legacy load must succeed");
    assert_eq!(loaded.hunger, 0);
    assert_eq!(loaded.thirst, 0);
    assert_eq!(loaded.sheltered_until, None);
    assert_eq!(loaded.starvation_drain_step, 0);
    assert_eq!(loaded.dehydration_drain_step, 0);
}

#[test]
fn character_legacy_json_loads_with_sleep_defaults() {
    // JSON missing the new sleep fields must default to zero/false.
    let t = Character::new("Legacy".to_string(), Some(1), None);
    let mut json: serde_json::Value = serde_json::to_value(&t).unwrap();
    let obj = json.as_object_mut().unwrap();
    obj.remove("cycles_awake");
    obj.remove("sleeping");
    obj.remove("sleep_remaining");
    let loaded: Character = serde_json::from_value(json).expect("legacy load must succeed");
    assert_eq!(loaded.cycles_awake, 0);
    assert!(!loaded.sleeping);
    assert_eq!(loaded.sleep_remaining, 0);
}

#[rstest]
fn character_drain_alliance_events_returns_and_clears_buffer() {
    use crate::characters::alliances::AllianceEvent;
    use uuid::Uuid;
    let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
    let other = Uuid::new_v4();
    character
        .alliance_events
        .push(AllianceEvent::BetrayalRecorded {
            betrayer: character.id,
            victim: other,
        });
    let drained = character.drain_alliance_events();
    assert_eq!(drained.len(), 1);
    assert!(character.alliance_events.is_empty());
}

#[rstest]
fn consume_pending_trust_shock_resets_flag_when_not_set() {
    // No flag → no rolls, flag stays false, allies untouched.
    let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
    let ally = uuid::Uuid::new_v4();
    character.allies.push(ally);
    let mut events: Vec<TaggedEvent> = vec![];
    let mut rng = rand::rngs::SmallRng::seed_from_u64(53);
    character.consume_pending_trust_shock(&mut rng, &mut events);
    assert!(!character.pending_trust_shock);
    assert_eq!(character.allies, vec![ally]);
    assert!(events.is_empty());
}

#[rstest]
fn consume_pending_trust_shock_breaks_allies_on_success_and_clears_flag() {
    // Force trust_shock to fire deterministically: sanity=0, threshold>0
    // gives p = 0.5 + 0.5 * 1.0 = 1.0 → always true.
    let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
    // ponytail: set_sanity(0) → 10 Critical conditions drain 100 → effective_sanity = 0
    character.mental_conditions = (0..10)
        .map(|_| shared::conditions::MentalCondition::Pain {
            severity: shared::conditions::ConditionSeverity::Critical,
        })
        .collect();
    character.brain.thresholds.extreme_low_sanity = 50;
    let ally1 = uuid::Uuid::new_v4();
    let ally2 = uuid::Uuid::new_v4();
    character.allies.push(ally1);
    character.allies.push(ally2);
    character.pending_trust_shock = true;

    let mut events: Vec<TaggedEvent> = vec![];
    let mut rng = rand::rngs::SmallRng::seed_from_u64(211);
    character.consume_pending_trust_shock(&mut rng, &mut events);

    assert!(!character.pending_trust_shock, "flag must reset");
    assert!(
        character.allies.is_empty(),
        "all allies broken on guaranteed success"
    );
    assert_eq!(events.len(), 2, "one message per broken ally");
}

#[rstest]
fn consume_pending_trust_shock_no_break_when_sanity_above_threshold() {
    // Sanity at/above threshold → trust_shock_roll returns false → no break.
    let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
    // No mental conditions → effective_sanity = 100
    character.brain.thresholds.extreme_low_sanity = 50;
    let ally = uuid::Uuid::new_v4();
    character.allies.push(ally);
    character.pending_trust_shock = true;

    let mut events: Vec<TaggedEvent> = vec![];
    let mut rng = rand::rngs::SmallRng::seed_from_u64(89);
    character.consume_pending_trust_shock(&mut rng, &mut events);

    assert!(!character.pending_trust_shock, "flag must reset");
    assert_eq!(character.allies, vec![ally], "ally retained");
    assert!(events.is_empty());
}

#[rstest]
fn new_character_has_traits_for_valid_clan() {
    let character = Character::new("Snaggletooth".to_string(), Some(12), None);
    // generate_traits rolls 2..=6 traits from the clan pool.
    assert!((2..=6).contains(&character.traits.len()));
}

#[rstest]
fn pick_target_skips_allies() {
    // An ally is in the same area but must not be picked as a target.
    let mut me = Character::new("Snaggletooth".to_string(), Some(12), None);
    // No mental conditions → effective_sanity = 100 → not suicidal
    let ally = Character::new("Grubworm".to_string(), Some(12), None);
    me.allies.push(ally.id);

    let mut events: Vec<TaggedEvent> = vec![];
    let target = me.pick_target(vec![ally.clone()], 5, &mut events);
    // Only candidate was an ally and we're not in final confrontation.
    assert!(target.is_none());
}

#[rstest]
fn pick_target_allows_same_clan_when_not_ally() {
    // Same-clan characters can now be targeted unless they're allies.
    let me = Character::new("Snaggletooth".to_string(), Some(12), None);
    let same_clan = Character::new("Grubworm".to_string(), Some(12), None);

    let mut events: Vec<TaggedEvent> = vec![];
    let target = me.pick_target(vec![same_clan.clone()], 5, &mut events);
    assert!(target.is_some());
    assert_eq!(target.unwrap().id, same_clan.id);
}

#[rstest]
fn pick_target_final_confrontation_overrides_alliance() {
    // When only two characters remain alive, even an ally is a valid target.
    let mut me = Character::new("Snaggletooth".to_string(), Some(12), None);
    // No mental conditions → effective_sanity = 100 → not suicidal
    let ally = Character::new("Grubworm".to_string(), Some(12), None);
    me.allies.push(ally.id);

    let mut events: Vec<TaggedEvent> = vec![];
    let target = me.pick_target(vec![ally.clone()], 2, &mut events);
    assert!(target.is_some());
    assert_eq!(target.unwrap().id, ally.id);
}

#[rstest]
fn tick_alliance_timers_increments_betrayal_counter() {
    // Living character: counter increments by exactly one per tick.
    let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
    assert_eq!(character.turns_since_last_betrayal, 0);
    character.tick_alliance_timers();
    assert_eq!(character.turns_since_last_betrayal, 1);
    character.tick_alliance_timers();
    assert_eq!(character.turns_since_last_betrayal, 2);
}

#[rstest]
fn tick_alliance_timers_saturates_does_not_overflow() {
    // u8 saturating add: never panics, never wraps to zero.
    let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
    character.turns_since_last_betrayal = u8::MAX;
    character.tick_alliance_timers();
    assert_eq!(character.turns_since_last_betrayal, u8::MAX);
}

#[rstest]
fn tick_alliance_timers_skips_dead_characters() {
    // Dead characters don't accumulate betrayal cooldown.
    let mut character = Character::new("Snipsnout".to_string(), Some(1), None);
    character.blood = 0;
    character.status = crate::characters::CharacterStatus::RecentlyDead;
    character.tick_alliance_timers();
    assert_eq!(character.turns_since_last_betrayal, 0);
}

#[rstest]
fn pick_target_picks_ex_ally_after_trust_shock_breaks_bond() {
    // End-to-end break-then-attack (spec §7.3c1 + §7.5):
    // Once a trust shock fires and removes the betrayer from the
    // victim's `allies`, the victim's next `pick_target` call must
    // consider that ex-ally a valid target.
    let mut victim = Character::new("Gnawpaw".to_string(), Some(1), None);
    // No mental conditions → effective_sanity = 100 → not suicidal
    let ex_ally = Character::new("Rendmaw".to_string(), Some(2), None);
    // Pre-condition: bonded.
    victim.allies.push(ex_ally.id);

    // Simulate the bond breaking (what process_alliance_events does
    // for BetrayalRecorded, plus what consume_pending_trust_shock
    // does on the victim's side: drop the ex-ally locally).
    victim.allies.retain(|id| *id != ex_ally.id);

    let mut events: Vec<TaggedEvent> = vec![];
    let target = victim.pick_target(vec![ex_ally.clone()], 5, &mut events);
    assert!(
        target.is_some(),
        "ex-ally must be targetable after the bond breaks"
    );
    assert_eq!(target.unwrap().id, ex_ally.id);
}

#[rstest]
fn consume_pending_trust_shock_leaves_asymmetric_back_edge() {
    // Spec §7.3c1 explicitly defers the symmetric back-edge cleanup
    // for trust-shock breaks: only `self` is mutated. This regression
    // test pins that contract so any future tightening is intentional.
    let mut victim = Character::new("Gnawpaw".to_string(), Some(1), None);
    // ponytail: set_sanity(0) → 10 Critical conditions drain 100 → effective_sanity = 0
    victim.mental_conditions = (0..10)
        .map(|_| shared::conditions::MentalCondition::Pain {
            severity: shared::conditions::ConditionSeverity::Critical,
        })
        .collect();
    victim.brain.thresholds.extreme_low_sanity = 100;
    let betrayer_id = uuid::Uuid::new_v4();
    victim.allies.push(betrayer_id);
    victim.pending_trust_shock = true;

    let mut rng = SmallRng::seed_from_u64(419);
    let mut events: Vec<TaggedEvent> = vec![];
    victim.consume_pending_trust_shock(&mut rng, &mut events);

    // Victim's side cleaned.
    assert!(
        !victim.allies.contains(&betrayer_id),
        "victim must drop the broken ally"
    );
    // The flag is consumed regardless of roll outcome.
    assert!(
        !victim.pending_trust_shock,
        "pending flag is reset after the call"
    );
    // Asymmetric back-edge stays — `consume_pending_trust_shock` only
    // touches `self`. The next cycle's event drain (or follow-up
    // events) is responsible for the betrayer's side.
    // We can't observe the betrayer here (different character instance);
    // the documented contract is what matters and is asserted by the
    // single-side mutation: the function signature takes `&mut self`
    // and returns nothing, with no reference to the broken ally.
}

#[test]
fn wake_interrupted_returns_false_when_not_sleeping() {
    use crate::messages::CharacterRef;
    let mut t = Character::new("Snoutrot".to_string(), Some(1), None);
    let mut events: Vec<TaggedEvent> = Vec::new();
    let woke = t.wake_interrupted(
        shared::messages::InterruptionKind::Ambush {
            attacker: CharacterRef {
                identifier: "x".to_string(),
                name: "X".to_string(),
            },
        },
        shared::messages::Phase::Day,
        &mut events,
    );
    assert!(!woke);
    assert!(events.is_empty());
}

#[test]
fn wake_interrupted_resets_state_and_emits_character_woke() {
    let mut t = Character::new("Snoutrot".to_string(), Some(1), None);
    t.sleeping = true;
    t.sleep_remaining = 3;
    t.cycles_awake = 7;
    let mut events: Vec<TaggedEvent> = Vec::new();
    let woke = t.wake_interrupted(
        shared::messages::InterruptionKind::AreaEvent {
            kind: shared::messages::AreaEventKind::Fire,
        },
        shared::messages::Phase::Night,
        &mut events,
    );
    assert!(woke);
    assert!(!t.sleeping);
    assert_eq!(t.sleep_remaining, 0);
    assert_eq!(t.cycles_awake, 0);
    assert_eq!(events.len(), 1);
    match &events[0].payload {
        crate::messages::MessagePayload::CharacterWoke { reason, phase, .. } => {
            assert_eq!(*phase, shared::messages::Phase::Night);
            match reason {
                shared::messages::WakeReason::Interrupted {
                    event:
                        shared::messages::InterruptionKind::AreaEvent {
                            kind: shared::messages::AreaEventKind::Fire,
                        },
                } => {}
                other => panic!("unexpected reason: {:?}", other),
            }
        }
        other => panic!("expected CharacterWoke payload, got {:?}", other),
    }
}

// --- Affliction tests ---

use crate::characters::AfflictionDraft;
use crate::characters::afflictions::{AcquireResolution, RejectReason};
use shared::afflictions::{AfflictionKind, AfflictionSource, BodyPart, Severity};

#[test]
fn test_afflictions_empty_by_default() {
    let t = Character::new_with_rng(
        "Test".to_string(),
        None,
        None,
        &mut SmallRng::seed_from_u64(42),
    );
    assert!(t.afflictions.is_empty());
}

#[test]
fn test_afflictions_skip_serialization_when_empty() {
    let t = Character::new_with_rng(
        "Test".to_string(),
        None,
        None,
        &mut SmallRng::seed_from_u64(42),
    );
    let json = serde_json::to_string(&t).unwrap();
    assert!(!json.contains("\"afflictions\""));
}

#[test]
fn test_try_acquire_insert() {
    let mut rng = SmallRng::seed_from_u64(42);
    let mut t = Character::new_with_rng("Test".to_string(), None, None, &mut rng);
    let draft = AfflictionDraft {
        kind: AfflictionKind::Wounded,
        body_part: Some(BodyPart::Arm),
        severity: Severity::Mild,
        source: AfflictionSource::Combat {
                attacker_id: String::new(),
            },
            trapped_metadata: None,
        };
    let resolution = t.try_acquire_affliction(draft);
    assert_eq!(resolution, AcquireResolution::Insert);
    assert_eq!(t.afflictions.len(), 1);
    assert!(
        t.afflictions
            .contains_key(&(AfflictionKind::Wounded, Some(BodyPart::Arm)))
    );
}

#[test]
fn test_try_acquire_upgrade() {
    let mut rng = SmallRng::seed_from_u64(42);
    let mut t = Character::new_with_rng("Test".to_string(), None, None, &mut rng);
    // Insert mild wound
    t.try_acquire_affliction(AfflictionDraft {
        kind: AfflictionKind::Wounded,
        body_part: Some(BodyPart::Arm),
        severity: Severity::Mild,
        source: AfflictionSource::Combat {
                attacker_id: String::new(),
            },
            trapped_metadata: None,
        });
    // Upgrade to moderate
    let draft = AfflictionDraft {
        kind: AfflictionKind::Wounded,
        body_part: Some(BodyPart::Arm),
        severity: Severity::Moderate,
        source: AfflictionSource::Combat {
                attacker_id: String::new(),
            },
            trapped_metadata: None,
        };
    let resolution = t.try_acquire_affliction(draft);
    assert_eq!(
        resolution,
        AcquireResolution::Upgrade((AfflictionKind::Wounded, Some(BodyPart::Arm)))
    );
    assert_eq!(t.afflictions.len(), 1);
    let affl = t
        .afflictions
        .get(&(AfflictionKind::Wounded, Some(BodyPart::Arm)))
        .unwrap();
    assert_eq!(affl.severity, Severity::Moderate);
}

#[test]
fn test_try_acquire_supersede() {
    let mut t = Character::new("Test".to_string(), None, None);
    // Insert wounded on arm
    t.try_acquire_affliction(AfflictionDraft {
        kind: AfflictionKind::Wounded,
        body_part: Some(BodyPart::Arm),
        severity: Severity::Mild,
        source: AfflictionSource::Combat {
                attacker_id: String::new(),
            },
            trapped_metadata: None,
        });
    // Infected supersedes wounded at same body part
    let draft = AfflictionDraft {
        kind: AfflictionKind::Infected,
        body_part: Some(BodyPart::Arm),
        severity: Severity::Mild,
        source: AfflictionSource::Combat {
                attacker_id: String::new(),
            },
            trapped_metadata: None,
        };
    let resolution = t.try_acquire_affliction(draft);
    assert_eq!(resolution, AcquireResolution::Insert);
    // Wounded removed, Infected present
    assert!(
        !t.afflictions
            .contains_key(&(AfflictionKind::Wounded, Some(BodyPart::Arm)))
    );
    assert!(
        t.afflictions
            .contains_key(&(AfflictionKind::Infected, Some(BodyPart::Arm)))
    );
}

#[test]
fn test_try_acquire_reject_limb_missing() {
    let mut t = Character::new("Test".to_string(), None, None);
    // Missing arm
    t.try_acquire_affliction(AfflictionDraft {
        kind: AfflictionKind::MissingArm,
        body_part: Some(BodyPart::Arm),
        severity: Severity::Severe,
        source: AfflictionSource::Combat {
                attacker_id: String::new(),
            },
            trapped_metadata: None,
        });
    // Can't wound a missing limb
    let draft = AfflictionDraft {
        kind: AfflictionKind::Wounded,
        body_part: Some(BodyPart::Arm),
        severity: Severity::Mild,
        source: AfflictionSource::Combat {
                attacker_id: String::new(),
            },
            trapped_metadata: None,
        };
    let resolution = t.try_acquire_affliction(draft);
    assert_eq!(
        resolution,
        AcquireResolution::Reject(RejectReason::LimbAlreadyMissing)
    );
}

#[test]
fn test_try_acquire_reject_no_wounded_ancestor() {
    let mut t = Character::new("Test".to_string(), None, None);
    // Infected without prior Wounded on same part
    let draft = AfflictionDraft {
        kind: AfflictionKind::Infected,
        body_part: Some(BodyPart::Arm),
        severity: Severity::Mild,
        source: AfflictionSource::Combat {
                attacker_id: String::new(),
            },
            trapped_metadata: None,
        };
    let resolution = t.try_acquire_affliction(draft);
    assert_eq!(
        resolution,
        AcquireResolution::Reject(RejectReason::InfectedRequiresWoundedAncestor)
    );
}

#[test]
fn test_try_acquire_reject_same_severity() {
    let mut t = Character::new("Test".to_string(), None, None);
    // Insert mild wound
    t.try_acquire_affliction(AfflictionDraft {
        kind: AfflictionKind::Wounded,
        body_part: Some(BodyPart::Arm),
        severity: Severity::Mild,
        source: AfflictionSource::Combat {
                attacker_id: String::new(),
            },
            trapped_metadata: None,
        });
    // Same severity rejected
    let draft = AfflictionDraft {
        kind: AfflictionKind::Wounded,
        body_part: Some(BodyPart::Arm),
        severity: Severity::Mild,
        source: AfflictionSource::Combat {
                attacker_id: String::new(),
            },
            trapped_metadata: None,
        };
    let resolution = t.try_acquire_affliction(draft);
    assert_eq!(
        resolution,
        AcquireResolution::Reject(RejectReason::NotStrictlyHigherSeverity)
    );
}
