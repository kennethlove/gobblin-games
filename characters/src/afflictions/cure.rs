//! Item-based cure logic for afflictions.
//!
//! Cures are typed ([`CureKind`] on the item), so cure behavior never
//! depends on an item's display name. Applying a cure steps severity down
//! by one tier. Mild afflictions are removed entirely.
//!
//! See spec §7 Cure.

use shared::afflictions::{Affliction, AfflictionKind, CureKind, Severity};

/// Outcome of applying a cure item to a character's afflictions.
#[derive(Debug, Clone, PartialEq)]
pub enum CureOutcome {
    /// Affliction severity stepped down (Severe→Moderate, Moderate→Mild) or removed (Mild→gone).
    Cured {
        affliction: AfflictionKind,
        from: Severity,
        /// None means the affliction was removed (was Mild).
        to: Option<Severity>,
    },
    /// No matching affliction found or item has no effect.
    NoEffect { reason: String },
}

/// Recovery cycles needed per affliction kind when sheltered.
/// Wounded=1, Infected=3, Broken=4. Other reversible afflictions default to 2.
pub fn recovery_cycles(kind: AfflictionKind) -> u32 {
    match kind {
        AfflictionKind::Wounded => 1,
        AfflictionKind::Infected => 3,
        AfflictionKind::BrokenBone => 4,
        _ => 2,
    }
}

