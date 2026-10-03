//! Integration tests for the announcers commentary pipeline.
//!
//! Tests the full public API flow as the API layer uses it:
//!   1. Create CharacterHistories from a roster
//!   2. Feed phase events through update()
//!   3. Build BroadcastPackage via BroadcastPackageBuilder
//!   4. Generate commentary with a mock Commentator
//!   5. Assert package structure and accumulated history

use announcers::{
    BroadcastPackageBuilder, CharacterDigest, CharacterHistories, CommentaryError, CommentaryLine,
    CommentarySegment, Commentator, GameStateSnapshot,
};
use async_trait::async_trait;
use futures::stream::Stream;
use shared::messages::{
    AreaRef, CharacterRef, GameMessage, ItemRef, MessagePayload, MessageSource, Phase,
};
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

static MSG_COUNTER: AtomicU32 = AtomicU32::new(1);

fn test_uuid(name: &str) -> uuid::Uuid {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();
    uuid::Uuid::from_u128(hash as u128)
}

fn tr(name: &str) -> CharacterRef {
    CharacterRef {
        identifier: test_uuid(name).to_string().into(),
        name: name.into(),
    }
}

fn ar(name: &str) -> AreaRef {
    AreaRef {
        identifier: test_uuid(name).to_string().into(),
        name: name.into(),
    }
}

fn ir(name: &str) -> ItemRef {
    ItemRef {
        identifier: test_uuid(name).to_string().into(),
        name: name.into(),
    }
}

fn make_msg(payload: MessagePayload) -> GameMessage {
    let n = MSG_COUNTER.fetch_add(1, Ordering::SeqCst);
    GameMessage {
        identifier: format!("msg-{n}"),
        source: MessageSource::Game("test-game".into()),
        game_day: 1,
        phase: Phase::Day,
        tick: 0,
        emit_index: n,
        subject: String::new(),
        timestamp: chrono::Utc::now(),
        content: String::new(),
        payload,
    }
}

fn make_character(name: &str, clan: u8) -> CharacterDigest {
    CharacterDigest {
        identifier: test_uuid(name).to_string(),
        name: name.into(),
        clan,
        status: "alive".into(),
        injury_level: "unharmed".into(),
        location: "Hub".into(),
        allies: vec![],
        kill_streak: 0,
        highlights: vec![],
        notable_events: vec![],
    }
}

/// A mock commentator that returns a fixed segment.
struct MockCommentator;

#[async_trait]
impl Commentator for MockCommentator {
    async fn generate(
        &self,
        _package: &announcers::BroadcastPackage,
    ) -> Result<CommentarySegment, CommentaryError> {
        Ok(CommentarySegment {
            id: "int-test-mock".into(),
            game_id: String::new(),
            day: 0,
            phase: String::new(),
            lines: vec![
                announcers::CommentaryLine {
                    speaker: "Verity".into(),
                    text: "Let's check in on the action!".into(),
                },
                announcers::CommentaryLine {
                    speaker: "Rex".into(),
                    text: "What a day it's been!".into(),
                },
            ],
            generated_at: chrono::Utc::now(),
            model_used: "mock".into(),
        })
    }

