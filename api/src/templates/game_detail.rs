use shared::{DisplayGame, GameStatus};

/// Determine the phase label and class from game state and messages.
pub fn current_broadcast_phase(
    game: &DisplayGame,
    messages: &[shared::messages::GameMessage],
    cfg: &world::config::GameConfig,
) -> (&'static str, &'static str) {
    use world::config::DaySlot;
    match game.status {
        GameStatus::Finished => ("finished", "FINISHED"),
        GameStatus::NotStarted => ("day", "STAGING"),
        GameStatus::InProgress => {
            if let Some(last) = messages.last() {
                // Hour-label badge (00–22) with the configured day/night
                // band class used for styling.
                let class = match cfg.day_slot(last.phase) {
                    DaySlot::Dawn => "dawn",
                    DaySlot::Day => "day",
                    DaySlot::Dusk => "dusk",
                    DaySlot::Night => "night",
                };
                let label: &'static str = match last.phase.hour() {
                    0 => "00",
                    2 => "02",
                    4 => "04",
                    6 => "06",
                    8 => "08",
                    10 => "10",
                    12 => "12",
                    14 => "14",
                    16 => "16",
                    18 => "18",
                    20 => "20",
                    _ => "22",
                };
                (class, label)
            } else {
                ("day", "06")
            }
        }
    }
}

fn message_archetype(payload: &shared::messages::MessagePayload) -> &'static str {
    use shared::messages::MessageKind::*;
    match payload.kind() {
        CharacterKilled => "death",
        Combat | CombatBeat | CombatRoundStart | CombatJoined | CombatEnded | CharacterAttacked
        | CharacterWounded | TrapSet | TrapTriggered => "action",
        AllianceFormed | AllianceProposed | AllianceDissolved | BetrayalTriggered
        | TrustShockBreak => "commentary",
        CharacterMoved | CharacterHidden | AreaClosed | AreaEvent => "commentary",
        ItemFound | ItemUsed | ItemDropped | PatronGift => "event",
        CharacterRested | CharacterStarved | CharacterDehydrated | SanityBreak
        | HungerBandChanged | ThirstBandChanged | StaminaBandChanged | ShelterSought | Foraged
        | Drank | Ate | CharacterSlept | CharacterWoke | CycleStart | CycleEnd | PhaseStarted
        | PhaseEnded | GameEnded | Generic | CharacterBledOut | WoundInfected | WoundHealed
        | WoundInflicted | WoundBled | WoundTreated | WoundAmputated | ConditionAcquired
        | ConditionResolved | CharacterDesperate => "commentary",
        TraumaAcquired | TraumaReinforced | TraumaEscalated | TraumaFlashback | TraumaAvoidance
        | TraumaObserved | TraumaForgotten | TraumaHabituated => "commentary",
        PhobiaAcquired | PhobiaTriggered | PhobiaEscalated | PhobiaHabituated | PhobiaObserved
        | PhobiaForgotten => "commentary",
        FixationAcquired | FixationEscalated | FixationFired | FixationConsummated
        | FixationThwarted | FixationFaded => "commentary",
        AfflictionAcquired | AfflictionProgressed | AfflictionHealed | AfflictionCascaded
        | SubstanceUsed | AddictionAcquired | AddictionReinforced | AddictionEscalated
        | AddictionResisted | AddictionRelapse | AddictionCraving | AddictionObserved
        | AddictionForgotten | AddictionHabituated => "commentary",
        CharacterTrapped
        | Struggling
        | TrappedEscaped
        | CharacterDiedWhileTrapped
        | RescueAttempted
        | PartialRescueProgress => "event",
        SleepIncident => "commentary",
        DaySummary => "summary",
    }
}

fn archetype_label(archetype: &str) -> &'static str {
    match archetype {
        "action" => "ACTION",
        "death" => "ELIMINATED",
        "event" => "ARENA EVENT",
        "commentary" => "ANALYSIS",
        "summary" => "DAY SUMMARY",
        _ => "EVENT",
    }
}

fn message_kind_label(payload: &shared::messages::MessagePayload) -> &'static str {
    use shared::messages::MessageKind::*;
    match payload.kind() {
        CharacterKilled => "Death",
        Combat | CharacterAttacked | CharacterWounded | TrapSet | TrapTriggered => "Combat",
        CombatBeat | CombatRoundStart | CombatJoined | CombatEnded => "Combat",
        AllianceFormed | AllianceProposed | AllianceDissolved | BetrayalTriggered
        | TrustShockBreak => "Alliance",
        CharacterMoved | CharacterHidden | AreaClosed => "Movement",
        AreaEvent => "Hazard",
        ItemFound | ItemUsed | ItemDropped => "Item",
        PatronGift => "Patron",
        CharacterRested | CharacterStarved | CharacterDehydrated | SanityBreak
        | HungerBandChanged | ThirstBandChanged | StaminaBandChanged | ShelterSought | Foraged
        | Drank | Ate | CharacterSlept | CharacterWoke | CycleStart | CycleEnd | PhaseStarted
        | PhaseEnded | GameEnded | Generic | CharacterBledOut | WoundInfected | WoundHealed
        | WoundInflicted | WoundBled | WoundTreated | WoundAmputated | ConditionAcquired
        | ConditionResolved | CharacterDesperate => "Wound",
        TraumaAcquired | TraumaReinforced | TraumaEscalated | TraumaFlashback | TraumaAvoidance
        | TraumaObserved | TraumaForgotten | TraumaHabituated => "Trauma",
        PhobiaAcquired | PhobiaTriggered | PhobiaEscalated | PhobiaHabituated | PhobiaObserved
        | PhobiaForgotten => "Fear",
        FixationAcquired | FixationEscalated | FixationFired | FixationConsummated
        | FixationThwarted | FixationFaded => "Fixation",
        AfflictionAcquired | AfflictionProgressed | AfflictionHealed | AfflictionCascaded
        | SubstanceUsed | AddictionAcquired | AddictionReinforced | AddictionEscalated
        | AddictionResisted | AddictionRelapse | AddictionCraving | AddictionObserved
        | AddictionForgotten | AddictionHabituated => "Health",
        CharacterTrapped
        | Struggling
        | TrappedEscaped
        | CharacterDiedWhileTrapped
        | RescueAttempted
        | PartialRescueProgress => "Trapped",
        SleepIncident => "Sleep",
        DaySummary => "Summary",
    }
}

fn kind_color(payload: &shared::messages::MessagePayload) -> &'static str {
    use shared::messages::MessageKind::*;
    match payload.kind() {
        CharacterKilled => "danger",
        Combat | CombatBeat | CombatRoundStart | CombatJoined | CombatEnded | CharacterAttacked
        | CharacterWounded | TrapSet | TrapTriggered => "waiting",
        AllianceFormed | AllianceProposed | AllianceDissolved | BetrayalTriggered
        | TrustShockBreak => "info",
        CharacterMoved | CharacterHidden | AreaClosed => "accent",
        AreaEvent => "danger",
        ItemFound | ItemUsed | ItemDropped | PatronGift => "gold",
        CharacterRested | CharacterStarved | CharacterDehydrated | SanityBreak
        | HungerBandChanged | ThirstBandChanged | StaminaBandChanged | ShelterSought | Foraged
        | Drank | Ate | CharacterSlept | CharacterWoke | CycleStart | CycleEnd | PhaseStarted
        | PhaseEnded | GameEnded | Generic | CharacterBledOut | WoundInfected | WoundHealed
        | WoundInflicted | WoundBled | WoundTreated | WoundAmputated | ConditionAcquired
        | ConditionResolved | CharacterDesperate => "danger",
        TraumaAcquired | TraumaReinforced | TraumaEscalated | TraumaFlashback | TraumaAvoidance
        | TraumaObserved | TraumaForgotten | TraumaHabituated => "purple",
        PhobiaAcquired | PhobiaTriggered | PhobiaEscalated | PhobiaHabituated | PhobiaObserved
        | PhobiaForgotten => "purple",
        FixationAcquired | FixationEscalated | FixationFired | FixationConsummated
        | FixationThwarted | FixationFaded => "purple",
        AfflictionAcquired | AfflictionProgressed | AfflictionHealed | AfflictionCascaded
        | SubstanceUsed | AddictionAcquired | AddictionReinforced | AddictionEscalated
        | AddictionResisted | AddictionRelapse | AddictionCraving | AddictionObserved
        | AddictionForgotten | AddictionHabituated => "warning",
        CharacterTrapped
        | Struggling
        | TrappedEscaped
        | CharacterDiedWhileTrapped
        | RescueAttempted
        | PartialRescueProgress => "warning",
        SleepIncident => "info",
        DaySummary => "muted",
    }
}

fn hunger_label(hunger: u8) -> &'static str {
    match hunger {
        0 => "Sated",
        1 => "Hungry",
        _ => "Starving",
    }
}

fn hunger_color(hunger: u8) -> &'static str {
    match hunger {
        0 => "var(--running)",
        1 => "var(--waiting)",
        _ => "var(--danger)",
    }
}

fn thirst_label(thirst: u8) -> &'static str {
    match thirst {
        0 => "Hydrated",
        1 => "Thirsty",
        _ => "Dehydrated",
    }
}

