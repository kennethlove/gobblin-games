//! Sub-tile travel for characters.
//!
//! A move is one *sub-hop* along the occupied-cell graph
//! (`areas::location`); an area change is a side effect of the hop
//! (the parent component of the `Location` flips when the hop crosses a
//! facing-pair door). Per the owner decision in the sub-hex spike §8
//! Q1, a phase allows at most two sub-hops: the move slot hops once,
//! and the action slot may hop again. Each hop pays its OWN
//! destination price (`calculate_stamina_cost` for that area's
//! terrain); whatever stamina the hops leave stays for later rounds.
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
    /// The stamina price of one sub-hop into `dest`: the standard move
    /// cost for that area's terrain (base 20 x terrain multiplier x
    /// affinity x desperation). WHY: destination-priced movement — each
    /// hop pays for the tile it enters, never a share of the pool (owner
    /// decision after the PR #58 20/80 example turned out too literal).
    /// Unknown areas price at the default terrain — test harnesses pass
    /// empty `all_areas`.
    pub(crate) fn hop_stamina_cost(&self, dest: Area, all_areas: &[AreaDetails]) -> u32 {
        let terrain = all_areas
            .iter()
            .find(|ad| ad.area == Some(dest))
            .map(|ad| ad.terrain.clone())
            .unwrap_or_else(areas::default_terrain);
        crate::calculate_stamina_cost(&Action::Move(None), &terrain, self)
    }

    /// Moves the character up to two sub-hops this phase.
    ///
    /// Move slot: one hop toward `suggested_location` — or a random
    /// open neighbor when `None`. Each hop pays its destination's own
    /// move cost (see [`Character::hop_stamina_cost`]). Action slot:
    /// if the goal is not yet reached and stamina covers the next
    /// hop's price, hop again toward the same goal; otherwise the
    /// hop is skipped and the stamina stays.
    ///
    /// Cells inside closed areas are never entered: the path avoids
    /// them, and a suggested goal unreachable around a closure
    /// degrades to wandering. The move fails only when no hop is
    /// possible or stamina cannot pay; the latter emits the
    /// travel-exhausted event.
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

        // 3. Move slot: first sub-hop, priced by its destination.
        let Some(hop_one) = self.next_sub_hop(goal, closed_areas, &mut rng) else {
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

        let cost_one = self.hop_stamina_cost(hop_one.area, all_areas);
        if self.stamina < cost_one {
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
        self.location = hop_one;
        self.stamina = self.stamina.saturating_sub(cost_one);
        self.emit_sub_hop(current, hop_one, events);

        // 4. Action slot: move again toward the same goal when the
        // next hop's own price is affordable; otherwise the goblin
        // keeps its remaining stamina.
        if let Some(hop_two) = self.next_sub_hop(goal, closed_areas, &mut rng) {
            let cost_two = self.hop_stamina_cost(hop_two.area, all_areas);
            if self.stamina >= cost_two {
                let from = self.location;
                self.location = hop_two;
                self.stamina = self.stamina.saturating_sub(cost_two);
                self.emit_sub_hop(from, hop_two, events);
            }
        }

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
        crate::calculate_stamina_cost(&Action::Move(None), &areas::default_terrain(), c)
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
    async fn travels_wander_takes_two_sub_hops(character: Character) {
        let mut character = character;
        let mut events = Vec::new();
        let result = character.travels(&[], None, &[], &mut events);
        assert!(
            matches!(result, TravelResult::Success(_)),
            "wander must succeed: {result:?}"
        );
        // The random walk may legally end where it started (hop 2
        // back to hop 0's cell); two committed hops are the proof.
        assert_eq!(moved_events(&events).len(), 2, "both slots hop");
        assert_eq!(
            character.stamina,
            100 - 2 * hop_price(&character),
            "each hop paid its destination price; the remainder stays"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn move_takes_two_sub_hops_pays_each_destination(character: Character) {
        let mut character = character;
        let mut events = Vec::new();
        // Hub triangle 0 -> Sector 1 triangle 0: the door into
        // Sector 1 triangle 3, then around the ring — 4 hops.
        let result = character.travels(&[], Some(Location::center(Sector1)), &[], &mut events);
        assert_eq!(result, TravelResult::Success(at(Sector1, 2)));
        assert_eq!(
            character.location,
            at(Sector1, 2),
            "two hops: through the door, one step along the ring"
        );
        assert_eq!(
            character.stamina,
            100 - 2 * hop_price(&character),
            "both destination prices paid, remainder kept"
        );

        let moved = moved_events(&events);
        assert_eq!(moved.len(), 2, "one CharacterMoved per hop");
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
    async fn single_hop_pays_only_its_destination_price(character: Character) {
        let mut character = character;
        let mut events = Vec::new();
        let goal = at(Hub, 1); // adjacent ring triangle
        let result = character.travels(&[], Some(goal), &[], &mut events);
        assert_eq!(result, TravelResult::Success(goal));
        assert_eq!(character.location, goal);
        assert_eq!(
            character.stamina,
            100 - hop_price(&character),
            "goal reached: only the move slot's hop was paid"
        );
        assert_eq!(moved_events(&events).len(), 1);
    }

    #[rstest]
    #[tokio::test]
    async fn second_hop_skipped_when_stamina_runs_out(character: Character) {
        let mut character = character;
        // Exactly one destination price: hop 1 empties the pool.
        character.stamina = hop_price(&character);
        let result = character.travels(&[], Some(Location::center(Sector1)), &[], &mut Vec::new());
        assert_eq!(result, TravelResult::Success(at(Sector1, 3)));
        assert_eq!(
            character.stamina, 0,
            "the one affordable hop spent everything"
        );
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
        let expected_second_hop = shortest_path(character.location, goal).unwrap()[2];
        let result = character.travels(&[Sector2, Sector4], Some(goal), &[], &mut Vec::new());
        assert_eq!(result, TravelResult::Success(expected_second_hop));
        assert_eq!(
            character.stamina,
            100 - 2 * hop_price(&character),
            "both hops paid their destination price"
        );
    }

    /// Every other area closed: wander stays inside the open area
    /// and still spends both slots.
    #[rstest]
    #[tokio::test]
    async fn travels_stay_in_open_area(character: Character) {
        let mut character = character;
        let closed = [Sector1, Sector2, Sector3, Sector4, Sector5, Sector6];
        let result = character.travels(&closed, None, &[], &mut Vec::new());
        assert_eq!(result, TravelResult::Success(character.location));
        assert_eq!(character.location.area, Hub);
        assert_eq!(character.stamina, 100 - 2 * hop_price(&character));
    }
}