/// Apply a cure to a character's afflictions.
///
/// Finds the affliction the cure treats ([`CureKind::affliction_kind`]) and
/// steps severity down by one tier. If the affliction was Mild, it is
/// removed entirely.
///
/// Returns `CureOutcome::Cured` on success, `NoEffect` if no matching affliction.
pub fn apply_cure(afflictions: &mut Vec<Affliction>, cure: CureKind) -> CureOutcome {
    let target_kind = cure.affliction_kind();
    // Find the highest-severity matching affliction to treat.
    let mut best_idx: Option<usize> = None;
    let mut best_severity: Option<Severity> = None;

    for (i, aff) in afflictions.iter().enumerate() {
        if aff.kind == target_kind && !aff.is_permanent() {
            let should_pick = match best_severity {
                None => true,
                Some(best) => aff.severity > best,
            };
            if should_pick {
                best_idx = Some(i);
                best_severity = Some(aff.severity);
            }
        }
    }

    let idx = match best_idx {
        Some(i) => i,
        None => {
            return CureOutcome::NoEffect {
                reason: format!("no {target_kind} affliction to cure"),
            };
        }
    };

    let aff = &afflictions[idx];
    let from = aff.severity;

    match from {
        Severity::Mild => {
            // Mild → remove entirely.
            afflictions.remove(idx);
            CureOutcome::Cured {
                affliction: target_kind,
                from,
                to: None,
            }
        }
        Severity::Moderate => {
            afflictions[idx].severity = Severity::Mild;
            CureOutcome::Cured {
                affliction: target_kind,
                from,
                to: Some(Severity::Mild),
            }
        }
        Severity::Severe => {
            afflictions[idx].severity = Severity::Moderate;
            CureOutcome::Cured {
                affliction: target_kind,
                from,
                to: Some(Severity::Moderate),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::afflictions::{AfflictionSource, BodyPart};

    fn make_affliction(kind: AfflictionKind, severity: Severity) -> Affliction {
        Affliction {
            kind,
            body_part: None,
            severity,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            acquired_cycle: 1,
            last_progressed_cycle: 1,
            trauma_metadata: None,
            phobia_metadata: None,
            fixation_metadata: None,
            addiction_metadata: None,
            trapped_metadata: None,
        }
    }

    #[test]
    fn cure_kind_maps_to_affliction_kind() {
        assert_eq!(CureKind::Wounded.affliction_kind(), AfflictionKind::Wounded);
        assert_eq!(
            CureKind::Infected.affliction_kind(),
            AfflictionKind::Infected
        );
        assert_eq!(
            CureKind::BrokenBone.affliction_kind(),
            AfflictionKind::BrokenBone
        );
    }

    #[test]
    fn apply_cure_severe_to_moderate() {
        let mut affs = vec![make_affliction(AfflictionKind::Wounded, Severity::Severe)];
        let result = apply_cure(&mut affs, CureKind::Wounded);
        assert_eq!(
            result,
            CureOutcome::Cured {
                affliction: AfflictionKind::Wounded,
                from: Severity::Severe,
                to: Some(Severity::Moderate),
            }
        );
        assert_eq!(affs[0].severity, Severity::Moderate);
    }

    #[test]
    fn apply_cure_moderate_to_mild() {
        let mut affs = vec![make_affliction(AfflictionKind::Wounded, Severity::Moderate)];
        let result = apply_cure(&mut affs, CureKind::Wounded);
        assert_eq!(
            result,
            CureOutcome::Cured {
                affliction: AfflictionKind::Wounded,
                from: Severity::Moderate,
                to: Some(Severity::Mild),
            }
        );
        assert_eq!(affs[0].severity, Severity::Mild);
    }

    #[test]
    fn apply_cure_mild_removes_affliction() {
        let mut affs = vec![make_affliction(AfflictionKind::Wounded, Severity::Mild)];
        let result = apply_cure(&mut affs, CureKind::Wounded);
        assert_eq!(
            result,
            CureOutcome::Cured {
                affliction: AfflictionKind::Wounded,
                from: Severity::Mild,
                to: None,
            }
        );
        assert!(affs.is_empty());
    }

    #[test]
    fn apply_cure_no_matching_affliction() {
        let mut affs = vec![make_affliction(
            AfflictionKind::BrokenBone,
            Severity::Moderate,
        )];
        let result = apply_cure(&mut affs, CureKind::Wounded);
        assert!(
            matches!(result, CureOutcome::NoEffect { .. }),
            "expected NoEffect, got {:?}",
            result
        );
    }

    #[test]
    fn apply_cure_treats_highest_severity() {
        let mut affs = vec![
            make_affliction(AfflictionKind::Wounded, Severity::Mild),
            make_affliction(AfflictionKind::Wounded, Severity::Severe),
            make_affliction(AfflictionKind::Wounded, Severity::Moderate),
        ];
        let result = apply_cure(&mut affs, CureKind::Wounded);
        assert_eq!(
            result,
            CureOutcome::Cured {
                affliction: AfflictionKind::Wounded,
                from: Severity::Severe,
                to: Some(Severity::Moderate),
            }
        );
        // Severe became Moderate, Mild and Moderate remain.
        assert_eq!(affs.len(), 3);
        let severities: Vec<_> = affs.iter().map(|a| a.severity).collect();
        assert!(severities.contains(&Severity::Moderate));
        assert!(severities.contains(&Severity::Mild));
    }

    #[test]
    fn apply_cure_splint_for_broken_bone() {
        let mut affs = vec![make_affliction(
            AfflictionKind::BrokenBone,
            Severity::Severe,
        )];
        let result = apply_cure(&mut affs, CureKind::BrokenBone);
        assert_eq!(
            result,
            CureOutcome::Cured {
                affliction: AfflictionKind::BrokenBone,
                from: Severity::Severe,
                to: Some(Severity::Moderate),
            }
        );
    }

    #[test]
    fn apply_cure_antibiotic_for_infected() {
        let mut affs = vec![make_affliction(
            AfflictionKind::Infected,
            Severity::Moderate,
        )];
        let result = apply_cure(&mut affs, CureKind::Infected);
        assert_eq!(
            result,
            CureOutcome::Cured {
                affliction: AfflictionKind::Infected,
                from: Severity::Moderate,
                to: Some(Severity::Mild),
            }
        );
    }

    #[test]
    fn recovery_cycles_values() {
        assert_eq!(recovery_cycles(AfflictionKind::Wounded), 1);
        assert_eq!(recovery_cycles(AfflictionKind::Infected), 3);
        assert_eq!(recovery_cycles(AfflictionKind::BrokenBone), 4);
        // Default for others.
        assert_eq!(recovery_cycles(AfflictionKind::Burned), 2);
        assert_eq!(recovery_cycles(AfflictionKind::Poisoned), 2);
    }

    #[test]
    fn apply_cure_with_body_part() {
        let mut affs = vec![Affliction {
            kind: AfflictionKind::Wounded,
            body_part: Some(BodyPart::Arm),
            severity: Severity::Moderate,
            source: AfflictionSource::Combat {
                attacker_id: "characters:test".into(),
            },
            acquired_cycle: 1,
            last_progressed_cycle: 1,
            trauma_metadata: None,
            phobia_metadata: None,
            fixation_metadata: None,
            addiction_metadata: None,
            trapped_metadata: None,
        }];
        let result = apply_cure(&mut affs, CureKind::Wounded);
        assert!(matches!(result, CureOutcome::Cured { .. }));
        assert_eq!(affs[0].severity, Severity::Mild);
        assert_eq!(affs[0].body_part, Some(BodyPart::Arm));
    }
}