fn thirst_color(thirst: u8) -> &'static str {
    match thirst {
        0 => "var(--running)",
        1 => "var(--waiting)",
        _ => "var(--danger)",
    }
}

fn stamina_label(stamina: u32, max_stamina: u32) -> &'static str {
    if max_stamina == 0 {
        return "Unknown";
    }
    let ratio = stamina as f64 / max_stamina as f64;
    if ratio > 0.66 {
        "Fresh"
    } else if ratio > 0.33 {
        "Winded"
    } else {
        "Exhausted"
    }
}

fn stamina_color(stamina: u32, max_stamina: u32) -> &'static str {
    if max_stamina == 0 {
        return "band-none";
    }
    let ratio = stamina as f64 / max_stamina as f64;
    if ratio > 0.66 {
        "band-good"
    } else if ratio > 0.33 {
        "band-warn"
    } else {
        "band-danger"
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Rich card for the compiled end-of-day digest: rollup header plus a
/// collapsible per-goblin table. Markup lives in
/// `templates/day_summary_card.html` (Tera autoescapes names/values).
pub fn render_day_summary_card(msg: &shared::messages::GameMessage) -> String {
    let shared::messages::MessagePayload::DaySummary {
        day,
        rollup,
        goblins,
    } = &msg.payload
    else {
        // Dispatch only happens on the DaySummary variant; fall back to the
        // plain card if that ever changes.
        return render_plain_event_card(msg);
    };

    let mut ctx = tera::Context::new();
    ctx.insert("day", day);
    ctx.insert("fallen", &rollup.fallen);
    ctx.insert("survivors", &rollup.survivors);
    ctx.insert("deaths", &rollup.deaths);
    ctx.insert("formed", &rollup.alliances_formed);
    ctx.insert("dissolved", &rollup.alliances_dissolved);
    ctx.insert("betrays", &rollup.betrayals);
    ctx.insert("found", &rollup.items_found);
    ctx.insert("used", &rollup.items_used);
    ctx.insert(
        "leaders",
        &rollup
            .kill_leaders
            .iter()
            .map(|l| serde_json::json!({ "name": l.character.name, "kills": l.kills }))
            .collect::<Vec<_>>(),
    );
    ctx.insert(
        "goblins",
        &goblins
            .iter()
            .map(|g| {
                serde_json::json!({
                    "name": g.character.name,
                    "team": g.team,
                    "kills": g.kills,
                    "wounds": g.wounds_taken,
                    "found": g.items_found,
                    "used": g.items_used,
                    "alliances": g.alliance_changes,
                    "state": if g.alive {
                        format!("{} hp", g.blood)
                    } else {
                        "fallen".to_string()
                    },
                })
            })
            .collect::<Vec<_>>(),
    );

    crate::templates::tera_engine::render("day_summary_card.html", &ctx)
}

pub fn render_event_card(msg: &shared::messages::GameMessage) -> String {
    if let shared::messages::MessagePayload::DaySummary { .. } = &msg.payload {
        return render_day_summary_card(msg);
    }
    render_plain_event_card(msg)
}

fn render_plain_event_card(msg: &shared::messages::GameMessage) -> String {
    let archetype = message_archetype(&msg.payload);
    let badge = archetype_label(archetype);
    let kind = message_kind_label(&msg.payload);
    let color = kind_color(&msg.payload);
    // Display = the hour ("12"), same D-hour-T form as fight_ts. The old
    // `format!("{:?}", ...).to_uppercase()` produced "D2 PHASE(12) T50".
    let ts = format!("D{} {} T{}", msg.game_day, msg.phase, msg.tick);

    let mut ctx = tera::Context::new();
    ctx.insert("archetype", archetype);
    ctx.insert("badge", badge);
    ctx.insert("kind", kind);
    ctx.insert("color", color);
    ctx.insert("ts", &ts);
    // Tera autoescapes .html; content passes raw (was html_escape()).
    ctx.insert("content", &msg.content);
    crate::templates::tera_engine::render("event_card.html", &ctx)
}

/// Render a whole message slice for the timeline: contiguous runs of
/// combat messages (`CombatBeat` / `CombatEnded` / `CombatJoined`; `CombatRoundStart` is
/// server-log-only and never reaches persisted timelines) collapse into one
/// grouped fight card keyed by `combat_id` — joins stay inside the fight
/// they join — everything else renders as a
/// plain event card. Beats carry `round`/`order`/`significant`, so the
/// grouped card is also the replay surface — the persisted beat stream is
/// the full fight, round by round (§10). Significance down-ranking: significant
/// beats (kills, cower, verdicts, fled, equipment breaks) render as full
/// narrative lines; ordinary swings/misses collapse into a compact muted
/// list per round.
/// "D2 12 T50" header timestamp for a fight (from its first beat).
fn fight_ts(beats: &[&shared::messages::GameMessage]) -> String {
    beats
        .first()
        .map(|m| {
            // Phase's Display is the hour ("12"); its Debug is "Phase(12)".
            format!("D{} {} T{}", m.game_day, m.phase, m.tick)
        })
        .unwrap_or_default()
}

/// "A vs B" from the fight's first beat; falls back to the join sides
/// (a join-only fragment whose beats landed in another group) and then to
/// the result line when all else fails.
fn fight_title(
    beats: &[&shared::messages::GameMessage],
    joins: &[&shared::messages::GameMessage],
    result: Option<(&String, &shared::combat_beat::CombatResult)>,
) -> String {
    use shared::combat_beat::CombatResult;
    use shared::messages::MessagePayload;

    let combatants = match beats.first().map(|m| &m.payload) {
        Some(MessagePayload::CombatBeat(first)) => match &first.target {
            Some(t) => format!("{} vs {}", first.actor.name, t.name),
            None => first.actor.name.clone(),
        },
        _ => {
            let side1 = joins.iter().find_map(|m| match &m.payload {
                MessagePayload::CombatJoined {
                    side, character, ..
                } if side == "side-1" => Some(character.name.clone()),
                _ => None,
            });
            let side2 = joins.iter().find_map(|m| match &m.payload {
                MessagePayload::CombatJoined {
                    side, character, ..
                } if side == "side-2" => Some(character.name.clone()),
                _ => None,
            });
            match (side1, side2) {
                (Some(a), Some(b)) => format!("{a} vs {b}"),
                _ => "Combat".to_string(),
            }
        }
    };
    if combatants == "Combat" {
        match result.map(|(_, r)| r) {
            Some(CombatResult::Victory { victors }) => join_names(victors, "win", "wins"),
            Some(CombatResult::Draw) | None => combatants,
        }
    } else {
        combatants
    }
}

/// Header/verdict result line (plain text; templates escape it).
fn fight_result_text(result: Option<(&String, &shared::combat_beat::CombatResult)>) -> String {
    use shared::combat_beat::CombatResult;

    match result.map(|(_, r)| r) {
        Some(CombatResult::Victory { victors }) => {
            format!("{} the fight", join_names(victors, "win", "wins"))
        }
        Some(CombatResult::Draw) => "A draw — both break off".to_string(),
        None => "fight in progress…".to_string(),
    }
}

/// One contiguous run of combat payloads (beats, joins, the closing
/// result) delimited the way the event stream renders it: any non-combat
/// message ends the run. Shared by the timeline card and the dedicated
/// replay view (vt9).
pub struct FightGroup<'a> {
    /// Index of the run's first combat message — its placement in the stream.
    pub start: usize,
    pub beats: Vec<&'a shared::messages::GameMessage>,
    pub joins: Vec<&'a shared::messages::GameMessage>,
    pub result: Option<(&'a String, &'a shared::combat_beat::CombatResult)>,
}

/// Split a message stream into fight groups (combat runs) in stream order.
pub fn group_fights(msgs: &[shared::messages::GameMessage]) -> Vec<FightGroup<'_>> {
    use shared::messages::MessagePayload;

    let mut groups: Vec<FightGroup<'_>> = Vec::new();
    let mut beats: Vec<&shared::messages::GameMessage> = Vec::new();
    let mut joins: Vec<&shared::messages::GameMessage> = Vec::new();
    let mut result: Option<(&String, &shared::combat_beat::CombatResult)> = None;
    let mut start = 0usize;

    for (i, msg) in msgs.iter().enumerate() {
        match &msg.payload {
            MessagePayload::CombatBeat(_)
            | MessagePayload::CombatJoined { .. }
            | MessagePayload::CombatEnded { .. } => {
                if beats.is_empty() && joins.is_empty() && result.is_none() {
                    start = i;
                }
                match &msg.payload {
                    MessagePayload::CombatBeat(_) => beats.push(msg),
                    MessagePayload::CombatJoined { .. } => joins.push(msg),
                    MessagePayload::CombatEnded {
                        combat_id,
                        result: r,
                        ..
                    } => result = Some((combat_id, r)),
                    _ => unreachable!(),
                }
            }
            MessagePayload::CombatRoundStart { .. } => {
                // Server-log-only pass-through: never starts a run, never
                // flushes — mirrors the old render_event_cards arm.
            }
            _ => {
                if !beats.is_empty() || !joins.is_empty() || result.is_some() {
                    groups.push(FightGroup {
                        start,
                        beats: std::mem::take(&mut beats),
                        joins: std::mem::take(&mut joins),
                        result: result.take(),
                    });
                }
            }
        }
    }
    if !beats.is_empty() || !joins.is_empty() || result.is_some() {
        groups.push(FightGroup {
            start,
            beats,
            joins,
            result,
        });
    }
    groups
}

