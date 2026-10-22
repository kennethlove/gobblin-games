//! Tunable knobs for combat: existing magic numbers (decisive-win multiplier,
//! stress contributions) plus the new stamina-as-combat-resource constants
//! introduced by the stamina combat-resource design.
//!
//! All defaults preserve current behavior; tuning is a separate post-ship pass.
//! See `docs/superpowers/specs/2026-05-03-stamina-combat-resource-design.md`.

use serde::{Deserialize, Serialize};

// Container-level default: tuning rows persisted before newer knobs existed
// deserialize against `Default` instead of failing on missing fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CombatTuning {
    // --- Existing constants (verbatim from combat.rs:21-26) ---
    pub decisive_win_multiplier: f64,
    pub base_stress_no_engagements: f64,
    pub stress_sanity_normalization: f64,
    pub stress_final_divisor: f64,
    pub kill_stress_contribution: f64,
    pub non_kill_win_stress_contribution: f64,

    // --- Per-swing stamina costs (asymmetric: swinging is harder than defending) ---
    pub stamina_cost_attacker: u32,
    pub stamina_cost_target: u32,

    // --- Band thresholds (% of max_stamina; > => Fresh, > => Winded, else Exhausted) ---
    pub band_winded_pct: u8,
    pub band_exhausted_pct: u8,

    // --- Per-band roll penalties (subtracted from attack/defense rolls) ---
    pub winded_roll_penalty: i32,
    pub exhausted_roll_penalty: i32,

    // --- Per-phase recovery (gross, before survival-debuff multiplier) ---
    pub recovery_idle: u32,
    pub recovery_resting: u32,
    pub recovery_sheltered_resting: u32,
    pub recovery_starving_dehydrated_mult: f64,

    // --- Brain scoring nudges ---
    pub winded_attack_score_penalty: i32,
    pub fresh_target_visibly_tired_bonus: i32,

    // --- Multi-round combat (spec 2026-10-09 §3.3, §5, §6) ---
    /// Initiative roll is `1..=initiative_die` + modifiers.
    pub initiative_die: u32,
    /// Stamina-band initiative modifiers (Fresh / Winded / Exhausted).
    pub initiative_fresh_bonus: i32,
    pub initiative_winded_bonus: i32,
    pub initiative_exhausted_bonus: i32,
    /// Weapon speed contribution: weapon `effect` is halved and added to
    /// initiative. Placeholder divisor, tuned post-observability.
    pub initiative_weapon_speed_divisor: i32,
    /// Safety cap on rounds; reaching it without a winner ends in a Draw.
    pub max_rounds: u32,
    /// Total fight capacity: once both sides together hold this many
    /// members, round-boundary joins are blocked. Never removes members
    /// mid-fight (design §8/§10).
    pub max_combatants: usize,
    /// Flee contest: fleer rolls `d20 + speed + band + flee_contest_modifier`
    /// against the chaser's `d20 + speed + band`. Negative: running is risky —
    /// tuned so roughly a fifth of flee attempts actually escape (measured
    /// via `cargo run --example tuning_harness`).
    pub flee_contest_modifier: i32,

    // --- Combat brain weights (spec 2026-10-09 §4, §7) ---
    /// Baseline score for Attack.
    pub brain_attack_base: i32,
    /// Added for Aggressive/Reckless traits.
    pub brain_attack_aggressive_bonus: i32,
    /// Applied to Attack when badly wounded (blood below the wounded band).
    pub brain_attack_wounded_penalty: i32,
    /// Score for UseItem when wounded and a health consumable is carried.
    pub brain_heal_wounded_bonus: i32,
    /// Baseline score for Flee.
    pub brain_flee_base: i32,
    /// Added to Flee when wounded.
    pub brain_flee_wounded_bonus: i32,
    /// Added to Flee in the Exhausted stamina band.
    pub brain_flee_exhausted_bonus: i32,
    /// Added to Flee for the Cautious trait (the cowardly proxy).
    pub brain_flee_cautious_bonus: i32,
    /// Added to Flee when the opponent has the higher strength.
    pub brain_flee_outmatched_bonus: i32,
    /// Baseline score for Call Ally (outmatched — or the GangUp drive —
    /// with a recorded ally). Beats Attack's worst case (40+10 aggressive)
    /// so a ganger's summon outranks their swing; a hopeless Cower (60)
    /// still wins.
    pub brain_call_ally_base: i32,
    /// Baseline score for Cower.
    pub brain_cower_base: i32,
    /// Added to Cower when hopeless (near-dead AND Exhausted).
    pub brain_cower_hopeless_bonus: i32,
    /// Spare score: opponent is a recorded ally.
    pub brain_spare_ally_bonus: i32,
    /// Spare score: same non-zero team.
    pub brain_spare_team_bonus: i32,
    /// Spare score for Loyal/Friendly traits.
    pub brain_spare_trait_bonus: i32,
    /// Execute score for the Treacherous trait.
    pub brain_execute_treacherous_bonus: i32,
    /// Execute score for the Aggressive trait.
    pub brain_execute_aggressive_bonus: i32,

    // --- Melee v2 target choice (design 2026-10-20 §4.3) ---
    /// Wounded members score up when picking a target (goblins finish the
    /// weak).
    pub brain_target_wounded_bonus: i32,
    /// Strength preference weight: added for aggressive actors, subtracted
    /// for cowards (aggressive -> strongest, cowardly -> weakest).
    pub brain_target_strong_bonus: i32,

    // --- Conditional weapon/shield wear tiers (cg9 balance pass) ---
    // Wear fires only on gated outcomes; clean hits and plain misses wear
    // nothing. Tuned with `tuning_harness` gear mode (2000 fights, Common
    // fixtures: weapon dur 4, shield dur 5, armor dur 5 — real Common ranges
    // are weapon 2-4 / shield 3-5, so harness breaks understate real ones).
    //
    // Rationale: the bead's strict 3>2>1 rank order breaks a weapon in 53% of
    // fights — unaffordable at dur 2-4. Scaled order kept: blocks wear the
    // blade (2) > armor block / fumble (1).
    //
    // Measured at these values per 100 fights: weapon 218 pts / 8.2% breaks,
    // shield 248 pts / 16.1%, armor 320 pts / 1.8%, any-break fights 20.1%.
    /// Decisive shield block (attack landed inside the shield's margin):
    /// heaviest wear — the blade struck the shield and the block held.
    pub weapon_wear_on_shield_block: u32,
    /// Hit absorbed by the target's armor: middle tier.
    pub weapon_wear_on_armor_block: u32,
    /// Natural 20 defense (perfect block): heavy, like the shield block.
    pub weapon_wear_on_perfect_block: u32,
    /// Natural 1 fumble: least wear.
    pub weapon_wear_on_fumble: u32,
    /// Shield wear amount; fires only on a decisive shield block. At 1, 2000
    /// fights broke 0.0% of shields; at 5 a Common (3-5) shield dies on its
    /// first decisive block, Rare (6-10) on the second — the drama the wzk
    /// bead asks for. Harness: 16.1% of fights lose a shield (dur-5 fixture).
    pub shield_wear_on_decisive_block: u32,
}

