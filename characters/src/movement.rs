//! Sub-tile travel for characters.
//!
//! A move is one *sub-hop* along the occupied-cell graph
//! (`areas::location`); an area change is a side effect of the hop
//! (the parent component of the `Location` flips when the hop crosses a
//! facing-pair door). Per the owner decision in the sub-hex spike §8
//! Q1 (choice C), a phase runs two beats and every Move beat is
//! exactly ONE sub-hop: two Move beats are the two sub-hops cap.
//! Each hop pays its CROSSING price (`crossing_stamina_cost`: half the
//! source cell's standard move cost plus half the destination's, each
//! rounded up); whatever stamina the hops leave stays for later
//! rounds.
//!
//! No attribute gates live here: stamina decides whether a hop
//! happens (the action dispatch's stamina pre-gate and the hop
//! engine's own checks below).

use crate::Character;
use crate::actions::Action;
use areas::location::{Location, shortest_path_while, sub_neighbors};
use areas::{Area, AreaDetails};
use rand::prelude::*;
use rand::rngs::SmallRng;
use shared::messages::{AreaRef, CharacterRef, MessagePayload};
use world::messages::TaggedEvent;
use world::output::GameOutput;

#[derive(Debug, PartialEq)]
pub enum TravelResult {
    Success(Location),
    Failure,
}

impl Character {
    /// The stamina price of one sub-hop from `from` to `to`: the
    /// crossing cost of the two cells' standard move costs (base 20 x
    /// terrain multiplier x affinity x desperation, per area's
    /// terrain) — half the source's plus half the destination's, each
    /// rounded up (see [`crate::crossing_stamina_cost`]). WHY:
    /// crossing-priced movement — neither tile dominates a crossing
    /// (owner decision; see the formula's doc for the 30/40 example).
    /// Unknown areas price at the default terrain — test harnesses
    /// pass empty `all_areas`.
    pub(crate) fn hop_stamina_cost(&self, from: Area, to: Area, all_areas: &[AreaDetails]) -> u32 {
        let standard_cost = |area: Area| {
            let terrain = all_areas
                .iter()
                .find(|ad| ad.area == Some(area))
                .map(|ad| ad.terrain.clone())
                .unwrap_or_else(areas::default_terrain);
            crate::calculate_stamina_cost(&Action::Move(None), &terrain, self)
        };
        crate::crossing_stamina_cost(standard_cost(from), standard_cost(to))
    }

    /// Moves the character ONE sub-hop this phase (one Move beat).
    ///
    /// The hop goes toward `suggested_location` — or to a random open
    /// neighbor when `None` (wander) — and pays the crossing price of
    /// the source and destination cells (see
    /// [`Character::hop_stamina_cost`]). Two Move beats in one phase
    /// are the two sub-hops cap; each beat calls this once.
    ///
    /// Cells inside closed areas are never entered: the path avoids
    /// them, and a suggested goal unreachable around a closure
    /// degrades to wandering. The move fails only when no hop is
    /// possible or stamina cannot pay the hop's price; the latter
    /// emits the travel-exhausted event.
    pub(crate) fn travels(
        &mut self,
        closed_areas: &[Area],
        suggested_location: Option<Location>,
        all_areas: &[AreaDetails],
        events: &mut Vec<TaggedEvent>,
    ) -> TravelResult {
        let mut rng = SmallRng::from_rng(&mut rand::rng());
        let current = self.location;

        let character_ref = || CharacterRef {
            identifier: self.identifier.clone().into(),
            name: self.name.clone(),
        };
        let area_ref = |a: Area| {
            let s = a.to_string();
            AreaRef {
                identifier: s.clone().into(),
                name: s,
            }
        };

        // 1. Already standing on the suggested cell?
        if let Some(goal) = suggested_location
            && goal == current
        {
            let area_name = goal.area.to_string();
            let line =
                GameOutput::CharacterTravelAlreadyThere(self.name.as_str(), area_name.as_str())
                    .to_string();
            events.push(TaggedEvent::new(
                line,
                MessagePayload::CharacterHidden {
                    character: character_ref(),
                    area: area_ref(goal.area),
                },
            ));
            return TravelResult::Failure;
        }

        // 2. Normalize the goal: unreachable-around-closures degrades
        // to wandering (absent suggestion).
        let is_blocked = |cell: Location| closed_areas.contains(&cell.area);
        let goal = match suggested_location {
            Some(goal)
                if !is_blocked(goal)
                    && shortest_path_while(current, goal, is_blocked).is_some() =>
            {
                Some(goal)
            }
            _ => None,
        };

        // 3. The beat's single sub-hop, priced by its crossing.
        let Some(hop) = self.next_sub_hop(goal, closed_areas, &mut rng) else {
            let current_area_name = current.area.to_string();
            let line = GameOutput::CharacterTravelNoOptions(
                self.name.as_str(),
                current_area_name.as_str(),
            )
            .to_string();
            events.push(TaggedEvent::new(
                line,
                MessagePayload::CharacterHidden {
                    character: character_ref(),
                    area: area_ref(current.area),
                },
            ));
            return TravelResult::Failure;
        };

        let cost = self.hop_stamina_cost(current.area, hop.area, all_areas);
        if self.stamina < cost {
            let current_area_name = current.area.to_string();
            let line = GameOutput::CharacterTravelExhausted(
                self.name.as_str(),
                current_area_name.as_str(),
            )
            .to_string();
            events.push(TaggedEvent::new(
                line,
                MessagePayload::CharacterRested {
                    character: character_ref(),
                    hp_restored: 0,
                },
            ));
            return TravelResult::Failure;
        }
        self.location = hop;
        self.stamina = self.stamina.saturating_sub(cost);
        self.emit_sub_hop(current, hop, events);

        TravelResult::Success(self.location)
    }