/// Fight sections for the dedicated replay view (vt9): nav facts plus the
/// full card with EVERY round open — the timeline down-ranks, replay inspects.
pub fn replay_sections(msgs: &[shared::messages::GameMessage]) -> Vec<serde_json::Value> {
    group_fights(msgs)
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let anchor = format!("f{}", i + 1);
            let mut rounds: Vec<u32> = g
                .beats
                .iter()
                .filter_map(|m| match &m.payload {
                    shared::messages::MessagePayload::CombatBeat(b) => Some(b.round),
                    _ => None,
                })
                .collect();
            rounds.sort_unstable();
            rounds.dedup();
            serde_json::json!({
                "id": anchor,
                "title": fight_title(&g.beats, &g.joins, g.result),
                "result_short": fight_result_text(g.result),
                "ts": fight_ts(&g.beats),
                "rounds": rounds,
                "card": render_combat_card(&g.beats, &g.joins, g.result, &anchor, true),
            })
        })
        .collect()
}

pub fn render_event_cards(msgs: &[shared::messages::GameMessage]) -> String {
    use shared::messages::MessagePayload;

    let groups = group_fights(msgs);
    let mut html = String::new();
    let mut gi = 0usize;
    for (i, msg) in msgs.iter().enumerate() {
        if gi < groups.len() && groups[gi].start == i {
            let g = &groups[gi];
            html.push_str(&render_combat_card(&g.beats, &g.joins, g.result, "", false));
            gi += 1;
            continue;
        }
        match &msg.payload {
            MessagePayload::CombatBeat(_)
            | MessagePayload::CombatJoined { .. }
            | MessagePayload::CombatEnded { .. }
            | MessagePayload::CombatRoundStart { .. } => continue,
            _ => html.push_str(&render_event_card(msg)),
        }
    }
    html
}

/// Play-page event list: messages plus announcer commentary interleaved by
/// (day, period). Each period's messages render first (fight grouping
/// intact), then that period's commentary.
///
/// WHY: commentary used to be appended after the whole log unfiltered, so a
/// day/period-filtered view still showed other days' lines, and the all-days
/// view read out of order (dfp).
pub fn render_timeline_cards(
    msgs: &[shared::messages::GameMessage],
    segs: &[announcers::CommentarySegment],
) -> String {
    use std::collections::BTreeSet;

    // Input is sorted by (day, period, ...), so equal keys are contiguous.
    let mut runs: Vec<((u32, String), Vec<shared::messages::GameMessage>)> = Vec::new();
    for m in msgs {
        let key = (m.game_day, m.phase.to_string());
        match runs.last_mut() {
            Some((k, run)) if *k == key => run.push(m.clone()),
            _ => runs.push((key, vec![m.clone()])),
        }
    }
    let seg_key = |s: &announcers::CommentarySegment| (s.day, s.phase.clone());

    // Commentaries are one per (day, period); SQL orders them by day, phase.
    let mut segs_by_key: std::collections::BTreeMap<(u32, String), &announcers::CommentarySegment> =
        std::collections::BTreeMap::new();
    for s in segs {
        segs_by_key.insert(seg_key(s), s);
    }

    let keys: BTreeSet<(u32, String)> = runs
        .iter()
        .map(|(k, _)| k.clone())
        .chain(segs_by_key.keys().cloned())
        .collect();

    let mut html = String::new();
    let mut run_i = 0usize;
    for key in keys {
        if run_i < runs.len() && runs[run_i].0 == key {
            html.push_str(&render_event_cards(&runs[run_i].1));
            run_i += 1;
        }
        if let Some(seg) = segs_by_key.get(&key) {
            html.push_str(&render_commentary_card(seg));
        }
    }
    html
}

/// "Name" or "A & B" with the right verb form ("X wins" / "X & Y win").
fn join_names(victors: &[shared::messages::CharacterRef], plural: &str, singular: &str) -> String {
    match victors {
        [] => "The fight ends".to_string(),
        [one] => format!("{} {}", one.name, singular),
        many => format!(
            "{} {}",
            many.iter()
                .map(|v| v.name.as_str())
                .collect::<Vec<_>>()
                .join(" & "),
            plural
        ),
    }
}

/// One grouped fight card, rendered through the `combat_card.html` /
/// `combat_round.html` Tera partials (Tera autoescapes `.html`, so names
/// and message content below are plain text — never HTML). All grouping,
/// slicing, and significance decisions stay here in Rust: beats group by
/// round, rounds render inside `<details>` (open when the round carries a
/// significant beat), significant beats prominent and ordinary swings
/// compact — replay = open every round.
fn render_combat_card(
    beats: &[&shared::messages::GameMessage],
    joins: &[&shared::messages::GameMessage],
    result: Option<(&String, &shared::combat_beat::CombatResult)>,
    anchor: &str,
    open_all: bool,
) -> String {
    let ts = fight_ts(beats);
    let title = fight_title(beats, joins, result);
    let result_text = fight_result_text(result);

    // Group beats into rounds (algorithm unchanged; feeds template data).
    let mut rounds: Vec<(
        u32,
        Vec<(
            &shared::messages::GameMessage,
            &shared::combat_beat::CombatBeat,
        )>,
    )> = Vec::new();
    for msg in beats {
        if let shared::messages::MessagePayload::CombatBeat(beat) = &msg.payload {
            match rounds.last_mut() {
                Some((n, list)) if *n == beat.round => list.push((msg, beat)),
                _ => rounds.push((beat.round, vec![(msg, beat)])),
            }
        }
    }

    let rounds_ctx: Vec<serde_json::Value> = rounds
        .iter()
        .map(|(round_no, list)| {
            let mut significant = Vec::new();
            let mut ordinary = Vec::new();
            for (msg, beat) in list {
                let (text, prominent) = beat_line(beat, &msg.content);
                let entry = serde_json::json!({ "text": text, "prominent": prominent });
                if beat.significant {
                    significant.push(entry);
                } else {
                    ordinary.push(entry);
                }
            }
            serde_json::json!({
                "number": round_no,
                "open": open_all || list.iter().any(|(_, b)| b.significant),
                "significant": significant,
                "ordinary": ordinary,
            })
        })
        .collect();

    let mut ctx = tera::Context::new();
    ctx.insert("anchor", anchor);
    ctx.insert("color", "var(--waiting)");
    ctx.insert("ts", &ts);
    ctx.insert("title", &title);
    ctx.insert("result_text", &result_text);
    ctx.insert("rounds", &rounds_ctx);

    // Sides reconstruct from the persisted stream: a fight opens a fresh
    // 1v1 pair (design §4), so the first beat's actor is side-1 and its
    // target side-2; each join declares its own side. Beats carry no side.
    let mut sides: [Vec<String>; 2] = [vec![], vec![]];
    if let Some(shared::messages::MessagePayload::CombatBeat(first)) =
        beats.first().map(|m| &m.payload)
    {
        sides[0].push(first.actor.name.clone());
        if let Some(t) = &first.target {
            sides[1].push(t.name.clone());
        }
    }
    let mut join_lines: Vec<String> = Vec::new();
    for m in joins {
        if let shared::messages::MessagePayload::CombatJoined {
            character, side, ..
        } = &m.payload
        {
            let slot = if side == "side-1" { 0 } else { 1 };
            if !sides[slot].contains(&character.name) {
                sides[slot].push(character.name.clone());
            }
            join_lines.push(format!("{} joins the fight", character.name));
        }
    }
    // WHY: "side-1"/"side-2" is wire jargon, not player language (qy2) —
    // the first chip carries bare names; the second reads "vs" only when
    // there is a first side to be against.
    let second_label = if sides[0].is_empty() { "" } else { "vs" };
    let labels = ["", second_label];
    let sides_ctx: Vec<serde_json::Value> = labels
        .iter()
        .enumerate()
        .filter(|(i, _)| !sides[*i].is_empty())
        .map(|(i, label)| serde_json::json!({ "label": label, "names": sides[i].join(", ") }))
        .collect();
    ctx.insert("sides", &sides_ctx);
    ctx.insert("joins", &join_lines);
    crate::templates::tera_engine::render("combat_card.html", &ctx)
}

/// One beat's narrative line: engine content plus whether it renders
/// prominent (bold). Pure data — no HTML, no escaping (Tera autoescapes).
fn beat_line(beat: &shared::combat_beat::CombatBeat, content: &str) -> (String, bool) {
    use shared::combat_beat::{BeatOutcome, CombatActionKind, SwingOutcome};

    let prominent = match (&beat.action, &beat.outcome) {
        // Engine content already narrates swings; only deaths go bold.
        (
            CombatActionKind::Attack,
            BeatOutcome::Swing {
                outcome:
                    SwingOutcome::Kill { .. }
                    | SwingOutcome::AttackerDied { .. }
                    | SwingOutcome::FumbleDeath { .. }
                    | SwingOutcome::Suicide { .. },
                ..
            },
        ) => true,
        (CombatActionKind::Attack, BeatOutcome::Swing { .. }) => false,
        (CombatActionKind::Flee, BeatOutcome::Fled) => true,
        (CombatActionKind::Cower, BeatOutcome::Cowered) => true,
        (CombatActionKind::CallAlly, _) => true,
        (CombatActionKind::Spare, _) | (CombatActionKind::Execute, _) => true,
        _ => false,
    };
    let text = if content.is_empty() {
        // WHY: the engine pushes CombatBeat events with EMPTY content — the
        // narrative lives in the payload. Derive it here; the old "engine
        // content narrates" assumption left every round body blank.
        use characters::combat_beat::CombatBeatExt;
        // One engine line per visual line: combat_card renders .beat with
        // white-space:pre-line, so the emoji column stacks instead of running
        // on behind mid-sentence " · " joins (qy2).
        beat.to_log_lines().join("\n")
    } else {
        content.to_string()
    };
    (text, prominent)
}

