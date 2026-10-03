//! The Chronicler — deterministic, calculable commentary.
//!
//! Narration is drawn from fixed template banks keyed by the phase context
//! (day, phase, alive count) and the kinds of events in the package. The
//! same package always produces the same lines: there is no RNG and no
//! network call, only a hash-derived index into each bank.

use async_trait::async_trait;
use futures::stream;
use futures::stream::Stream;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::pin::Pin;

use crate::commentator::Commentator;
use crate::types::{
    BroadcastPackage, CommentaryError, CommentaryLine, CommentarySegment, EventKind,
};

/// Speaker name used on every Chronicler line.
pub const SPEAKER: &str = "The Chronicler";

/// Reported as `model_used` on generated segments.
pub const MODEL: &str = "chronicler-v1";

const OPENERS: [&str; 4] = [
    "Day {day}, {phase}: {alive} goblins still drawing breath.",
    "The torches gutter over Day {day}'s {phase}. {alive} left standing.",
    "{alive} goblins crouch in the dark on Day {day}, {phase}. Nobody is safe.",
    "Day {day}, {phase}. The warrens hold their breath — {alive} remain.",
];

const LEADER_LINES: [&str; 3] = [
    "{name} of Clan {clan} has {kills} this phase and isn't slowing down.",
    "The skull count belongs to {name} of Clan {clan}: {kills}.",
    "The warrens mutter one name tonight — {name} of Clan {clan}, {kills}.",
];

const SPREE_LINES: [&str; 3] = [
    "{name} is on a streak — {streak} in a row.",
    "{streak} straight for {name}. Someone hide.",
    "The streak lives: {name}, {streak} kills deep.",
];

const ZONE_LINES: [&str; 2] = [
    "{zone} is crawling with trouble.",
    "All eyes on {zone} — that is where the noise is.",
];

const AMBIENT_LINES: [&str; 3] = [
    "Dripping water, scuffling claws, and no one friendly moving out there.",
    "Quiet. Too quiet — the kind of quiet that precedes screaming.",
    "Somewhere a goblin is sharpening something. Probably more than one.",
];

const CLOSERS: [&str; 4] = [
    "Someone's luck is about to run out.",
    "The next phase may decide the whole affair.",
    "Keep to the tunnels. It isn't over.",
    "Count your teeth while you still have them.",
];

const EVENT_PREFIXES: [(EventKind, [&str; 2]); 10] = [
    (EventKind::Death, ["The tally grows —", "Skull pile —"]),
    (EventKind::Combat, ["Steel sings —", "Claws out —"]),
    (
        EventKind::Wound,
        ["Blood on the stones —", "That's going to scar —"],
    ),
    (EventKind::Movement, ["Footsteps —", "Rustle in the dark —"]),
    (
        EventKind::Hazard,
        ["The arena strikes —", "Watch the ground —"],
    ),
    (EventKind::Item, ["Something glints —", "A prize nearby —"]),
    (
        EventKind::Patron,
        ["A gift from on high —", "The elders approve —"],
    ),
    (
        EventKind::Allied,
        ["Friends are a weakness —", "Clan mates close in —"],
    ),
    (
        EventKind::Betrayal,
        ["Trust is for fools —", "Knives from friends —"],
    ),
    (
        EventKind::Desperate,
        ["Desperation reeks —", "Cornered and snarling —"],
    ),
];

/// Pick a stable index for `key` within a bank of `len`, seeded by `seed`.
fn pick(seed: &str, key: &str, len: usize) -> usize {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    key.hash(&mut hasher);
    (hasher.finish() % len as u64) as usize
}

fn line(text: String) -> CommentaryLine {
    CommentaryLine {
        speaker: SPEAKER.to_string(),
        text,
    }
}

/// Deterministic commentator: same package in, same lines out.
#[derive(Debug, Default, Clone, Copy)]
pub struct Chronicler;

impl Chronicler {
    pub fn new() -> Self {
        Self
    }

