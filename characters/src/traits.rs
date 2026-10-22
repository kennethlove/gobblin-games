//! Character trait system. Replaces `BrainPersonality`. See spec
//! `docs/superpowers/specs/2026-04-25-character-alliances-design.md` §5.

use rand::Rng;
use rand::RngExt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Trait {
    // Combat stance
    Aggressive,
    Defensive,
    Cautious,
    Reckless,
    // Social
    Friendly,
    Loyal,
    Paranoid,
    LoneWolf,
    Treacherous,
    // Mental
    Resilient,
    Fragile,
    Cunning,
    Dim,
    // Physical
    Asthmatic,
    Nearsighted,
    Tough,
    // Survival (shelter + hunger/thirst spec)
    Builder,
    ResourcefulForager,
}

/// Display/tuning classification (spec §3: display-only for now).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TraitKind {
    Boon,
    Curse,
    Personality,
}

/// Character attribute axes referenced by [`Effect::AttrMod`].
///
/// Persistence-stable keys: old serialized `Defense`/`Agility` keep loading
/// via serde aliases. Adding an axis (e.g. `Swim`) is additive — append a
/// variant, add the `Attributes` field + `get` arm + config max, and every
/// exhaustive `match` here will point at the sites to update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CharacterAttribute {
    Strength,
    #[serde(alias = "Defense")]
    Toughness,
    Bravery,
    Intelligence,
    Persuasion,
    Luck,
    #[serde(alias = "Agility")]
    Speed,
    Perception,
}

impl CharacterAttribute {
    /// Display name for human-facing effect text ("-25% perception"), not
    /// the `Debug`/serde form ("Perception") — that is the persistence key.
    pub fn display_name(self) -> &'static str {
        use CharacterAttribute::*;
        match self {
            Strength => "strength",
            Toughness => "toughness",
            Bravery => "bravery",
            Intelligence => "intelligence",
            Persuasion => "persuasion",
            Luck => "luck",
            Speed => "speed",
            Perception => "perception",
        }
    }
}

/// Actions a trait can unlock; the single variant is reserved for the v2
/// Call Ally combat action (spec §3) and has no behavior yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionKind {
    CallAlly,
}

/// One typed effect carried by a trait (spec §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Effect {
    /// Attribute modifier: applied as `(base + flat) x (1 + percent/100)`.
    AttrMod {
        attr: CharacterAttribute,
        flat: i32,
        percent: i32,
    },
    /// Maximum-stamina percent modifier. Stamina predates the attribute
    /// axes (`Character` carries `max_stamina` directly), so this is a
    /// dedicated channel rather than a `CharacterAttribute` variant.
    /// Percentages accumulate additively and are applied at character
    /// creation via [`stamina_max_percent_sum`].
    MaxStaminaPercent {
        percent: i32,
    },
    /// Behavior-axis weight read by alliance/threshold/betrayal/brain/
    /// phobia consumers (see [`BehaviorAxis`]).
    BehaviorWeight {
        axis: BehaviorAxis,
        delta: i32,
    },
    Unlock {
        action: ActionKind,
    },
}

/// Every trait-influenced behavior axis that used to live as a hard-coded
/// `match` in a consumer (spec §3). Consumers read sums of these now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BehaviorAxis {
    // Alliances (decisions.rs, alliances.rs, brains/mod.rs)
    /// Per-mille multiplier: 1500 = x1.5 affinity (default 1000 when absent).
    AllianceAffinity,
    AllianceRefusal,
    // Brain thresholds (brains/mod.rs PersonalityThresholds::from_traits)
    LowHealthLimit,
    MidHealthLimit,
    LowSanityLimit,
    MidSanityLimit,
    HighSanityLimit,
    MovementLimit,
    LowIntelligenceLimit,
    HighIntelligenceLimit,
    PsychoticBreakThreshold,
    // Betrayal (lib.rs process_turn_phase; interval = delta)
    BetrayalCadence,
    // Combat brain (brains/combat.rs)
    AttackPosture,
    FleeCowardice,
    SpareInclination,
    ExecuteFromTreachery,
    ExecuteFromAggression,
    /// Positive while a trait drives ganging up: reinforcements when the
    /// actor's side already outnumbers (gangs legal only for such traits).
    GangUp,
    // Phobia/fixation severity (afflictions/phobia/reaction.rs,
    // brains/fixation_override.rs)
    SeverityTier,
    FixationCharacterTier,
    FreezeImmunity,
}

/// Static definition for a trait variant (spec §3's `Trait` struct, split
/// so the persisted unit enum stays the stable serde key).
#[derive(Debug, Clone, Copy)]
pub struct TraitDef {
    pub label: &'static str,
    pub kind: TraitKind,
    pub effects: &'static [Effect],
}

/// One trait as presentation surfaces consume it (spec §10.7): the display
/// name, a `Boon`/`Curse` tag when the kind is shown, and one plain phrase
/// per visible effect. Personality traits carry no tag and no effect lines —
/// that kind stays internal-only. Pure data: no HTML (Tera escapes it).
/// A temporary trait (event boon/curse with an N-game duration, see
/// `Character::temporary_traits`) additionally carries its games-left
/// count; permanent traits keep the old payload shape (field skipped when
/// `None`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraitView {
    pub name: String,
    pub kind_tag: Option<String>,
    pub effect_lines: Vec<String>,
    /// Games left before a temporary trait expires, `None` for permanent
    /// traits. Serialized only when present so old payloads keep their
    /// shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub games_remaining: Option<u32>,
}