pub fn render_commentary_card(seg: &announcers::CommentarySegment) -> String {
    let speaker = seg
        .lines
        .first()
        .map(|l| l.speaker.as_str())
        .unwrap_or("COMMENTATOR");

    let mut ctx = tera::Context::new();
    ctx.insert("day", &seg.day);
    ctx.insert("phase", &seg.phase.to_string());
    ctx.insert(
        "lines",
        &seg.lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>(),
    );
    ctx.insert("speaker", speaker);
    crate::templates::tera_engine::render("commentary_card.html", &ctx)
}

pub fn render_character_row(
    character: &characters::Character,
    game_id: &str,
    storage: &dyn crate::storage::StorageBackend,
) -> String {
    let is_alive = character.is_alive();
    let health = character.effective_health();
    let health_class = if health > 60 {
        "high"
    } else if health > 20 {
        "mid"
    } else if health > 0 {
        "low"
    } else {
        "empty"
    };
    // Avatar where it belongs (81u): the uploaded image, else the DiceBear
    // fallback seeded by identifier — same rule as the my-goblins page.
    let avatar_src = match character.avatar.as_deref() {
        Some(path) => storage.public_url(path),
        None => crate::storage::goblin_avatar_url(&character.identifier),
    };
    let avatar_color = if is_alive {
        "var(--broad-accent)"
    } else {
        "var(--broad-danger)"
    };
    let status_class = if is_alive { "alive" } else { "dead" };
    let status_text = if is_alive { "ALIVE" } else { "DEAD" };
    let dead_class = if !is_alive { " dead" } else { "" };
    let character_id = &character.identifier;
    let team_color = world::clans::team_color(character.team);

    format!(
        r#"<a href="/games/{game_id}/characters/{character_id}" class="roster-row{dead_class}" style="text-decoration:none;color:inherit;display:block;">
          <div>
            <div class="roster-avatar" style="border-color:{avatar_color};color:{avatar_color}"><img src="{avatar_src}" alt=""></div>
            <div class="roster-info">
              <span class="roster-name">{name}</span>
              <span class="roster-clan" style="background:{team_hex};color:#fff;padding:1px 6px;border-radius:3px;" title="{team_title}">{team_name}</span>
            </div>
            <div class="roster-health">
              <div class="roster-health-bar">
                <div class="roster-health-fill {health_class}" style="width:{health}%;"></div>
              </div>
            </div>
            <span class="roster-status {status_class}">{status_text}</span>
          </div>
        </a>"#,
        name = html_escape(&character.name),
        team_hex = team_color.hex,
        team_name = team_color.name,
        team_title = world::clans::team_label(character.team),
    )
}

pub fn render_character_card(character: &characters::Character) -> String {
    let is_alive = character.is_alive();
    let health = character.effective_health();
    let _health_class = if health > 60 {
        "high"
    } else if health > 20 {
        "mid"
    } else if health > 0 {
        "low"
    } else {
        "empty"
    };
    let status_class = if is_alive { "alive" } else { "dead" };
    let status_text = if is_alive { "ALIVE" } else { "DEAD" };
    let hunger = hunger_label(character.hunger);
    let hunger_c = hunger_color(character.hunger);
    let thirst = thirst_label(character.thirst);
    let thirst_c = thirst_color(character.thirst);
    let stamina = stamina_label(character.stamina, character.max_stamina);
    let stamina_c = stamina_color(character.stamina, character.max_stamina);

    format!(
        r#"<div class="character-card">
          <div class="card-top">
            <span class="card-name">{name}</span>
            <span class="card-status {status_class}">{status_text}</span>
          </div>
          <div class="card-stats">
            <div>HP <span class="stat-val">{health}</span></div>
            <div>STR <span class="stat-val">{strength}</span></div>
            <div>DEF <span class="stat-val">{defense}</span></div>
            <div>INT <span class="stat-val">{intelligence}</span></div>
          </div>
          <div class="card-bands">
            <span style="color:{hunger_c}">{hunger}</span>
            <span style="color:{thirst_c}">{thirst}</span>
            <span style="color:{stamina_c}">{stamina}</span>
          </div>
        </div>"#,
        name = html_escape(&character.name),
        strength = character.attributes.strength,
        defense = character.attributes.toughness,
        intelligence = character.attributes.intelligence,
    )
}

pub fn render_character_detail(character: &characters::Character, _game_id: &str) -> String {
    let is_alive = character.is_alive();
    let health = character.effective_health();
    let status_class = if is_alive { "alive" } else { "dead" };
    let status_text = if is_alive { "ALIVE" } else { "DEAD" };
    let hunger = hunger_label(character.hunger);
    let hunger_c = hunger_color(character.hunger);
    let thirst = thirst_label(character.thirst);
    let thirst_c = thirst_color(character.thirst);
    let stamina = stamina_label(character.stamina, character.max_stamina);
    let stamina_c = stamina_color(character.stamina, character.max_stamina);
    let team_color = world::clans::team_color(character.team);
    let team_label = world::clans::team_label(character.team);
    let clan_name_line = if character.clan_name.is_empty() {
        String::new()
    } else {
        format!(
            "<div style=\"color:var(--muted);\">Clan: {}</div>",
            html_escape(&character.clan_name)
        )
    };

    let mut items_html = String::new();
    for item in &character.items {
        items_html.push_str(&format!(
            "<span class=\"item-tag\">{}</span>",
            html_escape(&item.name)
        ));
    }
    if items_html.is_empty() {
        items_html = "<span style=\"color:var(--muted);font-size:var(--fs-xs);\">No items</span>"
            .to_string();
    }

    let mut afflictions_html = String::new();
    for kind in character.afflictions.keys() {
        afflictions_html.push_str(&format!(
            "<span class=\"affliction-badge severity-moderate\">{}</span>",
            html_escape(&format!("{:?}", kind))
        ));
    }

    // Trait rows render through the Tera partial (pure data in, escaped
    // markup out); only the wrapper div lives in this format! string.
    let traits_block = if character.traits.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"card-traits\">{}</div>",
            render_trait_panel(&character.traits, &character.temporary_traits)
        )
    };

    format!(
        r#"<div class="detail-header">
          <div>
            <h1>{name}</h1>
            <span class="card-status {status_class}">{status_text}</span>
            <div><span style="display:inline-block;width:10px;height:10px;border-radius:2px;background:{team_hex};vertical-align:middle;margin-right:4px;"></span>{team_label}</div>
            {clan_name_line}
          </div>
        </div>
        <div class="card-stats">
          <div>HP <span class="stat-val">{health}</span></div>
          <div>STR <span class="stat-val">{strength}</span></div>
          <div>DEF <span class="stat-val">{defense}</span></div>
          <div>INT <span class="stat-val">{intelligence}</span></div>
        </div>
        <div class="card-bands">
          <span style="color:{hunger_c}">{hunger}</span>
          <span style="color:{thirst_c}">{thirst}</span>
          <span style="color:{stamina_c}">{stamina}</span>
        </div>
        <div class="card-items">{items}</div>
        <div class="card-afflictions">{afflictions}</div>
        {traits_block}"#,
        name = html_escape(&character.name),
        team_hex = team_color.hex,
        team_label = team_label,
        clan_name_line = clan_name_line,
        strength = character.attributes.strength,
        defense = character.attributes.toughness,
        intelligence = character.attributes.intelligence,
        items = items_html,
        afflictions = afflictions_html,
        traits_block = traits_block,
    )
}

/// Trait rows for one character, rendered through the `trait_list.html`
/// partial (spec §10.7: Boon/Curse tag + effect lines, Personality name
/// only). Rust passes pure [`characters::traits::TraitView`] data; Tera
/// owns the markup and escapes it. Temporary traits (event boons/curses
/// with an N-game duration, see `Character::temporary_traits`) get their
/// games-left count attached from `temporary_traits`.
pub fn render_trait_panel(
    traits: &[characters::traits::Trait],
    temporary_traits: &std::collections::BTreeMap<characters::traits::Trait, u32>,
) -> String {
    let views: Vec<characters::traits::TraitView> = traits
        .iter()
        .map(|t| match temporary_traits.get(t) {
            Some(&games_remaining) => t.with_games_remaining(games_remaining),
            None => t.view(),
        })
        .collect();
    let mut ctx = tera::Context::new();
    ctx.insert("traits", &views);
    crate::templates::tera_engine::render("trait_list.html", &ctx)
}

