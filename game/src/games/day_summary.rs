//! End-of-day digest compilation — the one stored summary row per game
//! day. Folds the day's messages plus end-of-day roster state into a
//! [`MessagePayload::DaySummary`]; the persist gate keeps it (Persist
//! tier) while the per-event noise it replaces is server-log-only.

use std::collections::HashMap;

use shared::messages::{CharacterRef, DayKillLeader, DayRollup, GoblinDaySummary, MessagePayload};

use super::Game;

impl Game {
    /// Fold `game.messages` stamped with `day` (plus current character
    /// state) into a `DaySummary` payload. Call before the day counter
    /// advances so `day` matches the messages' stored `game_day`.
    pub(crate) fn compile_day_summary(&self, day: u32) -> MessagePayload {
        let mut deaths = 0u32;
        let mut alliances_formed = 0u32;
        let mut alliances_dissolved = 0u32;
        let mut betrayals = 0u32;
        let mut items_found = 0u32;
        let mut items_used = 0u32;

        // Per-goblin message-derived counters (keyed by identifier).
        let mut kills: HashMap<String, (CharacterRef, u32)> = HashMap::new();
        let mut wounds: HashMap<String, u32> = HashMap::new();
        let mut found: HashMap<String, u32> = HashMap::new();
        let mut used: HashMap<String, u32> = HashMap::new();
        let mut alliance_changes: HashMap<String, u32> = HashMap::new();

        for msg in self.messages.iter().filter(|m| m.game_day == day) {
            match &msg.payload {
                MessagePayload::CharacterKilled { killer, .. } => {
                    deaths += 1;
                    if let Some(k) = killer {
                        let entry = kills
                            .entry(k.identifier.to_string())
                            .or_insert_with(|| (k.clone(), 0));
                        entry.1 += 1;
                    }
                }
                MessagePayload::CharacterWounded { victim, .. } => {
                    *wounds.entry(victim.identifier.to_string()).or_default() += 1;
                }
                MessagePayload::ItemFound { character, .. } => {
                    items_found += 1;
                    *found.entry(character.identifier.to_string()).or_default() += 1;
                }
                MessagePayload::ItemUsed { character, .. } => {
                    items_used += 1;
                    *used.entry(character.identifier.to_string()).or_default() += 1;
                }
                MessagePayload::AllianceFormed { members } => {
                    alliances_formed += 1;
                    for m in members {
                        *alliance_changes
                            .entry(m.identifier.to_string())
                            .or_default() += 1;
                    }
                }
                MessagePayload::AllianceDissolved { members, .. } => {
                    alliances_dissolved += 1;
                    for m in members {
                        *alliance_changes
                            .entry(m.identifier.to_string())
                            .or_default() += 1;
                    }
                }
                MessagePayload::AllianceProposed { proposer, target } => {
                    for r in [proposer, target] {
                        *alliance_changes
                            .entry(r.identifier.to_string())
                            .or_default() += 1;
                    }
                }
                MessagePayload::BetrayalTriggered { betrayer, victim } => {
                    betrayals += 1;
                    for r in [betrayer, victim] {
                        *alliance_changes
                            .entry(r.identifier.to_string())
                            .or_default() += 1;
                    }
                }
                MessagePayload::TrustShockBreak { character, partner } => {
                    for r in [character, partner] {
                        *alliance_changes
                            .entry(r.identifier.to_string())
                            .or_default() += 1;
                    }
                }
                _ => {}
            }
        }

        let mut kill_leaders: Vec<DayKillLeader> = kills
            .values()
            .map(|(character, n)| DayKillLeader {
                character: character.clone(),
                kills: *n,
            })
            .collect();
        kill_leaders.sort_by(|a, b| {
            b.kills
                .cmp(&a.kills)
                .then(a.character.name.cmp(&b.character.name))
        });
        kill_leaders.truncate(3);

        let survivors = self.characters.iter().filter(|c| c.is_alive()).count() as u32;
        let fallen = self.characters.len() as u32 - survivors;

        let goblins: Vec<GoblinDaySummary> = self
            .characters
            .iter()
            .map(|c| {
                let ident = c.identifier.to_string();
                GoblinDaySummary {
                    character: CharacterRef {
                        identifier: c.identifier.clone().into(),
                        name: c.name.clone(),
                    },
                    team: c.team,
                    kills: kills.get(&ident).map_or(0, |(_, n)| *n),
                    wounds_taken: wounds.get(&ident).copied().unwrap_or(0),
                    items_found: found.get(&ident).copied().unwrap_or(0),
                    items_used: used.get(&ident).copied().unwrap_or(0),
                    alliance_changes: alliance_changes.get(&ident).copied().unwrap_or(0),
                    alive: c.is_alive(),
                    blood: c.blood,
                    sanity: c.effective_sanity(),
                }
            })
            .collect();

        MessagePayload::DaySummary {
            day,
            rollup: DayRollup {
                deaths,
                kill_leaders,
                alliances_formed,
                alliances_dissolved,
                betrayals,
                items_found,
                items_used,
                survivors,
                fallen,
            },
            goblins,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use characters::{Character, statuses::CharacterStatus};
    use shared::messages::{MessageSource, Phase};

    fn game_with(names: &[&str]) -> Game {
        let mut game = Game::new("summary-game");
        game.start().expect("start");
        for (i, name) in names.iter().enumerate() {
            let mut c = Character::new(name.to_string(), None, None);
            c.blood = 1000;
            c.team = i as u32 + 1;
            game.characters.push(c);
        }
        game
    }

    fn push(game: &mut Game, payload: MessagePayload) {
        let subject = format!("game:{}", game.identifier);
        game.push_message(
            MessageSource::Game(game.identifier.clone()),
            subject,
            String::new(),
            payload,
            0,
        );
    }

    fn reference(game: &Game, idx: usize) -> CharacterRef {
        CharacterRef {
            identifier: game.characters[idx].identifier.clone().into(),
            name: game.characters[idx].name.clone(),
        }
    }

    #[test]
    fn compile_day_summary_folds_messages_and_state() {
        use shared::afflictions::DeathCause;
        let mut game = game_with(&["Ash", "Bram", "Cinder"]);

        // Bram falls to Ash; Cinder gets wounded; Ash finds and uses an
        // item; Ash and Cinder ally. (References first: push borrows mutably.)
        game.characters[1].blood = 0;
        game.characters[1].status = CharacterStatus::Dead;
        let ash = reference(&game, 0);
        let bram = reference(&game, 1);
        let cinder = reference(&game, 2);
        let shiv = shared::messages::ItemRef {
            identifier: "item-1".into(),
            name: "Rusty Shiv".into(),
        };
        push(
            &mut game,
            MessagePayload::CharacterKilled {
                victim: bram,
                killer: Some(ash.clone()),
                cause: DeathCause::Combat,
            },
        );
        push(
            &mut game,
            MessagePayload::CharacterWounded {
                victim: cinder.clone(),
                attacker: Some(ash.clone()),
                hp_lost: 30,
            },
        );
        push(
            &mut game,
            MessagePayload::ItemFound {
                character: ash.clone(),
                item: shiv.clone(),
                area: shared::messages::AreaRef {
                    identifier: "area-1".into(),
                    name: "Hub".into(),
                },
            },
        );
        push(
            &mut game,
            MessagePayload::ItemUsed {
                character: ash.clone(),
                item: shiv,
            },
        );
        push(
            &mut game,
            MessagePayload::AllianceFormed {
                members: vec![ash, cinder],
            },
        );

        let MessagePayload::DaySummary {
            day,
            rollup,
            goblins,
        } = game.compile_day_summary(0)
        else {
            panic!("compiler must emit a DaySummary payload");
        };

        assert_eq!(day, 0);
        assert_eq!(rollup.deaths, 1);
        assert_eq!(rollup.survivors, 2);
        assert_eq!(rollup.fallen, 1);
        assert_eq!(rollup.items_found, 1);
        assert_eq!(rollup.items_used, 1);
        assert_eq!(rollup.alliances_formed, 1);
        assert_eq!(rollup.kill_leaders.len(), 1);
        assert_eq!(rollup.kill_leaders[0].character.name, "Ash");
        assert_eq!(rollup.kill_leaders[0].kills, 1);

        assert_eq!(goblins.len(), 3);
        let ash = goblins.iter().find(|g| g.character.name == "Ash").unwrap();
        assert_eq!(ash.kills, 1);
        assert_eq!(ash.items_found, 1);
        assert_eq!(ash.items_used, 1);
        assert_eq!(ash.alliance_changes, 1);
        assert!(ash.alive);
        assert_eq!(ash.blood, 1000);
        let bram = goblins.iter().find(|g| g.character.name == "Bram").unwrap();
        assert!(!bram.alive);
        let cinder = goblins
            .iter()
            .find(|g| g.character.name == "Cinder")
            .unwrap();
        assert_eq!(cinder.wounds_taken, 1);
        assert_eq!(cinder.alliance_changes, 1);
    }

    #[test]
    fn full_day_emits_day_summary() {
        let mut game = game_with(&["Ash", "Bram", "Cinder"]);
        game.run_full_day().expect("full day must run");

        let summaries: Vec<(u32, usize)> = game
            .messages
            .iter()
            .filter_map(|m| match &m.payload {
                MessagePayload::DaySummary { day, goblins, .. } => Some((*day, goblins.len())),
                _ => None,
            })
            .collect();
        assert_eq!(summaries.len(), 1, "exactly one digest per full day");
        // The digest's label must match the day stamped on the messages it
        // summarizes (whatever the engine's day counter is during the run).
        let other_day = game
            .messages
            .iter()
            .find(|m| !matches!(m.payload, MessagePayload::DaySummary { .. }))
            .map(|m| m.game_day)
            .expect("the run produced other messages");
        assert_eq!(summaries[0].0, other_day, "digest label matches the day");
        assert_eq!(summaries[0].1, 3, "one entry per goblin on the roster");
    }

    #[test]
    fn day_summary_is_persist_tier() {
        let game = game_with(&["Ash"]);
        let payload = game.compile_day_summary(0);
        assert_eq!(
            payload.importance(),
            shared::messages::Importance::Persist,
            "the compiled digest must survive the persist gate"
        );
        let _ = Phase::DAY; // keep Phase imported for future day-label tests
    }
}
