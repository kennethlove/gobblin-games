use world::terrain::{BaseTerrain, Visibility};
use crate::Character;
use crate::actions::Action;
use crate::brains::scoring::action_score;
use crate::brains::{Brain, LOW_ENEMY_LIMIT};
use crate::traits::{REFUSERS, geometric_mean_affinity};
use rand::Rng;
use rand::RngExt;

impl Brain {
    pub fn decide_action_with_terrain(
        &self, character: &Character, nearby_characters: u32,
        terrain: world::terrain::TerrainType, phase: shared::messages::Phase, rng: &mut impl Rng,
    ) -> Action {
        if let Some(early) = self.run_pre_decision_overrides(
            character, nearby_characters, Some(terrain.base), Some(phase), None,
            &world::config::GameConfig::default(), rng,
        ) { return early; }
        let scarce = matches!(terrain.base, BaseTerrain::Desert | BaseTerrain::Tundra | BaseTerrain::Badlands);
        let concealed = matches!(terrain.base.visibility(), Visibility::Concealed);
        let base = if nearby_characters == 0 { self.decide_action_no_enemies(character) }
            else if nearby_characters < LOW_ENEMY_LIMIT { self.decide_action_few_enemies_with_terrain(character, concealed) }
            else { self.decide_action_many_enemies_with_terrain(character, concealed) };
        let tuning = crate::combat_tuning::CombatTuning::default();
        let base = if matches!(base, Action::Attack) && action_score(character, &Action::Attack, &[], &tuning) == i32::MIN
        { Action::Rest } else { base };
        match base {
            Action::Move(None) if scarce => Action::Move(None),
            Action::Hide if concealed => Action::Hide,
            other => other,
        }
    }

    fn decide_action_few_enemies_with_terrain(&self, t: &Character, concealed: bool) -> Action {
        let lh = self.thresholds.low_health; let mh = self.thresholds.mid_health; let ls = self.thresholds.low_sanity;
        match t.effective_health() {
            h if h < lh => self.decide_action_few_enemies_low_health_with_terrain(t, concealed),
            h if h >= lh && h <= mh => {
                if t.effective_sanity() > ls && concealed { Action::Hide }
                else if t.effective_sanity() > ls { Action::Move(None) } else { Action::Attack }
            }
            _ if concealed && t.effective_sanity() > ls => Action::Hide,
            _ => Action::Attack,
        }
    }

    fn decide_action_few_enemies_low_health_with_terrain(&self, t: &Character, concealed: bool) -> Action {
        let lm = self.thresholds.low_movement; let ms = self.thresholds.mid_sanity; let es = self.thresholds.extreme_low_sanity;
        let s = (t.attributes.movement, t.effective_sanity(), t.attributes.is_hidden);
        match s {
            (m, sa, false) if m < lm && sa >= ms && concealed => Action::Hide,
            (m, sa, false) if m < lm && sa >= ms => Action::Hide,
            (m, sa, _) if m < lm && sa >= es && sa < ms => Action::Attack,
            (_, sa, false) if sa >= ms => Action::Move(None),
            (_, sa, false) if sa < ms => Action::Attack,
            (_, _, true) => Action::None,
            _ => Action::Move(None),
        }
    }

    fn decide_action_many_enemies_with_terrain(&self, t: &Character, _concealed: bool) -> Action {
        let hi = self.thresholds.high_intelligence; let li = self.thresholds.low_intelligence;
        let r = 100u32.saturating_sub(t.attributes.intelligence).saturating_sub(t.effective_sanity());
        match r {
            r if r < hi => Action::Move(None), r if r >= li => Action::Attack,
            _ => Action::Hide,
        }
    }

    pub(crate) fn decide_action_no_enemies(&self, t: &Character) -> Action {
        let lh = self.thresholds.low_health; let mh = self.thresholds.mid_health; let ls = self.thresholds.low_sanity;
        match t.effective_health() {
            h if h < lh => Action::Rest,
            h if h >= lh && h <= mh => {
                if t.effective_sanity() > ls && t.is_visible() { Action::Hide } else { Action::Move(None) }
            }
            _ => if t.attributes.movement == 0 { Action::Rest } else { Action::Move(None) },
        }
    }

    pub(crate) fn decide_action_few_enemies(&self, t: &Character) -> Action {
        let lh = self.thresholds.low_health; let mh = self.thresholds.mid_health; let ls = self.thresholds.low_sanity;
        match t.effective_health() {
            h if h < lh => self.decide_action_few_enemies_low_health(t),
            h if h >= lh && h <= mh => {
                if t.effective_sanity() > ls { Action::Move(None) } else { Action::Attack }
            }
            _ => Action::Attack,
        }
    }

    fn decide_action_few_enemies_low_health(&self, t: &Character) -> Action {
        let lm = self.thresholds.low_movement; let ms = self.thresholds.mid_sanity; let es = self.thresholds.extreme_low_sanity;
        let s = (t.attributes.movement, t.effective_sanity(), t.attributes.is_hidden);
        match s {
            (m, sa, false) if m < lm && sa >= ms => Action::Hide,
            (m, sa, _) if m < lm && sa >= es && sa < ms => Action::Attack,
            (_, sa, false) if sa >= ms => Action::Move(None),
            (_, sa, false) if sa < ms => Action::Attack,
            (_, _, true) => Action::None,
            _ => Action::Move(None),
        }
    }

    pub(crate) fn decide_action_many_enemies(&self, t: &Character) -> Action {
        let hi = self.thresholds.high_intelligence; let li = self.thresholds.low_intelligence;
        let r = 100u32.saturating_sub(t.attributes.intelligence).saturating_sub(t.effective_sanity());
        match r { r if r < hi => Action::Move(None), r if r >= li => Action::Attack, _ => Action::Hide }
    }

    pub(crate) fn wants_to_propose_alliance(
        &self, character: &Character, nearby_characters: u32, rng: &mut impl Rng,
    ) -> bool {
        use crate::alliances::MAX_ALLIES;
        if nearby_characters == 0 { return false; }
        if character.allies.len() >= MAX_ALLIES { return false; }
        if character.effective_health() < self.thresholds.low_health
            || character.effective_sanity() < self.thresholds.low_sanity { return false; }
        if character.traits.iter().any(|t| REFUSERS.contains(t)) { return false; }
        let affinity = geometric_mean_affinity(&character.traits);
        if affinity < 1.0 { return false; }
        rng.random_bool((0.05 * affinity).clamp(0.0, 0.15))
    }
}