/// Trait presentation data from a persisted character row: the `traits`
/// field is an array of PascalCase variant names, and a row's
/// `temporary_traits` map (Trait -> games remaining) attaches the
/// games-left countdown to the matching views. A missing or unknown
/// array degrades to no traits rather than failing the page.
pub fn trait_views_from_row(row: &serde_json::Value) -> Vec<characters::traits::TraitView> {
    let temporary: std::collections::BTreeMap<characters::traits::Trait, u32> = row
        .get("temporary_traits")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    row.get("traits")
        .cloned()
        .and_then(|v| serde_json::from_value::<Vec<characters::traits::Trait>>(v).ok())
        .map(|traits| {
            traits
                .iter()
                .map(|t| match temporary.get(t) {
                    Some(&games_remaining) => t.with_games_remaining(games_remaining),
                    None => t.view(),
                })
                .collect()
        })
        .unwrap_or_default()
}

const ALLIANCE_COLORS: &[&str] = &[
    "var(--broad-accent)",
    "var(--info)",
    "var(--gold)",
    "var(--purple)",
    "var(--waiting)",
    "var(--running)",
    "var(--danger)",
];

pub struct AllianceGroup {
    pub name: String,
    pub color: &'static str,
    pub characters: Vec<usize>,
}

pub fn build_alliance_groups(characters: &[&characters::Character]) -> Vec<AllianceGroup> {
    let n = characters.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }

    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    let id_to_idx: std::collections::HashMap<String, usize> = characters
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.to_string(), i))
        .collect();

    for (i, character) in characters.iter().enumerate() {
        for ally_id in &character.allies {
            if let Some(&j) = id_to_idx.get(&ally_id.to_string()) {
                union(&mut parent, i, j);
            }
        }
    }

    let mut groups: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }

    let mut alliance_groups: Vec<AllianceGroup> = groups
        .into_iter()
        .enumerate()
        .map(|(color_idx, (_, indices))| {
            let color = ALLIANCE_COLORS[color_idx % ALLIANCE_COLORS.len()];
            let name = if indices.len() == 1 {
                "SOLO".to_string()
            } else {
                format!("ALLIANCE {}", color_idx + 1)
            };
            AllianceGroup {
                name,
                color,
                characters: indices,
            }
        })
        .collect();

    alliance_groups.sort_by(|a, b| {
        let a_alive = a
            .characters
            .iter()
            .filter(|&&i| characters[i].is_alive())
            .count();
        let b_alive = b
            .characters
            .iter()
            .filter(|&&i| characters[i].is_alive())
            .count();
        b_alive.cmp(&a_alive)
    });

    alliance_groups
}

pub fn render_alliance_group(
    group: &AllianceGroup,
    characters: &[&characters::Character],
    game_id: &str,
    storage: &dyn crate::storage::StorageBackend,
) -> String {
    let alive_count = group
        .characters
        .iter()
        .filter(|&&i| characters[i].is_alive())
        .count();

    let mut rows = String::new();
    for &idx in &group.characters {
        rows.push_str(&render_character_row(characters[idx], game_id, storage));
    }

    format!(
        r#"<div class="alliance-group">
          <div class="alliance-header" style="border-left-color:{color};">
            <span class="alliance-name">{name}</span>
            <span class="alliance-count">{alive_count}/{total} alive</span>
          </div>
          {rows}
        </div>"#,
        color = group.color,
        name = group.name,
        total = group.characters.len(),
    )
}

pub struct TeamGroup<'a> {
    pub team: u32,
    pub alive_count: usize,
    pub total: usize,
    pub characters: Vec<&'a characters::Character>,
}

pub fn build_team_groups<'a>(characters: &[&'a characters::Character]) -> Vec<TeamGroup<'a>> {
    let mut groups: std::collections::HashMap<u32, Vec<&characters::Character>> =
        std::collections::HashMap::new();
    for &character in characters {
        groups.entry(character.team).or_default().push(character);
    }
    let mut result: Vec<TeamGroup<'a>> = groups
        .into_iter()
        .map(|(team, members)| {
            let alive_count = members.iter().filter(|t| t.is_alive()).count();
            TeamGroup {
                team,
                alive_count,
                total: members.len(),
                characters: members,
            }
        })
        .collect();
    result.sort_by_key(|g| g.team);
    result
}

pub fn render_team_group(
    group: &TeamGroup,
    game_id: &str,
    storage: &dyn crate::storage::StorageBackend,
) -> String {
    let mut rows = String::new();
    for character in &group.characters {
        rows.push_str(&render_character_row(character, game_id, storage));
    }
    let team_color = world::clans::team_color(group.team);
    format!(
        r#"<div class="clan-group">
          <div class="alliance-header" style="border-left-color:var(--broad-accent);">
            <span class="alliance-name"><span style="display:inline-block;width:10px;height:10px;border-radius:2px;background:{hex};vertical-align:middle;margin-right:4px;"></span>{label}</span>
            <span class="alliance-count">{alive}/{total} alive</span>
          </div>
          {rows}
        </div>"#,
        hex = team_color.hex,
        label = world::clans::team_label(group.team),
        alive = group.alive_count,
        total = group.total,
    )
}

// iak: the map renders client-side now — this payload is the contract
// between the play page and arena-map.js. Geometry (to_pixel, wedge
// centroids) lives in the renderer; the server only ships data.

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct MapState {
    pub areas: Vec<MapArea>,
    pub tokens: Vec<MapToken>,
}

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct MapArea {
    pub id: areas::AreaId,
    pub name: String,
    pub q: i32,
    pub r: i32,
    /// Area-wide base terrain; fallback fill when sub_tiles is empty.
    pub terrain: String,
    pub open: bool,
    pub sub_tiles: Vec<MapSubTile>,
    pub hazards: Vec<MapHazard>,
    pub items: usize,
}

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct MapSubTile {
    /// BaseTerrain variant name ("Forest", ...) — keys the renderer palette.
    pub terrain: String,
    pub feature: Option<String>,
}

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct MapHazard {
    pub event: String,
    pub sub: u8,
    pub blocks_left: u8,
}

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct MapToken {
    pub id: String,
    pub name: String,
    pub team_hex: &'static str,
    pub area: areas::AreaId,
    pub sub: u8,
    pub alive: bool,
    /// Avatar URL: the uploaded image, else the DiceBear fallback seeded by
    /// identifier — same rule as the roster (81u).
    pub avatar: String,
}

pub fn build_map_state(
    areas: &[areas::AreaDetails],
    characters: &[&characters::Character],
    storage: &dyn crate::storage::StorageBackend,
) -> MapState {
    let mut out: Vec<MapArea> = areas
        .iter()
        .filter_map(|a| {
            let id = a.area?;
            let ax = a.axial.unwrap_or_else(|| areas::location::area_axial(id));
            Some(MapArea {
                id,
                name: if a.name.is_empty() {
                    id.to_string()
                } else {
                    a.name.clone()
                },
                q: ax.q,
                r: ax.r,
                terrain: format!("{:?}", a.terrain.base),
                open: a.is_open(),
                sub_tiles: a
                    .sub_tiles
                    .iter()
                    .map(|t| MapSubTile {
                        terrain: format!("{:?}", t.terrain),
                        feature: t.feature.clone(),
                    })
                    .collect(),
                hazards: a
                    .cell_hazards
                    .iter()
                    .map(|h| MapHazard {
                        event: h.event.to_string(),
                        sub: h.sub.index(),
                        blocks_left: h.blocks_left,
                    })
                    .collect(),
                items: a.items.len(),
            })
        })
        .collect();
    // Stable order for tests; the renderer positions by q/r anyway.
    out.sort_by_key(|a| (a.q, a.r));

    let mut tokens: Vec<MapToken> = characters
        .iter()
        .map(|c| MapToken {
            id: c.identifier.clone(),
            name: c.name.clone(),
            team_hex: world::clans::team_color(c.team).hex,
            area: c.location.area,
            sub: c.location.sub.index(),
            alive: c.is_alive(),
            avatar: match c.avatar.as_deref() {
                Some(path) => storage.public_url(path),
                None => crate::storage::goblin_avatar_url(&c.identifier),
            },
        })
        .collect();
    tokens.sort_by(|a, b| a.id.cmp(&b.id));

    MapState { areas: out, tokens }
}

/// JSON for the `<script type="application/json">` embed: `<` escaped so
/// a hostile name can't break out of the script element.
pub fn map_state_json(
    areas: &[areas::AreaDetails],
    characters: &[&characters::Character],
    storage: &dyn crate::storage::StorageBackend,
) -> String {
    serde_json::to_string(&build_map_state(areas, characters, storage))
        .expect("MapState is always serializable")
        .replace('<', "\\u003c")
}

#[cfg(test)]
mod map_state_tests {
    use super::*;
    use areas::hex::{Axial, SubTriangle};
    use areas::location::Location;

    fn storage() -> crate::storage::LocalStorage {
        crate::storage::LocalStorage::new("/tmp/gg-map-state-test", "/uploads")
    }

    fn area(id: areas::AreaId) -> areas::AreaDetails {
        areas::AreaDetails {
            area: Some(id),
            name: format!("The {id}"),
            ..Default::default()
        }
    }

