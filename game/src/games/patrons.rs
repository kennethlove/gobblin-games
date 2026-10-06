use super::*;
use rand::Rng;

impl Game {
    /// Spawn one patron per archetype using the shared catalog.
    /// Loyalist gets a randomly-assigned team (1..=8). Budget is rolled
    /// inside the archetype's budget band. Idempotent: no-op if `self.patrons`
    /// is already populated.
    pub fn spawn_patrons(&mut self, rng: &mut impl Rng) {
        use shared::patrons::{ARCHETYPES, ArchetypeId, Patron};
        use std::collections::HashMap;

        if !self.patrons.is_empty() {
            return;
        }

        for (idx, archetype) in ARCHETYPES.iter().enumerate() {
            let (lo, hi) = archetype.budget_band;
            let budget = rng.random_range(lo..=hi);
            let bound_team = if archetype.id == ArchetypeId::Loyalist {
                Some(rng.random_range(1u8..=8))
            } else {
                None
            };

            self.patrons.push(Patron {
                id: idx as u32,
                archetype: archetype.id,
                budget_remaining: budget,
                bound_team,
                affinity: HashMap::new(),
            });
        }
    }

    /// Test helper: returns `(canonical_name, character_identifier, affinity)` triples.
    pub fn patron_affinity_snapshot(&self) -> Vec<(&'static str, String, i32)> {
        let mut out = Vec::new();
        for s in &self.patrons {
            let mut entries: Vec<_> = s.affinity.iter().collect();
            entries.sort_by_key(|(k, _)| (*k).clone());
            for (character, value) in entries {
                out.push((s.canonical_name(), character.clone(), *value));
            }
        }
        out
    }
}
