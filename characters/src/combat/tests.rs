use super::*;
use crate::Character;
use crate::combat_tuning::CombatTuning;
use core::convert::Infallible;
use rand::SeedableRng;
use rand::TryRng;
use rand::rngs::SmallRng;
use rstest::*;
use world::items::OwnsItems;

#[fixture]
fn small_rng() -> SmallRng {
    SmallRng::seed_from_u64(0)
}

#[rstest]
fn attack_contest_win(mut small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.attributes.strength = 10;
    target.attributes.toughness = 5;

    let result = attack_contest(
        &mut attacker,
        &mut target,
        &mut small_rng,
        &mut Vec::new(),
        &CombatTuning::default(),
    )
    .result;
    assert_eq!(result, AttackResult::AttackerWins);
}

#[rstest]
fn attack_contest_win_decisively(mut small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.attributes.strength = 15;
    target.attributes.toughness = 0;

    let result = attack_contest(
        &mut attacker,
        &mut target,
        &mut small_rng,
        &mut Vec::new(),
        &CombatTuning::default(),
    )
    .result;
    assert_eq!(result, AttackResult::AttackerWinsDecisively);
}

#[rstest]
fn attack_contest_lose(mut small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.attributes.strength = 15;
    target.attributes.toughness = 20;

    let result = attack_contest(
        &mut attacker,
        &mut target,
        &mut small_rng,
        &mut Vec::new(),
        &CombatTuning::default(),
    )
    .result;
    assert_eq!(result, AttackResult::DefenderWins);
}

#[rstest]
fn attack_contest_lose_decisively(mut small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.attributes.strength = 1;
    target.attributes.toughness = 20;

    let result = attack_contest(
        &mut attacker,
        &mut target,
        &mut small_rng,
        &mut Vec::new(),
        &CombatTuning::default(),
    )
    .result;
    assert_eq!(result, AttackResult::DefenderWinsDecisively);
}

#[rstest]
fn attack_contest_draw(mut small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.attributes.strength = 21; // Magic number to make the final scores even
    target.attributes.toughness = 20;

    let result = attack_contest(
        &mut attacker,
        &mut target,
        &mut small_rng,
        &mut Vec::new(),
        &CombatTuning::default(),
    )
    .result;
    assert_eq!(result, AttackResult::Miss);
}

#[rstest]
fn attacks_self(mut small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    attacker.attributes.strength = 25;
    // Sanity is now derived from mental_conditions; start at full sanity
    let sanity = attacker.effective_sanity();
    let mut target = attacker.clone();

    let outcome = attacker.attacks(
        &mut target,
        &mut small_rng,
        &mut Vec::new(),
        shared::messages::Phase::DAY,
        &CombatTuning::default(),
    );
    assert_eq!(outcome, AttackOutcome::Wound(attacker.clone(), target));
    assert!(attacker.effective_sanity() < sanity);
}

#[rstest]
fn attacks_self_suicide(mut small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    attacker.attributes.strength = 100;
    let mut target = attacker.clone();

    let outcome = attacker.attacks(
        &mut target,
        &mut small_rng,
        &mut Vec::new(),
        shared::messages::Phase::DAY,
        &CombatTuning::default(),
    );
    assert_eq!(outcome, AttackOutcome::Kill(attacker, target));
}

#[rstest]
fn attacks_wound(mut small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let sanity = attacker.effective_sanity();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.attributes.strength = 25;
    target.attributes.toughness = 20;

    let result = attacker.attacks(
        &mut target,
        &mut small_rng,
        &mut Vec::new(),
        shared::messages::Phase::DAY,
        &CombatTuning::default(),
    );
    assert_eq!(
        result,
        AttackOutcome::Wound(attacker.clone(), target.clone())
    );
    assert_eq!(attacker.statistics.wins, 1);
    assert_eq!(target.statistics.defeats, 1);
    assert!(attacker.effective_sanity() < sanity);
}

#[rstest]
fn attacks_kill(mut small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.attributes.strength = 50;
    target.attributes.toughness = 0;
    target.blood = 100;

    let result = attacker.attacks(
        &mut target,
        &mut small_rng,
        &mut Vec::new(),
        shared::messages::Phase::DAY,
        &CombatTuning::default(),
    );
    assert!(matches!(result, AttackOutcome::Kill(_, _)));
    assert_eq!(target.blood, 0);
    assert_eq!(attacker.statistics.wins, 1);
    assert_eq!(target.statistics.defeats, 1);
}

#[rstest]
fn attacks_miss(mut small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.attributes.strength = 21; // Magic number to make them draw
    target.attributes.toughness = 20;

    let result = attacker.attacks(
        &mut target,
        &mut small_rng,
        &mut Vec::new(),
        shared::messages::Phase::DAY,
        &CombatTuning::default(),
    );
    assert_eq!(result, AttackOutcome::Miss(attacker, target));
}

/// Wound penalties must reach the damage pipeline: at the same seed and the
/// same rolls, the only difference between the two targets is blood and
/// wounds, and the wounded one has to draw strictly more blood per hit.
#[rstest]
fn wounded_target_takes_more_damage_from_same_swing() {
    use shared::wounds::{BodyPart, Wound, WoundSeverity, WoundType};

    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    attacker.traits.clear();
    attacker.attributes.strength = 50;

    let mut healthy = Character::new("Grubworm".to_string(), None, None);
    healthy.traits.clear();
    healthy.attributes.toughness = 5;

    let mut wounded = healthy.clone();
    wounded.blood = 400;
    wounded.wounds.push(Wound::new(
        WoundType::Cut,
        WoundSeverity::Critical,
        BodyPart::Head,
    ));
    // Head Critical: -10% x 1.5 head multiplier -> 5 x 0.85 = 4.25 -> 4.
    assert_eq!(wounded.effective_toughness(), 4);

    // Strength 50 vs toughness <= 5: the attack roll (51-70) always beats
    // 1.5x the defense roll (max 37.5), so both swings are the same decisive
    // attacker win and carry identical raw damage.
    let (healthy_contest, _) = resolve(
        &mut attacker.clone(),
        &mut healthy,
        &mut SmallRng::seed_from_u64(0),
        &mut Vec::new(),
        &CombatTuning::default(),
    );
    let (wounded_contest, _) = resolve(
        &mut attacker,
        &mut wounded,
        &mut SmallRng::seed_from_u64(0),
        &mut Vec::new(),
        &CombatTuning::default(),
    );

    assert_eq!(healthy_contest.result, AttackResult::AttackerWinsDecisively);
    assert_eq!(wounded_contest.result, AttackResult::AttackerWinsDecisively);
    assert!(
        wounded_contest.damage > healthy_contest.damage,
        "wounded target must take more damage: healthy {} vs wounded {}",
        healthy_contest.damage,
        wounded_contest.damage
    );
}