/// One human phrase per effect for the trait-view surfaces. `Unlock` says
/// nothing yet: the single variant (Call Ally, spec §3) has no behavior.
fn effect_phrase(effect: &Effect) -> Option<String> {
    match effect {
        Effect::AttrMod {
            attr,
            flat,
            percent,
        } => match (*flat, *percent) {
            (0, 0) => None,
            // Percent wins when an effect carries both: it applies after
            // the flat term in the effective-value formula (spec §2).
            (_, 0) => Some(format!("{flat:+} {}", attr.display_name())),
            (_, percent) => Some(format!("{percent:+}% {}", attr.display_name())),
        },
        Effect::MaxStaminaPercent { percent } if *percent != 0 => {
            Some(format!("{percent:+}% max stamina"))
        }
        Effect::MaxStaminaPercent { .. } => None,
        Effect::BehaviorWeight { .. } => Some("steers decisions".to_string()),
        Effect::Unlock { .. } => None,
    }
}

/// Effect phrases for one trait, in effect order, with repeated phrases
/// collapsed (several `BehaviorWeight` rows on a Boon/Curse would otherwise
/// print "steers decisions" over and over).
fn visible_effect_lines(effects: &[Effect]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for phrase in effects.iter().filter_map(effect_phrase) {
        if !lines.contains(&phrase) {
            lines.push(phrase);
        }
    }
    lines
}

impl Trait {
    /// Display/tuning classification (spec §3: display-only for now).
    pub fn kind(&self) -> TraitKind {
        self.def().kind
    }