    #[test]
    fn positions_come_from_axial_with_default_fallback() {
        let mut with_coords = area(areas::AreaId::sector1());
        with_coords.axial = Some(Axial::new(2, -1));
        let no_coords = area(areas::AreaId::hub());
        let state = build_map_state(&[with_coords, no_coords], &[], &storage());

        assert_eq!(state.areas.len(), 2);
        let s1 = state
            .areas
            .iter()
            .find(|a| a.id == areas::AreaId::sector1())
            .expect("sector1 area present");
        assert_eq!((s1.q, s1.r), (2, -1));
        let hub = state
            .areas
            .iter()
            .find(|a| a.id == areas::AreaId::hub())
            .expect("hub area present");
        // No axial on the row: falls back to the default layout (pre-zkp.2).
        assert_eq!((hub.q, hub.r), (0, 0));
    }

    #[test]
    fn area_without_id_is_skipped() {
        let helper = areas::AreaDetails {
            name: "route helper".into(),
            ..Default::default()
        };
        let state = build_map_state(&[helper], &[], &storage());
        assert!(state.areas.is_empty());
    }

    #[test]
    fn hazards_sub_tiles_and_items_ride_the_payload() {
        let mut a = area(areas::AreaId::hub());
        a.sub_tiles = vec![areas::preset::SubTile {
            terrain: world::terrain::BaseTerrain::Forest,
            feature: Some("Hidden Valley".into()),
            flavor: None,
        }];
        a.cell_hazards = vec![areas::CellHazard {
            sub: SubTriangle::new(2),
            event: areas::events::AreaEvent::Avalanche,
            blocks_left: 3,
        }];
        let state = build_map_state(&[a], &[], &storage());
        let hub = &state.areas[0];
        assert!(!hub.open);
        assert_eq!(hub.terrain, "Clearing"); // AreaDetails::default terrain
        assert_eq!(hub.sub_tiles.len(), 1);
        assert_eq!(hub.sub_tiles[0].terrain, "Forest");
        assert_eq!(hub.sub_tiles[0].feature.as_deref(), Some("Hidden Valley"));
        assert_eq!(hub.hazards.len(), 1);
        assert_eq!(hub.hazards[0].sub, 2);
        assert_eq!(hub.hazards[0].blocks_left, 3);
        assert_eq!(hub.items, 0);
    }

    #[test]
    fn tokens_carry_team_color_position_and_liveness() {
        let mut goblin = characters::Character::new("Stinky".into(), Some(2), None);
        goblin.location = Location::new(areas::AreaId::sector1(), SubTriangle::new(3));
        let refs: Vec<&characters::Character> = vec![&goblin];
        let state = build_map_state(&[], &refs, &storage());

        assert_eq!(state.tokens.len(), 1);
        let t = &state.tokens[0];
        assert_eq!(t.id, goblin.identifier);
        assert_eq!(t.name, "Stinky");
        assert_eq!(t.team_hex, world::clans::team_color(2).hex);
        assert_eq!(t.area, areas::AreaId::sector1());
        assert_eq!(t.sub, 3);
        assert!(t.alive);
    }

    #[test]
    fn tokens_carry_avatar_by_roster_rule() {
        let mut uploaded =
            characters::Character::new("Uppy".into(), Some(1), Some("avatars/uppy.png".into()));
        uploaded.location = Location::new(areas::AreaId::hub(), SubTriangle::new(0));
        let mut plain = characters::Character::new("Plaino".into(), Some(1), None);
        plain.location = Location::new(areas::AreaId::hub(), SubTriangle::new(1));
        let refs: Vec<&characters::Character> = vec![&uploaded, &plain];

        let state = build_map_state(&[], &refs, &storage());

        let uppy = state
            .tokens
            .iter()
            .find(|t| t.name == "Uppy")
            .expect("uploaded token present");
        assert!(
            uppy.avatar.ends_with("/uploads/avatars/uppy.png"),
            "uploaded path resolved through storage: {}",
            uppy.avatar
        );
        let plaino = state
            .tokens
            .iter()
            .find(|t| t.name == "Plaino")
            .expect("fallback token present");
        // Not a storage URL: a DiceBear data URI (idRandomization makes
        // the bytes differ per call, so compare shape, not equality).
        assert!(
            plaino.avatar.starts_with("data:image/svg"),
            "fallback should be a DiceBear data URI: {}",
            &plaino.avatar[..plaino.avatar.len().min(48)]
        );
    }

    #[test]
    fn json_embed_escapes_script_breakout() {
        let mut hostile = area(areas::AreaId::hub());
        hostile.name = "</script><script>alert(1)</script>".into();
        let json = map_state_json(&[hostile], &[], &storage());
        assert!(!json.contains('<'), "raw < leaked: {json}");
        assert!(json.contains("\\u003c/script>"), "missing escape: {json}");
    }
}

#[cfg(test)]
mod day_summary_card_tests {
    use super::*;
    use shared::messages::{
        CharacterRef, DayKillLeader, DayRollup, GameMessage, GoblinDaySummary, MessagePayload,
        MessageSource, Phase,
    };

    fn summary_message() -> GameMessage {
        GameMessage::new(
            MessageSource::Game("g".into()),
            1,
            Phase::DAY_START,
            11,
            0,
            "game:g".into(),
            "Day 1: 1 fallen, 23 standing.".into(),
            MessagePayload::DaySummary {
                day: 1,
                rollup: DayRollup {
                    deaths: 1,
                    kill_leaders: vec![DayKillLeader {
                        character: CharacterRef {
                            identifier: "k".into(),
                            name: "Ash <Bane>".into(),
                        },
                        kills: 2,
                    }],
                    alliances_formed: 1,
                    alliances_dissolved: 0,
                    betrayals: 0,
                    items_found: 3,
                    items_used: 2,
                    survivors: 23,
                    fallen: 1,
                },
                goblins: vec![GoblinDaySummary {
                    character: CharacterRef {
                        identifier: "c".into(),
                        name: "Ash <Bane>".into(),
                    },
                    team: 1,
                    kills: 2,
                    wounds_taken: 1,
                    items_found: 3,
                    items_used: 2,
                    alliance_changes: 1,
                    alive: true,
                    blood: 820,
                    sanity: 100,
                }],
            },
        )
    }

    #[test]
    fn day_summary_card_shows_rollup_and_goblins() {
        let html = render_event_card(&summary_message());
        assert!(html.contains("DAY SUMMARY"), "badge: {html}");
        assert!(html.contains("1 fallen, 23 standing"), "rollup: {html}");
        assert!(
            html.contains("Kills: Ash &lt;Bane&gt; (2)"),
            "leaders: {html}"
        );
        assert!(html.contains("1 goblins"), "goblin count: {html}");
        assert!(html.contains("Ash &lt;Bane&gt;"), "escaped name: {html}");
        assert!(html.contains("820 hp"), "state snapshot: {html}");
        assert!(html.contains("<details"), "collapsible rows: {html}");
        // Plain messages still render through the plain card.
        let plain = GameMessage::new(
            MessageSource::Game("g".into()),
            1,
            Phase::DAY_START,
            1,
            0,
            "game:g".into(),
            "someone fell".into(),
            MessagePayload::CharacterBledOut {
                character: CharacterRef {
                    identifier: "c".into(),
                    name: "C".into(),
                },
            },
        );
        let html = render_event_card(&plain);
        assert!(!html.contains("DAY SUMMARY"), "plain path: {html}");
        assert!(html.contains("event-card"), "{html}");
        // Regression: plain-card timestamps use Phase's Display (the
        // hour), not its Debug ("PHASE(12)").
        assert!(!html.contains("PHASE("), "{html}");
    }

    /// Commentary segments render through commentary_card.html (Tera).
    #[test]
    fn commentary_card_renders_via_partial() {
        let seg = announcers::CommentarySegment {
            id: String::new(),
            game_id: "g".into(),
            day: 1,
            phase: "06".into(),
            lines: vec![announcers::CommentaryLine {
                speaker: "Verity".into(),
                text: "A <coup> at dawn".into(),
            }],
            generated_at: chrono::Utc::now(),
            model_used: "test".into(),
        };
        let html = render_commentary_card(&seg);
        assert!(html.contains("comment-speaker"), "{html}");
        assert!(html.contains("Verity"), "{html}");
        // Tera autoescapes: raw markup in line text must not survive.
        assert!(!html.contains("<coup>"), "{html}");
    }

    /// The timeline template passes pre-rendered cards straight through —
    /// prove a DaySummary card survives the full page render.
    #[test]
    fn timeline_template_renders_day_summary_card() {
        let auth = crate::templates::AuthState::Guest {
            csrf_token: "csrf".into(),
        };
        let mut ctx = crate::templates::tera_engine::base_context("Timeline", &auth);
        let card = render_event_card(&summary_message());
        ctx.insert("game_id", "g");
        ctx.insert("game_name", "Game");
        ctx.insert("current_day", &1u32);
        ctx.insert("current_phase", "06");
        ctx.insert(
            "periods",
            &[shared::messages::PeriodSummary {
                day: 1,
                phase: shared::messages::Phase::DAY_START,
                deaths: 1,
                event_count: 5,
                is_current: true,
            }],
        );
        ctx.insert("filter", "");
        ctx.insert("character_filter", "");
        ctx.insert("characters", &Vec::<shared::messages::CharacterRef>::new());
        ctx.insert("rendered_events", &[card]);
        // Cards render only once a period is selected (grid view otherwise).
        ctx.insert("selected_day", &Some(1u32));
        ctx.insert("selected_phase", &Some("04".to_string()));
        ctx.insert(
            "filter_options",
            &vec![serde_json::json!({"value": "", "label": "All", "icon_name": "list"})],
        );

        let html = crate::templates::tera_engine::render("timeline.html", &ctx);
        assert!(html.contains("DAY SUMMARY"), "badge missing: {html}");
        assert!(html.contains("1 fallen, 23 standing"), "rollup missing");
        assert!(html.contains("<tbody>"), "goblin rows missing");
        assert!(html.contains("Ash &lt;Bane&gt;"), "goblin row missing");
    }