    fn generate_stream(
        &self,
        _package: &announcers::BroadcastPackage,
    ) -> Pin<Box<dyn Stream<Item = Result<CommentaryLine, CommentaryError>> + Send>> {
        // Collect from batch generate and yield all lines at once.
        let segment = match futures::executor::block_on(self.generate(_package)) {
            Ok(s) => s,
            Err(e) => return Box::pin(futures::stream::iter(vec![Err(e)])),
        };
        let items: Vec<_> = segment.lines.into_iter().map(Ok).collect();
        Box::pin(futures::stream::iter(items))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Full pipeline: build histories → update with events → build package → verify.
#[tokio::test]
async fn full_pipeline_from_roster_to_package() {
    // Step 1: Create character roster.
    let roster = vec![
        make_character("Snaggletooth", 12),
        make_character("Grubworm", 12),
        make_character("Rendmaw", 2),
    ];

    // Step 2: Phase 1 events — Rendmaw kills Grubworm, Snaggletooth moves.
    let phase1 = vec![
        make_msg(MessagePayload::CharacterKilled {
            victim: tr("Grubworm"),
            killer: Some(tr("Rendmaw")),
            cause: shared::afflictions::DeathCause::Combat,
        }),
        make_msg(MessagePayload::CharacterMoved {
            character: tr("Snaggletooth"),
            from: ar("Hub"),
            to: ar("Forest"),
        }),
        make_msg(MessagePayload::AllianceFormed {
            members: vec![tr("Snaggletooth"), tr("Nib")],
        }),
    ];

    // Step 3: Build and update histories.
    let mut histories = CharacterHistories::new(roster);
    histories.update(&phase1);
    let digests = histories.digests();

    // Verify history accumulation.
    let rendmaw = digests.iter().find(|d| d.name == "Rendmaw").unwrap();
    assert_eq!(rendmaw.status, "alive");
    assert!(rendmaw.notable_events.iter().any(|e| e.contains("Killed")));

    let grubworm = digests.iter().find(|d| d.name == "Grubworm").unwrap();
    assert_eq!(grubworm.status, "deceased");

    let snaggletooth = digests.iter().find(|d| d.name == "Snaggletooth").unwrap();
    assert_eq!(snaggletooth.location, "Forest");
    assert!(
        snaggletooth
            .notable_events
            .iter()
            .any(|e| e.contains("Forest"))
    );
    assert!(snaggletooth.allies.contains(&"Nib".to_string()));

    // Step 4: Build the broadcast package.
    let header = GameStateSnapshot {
        day: 1,
        phase: "day".into(),
        alive_count: 2,
        kill_leaders: vec![],
        alliances: vec![],
        hot_zones: vec![],
        killing_sprees: vec![],
    };
    let package = BroadcastPackageBuilder::build(header, &phase1, digests);

    // Step 5: Verify package structure.
    assert_eq!(package.events.len(), 3);
    assert_eq!(package.events[0].kind, announcers::EventKind::Death);
    assert_eq!(package.events[1].kind, announcers::EventKind::Movement);
    assert_eq!(package.events[2].kind, announcers::EventKind::Allied);
    assert_eq!(package.header.alive_count, 2);
    assert_eq!(package.histories.len(), 3);

    // Histories are sorted by name.
    assert_eq!(package.histories[0].name, "Grubworm");
    assert_eq!(package.histories[1].name, "Rendmaw");
    assert_eq!(package.histories[2].name, "Snaggletooth");
}

/// Histories accumulate across phases — events from phase 1 appear alongside
/// events from phase 2 in the digest.
#[tokio::test]
async fn histories_accumulate_across_phases() {
    let roster = vec![
        make_character("Snaggletooth", 12),
        make_character("Rendmaw", 2),
    ];
    let mut histories = CharacterHistories::new(roster);

    // Phase 1: Rendmaw kills someone.
    let phase1 = vec![make_msg(MessagePayload::CharacterKilled {
        victim: tr("Grubworm"),
        killer: Some(tr("Rendmaw")),
        cause: shared::afflictions::DeathCause::Combat,
    })];
    histories.update(&phase1);

    let digests1 = histories.digests();
    let rendmaw1 = digests1.iter().find(|d| d.name == "Rendmaw").unwrap();
    assert_eq!(rendmaw1.notable_events.len(), 1);
    assert!(rendmaw1.notable_events[0].contains("Killed"));

    // Phase 2: Snaggletooth finds an item.
    let phase2 = vec![make_msg(MessagePayload::ItemFound {
        character: tr("Snaggletooth"),
        item: ir("bow"),
        area: ar("Forest"),
    })];
    histories.update(&phase2);

    let digests2 = histories.digests();
    let rendmaw2 = digests2.iter().find(|d| d.name == "Rendmaw").unwrap();
    let snaggletooth2 = digests2.iter().find(|d| d.name == "Snaggletooth").unwrap();

    // Rendmaw still has phase 1's kill event.
    assert!(rendmaw2.notable_events.iter().any(|e| e.contains("Killed")));

    // Snaggletooth has phase 2's item find.
    assert!(
        snaggletooth2
            .notable_events
            .iter()
            .any(|e| e.contains("Found"))
    );
}

/// Noteworthy events are capped at 8 — excess oldest entries are pruned.
#[tokio::test]
async fn notable_events_capped_across_phases() {
    let roster = vec![make_character("Snaggletooth", 12)];
    let mut histories = CharacterHistories::new(roster);

    // Push 35 phases worth of movement events (exceeds the 30-event cap).
    for i in 1..=35 {
        let events = vec![make_msg(MessagePayload::CharacterMoved {
            character: tr("Snaggletooth"),
            from: ar("A"),
            to: ar(&format!("Area{i}")),
        })];
        histories.update(&events);
    }

    let digests = histories.digests();
    let snaggletooth = digests.iter().find(|d| d.name == "Snaggletooth").unwrap();
    assert_eq!(snaggletooth.notable_events.len(), 30);
    // The most recent entry mentions Area35 (newest first).
    assert!(snaggletooth.notable_events[0].contains("Area35"));
}

/// BroadcastPackage built with kill leaders in the header.
#[tokio::test]
async fn package_includes_kill_leaders() {
    let phase1 = vec![
        make_msg(MessagePayload::CharacterKilled {
            victim: tr("Grubworm"),
            killer: Some(tr("Rendmaw")),
            cause: shared::afflictions::DeathCause::Combat,
        }),
        make_msg(MessagePayload::CharacterKilled {
            victim: tr("Nib"),
            killer: Some(tr("Rendmaw")),
            cause: shared::afflictions::DeathCause::Combat,
        }),
        make_msg(MessagePayload::CharacterKilled {
            victim: tr("Clove"),
            killer: Some(tr("Snaggletooth")),
            cause: shared::afflictions::DeathCause::Combat,
        }),
    ];

    // Build kill leaders the same way the API does.
    let mut kill_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for msg in &phase1 {
        if let MessagePayload::CharacterKilled {
            killer: Some(k), ..
        } = &msg.payload
        {
            *kill_counts.entry(k.name.clone()).or_insert(0) += 1;
        }
    }
    let mut kill_leaders: Vec<announcers::KillLeader> = kill_counts
        .into_iter()
        .map(|(name, count)| announcers::KillLeader {
            name,
            clan: 0,
            kill_count: count,
        })
        .collect();
    kill_leaders.sort_by_key(|k| std::cmp::Reverse(k.kill_count));

    assert_eq!(kill_leaders.len(), 2);
    assert_eq!(kill_leaders[0].name, "Rendmaw");
    assert_eq!(kill_leaders[0].kill_count, 2);
    assert_eq!(kill_leaders[1].name, "Snaggletooth");
    assert_eq!(kill_leaders[1].kill_count, 1);

    let header = GameStateSnapshot {
        day: 1,
        phase: "day".into(),
        alive_count: 21,
        kill_leaders: kill_leaders.clone(),
        alliances: vec![],
        hot_zones: vec![],
        killing_sprees: vec![],
    };
    let package = BroadcastPackageBuilder::build(header, &phase1, vec![]);
    assert_eq!(package.header.kill_leaders.len(), 2);
    assert_eq!(package.header.kill_leaders[0].name, "Rendmaw");
}

/// Generate commentary with the full generate_commentary convenience fn.
#[tokio::test]
async fn generate_commentary_integration() {
    let roster = vec![make_character("Snaggletooth", 12)];
    let mut histories = CharacterHistories::new(roster);
    histories.update(&[make_msg(MessagePayload::CharacterKilled {
        victim: tr("Grubworm"),
        killer: Some(tr("Snaggletooth")),
        cause: shared::afflictions::DeathCause::Combat,
    })]);

    let header = GameStateSnapshot {
        day: 1,
        phase: "day".into(),
        alive_count: 1,
        kill_leaders: vec![],
        alliances: vec![],
        hot_zones: vec![],
        killing_sprees: vec![],
    };

    let segment = announcers::generate_commentary(
        &MockCommentator,
        "game-123",
        5,
        "night",
        header,
        &[],
        histories.digests(),
    )
    .await
    .unwrap();

    assert_eq!(segment.game_id, "game-123");
    assert_eq!(segment.day, 5);
    assert_eq!(segment.phase, "night");
    assert_eq!(segment.lines.len(), 2);
    assert_eq!(segment.model_used, "mock");
}

/// Killing spree streak rules:
///   - Kill → increment
///   - Wound opponent (no kill) → maintain (no change)
///   - Get wounded yourself → reset to 0
#[tokio::test]
async fn killing_spree_streak_rules() {
    let roster = vec![
        make_character("Rendmaw", 2),
        make_character("Snaggletooth", 12),
        make_character("Scabface", 1),
    ];
    let mut histories = CharacterHistories::new(roster);

    // Phase 1: Rendmaw kills Grubworm → streak = 1.
    histories.update(&[make_msg(MessagePayload::CharacterKilled {
        victim: tr("Grubworm"),
        killer: Some(tr("Rendmaw")),
        cause: shared::afflictions::DeathCause::Combat,
    })]);
    let d = histories.digests();
    assert_eq!(
        d.iter().find(|d| d.name == "Rendmaw").unwrap().kill_streak,
        1
    );

    // Phase 2: Rendmaw wounds Scabface (no kill) → streak stays at 1.
    histories.update(&[make_msg(MessagePayload::CharacterWounded {
        victim: tr("Scabface"),
        attacker: Some(tr("Rendmaw")),
        hp_lost: 5,
    })]);
    let d = histories.digests();
    assert_eq!(
        d.iter().find(|d| d.name == "Rendmaw").unwrap().kill_streak,
        1
    );

    // Phase 3: Rendmaw kills Nib → streak = 2 ("heating up").
    histories.update(&[make_msg(MessagePayload::CharacterKilled {
        victim: tr("Nib"),
        killer: Some(tr("Rendmaw")),
        cause: shared::afflictions::DeathCause::Combat,
    })]);
    let d = histories.digests();
    assert_eq!(
        d.iter().find(|d| d.name == "Rendmaw").unwrap().kill_streak,
        2
    );

    // Phase 4: Rendmaw gets wounded by Snaggletooth → streak = 0 (reset).
    histories.update(&[make_msg(MessagePayload::CharacterWounded {
        victim: tr("Rendmaw"),
        attacker: Some(tr("Snaggletooth")),
        hp_lost: 8,
    })]);
    let d = histories.digests();
    assert_eq!(
        d.iter().find(|d| d.name == "Rendmaw").unwrap().kill_streak,
        0
    );
    // Snaggletooth wounded Rendmaw — her streak holds (she won the exchange).
    assert_eq!(
        d.iter()
            .find(|d| d.name == "Snaggletooth")
            .unwrap()
            .kill_streak,
        0
    );

    // Phase 5: Snaggletooth kills Scabface → streak = 1.
    histories.update(&[make_msg(MessagePayload::CharacterKilled {
        victim: tr("Scabface"),
        killer: Some(tr("Snaggletooth")),
        cause: shared::afflictions::DeathCause::Combat,
    })]);
    let d = histories.digests();
    assert_eq!(
        d.iter()
            .find(|d| d.name == "Snaggletooth")
            .unwrap()
            .kill_streak,
        1
    );
}

/// Spree milestones fire on tier thresholds; spree-break events fire when
/// an active spree (2+) is reset.
#[tokio::test]
async fn spree_milestone_and_break_events() {
    let roster = vec![make_character("Rendmaw", 2)];
    let mut histories = CharacterHistories::new(roster);

    // Kill 1 → streak = 1, no milestone (below tier 2 threshold).
    histories.update(&[make_msg(MessagePayload::CharacterKilled {
        victim: tr("Grubworm"),
        killer: Some(tr("Rendmaw")),
        cause: shared::afflictions::DeathCause::Combat,
    })]);
    let d = histories.digests();
    let rendmaw = d.iter().find(|d| d.name == "Rendmaw").unwrap();
    assert!(
        !rendmaw
            .notable_events
            .iter()
            .any(|e| e.contains("heating up"))
    );

    // Kill 2 → streak = 2 → "heating up" milestone fires.
    histories.update(&[make_msg(MessagePayload::CharacterKilled {
        victim: tr("Nib"),
        killer: Some(tr("Rendmaw")),
        cause: shared::afflictions::DeathCause::Combat,
    })]);
    let d = histories.digests();
    let rendmaw = d.iter().find(|d| d.name == "Rendmaw").unwrap();
    assert!(
        rendmaw
            .notable_events
            .iter()
            .any(|e| e.contains("heating up"))
    );

    // Kill 3 → streak = 3, same tier, no new milestone.
    // Kill 4 → streak = 4 → "on fire" milestone fires.
    histories.update(&[
        make_msg(MessagePayload::CharacterKilled {
            victim: tr("Clove"),
            killer: Some(tr("Rendmaw")),
            cause: shared::afflictions::DeathCause::Combat,
        }),
        make_msg(MessagePayload::CharacterKilled {
            victim: tr("Scabface"),
            killer: Some(tr("Rendmaw")),
            cause: shared::afflictions::DeathCause::Combat,
        }),
    ]);
    let d = histories.digests();
    let rendmaw = d.iter().find(|d| d.name == "Rendmaw").unwrap();
    assert!(rendmaw.notable_events.iter().any(|e| e.contains("on fire")));

    // Rendmaw gets wounded → streak = 0 → spree-break event fires.
    histories.update(&[make_msg(MessagePayload::CharacterWounded {
        victim: tr("Rendmaw"),
        attacker: Some(tr("Snaggletooth")),
        hp_lost: 5,
    })]);
    let d = histories.digests();
    let rendmaw = d.iter().find(|d| d.name == "Rendmaw").unwrap();
    assert_eq!(rendmaw.kill_streak, 0);
    assert!(
        rendmaw
            .notable_events
            .iter()
            .any(|e| e.contains("spree has been broken"))
    );
}

/// Hot zones in the snapshot header survive through package construction.
#[tokio::test]
async fn hot_zones_round_trip() {
    let header = GameStateSnapshot {
        day: 1,
        phase: "day".into(),
        alive_count: 12,
        kill_leaders: vec![],
        alliances: vec![],
        hot_zones: vec![
            announcers::AreaActivity {
                name: "Hub".into(),
                activity_level: "hot".into(),
            },
            announcers::AreaActivity {
                name: "Forest".into(),
                activity_level: "active".into(),
            },
        ],
        killing_sprees: vec![],
    };
    let package = BroadcastPackageBuilder::build(header, &[], vec![]);
    assert_eq!(package.header.hot_zones.len(), 2);
    assert_eq!(package.header.hot_zones[0].name, "Hub");
    assert_eq!(package.header.hot_zones[0].activity_level, "hot");
    assert_eq!(package.header.hot_zones[1].name, "Forest");
    assert_eq!(package.header.hot_zones[1].activity_level, "active");
}

/// Hot zone labels from severity function map correctly.
#[test]
fn severity_area_activity_labels() {
    assert_eq!(announcers::severity::describe_area_activity(0), "quiet");
    assert_eq!(announcers::severity::describe_area_activity(2), "active");
    assert_eq!(announcers::severity::describe_area_activity(5), "hot");
}

/// Highlights persist for kills, betrayals, and alliances — they survive
/// beyond the rolling 30-event cap.
#[tokio::test]
async fn permanent_highlights() {
    let roster = vec![make_character("Rendmaw", 2)];
    let mut histories = CharacterHistories::new(roster);

    // Rendmaw kills 35 characters (overflows the 30-event rolling cap).
    for i in 1..=35 {
        histories.update(&[make_msg(MessagePayload::CharacterKilled {
            victim: tr(&format!("Victim{i}")),
            killer: Some(tr("Rendmaw")),
            cause: shared::afflictions::DeathCause::Combat,
        })]);
    }

    let d = histories.digests();
    let rendmaw = d.iter().find(|d| d.name == "Rendmaw").unwrap();

    // Rolling events capped at 30.
    assert_eq!(rendmaw.notable_events.len(), 30);

    // Highlights still have all 20 (capped at MAX_HIGHLIGHTS).
    assert_eq!(rendmaw.highlights.len(), 20);

    // Each highlight mentions a kill.
    assert!(rendmaw.highlights[0].contains("Killed"));

    // Highlights survive serde_json round-trip (as they would through
    // SurrealDB persistence).
    let json = serde_json::to_value(rendmaw).unwrap();
    let restored: CharacterDigest = serde_json::from_value(json).unwrap();
    assert_eq!(restored.highlights.len(), 20);
    assert!(restored.highlights[0].contains("Killed"));
}