    /// This trait's effect rows — the data behind every consumer that used
    /// to hard-code a `match` on the variant (alliance, thresholds,
    /// betrayal cadence, combat brain, phobia/fixation tiers).
    pub fn effects(&self) -> &'static [Effect] {
        self.def().effects
    }

    /// The presentation view of this trait (spec §10.7): name plus
    /// Boon/Curse tag and effect lines; Personality shows name only.
    pub fn view(&self) -> TraitView {
        let def = self.def();
        let kind_tag = match def.kind {
            TraitKind::Personality => None,
            TraitKind::Boon => Some("Boon".to_string()),
            TraitKind::Curse => Some("Curse".to_string()),
        };
        let personality = kind_tag.is_none();
        TraitView {
            name: def.label.to_string(),
            kind_tag,
            effect_lines: if personality {
                Vec::new()
            } else {
                visible_effect_lines(def.effects)
            },
            games_remaining: None,
        }
    }

    /// This trait's view with the temporary-trait countdown attached (an
    /// event boon/curse that lasts N games — see
    /// `Character::temporary_traits`).
    pub fn with_games_remaining(&self, games_remaining: u32) -> TraitView {
        TraitView {
            games_remaining: Some(games_remaining),
            ..self.view()
        }
    }

    /// Static definition (label, kind, effects) for this variant.
    pub fn def(&self) -> TraitDef {
        use BehaviorAxis::*;
        use Effect::BehaviorWeight as W;

        const AGGRESSIVE: &[Effect] = &[
            W {
                axis: LowHealthLimit,
                delta: -5,
            },
            W {
                axis: MidHealthLimit,
                delta: -10,
            },
            W {
                axis: LowSanityLimit,
                delta: -2,
            },
            W {
                axis: PsychoticBreakThreshold,
                delta: 2,
            },
            W {
                axis: AttackPosture,
                delta: 1,
            },
            W {
                axis: ExecuteFromAggression,
                delta: 1,
            },
            W {
                axis: GangUp,
                delta: 1,
            },
        ];
        const DEFENSIVE: &[Effect] = &[
            W {
                axis: LowHealthLimit,
                delta: 10,
            },
            W {
                axis: MidHealthLimit,
                delta: 10,
            },
            W {
                axis: PsychoticBreakThreshold,
                delta: -2,
            },
        ];
        const CAUTIOUS: &[Effect] = &[
            W {
                axis: LowHealthLimit,
                delta: 15,
            },
            W {
                axis: MidHealthLimit,
                delta: 15,
            },
            W {
                axis: LowSanityLimit,
                delta: 10,
            },
            W {
                axis: MidSanityLimit,
                delta: 10,
            },
            W {
                axis: PsychoticBreakThreshold,
                delta: -3,
            },
            W {
                axis: FleeCowardice,
                delta: 1,
            },
        ];
        const RECKLESS: &[Effect] = &[
            W {
                axis: LowHealthLimit,
                delta: -10,
            },
            W {
                axis: LowSanityLimit,
                delta: -10,
            },
            W {
                axis: PsychoticBreakThreshold,
                delta: 4,
            },
            W {
                axis: AttackPosture,
                delta: 1,
            },
            W {
                axis: FreezeImmunity,
                delta: 1,
            },
            W {
                axis: GangUp,
                delta: 1,
            },
        ];
        const FRIENDLY: &[Effect] = &[
            W {
                axis: AllianceAffinity,
                delta: 1500,
            },
            W {
                axis: SpareInclination,
                delta: 1,
            },
        ];
        const LOYAL: &[Effect] = &[
            W {
                axis: AllianceAffinity,
                delta: 1400,
            },
            W {
                axis: SpareInclination,
                delta: 1,
            },
            W {
                axis: FixationCharacterTier,
                delta: 1,
            },
        ];
        const PARANOID: &[Effect] = &[
            W {
                axis: AllianceAffinity,
                delta: 500,
            },
            W {
                axis: AllianceRefusal,
                delta: 1,
            },
        ];
        const LONE_WOLF: &[Effect] = &[
            W {
                axis: AllianceAffinity,
                delta: 600,
            },
            W {
                axis: AllianceRefusal,
                delta: 1,
            },
        ];
        const TREACHEROUS: &[Effect] = &[
            W {
                axis: AllianceAffinity,
                delta: 1200,
            },
            // Mirrors alliances::TREACHEROUS_BETRAYAL_INTERVAL (parity-tested).
            W {
                axis: BetrayalCadence,
                delta: 5,
            },
            W {
                axis: ExecuteFromTreachery,
                delta: 1,
            },
        ];
        const RESILIENT: &[Effect] = &[
            W {
                axis: PsychoticBreakThreshold,
                delta: -3,
            },
            W {
                axis: LowSanityLimit,
                delta: -3,
            },
            W {
                axis: SeverityTier,
                delta: -1,
            },
        ];
        const FRAGILE: &[Effect] = &[
            W {
                axis: PsychoticBreakThreshold,
                delta: 3,
            },
            W {
                axis: LowSanityLimit,
                delta: 5,
            },
            W {
                axis: SeverityTier,
                delta: 1,
            },
        ];
        const CUNNING: &[Effect] = &[
            W {
                axis: LowIntelligenceLimit,
                delta: -5,
            },
            W {
                axis: HighIntelligenceLimit,
                delta: -5,
            },
        ];
        const DIM: &[Effect] = &[
            W {
                axis: LowIntelligenceLimit,
                delta: 10,
            },
            W {
                axis: HighIntelligenceLimit,
                delta: 5,
            },
        ];
        const NONE: &[Effect] = &[];

        match self {
            Trait::Aggressive => TraitDef {
                label: "aggressive",
                kind: TraitKind::Personality,
                effects: AGGRESSIVE,
            },
            Trait::Defensive => TraitDef {
                label: "defensive",
                kind: TraitKind::Personality,
                effects: DEFENSIVE,
            },
            Trait::Cautious => TraitDef {
                label: "cautious",
                kind: TraitKind::Personality,
                effects: CAUTIOUS,
            },
            Trait::Reckless => TraitDef {
                label: "reckless",
                kind: TraitKind::Personality,
                effects: RECKLESS,
            },
            Trait::Friendly => TraitDef {
                label: "friendly",
                kind: TraitKind::Personality,
                effects: FRIENDLY,
            },
            Trait::Loyal => TraitDef {
                label: "loyal",
                kind: TraitKind::Personality,
                effects: LOYAL,
            },
            Trait::Paranoid => TraitDef {
                label: "paranoid",
                kind: TraitKind::Personality,
                effects: PARANOID,
            },
            Trait::LoneWolf => TraitDef {
                label: "a lone wolf",
                kind: TraitKind::Personality,
                effects: LONE_WOLF,
            },
            Trait::Treacherous => TraitDef {
                label: "treacherous",
                kind: TraitKind::Personality,
                effects: TREACHEROUS,
            },
            Trait::Resilient => TraitDef {
                label: "resilient",
                kind: TraitKind::Boon,
                effects: RESILIENT,
            },
            Trait::Fragile => TraitDef {
                label: "fragile",
                kind: TraitKind::Curse,
                effects: FRAGILE,
            },
            Trait::Cunning => TraitDef {
                label: "cunning",
                kind: TraitKind::Boon,
                effects: CUNNING,
            },
            Trait::Dim => TraitDef {
                label: "dim",
                kind: TraitKind::Curse,
                effects: DIM,
            },
            Trait::Asthmatic => TraitDef {
                label: "asthmatic",
                kind: TraitKind::Curse,
                effects: &[Effect::MaxStaminaPercent { percent: -15 }],
            },
            Trait::Nearsighted => TraitDef {
                label: "nearsighted",
                kind: TraitKind::Curse,
                effects: &[Effect::AttrMod {
                    attr: CharacterAttribute::Perception,
                    flat: 0,
                    percent: -25,
                }],
            },
            Trait::Tough => TraitDef {
                label: "tough",
                kind: TraitKind::Boon,
                effects: &[Effect::AttrMod {
                    attr: CharacterAttribute::Toughness,
                    flat: 0,
                    percent: 20,
                }],
            },
            Trait::Builder => TraitDef {
                label: "a builder",
                kind: TraitKind::Boon,
                effects: NONE,
            },
            Trait::ResourcefulForager => TraitDef {
                label: "a resourceful forager",
                kind: TraitKind::Boon,
                effects: NONE,
            },
        }
    }

    /// Sum of this trait's weight on `axis` (0 when it carries none).
    pub fn behavior_weight(&self, axis: BehaviorAxis) -> i32 {
        self.effects()
            .iter()
            .filter_map(|e| match e {
                Effect::BehaviorWeight { axis: a, delta } if *a == axis => Some(*delta),
                _ => None,
            })
            .sum()
    }

    pub fn label(&self) -> &'static str {
        self.def().label
    }

    pub fn alliance_affinity(&self) -> f64 {
        let per_mille = self.behavior_weight(BehaviorAxis::AllianceAffinity);
        if per_mille == 0 {
            1.0
        } else {
            per_mille as f64 / 1000.0
        }
    }
}