    // ---- Multi-round combat cards ----

    use shared::combat_beat::{
        BeatOutcome, CombatActionKind, CombatBeat, CombatResult, StressReport, SwingOutcome,
    };
    fn cref(name: &str) -> CharacterRef {
        CharacterRef {
            identifier: format!("id-{name}").into(),
            name: name.to_string(),
        }
    }

    fn beat(
        round: u32,
        actor: &str,
        target: &str,
        outcome: BeatOutcome,
        significant: bool,
    ) -> GameMessage {
        let action = match &outcome {
            BeatOutcome::Swing { .. } => CombatActionKind::Attack,
            BeatOutcome::Fled | BeatOutcome::FailedFlee => CombatActionKind::Flee,
            BeatOutcome::Cowered => CombatActionKind::Cower,
            BeatOutcome::Spared => CombatActionKind::Spare,
            BeatOutcome::Executed { .. } => CombatActionKind::Execute,
            BeatOutcome::ItemUsed => CombatActionKind::UseItem {
                item: shared::messages::ItemRef {
                    identifier: "i1".into(),
                    name: "Potion".into(),
                },
            },
            BeatOutcome::CalledAlly => CombatActionKind::CallAlly,
        };
        let msg = format!("{actor} beat content");
        GameMessage::new(
            MessageSource::Game("g".into()),
            2,
            Phase::DAY_START,
            5,
            round,
            "game:g".into(),
            msg,
            MessagePayload::CombatBeat(CombatBeat {
                round,
                order: 0,
                actor: cref(actor),
                action,
                target: Some(cref(target)),
                weapon: None,
                shield: None,
                wear: vec![],
                outcome,
                significant,
            }),
        )
    }

    fn swing_miss() -> BeatOutcome {
        BeatOutcome::Swing {
            outcome: SwingOutcome::Miss,
            stress: StressReport::default(),
            actor_stamina_cost: 0,
            target_stamina_cost: 0,
        }
    }

    fn swing_kill() -> BeatOutcome {
        BeatOutcome::Swing {
            outcome: SwingOutcome::Kill { damage: 40 },
            stress: StressReport::default(),
            actor_stamina_cost: 5,
            target_stamina_cost: 2,
        }
    }