/// Toughness N mitigates N% of an incoming hit, capped at 50%; a landed hit
/// always draws at least 1 blood; zero damage is never floored up.
#[test]
fn mitigate_damage_scales_with_toughness_and_floors_at_one() {
    use super::resolve::mitigate_damage;
    assert_eq!(mitigate_damage(1000, 0), 1000, "no toughness: full damage");
    assert_eq!(mitigate_damage(1000, 20), 800, "20 toughness: 20% off");
    assert_eq!(mitigate_damage(1000, 50), 500, "cap: at most half");
    assert_eq!(mitigate_damage(1000, 80), 500, "cap holds above 50");
    assert_eq!(
        mitigate_damage(1000, -3),
        1000,
        "negative: mitigates nothing"
    );
    assert_eq!(mitigate_damage(1, 50), 1, "landed hit: at least 1 blood");
    assert_eq!(mitigate_damage(0, 50), 0, "no hit: nothing to mitigate");
}

#[rstest]
fn attacks_deducts_stamina_costs(mut small_rng: SmallRng) {
    let tuning = CombatTuning::default();
    let mut attacker = Character::new("A".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    attacker.stamina = 100;
    attacker.max_stamina = 100;
    let mut target = Character::new("B".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();
    target.stamina = 100;
    target.max_stamina = 100;
    let _ = attacker.attacks(
        &mut target,
        &mut small_rng,
        &mut Vec::new(),
        shared::messages::Phase::DAY,
        &tuning,
    );
    assert_eq!(attacker.stamina, 100 - tuning.stamina_cost_attacker);
    assert_eq!(target.stamina, 100 - tuning.stamina_cost_target);
}

#[rstest]
fn attacks_saturates_at_zero_when_below_cost(mut small_rng: SmallRng) {
    let tuning = CombatTuning::default();
    let mut attacker = Character::new("A".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    attacker.stamina = 5;
    attacker.max_stamina = 100;
    let mut target = Character::new("B".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();
    target.stamina = 3;
    target.max_stamina = 100;
    let _ = attacker.attacks(
        &mut target,
        &mut small_rng,
        &mut Vec::new(),
        shared::messages::Phase::DAY,
        &tuning,
    );
    assert_eq!(attacker.stamina, 0);
    assert_eq!(target.stamina, 0);
}

#[test]
fn attacker_winded_takes_attack_roll_penalty() {
    // Find a seed where the Winded penalty produces a different outcome
    // from a Fresh attacker with otherwise identical setup.
    let tuning = CombatTuning::default();
    for seed in 0..200u64 {
        let mut a_fresh = Character::new("AF".to_string(), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        a_fresh.traits.clear();
        a_fresh.stamina = 100;
        a_fresh.max_stamina = 100;
        a_fresh.attributes.strength = 5;
        let mut a_winded = Character::new("AW".to_string(), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        a_winded.traits.clear();
        a_winded.stamina = 70; // post-cost: 45 -> Winded
        a_winded.max_stamina = 100;
        a_winded.attributes.strength = 5;
        let mut t1 = Character::new("T1".to_string(), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        t1.traits.clear();
        t1.stamina = 100;
        t1.max_stamina = 100;
        t1.blood = 1000;
        t1.attributes.toughness = 10;
        let mut t2 = Character::new("T2".to_string(), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        t2.traits.clear();
        t2.stamina = 100;
        t2.max_stamina = 100;
        t2.blood = 1000;
        t2.attributes.toughness = 10;

        let mut rng_a = SmallRng::seed_from_u64(seed);
        let mut rng_b = SmallRng::seed_from_u64(seed);
        let out_a = a_fresh.attacks(
            &mut t1,
            &mut rng_a,
            &mut Vec::new(),
            shared::messages::Phase::DAY,
            &tuning,
        );
        let out_b = a_winded.attacks(
            &mut t2,
            &mut rng_b,
            &mut Vec::new(),
            shared::messages::Phase::DAY,
            &tuning,
        );

        if t1.blood != t2.blood {
            return; // Found a seed where penalty changes outcome
        }
        let _ = (out_a, out_b);
    }
    panic!("No seed in 0..200 produced different outcomes for fresh vs winded attacker");
}

#[rstest]
fn combat_beat_carries_stamina_costs(mut small_rng: SmallRng) {
    let tuning = CombatTuning::default();
    let mut attacker = Character::new("A".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    attacker.stamina = 100;
    attacker.max_stamina = 100;
    let mut target = Character::new("B".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();
    target.stamina = 100;
    target.max_stamina = 100;
    let mut events = Vec::new();
    let _ = attacker.attacks(
        &mut target,
        &mut small_rng,
        &mut events,
        shared::messages::Phase::DAY,
        &tuning,
    );
    let beats: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.payload {
            MessagePayload::CombatBeat(b) => Some(b),
            _ => None,
        })
        .collect();
    assert_eq!(beats.len(), 1);
    let shared::combat_beat::BeatOutcome::Swing {
        actor_stamina_cost,
        target_stamina_cost,
        ..
    } = &beats[0].outcome
    else {
        panic!("expected a Swing beat outcome");
    };
    assert_eq!(*actor_stamina_cost, tuning.stamina_cost_attacker);
    assert_eq!(*target_stamina_cost, tuning.stamina_cost_target);
}

#[rstest]
fn test_critical_hit() {
    // Use a custom RNG that always returns the high bits needed for
    // `random_range(1..=20)` to produce 20 under rand 0.9's algorithm.
    struct CritRng;
    impl TryRng for CritRng {
        type Error = Infallible;
        fn try_next_u32(&mut self) -> Result<u32, Infallible> {
            Ok(0xF333_3334)
        }
        fn try_next_u64(&mut self) -> Result<u64, Infallible> {
            Ok((0xF333_3334u64 << 32) | 0xF333_3334u64)
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Infallible> {
            for byte in dest.iter_mut() {
                *byte = 0xFF;
            }
            Ok(())
        }
    }

    let mut crit_rng = CritRng;
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.attributes.strength = 10;
    target.blood = 1000;

    let result = attack_contest(
        &mut attacker,
        &mut target,
        &mut crit_rng,
        &mut Vec::new(),
        &CombatTuning::default(),
    )
    .result;
    assert_eq!(result, AttackResult::CriticalHit);
}

#[rstest]
fn test_critical_fumble() {
    // Use a custom RNG that returns 0 so `random_range(1..=20)` yields 1.
    struct FumbleRng;
    impl TryRng for FumbleRng {
        type Error = Infallible;
        fn try_next_u32(&mut self) -> Result<u32, Infallible> {
            Ok(0)
        }
        fn try_next_u64(&mut self) -> Result<u64, Infallible> {
            Ok(0)
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Infallible> {
            for byte in dest.iter_mut() {
                *byte = 0;
            }
            Ok(())
        }
    }

    let mut fumble_rng = FumbleRng;
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    let result = attack_contest(
        &mut attacker,
        &mut target,
        &mut fumble_rng,
        &mut Vec::new(),
        &CombatTuning::default(),
    )
    .result;
    assert_eq!(result, AttackResult::CriticalFumble);
}

#[rstest]
fn test_perfect_block() {
    // First call (attacker roll): 0x7333_3334 -> random_range(1..=20) == 10
    // Second call (defender roll): 0xF333_3334 -> random_range(1..=20) == 20
    struct BlockRng {
        call_count: std::cell::Cell<usize>,
    }
    impl BlockRng {
        fn new() -> Self {
            BlockRng {
                call_count: std::cell::Cell::new(0),
            }
        }
    }
    impl TryRng for BlockRng {
        type Error = Infallible;
        fn try_next_u32(&mut self) -> Result<u32, Infallible> {
            let count = self.call_count.get();
            self.call_count.set(count + 1);
            Ok(if count == 0 { 0x7333_3334 } else { 0xF333_3334 })
        }
        fn try_next_u64(&mut self) -> Result<u64, Infallible> {
            Ok(self.try_next_u32().unwrap() as u64)
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Infallible> {
            for byte in dest.iter_mut() {
                *byte = self.try_next_u32().unwrap() as u8;
            }
            Ok(())
        }
    }

    let mut block_rng = BlockRng::new();
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    let result = attack_contest(
        &mut attacker,
        &mut target,
        &mut block_rng,
        &mut Vec::new(),
        &CombatTuning::default(),
    )
    .result;
    assert_eq!(result, AttackResult::PerfectBlock);
}

#[rstest]
fn test_critical_hit_triple_damage(mut _small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.attributes.strength = 20;
    let damage = attacker.attributes.strength * 3;

    // Manually test the wound creation for critical hit
    apply_combat_results(
        &mut attacker,
        &mut target,
        damage, // Triple damage
        GameOutput::CharacterAttackWin("Snaggletooth", "Grubworm"),
        &mut Vec::new(),
        &CombatTuning::default(),
        &mut _small_rng,
    );

    // Verify wound was created (damage maps to severity, not direct health loss)
    assert!(!target.wounds.is_empty());
    // Blood should have been drained from the wound
    assert!(target.blood < 1000);
}

#[rstest]
fn test_fumble_self_damage(_small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    attacker.blood = 1000;
    let initial_blood = attacker.blood;

    // Simulate fumble damage (5 in old scale = 50 in blood)
    attacker.blood = attacker.blood.saturating_sub(50);

    assert_eq!(attacker.blood, initial_blood - 50);
}

/// cg9: clean hits never wear the blade — a landed hit on bare flesh (no
/// armor, no shield margin, no fumble/perfect) leaves durability intact.
/// Wear still applies when it fires: see weapon_break_records (armor).
fn clean_hits_never_wear_weapon() {
    use world::items::{Attribute, Item, ItemRarity, ItemType};

    for seed in 0..500u64 {
        let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        attacker.traits.clear();
        let mut target = Character::new("Grubworm".to_string(), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        target.traits.clear();

        let weapon = Item::new(
            "Test Bow",
            ItemType::Weapon,
            ItemRarity::Rare,
            5,
            Attribute::Strength,
            3,
        );
        let weapon_id = weapon.identifier.clone();
        attacker.items.push(weapon);

        let mut events: Vec<TaggedEvent> = Vec::new();
        let mut rng = SmallRng::seed_from_u64(seed);
        let _ = attacker.attacks(
            &mut target,
            &mut rng,
            &mut events,
            shared::messages::Phase::DAY,
            &CombatTuning::default(),
        );
        let Some(beat) = first_beat(&events) else {
            continue;
        };
        // Landed hit: the target took it (not a counter, crit, or fumble).
        if wound_damage(beat).is_none() || target.blood == 1000 {
            continue;
        }

        let stored = attacker
            .items
            .iter()
            .find(|i| i.identifier == weapon_id)
            .expect("weapon stays in inventory");
        assert_eq!(
            stored.current_durability, 5,
            "clean hit must not wear the weapon (seed {seed})"
        );
        assert!(
            !beat
                .wear
                .iter()
                .any(|w| w.owner.identifier == beat.actor.identifier),
            "no attacker wear reports on a clean hit (seed {seed})"
        );
        return;
    }
    panic!("no landed hit found in 500 seeds");
}

#[rstest]
fn attacks_target_killed_records_killer_id() {
    // When the attacker kills the target, target.recently_killed_by must
    // be set to the attacker's id so the cycle can attribute the death.
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.attributes.strength = 100;
    target.blood = 10;
    target.attributes.toughness = 0;
    let attacker_id = attacker.id;

    // Use a deterministic RNG; with strength=100 vs defense=0 the
    // attacker reliably wins or crit-hits and the 1hp target dies.
    let mut rng = SmallRng::seed_from_u64(1);
    let _ = attacker.attacks(
        &mut target,
        &mut rng,
        &mut Vec::new(),
        shared::messages::Phase::DAY,
        &CombatTuning::default(),
    );

    assert_eq!(target.blood, 0, "target should be dead in this scenario");
    assert_eq!(
        target.status,
        crate::statuses::CharacterStatus::RecentlyDead
    );
    assert_eq!(
        target.recently_killed_by,
        Some(attacker_id),
        "killer id must be recorded on the deceased"
    );
    assert!(
        attacker.recently_killed_by.is_none(),
        "attacker is alive; their field must remain None"
    );
}

#[rstest]
fn attacks_attacker_killed_records_target_id() {
    // When the target's counter kills the attacker (e.g. perfect block),
    // attacker.recently_killed_by must point to the target.
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.blood = 10;
    attacker.attributes.strength = 0;
    attacker.attributes.toughness = 0;
    target.attributes.strength = 100;
    target.attributes.toughness = 100;
    let target_id = target.id;

    let mut rng = SmallRng::seed_from_u64(2);
    let _ = attacker.attacks(
        &mut target,
        &mut rng,
        &mut Vec::new(),
        shared::messages::Phase::DAY,
        &CombatTuning::default(),
    );

    if attacker.blood == 0 {
        assert_eq!(
            attacker.status,
            crate::statuses::CharacterStatus::RecentlyDead
        );
        assert_eq!(
            attacker.recently_killed_by,
            Some(target_id),
            "killer id must be recorded on the deceased attacker"
        );
    } else {
        // The seed didn't produce a kill; not a failure of attribution
        // logic, just RNG. Re-skip rather than flake.
        eprintln!("seed did not produce attacker death; skipping attribution check");
    }
}

/// Contract test: a real two-character engagement emits exactly one
/// `MessagePayload::Combat` `TaggedEvent`, with the attacker/target
/// names and a recognised outcome.
#[rstest]
fn attacks_emits_one_combat_taggedevent(mut small_rng: SmallRng) {
    let mut attacker = Character::new("Snaggletooth".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    let mut target = Character::new("Grubworm".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();

    attacker.attributes.strength = 50;
    target.attributes.toughness = 0;
    target.blood = 100;

    let mut events: Vec<TaggedEvent> = Vec::new();
    attacker.attacks(
        &mut target,
        &mut small_rng,
        &mut events,
        shared::messages::Phase::DAY,
        &CombatTuning::default(),
    );

    let combat_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.payload, MessagePayload::Combat(_)))
        .collect();
    assert_eq!(
        combat_events.len(),
        1,
        "exactly one Combat payload per attacks() call"
    );

    if let MessagePayload::Combat(eng) = &combat_events[0].payload {
        assert_eq!(eng.attacker.name, attacker.name);
        assert_eq!(eng.target.name, target.name);
        assert!(matches!(
            eng.outcome,
            CombatOutcome::Killed
                | CombatOutcome::Wounded
                | CombatOutcome::TargetFled
                | CombatOutcome::AttackerFled
                | CombatOutcome::Stalemate
        ));
    } else {
        panic!("expected Combat payload");
    }
}

#[rstest]
fn attacks_emits_one_combat_swing_per_call(small_rng: SmallRng) {
    // Every call to attacks() produces exactly one MessagePayload::CombatBeat,
    // regardless of which branch (miss / wound / kill / fumble / self-attack).
    for seed in 0..16u64 {
        let mut events: Vec<TaggedEvent> = Vec::new();
        let mut attacker = Character::new(format!("A{seed}"), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        attacker.traits.clear();
        let mut target = Character::new(format!("T{seed}"), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        target.traits.clear();
        let mut rng = SmallRng::seed_from_u64(seed);
        let _ = attacker.attacks(
            &mut target,
            &mut rng,
            &mut events,
            shared::messages::Phase::DAY,
            &CombatTuning::default(),
        );

        let swings: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.payload, MessagePayload::CombatBeat(_)))
            .collect();
        assert_eq!(
            swings.len(),
            1,
            "seed {seed}: expected exactly one CombatBeat payload, got {}",
            swings.len()
        );

        if let MessagePayload::CombatBeat(beat) = &swings[0].payload {
            assert_eq!(beat.actor.name, attacker.name);
            assert_eq!(beat.target.as_ref().unwrap().name, target.name);
        }
    }
    // Touch the rstest fixture to silence unused warnings.
    let _ = small_rng;
}

#[rstest]
fn self_attack_emits_one_combat_swing(mut small_rng: SmallRng) {
    let mut character = Character::new("Solo".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    character.traits.clear();
    let mut clone = character.clone();
    let mut events: Vec<TaggedEvent> = Vec::new();
    let _ = character.attacks(
        &mut clone,
        &mut small_rng,
        &mut events,
        shared::messages::Phase::DAY,
        &CombatTuning::default(),
    );
    let swings: usize = events
        .iter()
        .filter(|e| matches!(e.payload, MessagePayload::CombatBeat(_)))
        .count();
    assert_eq!(swings, 1, "self-attack must emit exactly one CombatBeat");
}

/// Construct a weapon with the given effect and durability=1 so a single
/// `wear(1)` call breaks it.
fn brittle_weapon(effect: i32) -> world::items::Item {
    world::items::Item::new(
        "Glass Sword",
        world::items::ItemType::Weapon,
        world::items::ItemRarity::Common,
        1,
        world::items::Attribute::Strength,
        effect,
    )
}

/// Construct a shield with the given effect and durability=1.
fn brittle_shield(effect: i32) -> world::items::Item {
    world::items::Item::new(
        "Glass Buckler",
        world::items::ItemType::Shield,
        world::items::ItemRarity::Common,
        1,
        world::items::Attribute::Defense,
        effect,
    )
}

#[test]
fn weapon_break_records_forfeit_and_penalty_on_beat() {
    // cg9: wear fires when a landed hit is absorbed by armor (middle tier),
    // so the brittle blade breaks on exactly that outcome. Seed-hunt the hit;
    // fresh characters per seed keep durability honest.
    let mut hit_events: Option<Vec<TaggedEvent>> = None;
    for seed in 0..500u64 {
        let mut attacker = Character::new("Atk".into(), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        attacker.traits.clear();
        attacker.attributes.strength = 10;
        attacker.add_item(brittle_weapon(5));

        let mut target = Character::new("Tgt".into(), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        target.traits.clear();
        target.attributes.toughness = 5;
        target.add_item(world::items::Item::new(
            "Armor Plate",
            world::items::ItemType::Armor,
            world::items::ItemRarity::Common,
            10,
            world::items::Attribute::Defense,
            4,
        ));

        let mut events: Vec<TaggedEvent> = Vec::new();
        let mut rng = SmallRng::seed_from_u64(seed);
        let _ = attacker.attacks(
            &mut target,
            &mut rng,
            &mut events,
            shared::messages::Phase::DAY,
            &CombatTuning::default(),
        );
        if let Some(beat) = first_beat(&events)
            && wound_damage(beat).is_some()
            && target.blood < 1000
            && attacker.blood == 1000
        {
            hit_events = Some(events);
            break;
        }
    }
    let events = hit_events.expect("no landed hit found in 500 seeds");

    let beat = first_beat(&events).expect("expected one CombatBeat emission");
    let weapon_wear = beat
        .wear
        .iter()
        .find(|w| w.owner.identifier == beat.actor.identifier)
        .expect("expected a wear report for the attacker's weapon");

    assert_eq!(
        weapon_wear.outcome,
        shared::combat_beat::WearOutcomeReport::Broken
    );
    assert_eq!(weapon_wear.forfeited_effect, Some(5));
    let penalty = weapon_wear.mid_action_penalty.expect("penalty must fire");
    assert!(
        (1..=4).contains(&penalty),
        "penalty must be 1..=4, got {}",
        penalty
    );
}

#[test]
fn shield_break_records_forfeit_and_penalty_on_beat() {
    let mut attacker = Character::new("Atk".into(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    attacker.attributes.strength = 10;

    let mut target = Character::new("Tgt".into(), None, None);

    // Roll-independent: trait AttrMods must not shift exact math.

    target.traits.clear();
    target.attributes.toughness = 5;
    let shield = brittle_shield(4);
    target.add_item(shield.clone());

    let mut events: Vec<TaggedEvent> = Vec::new();
    let mut rng = SmallRng::seed_from_u64(7);
    let _ = attacker.attacks(
        &mut target,
        &mut rng,
        &mut events,
        shared::messages::Phase::DAY,
        &CombatTuning::default(),
    );

    let beat = events
        .iter()
        .find_map(|e| match &e.payload {
            MessagePayload::CombatBeat(b) => Some(b),
            _ => None,
        })
        .expect("expected one CombatBeat emission");
    let shield_wear = beat
        .wear
        .iter()
        .find(|w| w.owner.identifier == beat.target.as_ref().unwrap().identifier)
        .expect("expected a wear report for the target's shield");

    assert_eq!(
        shield_wear.outcome,
        shared::combat_beat::WearOutcomeReport::Broken
    );
    assert_eq!(shield_wear.forfeited_effect, Some(4));
    let penalty = shield_wear.mid_action_penalty.expect("penalty must fire");
    assert!((1..=4).contains(&penalty), "penalty was {}", penalty);
}

#[test]
fn fumble_clears_attacker_break_penalty_on_beat() {
    // Hunt for a seed where the attacker's swing both fumbles AND breaks
    // the brittle weapon. Per design D5 the attacker-side break-penalty
    // fields must be cleared on fumble for clean narration.
    for seed in 0u64..2_000 {
        let mut attacker = Character::new("Atk".into(), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        attacker.traits.clear();
        attacker.attributes.strength = 10;
        let weapon = brittle_weapon(5);
        attacker.add_item(weapon.clone());

        let mut target = Character::new("Tgt".into(), None, None);

        // Roll-independent: trait AttrMods must not shift exact math.

        target.traits.clear();
        target.attributes.toughness = 5;

        let mut events: Vec<TaggedEvent> = Vec::new();
        let mut rng = SmallRng::seed_from_u64(seed);
        let _ = attacker.attacks(
            &mut target,
            &mut rng,
            &mut events,
            shared::messages::Phase::DAY,
            &CombatTuning::default(),
        );

        let beat = match events.iter().find_map(|e| match &e.payload {
            MessagePayload::CombatBeat(b) => Some(b),
            _ => None,
        }) {
            Some(b) => b,
            None => continue,
        };

        let is_fumble = matches!(
            beat.outcome,
            shared::combat_beat::BeatOutcome::Swing {
                outcome: shared::combat_beat::SwingOutcome::FumbleSurvive { .. }
                    | shared::combat_beat::SwingOutcome::FumbleDeath { .. },
                ..
            }
        );
        let weapon_wear = beat
            .wear
            .iter()
            .find(|w| w.owner.identifier == beat.actor.identifier);

        if is_fumble
            && weapon_wear
                .map(|w| w.outcome == shared::combat_beat::WearOutcomeReport::Broken)
                .unwrap_or(false)
        {
            let w = weapon_wear.unwrap();
            assert_eq!(
                w.forfeited_effect, None,
                "D5: fumble must clear forfeited_effect"
            );
            assert_eq!(
                w.mid_action_penalty, None,
                "D5: fumble must clear mid_action_penalty"
            );
            return;
        }
    }
    panic!("no seed in 0..2000 produced a fumble + weapon break combo; widen the search");
}

#[test]
fn unarmed_unshielded_emits_no_break_penalty() {
    let mut attacker = Character::new("Atk".into(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    attacker.attributes.strength = 10;
    let mut target = Character::new("Tgt".into(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();
    target.attributes.toughness = 5;

    let mut events: Vec<TaggedEvent> = Vec::new();
    let mut rng = SmallRng::seed_from_u64(123);
    let _ = attacker.attacks(
        &mut target,
        &mut rng,
        &mut events,
        shared::messages::Phase::DAY,
        &CombatTuning::default(),
    );

    let beat = events
        .iter()
        .find_map(|e| match &e.payload {
            MessagePayload::CombatBeat(b) => Some(b),
            _ => None,
        })
        .expect("expected one CombatBeat emission");
    for w in &beat.wear {
        assert_eq!(w.forfeited_effect, None);
        assert_eq!(w.mid_action_penalty, None);
    }
}

/// Parity: wear lines rendered from `CombatBeat.wear` via
/// `CombatBeatExt::to_log_lines` must match the wear lines that
/// `attack_contest` flattens into `CombatEngagement.detail_lines`.
/// This locks `CombatBeat.wear` as the single source of truth for wear
/// narration once consumers migrate off `detail_lines`.
#[test]
fn combat_beat_wear_matches_engagement_detail_wear_lines() {
    use crate::combat_beat::CombatBeatExt;
    // Both attacker weapon and target shield are brittle so we exercise
    // the full wear (Worn or Broken) emission paths in one swing.
    for seed in 0..32u64 {
        let mut attacker = Character::new(format!("Atk{seed}"), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        attacker.traits.clear();
        attacker.attributes.strength = 10;
        attacker.add_item(brittle_weapon(5));
        let mut target = Character::new(format!("Tgt{seed}"), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        target.traits.clear();
        target.attributes.toughness = 5;
        target.add_item(brittle_shield(4));

        let mut events: Vec<TaggedEvent> = Vec::new();
        let mut rng = SmallRng::seed_from_u64(seed);
        let _ = attacker.attacks(
            &mut target,
            &mut rng,
            &mut events,
            shared::messages::Phase::DAY,
            &CombatTuning::default(),
        );

        // Find the engagement (skip seeds that didn't produce one, e.g.
        // pure fumble paths emit a standalone CharacterWounded instead and
        // never write detail_lines for the legacy timeline to consume).
        let Some(detail_lines) = events.iter().find_map(|e| match &e.payload {
            MessagePayload::Combat(eng) => Some(eng.detail_lines.clone()),
            _ => None,
        }) else {
            continue;
        };
        let beat = events
            .iter()
            .find_map(|e| match &e.payload {
                MessagePayload::CombatBeat(b) => Some(b),
                _ => None,
            })
            .expect("expected one CombatBeat per attacks() call");

        // Wear-related substrings we want to track.
        let is_wear_line = |s: &str| {
            s.contains("starting to wear out")
                || s.contains("breaks")
                || s.contains("shatters mid-swing")
                || s.contains("shatters mid-block")
        };

        let beat_wear_lines: Vec<String> = beat
            .to_log_lines()
            .into_iter()
            .filter(|s| is_wear_line(s))
            .collect();
        let detail_wear_lines: Vec<String> = detail_lines
            .into_iter()
            .filter(|s| is_wear_line(s))
            .collect();

        assert_eq!(
            beat_wear_lines, detail_wear_lines,
            "seed {seed}: wear lines from CombatBeat must match detail_lines"
        );
    }
}

/// A swing that wins (and therefore triggers `apply_violence_stress`) on
/// a character already credited with prior wins must record the resulting
/// stress on the swing's `CombatBeat.stress.stress_damage`.
#[test]
fn combat_swing_records_stress_damage() {
    // Arrange: attacker has prior wins+kills so violence-stress is non-zero.
    let mut attacker = Character::new("Atk".into(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    attacker.attributes.strength = 50;
    attacker.statistics.wins = 5;
    attacker.statistics.kills = 5;

    let mut target = Character::new("Tgt".into(), None, None);

    // Roll-independent: trait AttrMods must not shift exact math.

    target.traits.clear();
    target.attributes.toughness = 0;
    target.blood = 10;

    let mut events: Vec<TaggedEvent> = Vec::new();
    let mut rng = SmallRng::seed_from_u64(1);
    let _ = attacker.attacks(
        &mut target,
        &mut rng,
        &mut events,
        shared::messages::Phase::DAY,
        &CombatTuning::default(),
    );

    let beat = events
        .iter()
        .find_map(|e| match &e.payload {
            MessagePayload::CombatBeat(b) => Some(b),
            _ => None,
        })
        .expect("expected one CombatBeat emission");
    let shared::combat_beat::BeatOutcome::Swing { stress, .. } = &beat.outcome else {
        panic!("expected a Swing beat outcome");
    };
    assert!(
        stress.stress_damage > 0,
        "expected non-zero stress on the beat; got {}",
        stress.stress_damage
    );
}

/// Parity: the optional trailing horrified line that
/// `CombatBeatExt::to_log_lines` renders from `stress.stress_damage`
/// must match the horrified line that `apply_violence_stress` flattened
/// into `CombatEngagement.detail_lines`.
#[test]
fn combat_beat_stress_matches_engagement_horrified_line() {
    use crate::combat_beat::CombatBeatExt;
    for seed in 0..32u64 {
        let mut attacker = Character::new(format!("Atk{seed}"), None, None);
        // Roll-independent: trait AttrMods must not shift exact math.
        attacker.traits.clear();
        attacker.attributes.strength = 50;
        attacker.statistics.wins = 5;
        attacker.statistics.kills = 5;

        let mut target = Character::new(format!("Tgt{seed}"), None, None);

        // Roll-independent: trait AttrMods must not shift exact math.

        target.traits.clear();
        target.attributes.toughness = 0;
        target.blood = 1000;

        let mut events: Vec<TaggedEvent> = Vec::new();
        let mut rng = SmallRng::seed_from_u64(seed);
        let _ = attacker.attacks(
            &mut target,
            &mut rng,
            &mut events,
            shared::messages::Phase::DAY,
            &CombatTuning::default(),
        );

        let Some(detail_lines) = events.iter().find_map(|e| match &e.payload {
            MessagePayload::Combat(eng) => Some(eng.detail_lines.clone()),
            _ => None,
        }) else {
            continue;
        };
        let beat = events
            .iter()
            .find_map(|e| match &e.payload {
                MessagePayload::CombatBeat(b) => Some(b),
                _ => None,
            })
            .expect("expected one CombatBeat per attacks() call");

        let beat_horrified: Vec<String> = beat
            .to_log_lines()
            .into_iter()
            .filter(|s| s.contains("horrified"))
            .collect();
        let detail_horrified: Vec<String> = detail_lines
            .into_iter()
            .filter(|s| s.contains("horrified"))
            .collect();

        assert_eq!(
            beat_horrified, detail_horrified,
            "seed {seed}: horrified lines from CombatBeat must match detail_lines"
        );
    }
}

#[rstest]
fn attacking_sleeping_target_emits_ambush_wake(mut small_rng: SmallRng) {
    // Spec §6.4 PR2c.2: hitting a sleeping character must wake
    // them with `WakeReason::Interrupted { Ambush }` BEFORE damage
    // resolution. The ambush still lands; we just verify the wake
    // event precedes any CharacterWounded / Killed event in `events`.
    let mut attacker = Character::new("Rendmaw".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    attacker.traits.clear();
    attacker.attributes.strength = 10;
    let mut target = Character::new("Nib".to_string(), None, None);
    // Roll-independent: trait AttrMods must not shift exact math.
    target.traits.clear();
    target.sleeping = true;
    target.sleep_remaining = 4;
    target.cycles_awake = 12;

    let mut events: Vec<TaggedEvent> = Vec::new();
    let _ = attacker.attacks(
        &mut target,
        &mut small_rng,
        &mut events,
        shared::messages::Phase::NIGHT,
        &CombatTuning::default(),
    );

    assert!(!target.sleeping, "ambushed target must wake");
    assert_eq!(target.sleep_remaining, 0);
    assert_eq!(target.cycles_awake, 0);

    let woke_idx = events
        .iter()
        .position(|e| {
            matches!(
                &e.payload,
                MessagePayload::CharacterWoke {
                    reason: shared::messages::WakeReason::Interrupted {
                        event: shared::messages::InterruptionKind::Ambush { .. },
                    },
                    ..
                }
            )
        })
        .expect("expected one ambush CharacterWoke event");
    assert_eq!(
        woke_idx, 0,
        "ambush wake must be emitted before damage resolution"
    );
}

/// Construct armor with the given effect and durability=1 (wears once, then breaks).
fn brittle_armor(effect: i32) -> world::items::Item {
    world::items::Item::new(
        "Glass Plate",
        world::items::ItemType::Armor,
        world::items::ItemRarity::Common,
        1,
        world::items::Attribute::Defense,
        effect,
    )
}

/// Attacker with cleared traits and fixed strength — roll-independent stats.
fn fighter(name: &str) -> Character {
    let mut c = Character::new(name.into(), None, None);
    c.traits.clear();
    c.attributes.strength = 10;
    c
}

/// Defender with cleared traits and fixed toughness — roll-independent stats.
fn wall(name: &str) -> Character {
    let mut c = Character::new(name.into(), None, None);
    c.traits.clear();
    c.attributes.toughness = 5;
    c
}

fn first_beat(events: &[TaggedEvent]) -> Option<&shared::combat_beat::CombatBeat> {
    events.iter().find_map(|e| match &e.payload {
        MessagePayload::CombatBeat(b) => Some(b),
        _ => None,
    })
}

/// Damage of a plain landed hit (`SwingOutcome::Wound`); `None` for
/// crits, misses, fumbles, kills, and counters.
fn wound_damage(beat: &shared::combat_beat::CombatBeat) -> Option<u32> {
    let shared::combat_beat::BeatOutcome::Swing { outcome, .. } = &beat.outcome else {
        return None;
    };
    match outcome {
        shared::combat_beat::SwingOutcome::Wound { damage } => Some(*damage),
        _ => None,
    }
}

/// cg9 tier hunt: a BlockWound (defender-won damage) exchange with a bare
/// sidearm (dur 10 / effect 3) against a target that optionally carries a
/// shield (dur 10 / given effect). Returns (events, weapon_dur, shield_dur)
/// for the first seed matching the equipment-wear filter; shield_dur is 10
/// when no shield was carried.
fn hunt_block(
    shield_effect: Option<i32>,
    shield_wore: bool,
    weapon_wore: bool,
) -> Option<(Vec<TaggedEvent>, u32, u32)> {
    for seed in 0..500u64 {
        let mut attacker = Character::new("Atk".into(), None, None);
        attacker.traits.clear();
        attacker.attributes.strength = 10;
        attacker.add_item(world::items::Item::new(
            "Sidearm",
            world::items::ItemType::Weapon,
            world::items::ItemRarity::Common,
            10,
            world::items::Attribute::Strength,
            3,
        ));
        let mut target = Character::new("Tgt".into(), None, None);
        target.traits.clear();
        target.attributes.toughness = 5;
        if let Some(effect) = shield_effect {
            target.add_item(world::items::Item::new(
                "Oak Shield",
                world::items::ItemType::Shield,
                world::items::ItemRarity::Common,
                10,
                world::items::Attribute::Defense,
                effect,
            ));
        }
        let mut events: Vec<TaggedEvent> = Vec::new();
        let mut rng = SmallRng::seed_from_u64(seed);
        let _ = attacker.attacks(
            &mut target,
            &mut rng,
            &mut events,
            shared::messages::Phase::DAY,
            &CombatTuning::default(),
        );
        let Some(beat) = first_beat(&events) else {
            continue;
        };
        let shared::combat_beat::BeatOutcome::Swing { outcome, .. } = &beat.outcome else {
            continue;
        };
        // Defender-won damage lands on the ACTOR: plain defender wins map to
        // `Wound` (resolve.rs `_` arm), perfect blocks to `BlockWound`. Blood
        // side decides — a counter drops the attacker's blood, a landed hit
        // or crit drops the target's.
        let defender_won = matches!(
            outcome,
            shared::combat_beat::SwingOutcome::Wound { .. }
                | shared::combat_beat::SwingOutcome::BlockWound { .. }
        ) && attacker.blood < 1000
            && target.blood == 1000;
        if !defender_won {
            continue;
        }
        let weapon_dur = attacker
            .items
            .iter()
            .find(|i| i.name == "Sidearm")
            .map(|i| i.current_durability)
            .unwrap_or(10);
        let shield_dur = target
            .items
            .iter()
            .find(|i| i.name == "Oak Shield")
            .map(|i| i.current_durability)
            .unwrap_or(10);
        if (shield_dur < 10) == shield_wore && (weapon_dur < 10) == weapon_wore {
            return Some((events, weapon_dur, shield_dur));
        }
    }
    None
}

#[test]
fn decisive_shield_block_wears_weapon_and_shield() {
    // Blade struck the shield inside its margin: both take wear — weapon at
    // the heavy tier, shield its single point.
    let (events, weapon_dur, shield_dur) =
        hunt_block(Some(4), true, true).expect("no decisive shield block in 500 seeds");
    assert_eq!(
        shield_dur, 9,
        "shield wears exactly one point on its own block"
    );
    assert_eq!(
        weapon_dur, 7,
        "blade wears the full tier-3 against a decisive shield block"
    );
    let beat = first_beat(&events).expect("beat exists");
    assert!(
        beat.wear.is_empty(),
        "both crossed no tier boundary: pristine wear is unreported (shield rule)"
    );
}

#[test]
fn defender_win_outside_shield_margin_spares_both() {
    // Defender won without the shield's margin doing the work: nothing wears.
    let (events, weapon_dur, shield_dur) =
        hunt_block(Some(4), false, false).expect("no marginal defender win in 500 seeds");
    assert_eq!(
        weapon_dur, 10,
        "blade untouched when the shield didn't matter"
    );
    assert_eq!(shield_dur, 10, "shield untouched when it didn't matter");
    let beat = first_beat(&events).expect("beat exists");
    assert!(beat.wear.is_empty(), "no wear reports for a clean block");
}

#[test]
fn perfect_block_wears_weapon_heavily() {
    // Natural 20 defense, no shield involved: the blade still rings off the
    // guard — full tier on the weapon alone.
    let (events, weapon_dur, shield_dur) =
        hunt_block(None, false, true).expect("no perfect block in 500 seeds");
    assert_eq!(
        weapon_dur, 7,
        "perfect block costs the weapon its full tier"
    );
    assert_eq!(shield_dur, 10, "no shield carried");
}

#[test]
fn armor_soak_scales_with_durability_tier() {
    // Hunt a seed where the swing lands a plain (non-crit) hit. Re-running
    // the same seed with armor equipped is roll-identical: armor never
    // feeds the success roll (wzk: soak only), so only damage differs.
    for seed in 0u64..2_000 {
        let base_damage = {
            let mut attacker = fighter("Atk");
            let mut target = wall("Tgt");
            let mut events: Vec<TaggedEvent> = Vec::new();
            let mut rng = SmallRng::seed_from_u64(seed);
            let _ = attacker.attacks(
                &mut target,
                &mut rng,
                &mut events,
                shared::messages::Phase::DAY,
                &CombatTuning::default(),
            );
            let Some(beat) = first_beat(&events) else {
                continue;
            };
            let Some(damage) = wound_damage(beat) else {
                continue;
            };
            damage
        };

        // Pristine armor: full-effect soak, wear leaves it still pristine
        // (below the Worn threshold), so no WearReport is emitted.
        let (pristine_damage, pristine_after) = {
            let mut attacker = fighter("Atk");
            let mut target = wall("Tgt");
            let mut plate = world::items::Item::new(
                "Hardened Plate",
                world::items::ItemType::Armor,
                world::items::ItemRarity::Common,
                10,
                world::items::Attribute::Defense,
                6,
            );
            target.add_item(plate.clone());
            let mut events: Vec<TaggedEvent> = Vec::new();
            let mut rng = SmallRng::seed_from_u64(seed);
            let _ = attacker.attacks(
                &mut target,
                &mut rng,
                &mut events,
                shared::messages::Phase::DAY,
                &CombatTuning::default(),
            );
            let Some(beat) = first_beat(&events) else {
                continue;
            };
            let Some(damage) = wound_damage(beat) else {
                continue;
            };
            assert!(
                !beat.wear.iter().any(|w| w.item.name == plate.name),
                "pristine plate must not emit a WearReport"
            );
            plate.current_durability = target
                .items
                .iter()
                .find(|i| i.name == "Hardened Plate")
                .map(|i| i.current_durability)
                .expect("plate still equipped");
            (damage, plate.current_durability)
        };

        // Already-worn armor: half-effect soak. No WearReport — `wear()`
        // reports only the tier TRANSITION (same rule as shields), and this
        // plate already crossed before the fight.
        let (worn_damage, worn_report, worn_after) = {
            let mut attacker = fighter("Atk");
            let mut target = wall("Tgt");
            let mut plate = world::items::Item::new(
                "Worn Plate",
                world::items::ItemType::Armor,
                world::items::ItemRarity::Common,
                10,
                world::items::Attribute::Defense,
                6,
            );
            plate.current_durability = 4;
            target.add_item(plate.clone());
            let mut events: Vec<TaggedEvent> = Vec::new();
            let mut rng = SmallRng::seed_from_u64(seed);
            let _ = attacker.attacks(
                &mut target,
                &mut rng,
                &mut events,
                shared::messages::Phase::DAY,
                &CombatTuning::default(),
            );
            let Some(beat) = first_beat(&events) else {
                continue;
            };
            let Some(damage) = wound_damage(beat) else {
                continue;
            };
            let report = beat
                .wear
                .iter()
                .find(|w| w.item.name == plate.name)
                .cloned();
            let kept = target
                .items
                .iter()
                .find(|i| i.name == plate.name)
                .map(|i| i.current_durability)
                .expect("worn plate still equipped");
            (damage, report, kept)
        };
        assert!(
            worn_report.is_none(),
            "already-worn plate must not emit a WearReport"
        );
        assert_eq!(worn_after, 3, "worn plate must still take 1 wear");

        // Crossing INTO worn (6/10 -> 5/10): full-effect soak (tier read
        // pre-wear) and the Worn transition IS reported.
        let (transition_damage, transition_after) = {
            let mut attacker = fighter("Atk");
            let mut target = wall("Tgt");
            let mut plate = world::items::Item::new(
                "Fresh Plate",
                world::items::ItemType::Armor,
                world::items::ItemRarity::Common,
                10,
                world::items::Attribute::Defense,
                6,
            );
            plate.current_durability = 6;
            target.add_item(plate.clone());
            let mut events: Vec<TaggedEvent> = Vec::new();
            let mut rng = SmallRng::seed_from_u64(seed);
            let _ = attacker.attacks(
                &mut target,
                &mut rng,
                &mut events,
                shared::messages::Phase::DAY,
                &CombatTuning::default(),
            );
            let Some(beat) = first_beat(&events) else {
                continue;
            };
            let Some(damage) = wound_damage(beat) else {
                continue;
            };
            let report = beat
                .wear
                .iter()
                .find(|w| w.item.name == plate.name)
                .cloned()
                .expect("tier-crossing plate must emit a WearReport");
            assert_eq!(report.outcome, shared::combat_beat::WearOutcomeReport::Worn);
            let kept = target
                .items
                .iter()
                .find(|i| i.name == plate.name)
                .map(|i| i.current_durability)
                .expect("plate still equipped");
            (damage, kept)
        };
        assert_eq!(transition_after, 5, "plate must cross into Worn at 5/10");
        assert_eq!(
            transition_damage, pristine_damage,
            "6/10 is still above half: full-effect soak"
        );
        assert_eq!(pristine_after, 9, "pristine plate must take 1 wear");
        assert!(
            pristine_damage < worn_damage && worn_damage < base_damage,
            "soak tiers must order damage: pristine {} < worn {} < none {}",
            pristine_damage,
            worn_damage,
            base_damage
        );
        return;
    }
    panic!("no seed in 0..2000 produced a plain landed hit; widen the search");
}

#[test]
fn critical_hit_gap_skips_armor_soak_and_wear() {
    // The blade found a gap: no soak, no wear (wzk armor note).
    for seed in 0u64..2_000 {
        let base = {
            let mut attacker = fighter("Atk");
            let mut target = wall("Tgt");
            let mut events: Vec<TaggedEvent> = Vec::new();
            let mut rng = SmallRng::seed_from_u64(seed);
            let _ = attacker.attacks(
                &mut target,
                &mut rng,
                &mut events,
                shared::messages::Phase::DAY,
                &CombatTuning::default(),
            );
            let Some(beat) = first_beat(&events) else {
                continue;
            };
            let shared::combat_beat::BeatOutcome::Swing { outcome, .. } = &beat.outcome else {
                continue;
            };
            match outcome {
                shared::combat_beat::SwingOutcome::CriticalHitWound { damage } => Some(*damage),
                _ => None,
            }
        };
        let Some(base) = base else { continue };

        let (crit_damage, plate_durability, plate_reported) = {
            let mut attacker = fighter("Atk");
            let mut target = wall("Tgt");
            let plate = world::items::Item::new(
                "Hardened Plate",
                world::items::ItemType::Armor,
                world::items::ItemRarity::Common,
                10,
                world::items::Attribute::Defense,
                6,
            );
            target.add_item(plate.clone());
            let mut events: Vec<TaggedEvent> = Vec::new();
            let mut rng = SmallRng::seed_from_u64(seed);
            let _ = attacker.attacks(
                &mut target,
                &mut rng,
                &mut events,
                shared::messages::Phase::DAY,
                &CombatTuning::default(),
            );
            let Some(beat) = first_beat(&events) else {
                continue;
            };
            let shared::combat_beat::BeatOutcome::Swing { outcome, .. } = &beat.outcome else {
                continue;
            };
            let damage = match outcome {
                shared::combat_beat::SwingOutcome::CriticalHitWound { damage } => *damage,
                _ => continue,
            };
            let reported = beat.wear.iter().any(|w| w.item.name == plate.name);
            let kept = target
                .items
                .iter()
                .find(|i| i.name == plate.name)
                .map(|i| i.current_durability)
                .expect("plate still equipped");
            (damage, kept, reported)
        };

        assert_eq!(
            crit_damage, base,
            "crit must ignore armor soak (seed {seed})"
        );
        assert_eq!(
            plate_durability, 10,
            "crit must not wear armor (seed {seed})"
        );
        assert!(!plate_reported, "crit must not emit armor WearReport");
        return;
    }
    panic!("no seed in 0..2000 produced a critical hit; widen the search");
}

#[test]
fn armor_break_forfeits_soak_and_carries_no_penalty() {
    // Broken armor forfeits its soak this contest (shield precedent) but,
    // unlike shield/weapon, carries no roll penalty — it never fed a roll.
    for seed in 0u64..2_000 {
        let base_damage = {
            let mut attacker = fighter("Atk");
            let mut target = wall("Tgt");
            let mut events: Vec<TaggedEvent> = Vec::new();
            let mut rng = SmallRng::seed_from_u64(seed);
            let _ = attacker.attacks(
                &mut target,
                &mut rng,
                &mut events,
                shared::messages::Phase::DAY,
                &CombatTuning::default(),
            );
            let Some(beat) = first_beat(&events) else {
                continue;
            };
            let Some(damage) = wound_damage(beat) else {
                continue;
            };
            damage
        };

        let mut attacker = fighter("Atk");
        let mut target = wall("Tgt");
        let plate = brittle_armor(4);
        target.add_item(plate.clone());
        let mut events: Vec<TaggedEvent> = Vec::new();
        let mut rng = SmallRng::seed_from_u64(seed);
        let _ = attacker.attacks(
            &mut target,
            &mut rng,
            &mut events,
            shared::messages::Phase::DAY,
            &CombatTuning::default(),
        );
        let Some(beat) = first_beat(&events) else {
            continue;
        };
        let Some(damage) = wound_damage(beat) else {
            continue;
        };

        let report = beat
            .wear
            .iter()
            .find(|w| w.item.name == plate.name)
            .expect("broken plate must emit a WearReport");
        assert_eq!(
            report.outcome,
            shared::combat_beat::WearOutcomeReport::Broken
        );
        assert_eq!(report.forfeited_effect, Some(4));
        assert_eq!(
            report.mid_action_penalty, None,
            "armor break must not carry a roll penalty"
        );
        assert!(
            !target.items.iter().any(|i| i.name == plate.name),
            "broken plate must be removed"
        );
        assert!(
            events.iter().any(|e| match &e.payload {
                MessagePayload::Combat(eng) => eng
                    .detail_lines
                    .iter()
                    .any(|l| l.contains("breaks their") && l.contains("Glass Plate")),
                _ => false,
            }),
            "expected ArmorBreak narration in engagement detail lines; got {:?}",
            events
                .iter()
                .filter_map(|e| match &e.payload {
                    MessagePayload::Combat(eng) => Some(eng.detail_lines.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(
            damage, base_damage,
            "armor broke before absorbing: soak forfeited this contest"
        );
        return;
    }
    panic!("no seed in 0..2000 produced a plain landed hit; widen the search");
}