/// Sum of `axis` across a trait set (0 when nothing contributes).
pub fn behavior_sum(traits: &[Trait], axis: BehaviorAxis) -> i32 {
    traits.iter().map(|t| t.behavior_weight(axis)).sum()
}

/// Sum of `MaxStaminaPercent` effects across a trait set (0 when nothing
/// contributes). Fed into the creation-time stamina roll by the
/// `Character::new` family.
pub fn stamina_max_percent_sum(traits: &[Trait]) -> i32 {
    traits
        .iter()
        .flat_map(|t| t.effects())
        .filter_map(|e| match e {
            Effect::MaxStaminaPercent { percent } => Some(*percent),
            _ => None,
        })
        .sum()
}

/// Why a trait could not be gained (spec §3: one API enforces conflicts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraitError {
    /// The character already carries this trait.
    Duplicate(Trait),
    /// Flavor-pair incompatibility from [`CONFLICTS`].
    FlavorConflict { candidate: Trait, held: Trait },
    /// Two curse-shaped negative percent modifiers on the same attribute
    /// (spec §3 effect-level rule; vacuous until scar traits land).
    NegativeAttrModCollision { candidate: Trait, held: Trait },
}

/// Effect-level conflict rule: at most one negative-percent `AttrMod` per
/// attribute across the held set, regardless of which traits carry it.
fn negative_attrmod_conflict(held: &[Effect], candidate: &[Effect]) -> bool {
    let held_attrs: Vec<CharacterAttribute> = held
        .iter()
        .filter_map(|e| match e {
            Effect::AttrMod { attr, percent, .. } if *percent < 0 => Some(*attr),
            _ => None,
        })
        .collect();
    candidate.iter().any(|e| match e {
        Effect::AttrMod { attr, percent, .. } if *percent < 0 => held_attrs.contains(attr),
        _ => false,
    })
}

/// The eligibility check behind every trait mutation: no duplicates, no
/// flavor-pair conflicts, no stacked negative attribute modifiers. Used by
/// `Character::gain_trait`, creation generation, and revival's picker.
pub fn can_gain(current: &[Trait], candidate: Trait) -> Result<(), TraitError> {
    if current.contains(&candidate) {
        return Err(TraitError::Duplicate(candidate));
    }
    if let Some(held) = current
        .iter()
        .find(|held| conflicts_with(candidate, **held))
    {
        return Err(TraitError::FlavorConflict {
            candidate,
            held: *held,
        });
    }
    let held_effects: Vec<Effect> = current.iter().flat_map(|t| t.effects()).copied().collect();
    if negative_attrmod_conflict(&held_effects, candidate.effects()) {
        return Err(TraitError::NegativeAttrModCollision {
            candidate,
            held: current
                .iter()
                .copied()
                .find(|held| negative_attrmod_conflict(held.effects(), candidate.effects()))
                .unwrap_or(candidate),
        });
    }
    Ok(())
}

pub const REFUSERS: &[Trait] = &[Trait::Paranoid, Trait::LoneWolf];

/// Every variant of [`Trait`]. Used by golden-capture tests and (per spec
/// §9) the pool-coverage invariant; append new variants here as they land.
pub const ALL_TRAITS: &[Trait] = &[
    Trait::Aggressive,
    Trait::Defensive,
    Trait::Cautious,
    Trait::Reckless,
    Trait::Friendly,
    Trait::Loyal,
    Trait::Paranoid,
    Trait::LoneWolf,
    Trait::Treacherous,
    Trait::Resilient,
    Trait::Fragile,
    Trait::Cunning,
    Trait::Dim,
    Trait::Asthmatic,
    Trait::Nearsighted,
    Trait::Tough,
    Trait::Builder,
    Trait::ResourcefulForager,
];

/// Whether this trait refuses alliance offers outright.
pub fn refuses_alliance(t: Trait) -> bool {
    REFUSERS.contains(&t)
}

/// The trait's additive threshold deltas (alias kept for golden capture).
pub fn threshold_of(t: Trait) -> ThresholdDelta {
    t.threshold_modifiers()
}

/// Geometric mean of trait affinity values. Returns 1.0 for empty input.
pub fn geometric_mean_affinity(traits: &[Trait]) -> f64 {
    if traits.is_empty() {
        return 1.0;
    }
    let n = traits.len() as f64;
    let product: f64 = traits.iter().map(|t| t.alliance_affinity()).product();
    product.powf(1.0 / n)
}

pub const CONFLICTS: &[(Trait, Trait)] = &[
    (Trait::Friendly, Trait::Paranoid),
    (Trait::Loyal, Trait::Treacherous),
    (Trait::Loyal, Trait::LoneWolf),
    (Trait::Aggressive, Trait::Cautious),
    (Trait::Aggressive, Trait::Defensive),
    (Trait::Reckless, Trait::Cautious),
    (Trait::Resilient, Trait::Fragile),
    (Trait::Cunning, Trait::Dim),
];

pub fn conflicts_with(a: Trait, b: Trait) -> bool {
    CONFLICTS
        .iter()
        .any(|(x, y)| (*x == a && *y == b) || (*x == b && *y == a))
}