    /// Compose the narration for one package. Pure function of its input.
    fn compose(&self, package: &BroadcastPackage) -> CommentarySegment {
        let header = &package.header;
        let seed = format!("{}:{}:{}", header.day, header.phase, header.alive_count);

        let mut lines = Vec::new();

        let opener = OPENERS[pick(&seed, "opener", OPENERS.len())]
            .replace("{day}", &header.day.to_string())
            .replace("{phase}", &header.phase)
            .replace("{alive}", &header.alive_count.to_string());
        lines.push(line(opener));

        if let Some(leader) = header.kill_leaders.first() {
            let text = LEADER_LINES[pick(&seed, "leader", LEADER_LINES.len())]
                .replace("{name}", &leader.name)
                .replace("{clan}", &leader.clan.to_string())
                .replace(
                    "{kills}",
                    &format!(
                        "{} kill{}",
                        leader.kill_count,
                        if leader.kill_count == 1 { "" } else { "s" }
                    ),
                );
            lines.push(line(text));
        }

        if let Some(spree) = header.killing_sprees.first() {
            let text = SPREE_LINES[pick(&seed, "spree", SPREE_LINES.len())]
                .replace("{name}", &spree.name)
                .replace("{streak}", &spree.streak.to_string());
            lines.push(line(text));
        }

        if let Some(zone) = header.hot_zones.first() {
            let text =
                ZONE_LINES[pick(&seed, "zone", ZONE_LINES.len())].replace("{zone}", &zone.name);
            lines.push(line(text));
        }

        for event in package
            .events
            .iter()
            .filter(|e| e.kind != EventKind::Other && !e.prose.trim().is_empty())
            .take(2)
        {
            let key = format!("event-{:?}", event.kind);
            let prefix_pool = EVENT_PREFIXES
                .iter()
                .find(|(kind, _)| *kind == event.kind)
                .map(|(_, pool)| *pool);
            let prefix = match prefix_pool {
                Some(pool) => pool[pick(&seed, &key, pool.len())],
                None => "Word travels fast —",
            };
            lines.push(line(format!("{prefix} {}", event.prose)));
        }

        // No leaders and no events: keep the segment alive with ambient color.
        if lines.len() == 1 {
            lines.push(line(
                AMBIENT_LINES[pick(&seed, "ambient", AMBIENT_LINES.len())].to_string(),
            ));
        }

        if lines.len() < 5 {
            lines.push(line(
                CLOSERS[pick(&seed, "closer", CLOSERS.len())].to_string(),
            ));
        }

        CommentarySegment {
            id: uuid::Uuid::new_v4().to_string(),
            game_id: String::new(),
            day: header.day,
            phase: header.phase.clone(),
            lines,
            generated_at: chrono::Utc::now(),
            model_used: MODEL.to_string(),
        }
    }
}

#[async_trait]
impl Commentator for Chronicler {
    async fn generate(
        &self,
        package: &BroadcastPackage,
    ) -> Result<CommentarySegment, CommentaryError> {
        Ok(self.compose(package))
    }

    fn generate_stream(
        &self,
        package: &BroadcastPackage,
    ) -> Pin<Box<dyn Stream<Item = Result<CommentaryLine, CommentaryError>> + Send>> {
        let items: Vec<_> = self.compose(package).lines.into_iter().map(Ok).collect();
        Box::pin(stream::iter(items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AreaActivity, GameStateSnapshot, KillLeader, KillingSpree};

    fn header(day: u32) -> GameStateSnapshot {
        GameStateSnapshot {
            day,
            phase: "night".into(),
            alive_count: 12,
            kill_leaders: vec![KillLeader {
                name: "Tears of the Mushroom".into(),
                clan: 4,
                kill_count: 2,
            }],
            alliances: vec![],
            hot_zones: vec![AreaActivity {
                name: "Mirefall".into(),
                activity_level: "hot".into(),
            }],
            killing_sprees: vec![KillingSpree {
                name: "Shine on the Moon".into(),
                clan: 7,
                streak: 3,
                label: "on fire".into(),
            }],
        }
    }

    fn package(day: u32) -> BroadcastPackage {
        BroadcastPackage {
            header: header(day),
            events: vec![],
            histories: vec![],
        }
    }

    #[tokio::test]
    async fn same_package_same_lines() {
        let chronicler = Chronicler::new();
        let pkg = package(3);
        let a = chronicler.generate(&pkg).await.unwrap();
        let b = chronicler.generate(&pkg).await.unwrap();
        let texts_a: Vec<_> = a.lines.iter().map(|l| l.text.clone()).collect();
        let texts_b: Vec<_> = b.lines.iter().map(|l| l.text.clone()).collect();
        assert_eq!(texts_a, texts_b);
    }

    #[tokio::test]
    async fn all_lines_speak_as_the_chronicler() {
        let chronicler = Chronicler::new();
        let segment = chronicler.generate(&package(1)).await.unwrap();
        assert!(!segment.lines.is_empty());
        for l in &segment.lines {
            assert_eq!(l.speaker, SPEAKER);
            assert!(!l.text.is_empty());
        }
        assert_eq!(segment.model_used, MODEL);
    }

    #[tokio::test]
    async fn different_contexts_change_the_narration() {
        let chronicler = Chronicler::new();
        let a = chronicler.generate(&package(1)).await.unwrap();
        let b = chronicler.generate(&package(997)).await.unwrap();
        assert_ne!(a.lines[0].text, b.lines[0].text);
    }

    #[tokio::test]
    async fn empty_package_still_narrates() {
        let chronicler = Chronicler::new();
        let pkg = BroadcastPackage {
            header: GameStateSnapshot {
                day: 5,
                phase: "dawn".into(),
                alive_count: 2,
                kill_leaders: vec![],
                alliances: vec![],
                hot_zones: vec![],
                killing_sprees: vec![],
            },
            events: vec![],
            histories: vec![],
        };
        let segment = chronicler.generate(&pkg).await.unwrap();
        assert!(segment.lines.len() >= 2, "ambient + closer minimum");
        assert!(segment.lines.len() <= 6);
    }
}