impl Default for CombatTuning {
    fn default() -> Self {
        Self {
            decisive_win_multiplier: 1.5,
            base_stress_no_engagements: 20.0,
            stress_sanity_normalization: 100.0,
            stress_final_divisor: 2.0,
            kill_stress_contribution: 50.0,
            non_kill_win_stress_contribution: 20.0,

            stamina_cost_attacker: 25,
            stamina_cost_target: 10,

            band_winded_pct: 50,
            band_exhausted_pct: 20,

            winded_roll_penalty: -2,
            exhausted_roll_penalty: -5,

            recovery_idle: 5,
            recovery_resting: 30,
            recovery_sheltered_resting: 60,
            recovery_starving_dehydrated_mult: 0.5,

            winded_attack_score_penalty: -10,
            fresh_target_visibly_tired_bonus: 5,

            initiative_die: 20,
            initiative_fresh_bonus: 3,
            initiative_winded_bonus: 0,
            initiative_exhausted_bonus: -3,
            initiative_weapon_speed_divisor: 2,
            max_rounds: 12,
            max_combatants: 8,
            flee_contest_modifier: -7,

            brain_attack_base: 40,
            brain_attack_aggressive_bonus: 10,
            brain_attack_wounded_penalty: -15,
            brain_heal_wounded_bonus: 45,
            brain_flee_base: 5,
            brain_flee_wounded_bonus: 12,
            brain_flee_exhausted_bonus: 20,
            brain_flee_cautious_bonus: 15,
            brain_flee_outmatched_bonus: 10,
            brain_call_ally_base: 55,
            brain_cower_base: 0,
            brain_cower_hopeless_bonus: 60,
            brain_spare_ally_bonus: 100,
            brain_spare_team_bonus: 40,
            brain_spare_trait_bonus: 30,
            brain_execute_treacherous_bonus: 50,
            brain_execute_aggressive_bonus: 20,

            brain_target_wounded_bonus: 15,
            brain_target_strong_bonus: 5,

            weapon_wear_on_shield_block: 2,
            weapon_wear_on_armor_block: 1,
            weapon_wear_on_perfect_block: 2,
            weapon_wear_on_fumble: 1,
            shield_wear_on_decisive_block: 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_current_behavior_constants() {
        let t = CombatTuning::default();
        assert_eq!(t.decisive_win_multiplier, 1.5);
        assert_eq!(t.base_stress_no_engagements, 20.0);
        assert_eq!(t.stress_sanity_normalization, 100.0);
        assert_eq!(t.stress_final_divisor, 2.0);
        assert_eq!(t.kill_stress_contribution, 50.0);
        assert_eq!(t.non_kill_win_stress_contribution, 20.0);
    }

    #[test]
    fn default_stamina_constants_match_spec() {
        let t = CombatTuning::default();
        assert_eq!(t.stamina_cost_attacker, 25);
        assert_eq!(t.stamina_cost_target, 10);
        assert_eq!(t.band_winded_pct, 50);
        assert_eq!(t.band_exhausted_pct, 20);
        assert_eq!(t.winded_roll_penalty, -2);
        assert_eq!(t.exhausted_roll_penalty, -5);
        assert_eq!(t.recovery_idle, 5);
        assert_eq!(t.recovery_resting, 30);
        assert_eq!(t.recovery_sheltered_resting, 60);
        assert_eq!(t.recovery_starving_dehydrated_mult, 0.5);
        assert_eq!(t.winded_attack_score_penalty, -10);
        assert_eq!(t.fresh_target_visibly_tired_bonus, 5);
    }

    #[test]
    fn round_trips_through_serde_json() {
        let t = CombatTuning::default();
        let s = serde_json::to_string(&t).unwrap();
        let back: CombatTuning = serde_json::from_str(&s).unwrap();
        assert_eq!(t, back);
    }
}