    /// The next sub-hop toward `goal`, or a random open neighbor when
    /// wandering (`goal` is `None`). `None` when no hop is possible.
    fn next_sub_hop(
        &self,
        goal: Option<Location>,
        closed_areas: &[Area],
        rng: &mut SmallRng,
    ) -> Option<Location> {
        if let Some(goal) = goal {
            return shortest_path_while(self.location, goal, |cell| {
                closed_areas.contains(&cell.area)
            })
            .and_then(|path| path.get(1).copied());
        }
        let open: Vec<Location> = sub_neighbors(self.location)
            .into_iter()
            .flatten()
            .filter(|neighbor| !closed_areas.contains(&neighbor.area))
            .collect();
        open.choose(rng).copied()
    }

    /// Emit one committed sub-hop: prose line (area-to-area only when
    /// the hop crosses a big-hex boundary) plus the additive
    /// `CharacterMoved` wire message carrying both sub-tile slots.
    fn emit_sub_hop(&self, from: Location, to: Location, events: &mut Vec<TaggedEvent>) {
        let line = if from.area != to.area {
            GameOutput::CharacterTravel(
                self.name.as_str(),
                from.area.to_string().as_str(),
                to.area.to_string().as_str(),
            )
            .to_string()
        } else {
            GameOutput::CharacterSubHop(self.name.as_str(), to.area.to_string().as_str())
                .to_string()
        };
        events.push(TaggedEvent::new(
            line,
            MessagePayload::CharacterMoved {
                character: CharacterRef {
                    identifier: self.identifier.clone().into(),
                    name: self.name.clone(),
                },
                from: AreaRef {
                    identifier: from.area.to_string().into(),
                    name: from.area.to_string(),
                },
                to: AreaRef {
                    identifier: to.area.to_string().into(),
                    name: to.area.to_string(),
                },
                from_sub: Some(from.sub.index()),
                to_sub: Some(to.sub.index()),
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Character;
    use areas::Area::*;
    use areas::hex::SubTriangle;
    use areas::location::shortest_path;
    use rstest::*;

    #[fixture]
    fn character() -> Character {
        let mut c = Character::new("Snaggletooth".to_string(), None, None);
        c.stamina = 100;
        c
    }

    fn at(area: Area, index: u8) -> Location {
        Location::new(area, SubTriangle::new(index))
    }

    fn hop_price(c: &Character) -> u32 {
        let standard =
            crate::calculate_stamina_cost(&Action::Move(None), &areas::default_terrain(), c);
        crate::crossing_stamina_cost(standard, standard)
    }

    fn moved_events(events: &[TaggedEvent]) -> Vec<&MessagePayload> {
        events
            .iter()
            .map(|e| &e.payload)
            .filter(|m| matches!(m, MessagePayload::CharacterMoved { .. }))
            .collect()
    }

    #[rstest]
    #[tokio::test]
    async fn travels_wander_takes_one_sub_hop_per_beat(character: Character) {
        let mut character = character;
        let mut events = Vec::new();
        let result = character.travels(&[], None, &[], &mut events);
        assert!(
            matches!(result, TravelResult::Success(_)),
            "wander must succeed: {result:?}"
        );
        // One Move beat = one wander hop; the random walk may legally
        // end where it started, so count the committed hops.
        assert_eq!(moved_events(&events).len(), 1, "one hop per beat");
        assert_eq!(
            character.stamina,
            100 - hop_price(&character),
            "the hop paid its crossing price; the remainder stays"
        );

        // Second Move beat: one more hop, one more price.
        let result = character.travels(&[], None, &[], &mut events);
        assert!(
            matches!(result, TravelResult::Success(_)),
            "second wander must succeed: {result:?}"
        );
        assert_eq!(moved_events(&events).len(), 2, "the second beat hops too");
        assert_eq!(
            character.stamina,
            100 - 2 * hop_price(&character),
            "each beat paid its crossing price; the remainder stays"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn move_beats_take_one_sub_hop_each_paying_its_crossing(character: Character) {
        let mut character = character;
        let mut events = Vec::new();
        // Hub triangle 0 -> Sector 1 triangle 0: the door into
        // Sector 1 triangle 3, then around the ring — 4 hops total;
        // each Move beat commits exactly one of them.
        let result = character.travels(&[], Some(Location::center(Sector1)), &[], &mut events);
        assert_eq!(result, TravelResult::Success(at(Sector1, 3)));
        assert_eq!(
            character.location,
            at(Sector1, 3),
            "beat 1 hops through the door"
        );
        assert_eq!(
            character.stamina,
            100 - hop_price(&character),
            "first crossing price paid, remainder kept"
        );

        // Second Move beat toward the same goal: one more hop.
        let result = character.travels(&[], Some(Location::center(Sector1)), &[], &mut events);
        assert_eq!(result, TravelResult::Success(at(Sector1, 2)));
        assert_eq!(
            character.location,
            at(Sector1, 2),
            "beat 2 walks one step along the ring"
        );
        assert_eq!(
            character.stamina,
            100 - 2 * hop_price(&character),
            "both crossing prices paid, remainder kept"
        );

        let moved = moved_events(&events);
        assert_eq!(moved.len(), 2, "one CharacterMoved per beat");
        assert!(
            matches!(
                moved[0],
                MessagePayload::CharacterMoved {
                    from_sub: Some(0),
                    to_sub: Some(3),
                    ..
                }
            ),
            "hop 1 crosses the door: {:?}",
            moved[0]
        );
        assert!(
            matches!(
                moved[1],
                MessagePayload::CharacterMoved {
                    from_sub: Some(3),
                    to_sub: Some(2),
                    ..
                }
            ),
            "hop 2 walks the ring: {:?}",
            moved[1]
        );
    }

    #[rstest]
    #[tokio::test]
    async fn single_hop_pays_only_its_crossing_price(character: Character) {
        let mut character = character;
        let mut events = Vec::new();
        let goal = at(Hub, 1); // adjacent ring triangle
        let result = character.travels(&[], Some(goal), &[], &mut events);
        assert_eq!(result, TravelResult::Success(goal));
        assert_eq!(character.location, goal);
        assert_eq!(
            character.stamina,
            100 - hop_price(&character),
            "goal reached: only the beat's hop was paid"
        );
        assert_eq!(moved_events(&events).len(), 1);
    }

    /// A cross-area hop prices at half the SOURCE area's standard
    /// cost plus half the DESTINATION's, each rounded up: Clearing
    /// (20) -> Desert (40) costs 10 + 20 = 30, not the destination's
    /// full 40.
    #[rstest]
    #[tokio::test]
    async fn cross_area_hop_averages_source_and_destination_terrain(character: Character) {
        let mut character = character;
        character.blood = 1000; // full health -> desperation 1.0
        character.terrain_affinity = vec![]; // no 0.8 affinity discount
        let all_areas = vec![
            AreaDetails::new_with_terrain(
                Some("Hub".to_string()),
                Hub,
                world::terrain::TerrainType::new(world::terrain::BaseTerrain::Clearing, vec![])
                    .unwrap(),
            ),
            AreaDetails::new_with_terrain(
                Some("S1".to_string()),
                Sector1,
                world::terrain::TerrainType::new(world::terrain::BaseTerrain::Desert, vec![])
                    .unwrap(),
            ),
        ];
        // Hub triangle 0 -> Sector1 triangle 3 crosses the door.
        let result = character.travels(
            &[],
            Some(Location::center(Sector1)),
            &all_areas,
            &mut Vec::new(),
        );
        assert_eq!(result, TravelResult::Success(at(Sector1, 3)));
        assert_eq!(
            character.stamina,
            100 - (10 + 20),
            "hop paid ceil(20/2) from Hub plus ceil(40/2) into Desert"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn second_beat_fails_when_stamina_covers_only_one_hop(character: Character) {
        let mut character = character;
        // Exactly one crossing price: beat 1 empties the pool.
        character.stamina = hop_price(&character);
        let result = character.travels(&[], Some(Location::center(Sector1)), &[], &mut Vec::new());
        assert_eq!(result, TravelResult::Success(at(Sector1, 3)));
        assert_eq!(
            character.stamina, 0,
            "the one affordable hop spent everything"
        );

        // The second beat cannot pay its crossing price: the hop
        // fails with the travel-exhausted event and the location stays.
        let mut events = Vec::new();
        let result = character.travels(&[], Some(Location::center(Sector1)), &[], &mut events);
        assert_eq!(result, TravelResult::Failure);
        assert_eq!(character.location, at(Sector1, 3));
        assert_eq!(character.stamina, 0);
        let exhausted_line = events
            .iter()
            .any(|e| e.content.contains("too exhausted to move"));
        assert!(exhausted_line, "exhausted event expected: {events:?}");
    }

    #[rstest]
    #[tokio::test]
    async fn move_fails_when_exhausted(character: Character) {
        let mut character = character;
        character.stamina = 0;
        let start = character.location;
        let mut events = Vec::new();
        let result = character.travels(&[], Some(Location::center(Sector1)), &[], &mut events);
        assert_eq!(result, TravelResult::Failure);
        assert_eq!(character.location, start);
        let exhausted_line = events
            .iter()
            .any(|e| e.content.contains("too exhausted to move"));
        assert!(exhausted_line, "exhausted event expected: {events:?}");
    }

    #[rstest]
    #[tokio::test]
    async fn travels_fail_already_there(character: Character) {
        let mut character = character;
        character.location = Location::center(Sector1);
        let result = character.travels(&[], Some(Location::center(Sector1)), &[], &mut Vec::new());
        assert_eq!(result, TravelResult::Failure);
    }

    /// A suggested goal behind a closure degrades to absent; the
    /// character wanders into an open neighbor instead of failing.
    #[rstest]
    #[tokio::test]
    async fn travels_blocked_suggestion_falls_back_to_wander(character: Character) {
        let mut character = character;
        character.location = Location::center(Sector3);
        let result = character.travels(
            &[Hub, Sector1, Sector2, Sector5],
            Some(Location::center(Sector1)),
            &[],
            &mut Vec::new(),
        );
        assert_eq!(result, TravelResult::Success(character.location));
        assert!(
            !matches!(character.location.area, Hub | Sector1 | Sector2 | Sector5),
            "wandered into an open area: {:?}",
            character.location.area
        );
    }

    #[rstest]
    #[tokio::test]
    async fn travels_success_suggestion_with_closed_areas(character: Character) {
        let mut character = character;
        character.location = Location::center(Sector1);
        let goal = Location::center(Hub);
        let expected_first_hop = shortest_path(character.location, goal).unwrap()[1];
        let result = character.travels(&[Sector2, Sector4], Some(goal), &[], &mut Vec::new());
        assert_eq!(result, TravelResult::Success(expected_first_hop));
        assert_eq!(character.location, expected_first_hop);
        assert_eq!(
            character.stamina,
            100 - hop_price(&character),
            "the single hop paid its crossing price"
        );
    }

    /// Every other area closed: wander stays inside the open area
    /// and spends the beat's one hop.
    #[rstest]
    #[tokio::test]
    async fn travels_stay_in_open_area(character: Character) {
        let mut character = character;
        let closed = [Sector1, Sector2, Sector3, Sector4, Sector5, Sector6];
        let result = character.travels(&closed, None, &[], &mut Vec::new());
        assert_eq!(result, TravelResult::Success(character.location));
        assert_eq!(character.location.area, Hub);
        assert_eq!(character.stamina, 100 - hop_price(&character));
    }
}