/// Weighted trait pool shared by every goblin. Weights sum the eight legacy
/// clan-keyed pools (90), so the overall mix matches the old uniform-clan
/// distribution without keying traits to any grouping. Reckless and LoneWolf
/// never belonged to a legacy clan pool; they join at modest weights below
/// their flavor siblings (Cautious 6, Paranoid 3). Builder and
/// ResourcefulForager (the shelter/forage survival pair) join at moderate
/// weight 4 — beside Reckless/Nearsighted (4), below the unconditional boons
/// (Tough 9, Resilient 8) because their payoff is gated on terrain and day/
/// night phase. Traits roll per goblin from that goblin's own RNG.
pub const TRAIT_POOL: &[(Trait, u8)] = &[
    (Trait::Loyal, 13),
    (Trait::Aggressive, 11),
    (Trait::Tough, 9),
    (Trait::Cunning, 8),
    (Trait::Resilient, 8),
    (Trait::Defensive, 7),
    (Trait::Friendly, 7),
    (Trait::Cautious, 6),
    (Trait::Asthmatic, 5),
    (Trait::Fragile, 5),
    (Trait::Reckless, 4),
    (Trait::Nearsighted, 4),
    (Trait::Builder, 4),
    (Trait::ResourcefulForager, 4),
    (Trait::LoneWolf, 3),
    (Trait::Paranoid, 3),
    (Trait::Dim, 2),
    (Trait::Treacherous, 2),
];

/// Generate a trait set for a single goblin. Rolls 2–6 uniformly,
/// then draws weighted picks from [`TRAIT_POOL`], rejecting conflicts and
/// duplicates. Stops early if the pool cannot satisfy the count; never spins.
pub fn generate_traits(rng: &mut impl Rng) -> Vec<Trait> {
    let pool = TRAIT_POOL;
    let target_count = rng.random_range(2..=6);
    let mut chosen: Vec<Trait> = Vec::with_capacity(target_count);

    let mut remaining: Vec<(Trait, u8)> = pool.to_vec();

    while chosen.len() < target_count && !remaining.is_empty() {
        let total: u32 = remaining.iter().map(|(_, w)| *w as u32).sum();
        if total == 0 {
            break;
        }
        let mut roll = rng.random_range(0..total);
        let mut picked_idx: Option<usize> = None;
        for (i, (_, w)) in remaining.iter().enumerate() {
            if roll < *w as u32 {
                picked_idx = Some(i);
                break;
            }
            roll -= *w as u32;
        }
        let idx = picked_idx.expect("weighted pick must succeed when total > 0");
        let (candidate, _) = remaining.remove(idx);

        // One eligibility check for every mutation path (spec §3).
        if can_gain(&chosen, candidate).is_err() {
            continue;
        }
        chosen.push(candidate);
    }

    chosen
}

/// Additive deltas applied to `PersonalityThresholds`. `i32` so deltas can be
/// signed; final values clamp to u32 ranges in `compute_thresholds`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThresholdDelta {
    pub low_health_limit: i32,
    pub mid_health_limit: i32,
    pub low_sanity_limit: i32,
    pub mid_sanity_limit: i32,
    pub high_sanity_limit: i32,
    pub movement_limit: i32,
    pub low_intelligence_limit: i32,
    pub high_intelligence_limit: i32,
    pub psychotic_break_threshold: i32,
}

impl std::ops::Add for ThresholdDelta {
    type Output = ThresholdDelta;
    fn add(self, rhs: Self) -> Self {
        ThresholdDelta {
            low_health_limit: self.low_health_limit + rhs.low_health_limit,
            mid_health_limit: self.mid_health_limit + rhs.mid_health_limit,
            low_sanity_limit: self.low_sanity_limit + rhs.low_sanity_limit,
            mid_sanity_limit: self.mid_sanity_limit + rhs.mid_sanity_limit,
            high_sanity_limit: self.high_sanity_limit + rhs.high_sanity_limit,
            movement_limit: self.movement_limit + rhs.movement_limit,
            low_intelligence_limit: self.low_intelligence_limit + rhs.low_intelligence_limit,
            high_intelligence_limit: self.high_intelligence_limit + rhs.high_intelligence_limit,
            psychotic_break_threshold: self.psychotic_break_threshold
                + rhs.psychotic_break_threshold,
        }
    }
}

impl std::iter::Sum for ThresholdDelta {
    fn sum<I: Iterator<Item = ThresholdDelta>>(iter: I) -> Self {
        iter.fold(ThresholdDelta::default(), |a, b| a + b)
    }
}