    /// vt9 replay: every round open, round anchors present, nav facts right.
    #[test]
    fn replay_opens_every_round_and_anchors_them() {
        let msgs = vec![
            beat(1, "Boblin", "Stinky", swing_miss(), false),
            beat(2, "Boblin", "Stinky", swing_kill(), true),
            ended(CombatResult::Victory {
                victors: vec![cref("Boblin")],
            }),
        ];
        let sections = replay_sections(&msgs);
        assert_eq!(sections.len(), 1, "one fight group");
        let fight = &sections[0];
        assert_eq!(fight["id"].as_str(), Some("f1"));
        assert_eq!(fight["title"].as_str(), Some("Boblin vs Stinky"));
        assert_eq!(
            fight["result_short"].as_str(),
            Some("Boblin wins the fight")
        );
        assert_eq!(fight["rounds"].as_array().map(|a| a.len()), Some(2));
        let card = fight["card"].as_str().unwrap();
        // Both rounds open — replay inspects, the timeline down-ranks.
        assert!(card.contains(r#"id="f1-r1" open"#), "r1 open: {card}");
        assert!(card.contains(r#"id="f1-r2" open"#), "r2 open: {card}");

        let auth = crate::templates::AuthState::Guest {
            csrf_token: "csrf".into(),
        };
        let mut ctx = crate::templates::tera_engine::base_context("Fight replay", &auth);
        ctx.insert("game_id", "g");
        ctx.insert("fights", &sections);
        let html = crate::templates::tera_engine::render("replay.html", &ctx);
        assert!(html.contains(r#"id="f1""#), "fight section: {html}");
        assert!(html.contains("href=\"#f1-r2\""), "round scrub link");
    }

    /// Stream order: a non-combat message closes a fight; the next run is
    /// its own section, and the timeline card keeps its position.
    #[test]
    fn replay_lists_fights_in_stream_order() {
        let plain = GameMessage::new(
            MessageSource::Game("g".into()),
            1,
            Phase::DAY_START,
            1,
            0,
            "game:g".into(),
            "someone fell".into(),
            MessagePayload::CharacterBledOut {
                character: CharacterRef {
                    identifier: "c".into(),
                    name: "C".into(),
                },
            },
        );
        let msgs = vec![
            beat(1, "A", "B", swing_miss(), false),
            ended(CombatResult::Draw),
            plain,
            beat(1, "D", "E", swing_miss(), false),
        ];
        let sections = replay_sections(&msgs);
        assert_eq!(sections.len(), 2, "flushed at the plain message");
        assert_eq!(sections[0]["title"].as_str(), Some("A vs B"));
        assert_eq!(
            sections[0]["result_short"].as_str(),
            Some("A draw — both break off")
        );
        assert_eq!(sections[1]["id"].as_str(), Some("f2"));
        assert_eq!(
            sections[1]["result_short"].as_str(),
            Some("fight in progress…")
        );
        // Timeline order unchanged: the fight card lands before the plain
        // message that closed it.
        let html = render_event_cards(&msgs);
        let card_at = html.find("A vs B").expect("fight card");
        let plain_at = html.find("someone fell").expect("plain card");
        assert!(card_at < plain_at, "card before closer: {html}");
    }

    fn ended(result: CombatResult) -> GameMessage {
        GameMessage::new(
            MessageSource::Game("g".into()),
            2,
            Phase::DAY_START,
            5,
            9,
            "game:g".into(),
            "fight over".into(),
            MessagePayload::CombatEnded {
                combat_id: "c1".into(),
                result,
                location: None,
            },
        )
    }

    #[test]
    fn combat_beats_collapse_into_one_grouped_card() {
        let msgs = vec![
            beat(1, "Boblin", "Stinky", swing_miss(), false),
            beat(1, "Stinky", "Boblin", swing_miss(), false),
            beat(2, "Boblin", "Stinky", swing_kill(), true),
            ended(CombatResult::Victory {
                victors: vec![cref("Boblin")],
            }),
        ];
        let html = render_event_cards(&msgs);
        // One card, titled, with the result in the header.
        assert_eq!(
            html.matches("event-card combat").count(),
            1,
            "single card: {html}"
        );
        assert!(html.contains("Boblin vs Stinky"), "title: {html}");
        assert!(html.contains("Boblin wins the fight"), "result: {html}");
        assert!(html.contains("Round 1"), "round 1: {html}");
        assert!(html.contains("Round 2"), "round 2: {html}");
        // Ordinary round collapsed, significant round expanded.
        assert!(
            html.contains(r#"<details class="combat-rounds">"#),
            "closed: {html}"
        );
        assert!(
            html.contains(r#"<details class="combat-rounds" open>"#),
            "open: {html}"
        );
        // Kill beat renders prominent (bold), miss beats in the ordinary list.
        assert!(
            html.contains("<strong>Boblin beat content</strong>"),
            "kill bold: {html}"
        );
        assert!(
            html.contains(r#"<ul class="beats-ordinary">"#),
            "ordinary: {html}"
        );
    }

    #[test]
    fn combat_card_interleaves_with_plain_cards() {
        let plain = GameMessage::new(
            MessageSource::Game("g".into()),
            2,
            Phase::DAY_START,
            4,
            0,
            "game:g".into(),
            "someone foraged".into(),
            MessagePayload::Foraged {
                character: cref("Boblin"),
                area: shared::messages::AreaRef {
                    identifier: "a1".into(),
                    name: "Woods".into(),
                },
                success: true,
                debt_recovered: 0,
            },
        );
        let msgs = vec![
            plain,
            beat(1, "Boblin", "Stinky", swing_kill(), true),
            ended(CombatResult::Victory {
                victors: vec![cref("Boblin")],
            }),
        ];
        let html = render_event_cards(&msgs);
        assert!(html.contains("someone foraged"), "plain card: {html}");
        assert!(html.contains("event-card combat"), "combat card: {html}");
        // Plain card comes first.
        assert!(html.find("someone foraged").unwrap() < html.find("Boblin vs Stinky").unwrap());
    }

    #[test]
    fn combat_result_variants_render() {
        for (result, expected) in [
            (
                CombatResult::Victory {
                    victors: vec![cref("Boblin")],
                },
                "Boblin wins the fight",
            ),
            (
                CombatResult::Victory {
                    victors: vec![cref("Boblin"), cref("Stinky")],
                },
                // Tera escapes the joiner: raw "&" becomes "&amp;".
                "Boblin &amp; Stinky win the fight",
            ),
            (CombatResult::Draw, "A draw"),
        ] {
            let html = render_event_cards(&[ended(result)]);
            assert!(html.contains(expected), "expected {expected:?} in {html}");
        }
    }

    #[test]
    fn combat_ongoing_fight_renders_placeholder_result() {
        let html = render_event_cards(&[beat(1, "Boblin", "Stinky", swing_miss(), false)]);
        assert!(html.contains("fight in progress"), "{html}");
    }

    fn joined(round: u32, name: &str, side: &str) -> GameMessage {
        GameMessage::new(
            MessageSource::Game("g".into()),
            2,
            Phase::DAY_START,
            5,
            round,
            "game:g".into(),
            format!("{name} joins the fight!"),
            MessagePayload::CombatJoined {
                combat_id: "c1".into(),
                character: cref(name),
                side: side.into(),
                round,
            },
        )
    }

    #[test]
    fn combat_joined_stays_inside_the_fight_card() {
        let msgs = vec![
            beat(1, "Boblin", "Stinky", swing_miss(), false),
            joined(2, "Carol", "side-1"),
            beat(2, "Boblin", "Stinky", swing_miss(), false),
            ended(CombatResult::Victory {
                victors: vec![cref("Boblin")],
            }),
        ];
        let html = render_event_cards(&msgs);
        assert_eq!(
            html.matches("event-card combat").count(),
            1,
            "join must not split the fight card: {html}"
        );
        assert!(html.contains("Carol joins the fight"), "join line: {html}");
        assert!(
            html.contains("<span class=\"kind-chip\">Boblin, Carol</span>"),
            "first chip: {html}"
        );
        assert!(
            html.contains("<span class=\"kind-chip\">vs Stinky</span>"),
            "second chip: {html}"
        );
        assert!(!html.contains("side-1"), "wire jargon leaked: {html}");
    }

    #[test]
    fn combat_card_without_beats_renders_joins_and_placeholder() {
        let html = render_event_cards(&[joined(1, "Carol", "side-2")]);
        assert_eq!(html.matches("event-card combat").count(), 1, "{html}");
        assert!(html.contains("fight in progress"), "{html}");
        assert!(
            html.contains("<span class=\"kind-chip\">Carol</span>"),
            "{html}"
        );
        assert!(!html.contains("side-2"), "wire jargon leaked: {html}");
    }

    #[test]
    fn combat_names_are_escaped_in_card() {
        let evil = "<script>".to_string();
        let b = beat(1, &evil, "Stinky", swing_kill(), true);
        let html = render_event_cards(&[b]);
        assert!(!html.contains("<script>"), "unescaped: {html}");
        assert!(html.contains("&lt;script&gt;"), "escaped: {html}");
    }
}

/// Trait display on the two surfaces (spec §10.7): the character sheet's
/// trait panel and the timeline's per-character trait include.
#[cfg(test)]
mod trait_panel_tests {
    use super::*;
    use characters::traits::Trait;

    fn sheet_character() -> characters::Character {
        let mut character = characters::Character::new("Stinky".into(), None, None);
        character.traits = vec![
            Trait::Tough,
            Trait::Asthmatic,
            Trait::Nearsighted,
            Trait::Friendly,
        ];
        character
    }

    #[test]
    fn character_detail_shows_kind_tags_and_effect_lines() {
        let html = render_character_detail(&sheet_character(), "g");
        assert!(html.contains(">Boon<"), "tough tag missing: {html}");
        assert!(
            html.contains("+20% toughness"),
            "tough effect missing: {html}"
        );
        assert!(html.contains(">Curse<"), "curse tag missing: {html}");
        assert!(html.contains("-15% max stamina"), "asthmatic: {html}");
        assert!(html.contains("-25% perception"), "nearsighted: {html}");
        // Personality: name only — no kind tag, no effect lines.
        assert!(html.contains("friendly"), "personality name: {html}");
        assert!(!html.contains("Personality"), "kind leaked: {html}");
        assert!(
            !html.contains("steers decisions"),
            "personality effects: {html}"
        );
    }

    #[test]
    fn trait_panel_escapes_hostile_names_and_phrases() {
        // Trait names are enum labels today, but flavor text can arrive
        // from elsewhere — the partial must rely on Tera autoescape.
        let hostile = characters::traits::TraitView {
            name: "<script>alert(1)</script>".into(),
            kind_tag: Some("Curse".into()),
            effect_lines: vec!["<b>−25% movement</b>".into()],
            games_remaining: None,
        };
        let mut ctx = tera::Context::new();
        ctx.insert("traits", &[hostile]);
        let html = crate::templates::tera_engine::render("trait_list.html", &ctx);
        assert!(!html.contains("<script>"), "unescaped: {html}");
        assert!(html.contains("&lt;script&gt;"), "escaped: {html}");
        assert!(!html.contains("<b>"), "unescaped phrase: {html}");
        assert!(html.contains("&lt;b&gt;"), "escaped phrase: {html}");
    }

    #[test]
    fn trait_panel_shows_games_left_for_temporary_traits() {
        use std::collections::BTreeMap;
        let traits = vec![
            characters::traits::Trait::Asthmatic,
            characters::traits::Trait::Friendly,
        ];
        let mut temporary = BTreeMap::new();
        temporary.insert(characters::traits::Trait::Asthmatic, 2u32);

        let html = render_trait_panel(&traits, &temporary);
        assert!(html.contains("2 games left"), "{html}");
        // One countdown per temporary trait only.
        assert_eq!(html.matches("games left").count(), 1, "{html}");

        // Singular at one game.
        temporary.insert(characters::traits::Trait::Asthmatic, 1u32);
        let html = render_trait_panel(&traits, &temporary);
        assert!(html.contains("1 game left"), "{html}");

        // All-permanent panel carries no countdown.
        let html = render_trait_panel(&traits, &BTreeMap::new());
        assert!(!html.contains("game left"), "{html}");
    }

    #[test]
    fn trait_views_from_persisted_row_and_garbage() {
        let row = serde_json::json!({ "name": "Stinky", "traits": ["Asthmatic", "Tough"] });
        let views = trait_views_from_row(&row);
        assert_eq!(views.len(), 2);
        assert_eq!(views[0].name, "asthmatic");
        assert_eq!(views[0].effect_lines, vec!["-15% max stamina"]);
        assert_eq!(views[1].kind_tag.as_deref(), Some("Boon"));

        // Missing or unknown fields degrade to no traits, no page failure.
        assert!(trait_views_from_row(&serde_json::json!({"name": "x"})).is_empty());
        assert!(
            trait_views_from_row(&serde_json::json!({"traits": ["NotATrait"]})).is_empty(),
            "unknown variant must degrade quietly"
        );

        // A row's temporary_traits map attaches the games-left countdown
        // to exactly the matching views; permanent traits stay None.
        let row = serde_json::json!({
            "name": "Stinky",
            "traits": ["Asthmatic", "Tough"],
            "temporary_traits": { "Tough": 2 }
        });
        let views = trait_views_from_row(&row);
        assert_eq!(
            views[0].games_remaining, None,
            "asthmatic is permanent here"
        );
        assert_eq!(views[1].games_remaining, Some(2), "tough has 2 games left");

        // Garbage durations degrade to permanent, not a page failure.
        let row = serde_json::json!({
            "traits": ["Tough"],
            "temporary_traits": { "Tough": "many" }
        });
        assert_eq!(trait_views_from_row(&row)[0].games_remaining, None);
    }

    /// The timeline include renders the selected character's panel and
    /// disappears when nobody is selected (no traits in context).
    #[test]
    fn timeline_template_renders_selected_characters_traits() {
        let auth = crate::templates::AuthState::Guest {
            csrf_token: "csrf".into(),
        };
        let mut ctx = crate::templates::tera_engine::base_context("Timeline", &auth);
        ctx.insert("game_id", "g");
        ctx.insert("game_name", "Game");
        ctx.insert("current_day", &1u32);
        ctx.insert("current_phase", "06");
        ctx.insert("periods", &Vec::<shared::messages::PeriodSummary>::new());
        ctx.insert("filter", "");
        ctx.insert("character_filter", "Stinky");
        ctx.insert("characters", &Vec::<shared::messages::CharacterRef>::new());
        ctx.insert("rendered_events", &Vec::<String>::new());
        ctx.insert("selected_day", &Some(1u32));
        ctx.insert("selected_phase", &Some("06".to_string()));
        ctx.insert(
            "filter_options",
            &vec![serde_json::json!({"value": "", "label": "All", "icon_name": "list"})],
        );
        ctx.insert("traits", &[Trait::Asthmatic.view(), Trait::Friendly.view()]);

        let html = crate::templates::tera_engine::render("timeline.html", &ctx);
        assert!(html.contains("Traits"), "panel heading: {html}");
        assert!(html.contains(">Curse<"), "kind tag: {html}");
        assert!(html.contains("-15% max stamina"), "effect line: {html}");
        assert!(html.contains("friendly"), "personality name: {html}");
        assert!(!html.contains("Personality"), "kind leaked: {html}");

        ctx.insert("traits", &Vec::<characters::traits::TraitView>::new());
        let html = crate::templates::tera_engine::render("timeline.html", &ctx);
        assert!(!html.contains("-15% max stamina"), "panel shown: {html}");
        assert!(!html.contains("Traits"), "heading shown: {html}");
    }
}