impl Trait {
    /// The trait's additive threshold deltas, read from its
    /// [`BehaviorWeight`] effect rows (formerly a per-variant `match`).
    pub fn threshold_modifiers(&self) -> ThresholdDelta {
        use BehaviorAxis::*;
        let w = |axis| self.behavior_weight(axis);
        ThresholdDelta {
            low_health_limit: w(LowHealthLimit),
            mid_health_limit: w(MidHealthLimit),
            low_sanity_limit: w(LowSanityLimit),
            mid_sanity_limit: w(MidSanityLimit),
            high_sanity_limit: w(HighSanityLimit),
            movement_limit: w(MovementLimit),
            low_intelligence_limit: w(LowIntelligenceLimit),
            high_intelligence_limit: w(HighIntelligenceLimit),
            psychotic_break_threshold: w(PsychoticBreakThreshold),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn affinity_known_values() {
        assert_eq!(Trait::Friendly.alliance_affinity(), 1.5);
        assert_eq!(Trait::Loyal.alliance_affinity(), 1.4);
        assert_eq!(Trait::Treacherous.alliance_affinity(), 1.2);
        assert_eq!(Trait::Aggressive.alliance_affinity(), 1.0);
        assert_eq!(Trait::Tough.alliance_affinity(), 1.0);
        assert_eq!(Trait::LoneWolf.alliance_affinity(), 0.6);
        assert_eq!(Trait::Paranoid.alliance_affinity(), 0.5);
    }

    #[test]
    fn refusers_membership() {
        assert!(REFUSERS.contains(&Trait::Paranoid));
        assert!(REFUSERS.contains(&Trait::LoneWolf));
        assert!(!REFUSERS.contains(&Trait::Friendly));
    }

    #[test]
    fn geometric_mean_empty_is_one() {
        assert_eq!(geometric_mean_affinity(&[]), 1.0);
    }

    #[test]
    fn geometric_mean_single_is_identity() {
        assert!((geometric_mean_affinity(&[Trait::Friendly]) - 1.5).abs() < f64::EPSILON * 10.0);
    }

    #[test]
    fn geometric_mean_two_friendly_one_lonewolf() {
        let g = geometric_mean_affinity(&[Trait::Friendly, Trait::Friendly, Trait::LoneWolf]);
        let expected = (1.5_f64 * 1.5 * 0.6).powf(1.0 / 3.0);
        assert!((g - expected).abs() < f64::EPSILON * 10.0);
    }

    #[test]
    fn conflict_symmetry() {
        let pairs = [
            (Trait::Friendly, Trait::Paranoid),
            (Trait::Loyal, Trait::Treacherous),
            (Trait::Loyal, Trait::LoneWolf),
            (Trait::Aggressive, Trait::Cautious),
            (Trait::Aggressive, Trait::Defensive),
            (Trait::Reckless, Trait::Cautious),
            (Trait::Resilient, Trait::Fragile),
            (Trait::Cunning, Trait::Dim),
        ];
        for (a, b) in pairs {
            assert!(conflicts_with(a, b), "{a:?} should conflict with {b:?}");
            assert!(
                conflicts_with(b, a),
                "{b:?} should conflict with {a:?} (symmetry)"
            );
        }
    }

    #[test]
    fn conflict_allowed_combos_do_not_conflict() {
        assert!(!conflicts_with(Trait::Friendly, Trait::Treacherous));
        assert!(!conflicts_with(Trait::Paranoid, Trait::LoneWolf));
    }

    #[test]
    fn trait_pool_covers_expected_traits() {
        // Spot-check the merged pool still carries traits from opposite
        // legacy clan pools (Loyal was clan 1/2/4/8, Friendly 6/8).
        assert!(TRAIT_POOL.iter().any(|(t, _)| *t == Trait::Loyal));
        assert!(TRAIT_POOL.iter().any(|(t, _)| *t == Trait::Friendly));
        let total: u32 = TRAIT_POOL.iter().map(|(_, w)| *w as u32).sum();
        assert_eq!(
            total, 105,
            "legacy pools (90) plus Reckless 4 + LoneWolf 3 + Builder 4 + ResourcefulForager 4"
        );
    }

    #[test]
    fn every_variant_is_pool_event_only_or_pending() {
        // The pool-coverage invariant (spec §9): every `Trait` variant must
        // sit in exactly one acquisition bucket, so no variant can go dead
        // (defined but unreachable) without a test failure.

        // Traits gained only through explicit game events, never rolled:
        // revival scar/bonus picks all draw from pool traits today.
        const EVENT_ONLY: &[Trait] = &[];

        // Nothing awaits a resolver system today: Builder and
        // ResourcefulForager moved to TRAIT_POOL once the SeekShelter/Forage
        // executors wired their effects. New variants land here until they
        // are pool- or event-wired.
        const PENDING: &[Trait] = &[];

        let mut bucketed: Vec<Trait> = TRAIT_POOL.iter().map(|(t, _)| *t).collect();
        bucketed.extend_from_slice(EVENT_ONLY);
        bucketed.extend_from_slice(PENDING);

        // Exactly once per bucket: no duplicates within or across sets.
        let mut sorted = bucketed.clone();
        sorted.sort_by_key(|t| *t as u8);
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            bucketed.len(),
            "each variant must be bucketed exactly once"
        );

        // Coverage: the union equals every variant.
        let mut expected = ALL_TRAITS.to_vec();
        expected.sort_by_key(|t| *t as u8);
        assert_eq!(
            sorted, expected,
            "TRAIT_POOL ∪ event-only ∪ pending must cover ALL_TRAITS"
        );
    }

    #[test]
    fn asthmatic_carries_a_midrange_stamina_penalty() {
        assert_eq!(stamina_max_percent_sum(&[Trait::Asthmatic]), -15);
        for t in ALL_TRAITS {
            for e in t.effects() {
                if let Effect::MaxStaminaPercent { percent } = e {
                    assert!(
                        (-20..=-5).contains(percent),
                        "{t:?} stamina penalty {percent} must stay in the 5-20% curse band"
                    );
                }
            }
        }
    }

    #[test]
    fn max_stamina_percent_serde_roundtrip() {
        let json = serde_json::to_string(&Effect::MaxStaminaPercent { percent: -15 }).unwrap();
        assert_eq!(json, r#"{"MaxStaminaPercent":{"percent":-15}}"#);
        let back: Effect = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Effect::MaxStaminaPercent { percent: -15 });
    }

    // ---- Trait view rendering (spec §10.7: Boon/Curse tags + effects) ----

    #[test]
    fn curse_views_carry_tag_and_effect_lines() {
        let asthmatic = Trait::Asthmatic.view();
        assert_eq!(asthmatic.name, "asthmatic");
        assert_eq!(asthmatic.kind_tag.as_deref(), Some("Curse"));
        assert_eq!(asthmatic.effect_lines, vec!["-15% max stamina"]);

        let nearsighted = Trait::Nearsighted.view();
        assert_eq!(nearsighted.kind_tag.as_deref(), Some("Curse"));
        assert_eq!(nearsighted.effect_lines, vec!["-25% perception"]);
    }

    #[test]
    fn boon_view_carries_tag_and_flat_or_percent_phrases() {
        let tough = Trait::Tough.view();
        assert_eq!(tough.kind_tag.as_deref(), Some("Boon"));
        assert_eq!(tough.effect_lines, vec!["+20% toughness"]);

        // Flat AttrMods read as "+3 strength"-style phrases; the display
        // name (not the Debug enum) is the axis word.
        let flat = Effect::AttrMod {
            attr: CharacterAttribute::Strength,
            flat: 3,
            percent: 0,
        };
        assert_eq!(effect_phrase(&flat).as_deref(), Some("+3 strength"));
        let down = Effect::AttrMod {
            attr: CharacterAttribute::Perception,
            flat: -2,
            percent: 0,
        };
        assert_eq!(effect_phrase(&down).as_deref(), Some("-2 perception"));
    }

    #[test]
    fn personality_view_is_name_only() {
        let friendly = Trait::Friendly.view();
        assert_eq!(friendly.name, "friendly");
        assert_eq!(friendly.kind_tag, None);
        assert!(friendly.effect_lines.is_empty(), "{friendly:?}");
    }

    #[test]
    fn repeated_behavior_weight_phrases_collapse_to_one_line() {
        // Resilient carries three BehaviorWeight rows — one phrase, once.
        let resilient = Trait::Resilient.view();
        assert_eq!(resilient.effect_lines, vec!["steers decisions"]);
    }

    #[test]
    fn trait_view_serde_roundtrip() {
        let view = Trait::Asthmatic.view();
        let json = serde_json::to_string(&view).unwrap();
        assert!(json.contains(r#""kind_tag":"Curse""#), "{json}");
        assert!(json.contains("-15% max stamina"), "{json}");
        let back: TraitView = serde_json::from_str(&json).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn view_games_remaining_only_on_request() {
        assert_eq!(Trait::Tough.view().games_remaining, None);
        let temporary = Trait::Tough.with_games_remaining(2);
        assert_eq!(temporary.games_remaining, Some(2));
        assert_eq!(temporary.name, "tough");

        // The None case keeps the old payload shape (field omitted), and
        // pre-field JSON still deserializes.
        let json = serde_json::to_string(&Trait::Tough.view()).unwrap();
        assert!(!json.contains("games_remaining"), "{json}");
        let legacy = r#"{"name":"tough","kind_tag":"Boon","effect_lines":["+20% toughness"]}"#;
        let view: TraitView = serde_json::from_str(legacy).unwrap();
        assert_eq!(view.games_remaining, None);
    }

    #[test]
    fn generate_respects_count_when_pool_supports() {
        let mut rng = StdRng::seed_from_u64(42);
        let traits = generate_traits(&mut rng);
        assert!(traits.len() >= 2 && traits.len() <= 6);
        for i in 0..traits.len() {
            for j in (i + 1)..traits.len() {
                assert!(!conflicts_with(traits[i], traits[j]));
            }
        }
    }

    #[test]
    fn generate_no_duplicates() {
        let mut rng = StdRng::seed_from_u64(7);
        let traits = generate_traits(&mut rng);
        let mut sorted: Vec<_> = traits.clone();
        sorted.sort_by_key(|t| *t as u8);
        sorted.dedup();
        assert_eq!(sorted.len(), traits.len());
    }

    #[test]
    fn threshold_delta_aggressive_lowers_health_threshold() {
        let d = Trait::Aggressive.threshold_modifiers();
        assert!(d.low_health_limit < 0);
    }

    #[test]
    fn threshold_delta_zero_traits_is_identity() {
        let total: ThresholdDelta = [].iter().map(|t: &Trait| t.threshold_modifiers()).sum();
        assert_eq!(total, ThresholdDelta::default());
    }

    #[test]
    fn survival_traits_round_trip_serde() {
        for t in [Trait::Builder, Trait::ResourcefulForager] {
            let json = serde_json::to_string(&t).unwrap();
            let back: Trait = serde_json::from_str(&json).unwrap();
            assert_eq!(t, back);
        }
    }

    #[test]
    fn survival_traits_have_labels() {
        assert_eq!(Trait::Builder.label(), "a builder");
        assert_eq!(Trait::ResourcefulForager.label(), "a resourceful forager");
    }

    #[test]
    fn survival_traits_do_not_conflict_with_each_other_or_self() {
        assert!(!conflicts_with(Trait::Builder, Trait::ResourcefulForager));
        assert!(!conflicts_with(Trait::Builder, Trait::Builder));
        assert!(!conflicts_with(
            Trait::ResourcefulForager,
            Trait::ResourcefulForager
        ));
    }

    // ---- Effect-data migration invariants (spec §8.1, §9) ----

    #[test]
    fn every_trait_has_a_def_and_def_matches_enum_count() {
        for t in ALL_TRAITS {
            let def = t.def();
            assert!(!def.label.is_empty(), "{t:?} needs a label");
        }
        assert_eq!(ALL_TRAITS.len(), 18, "ALL_TRAITS must cover every variant");
    }

    #[test]
    fn refusers_const_matches_alliance_refusal_effect_rows() {
        let data: Vec<Trait> = ALL_TRAITS
            .iter()
            .copied()
            .filter(|t| t.behavior_weight(BehaviorAxis::AllianceRefusal) > 0)
            .collect();
        let mut sorted = data.clone();
        sorted.sort_by_key(|t| *t as u8);
        let mut expected = REFUSERS.to_vec();
        expected.sort_by_key(|t| *t as u8);
        assert_eq!(sorted, expected, "REFUSERS must match the effect data");
    }

    #[test]
    fn treacherous_cadence_row_matches_alliances_interval() {
        assert_eq!(
            Trait::Treacherous.behavior_weight(BehaviorAxis::BetrayalCadence) as u8,
            crate::alliances::TREACHEROUS_BETRAYAL_INTERVAL,
            "effect row and the alliances constant must stay in lockstep"
        );
    }

    #[test]
    fn effect_and_kind_serde_roundtrip() {
        let json = serde_json::to_string(&Effect::BehaviorWeight {
            axis: BehaviorAxis::FleeCowardice,
            delta: 1,
        })
        .unwrap();
        let back: Effect = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back,
            Effect::BehaviorWeight {
                axis: BehaviorAxis::FleeCowardice,
                delta: 1
            }
        );

        let kind_json = serde_json::to_string(&TraitKind::Curse).unwrap();
        assert_eq!(
            serde_json::from_str::<TraitKind>(&kind_json).unwrap(),
            TraitKind::Curse
        );
    }

    #[test]
    fn old_pascal_case_trait_arrays_still_deserialize() {
        // Persisted rows store trait arrays as PascalCase variant names; the
        // unit enum keeps that wire format untouched.
        let json = r#"["Friendly","LoneWolf","Tough"]"#;
        let traits: Vec<Trait> = serde_json::from_str(json).unwrap();
        assert_eq!(traits, vec![Trait::Friendly, Trait::LoneWolf, Trait::Tough]);
        let back = serde_json::to_string(&traits).unwrap();
        assert_eq!(back, json);
    }

    #[test]
    fn behavior_sum_only_counts_matching_axes() {
        let set = vec![Trait::Cautious, Trait::Resilient];
        assert_eq!(behavior_sum(&set, BehaviorAxis::FleeCowardice), 1);
        assert_eq!(behavior_sum(&set, BehaviorAxis::SeverityTier), -1);
        assert_eq!(behavior_sum(&set, BehaviorAxis::BetrayalCadence), 0);
    }

    // ---- gain_trait/lose_trait eligibility (spec §3) ----

    #[test]
    fn can_gain_rejects_duplicates_and_flavor_conflicts() {
        assert_eq!(
            can_gain(&[Trait::Friendly], Trait::Friendly),
            Err(TraitError::Duplicate(Trait::Friendly))
        );
        assert!(matches!(
            can_gain(&[Trait::Friendly], Trait::Paranoid),
            Err(TraitError::FlavorConflict { .. })
        ));
        assert!(can_gain(&[Trait::Friendly], Trait::Tough).is_ok());
    }

    #[test]
    fn negative_attrmod_rule_is_per_attribute_across_held_traits() {
        use CharacterAttribute::*;
        let held = vec![Effect::AttrMod {
            attr: Perception,
            flat: 0,
            percent: -25,
        }];
        // Same attribute, another negative percent -> collision.
        let collide = vec![Effect::AttrMod {
            attr: Perception,
            flat: -2,
            percent: -10,
        }];
        assert!(negative_attrmod_conflict(&held, &collide));
        // Different attribute -> fine.
        let other_attr = vec![Effect::AttrMod {
            attr: Strength,
            flat: 0,
            percent: -10,
        }];
        assert!(!negative_attrmod_conflict(&held, &other_attr));
        // Positive modifiers never collide.
        let boon = vec![Effect::AttrMod {
            attr: Perception,
            flat: 0,
            percent: 10,
        }];
        assert!(!negative_attrmod_conflict(&held, &boon));
    }

    #[test]
    fn generation_still_rejects_conflicts_via_can_gain() {
        // Behavior parity: generation routes through can_gain now; every
        // rolled set must satisfy it end-to-end.
        for seed in 0..32u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let rolled = generate_traits(&mut rng);
            for t in &rolled {
                let others: Vec<Trait> = rolled.iter().copied().filter(|x| x != t).collect();
                assert!(can_gain(&others, *t).is_ok(), "seed {seed} set {rolled:?}");
            }
        }
    }
}
