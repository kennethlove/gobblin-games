//! P5 slice 3 (spike §6 item 9): witness co-presence filtering.
//!
//! The trauma/phobia/addiction observer slices are built from
//! `characters_by_area`. With the matching `WitnessingConfig` flag off
//! (default) every character in the area counts as a witness; with it on,
//! only characters sharing the subject's exact sub-tile (`Location`) do.

use areas::location::Location;
use characters::Character;
use std::borrow::Cow;

/// Filters `raw` down to co-located witnesses of `subject` when
/// `co_presence` is on; passes the slice through unchanged when off.
pub(crate) fn witnesses<'a>(
    raw: &'a [Character],
    subject: Location,
    co_presence: bool,
) -> Cow<'a, [Character]> {
    if co_presence {
        Cow::Owned(
            raw.iter()
                .filter(|t| t.location == subject)
                .cloned()
                .collect(),
        )
    } else {
        Cow::Borrowed(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use areas::Area;
    use areas::hex::SubTriangle;

    fn char_at(sub: u8) -> Character {
        let mut t = Character::new("w".into(), None, None);
        t.location = Location {
            area: Area::Hub,
            sub: SubTriangle::new(sub),
        };
        t
    }

    #[test]
    fn area_mode_keeps_every_character() {
        let raw: Vec<Character> = (0..3).map(char_at).collect();
        let subject = raw[0].location;
        assert_eq!(witnesses(&raw, subject, false).len(), 3);
    }

    #[test]
    fn co_presence_mode_keeps_only_the_same_sub_tile() {
        let mut raw: Vec<Character> = (0..3).map(char_at).collect();
        let subject = raw[0].location;
        // Two characters at sub 0, one at sub 1.
        raw[1].location = subject;
        let kept = witnesses(&raw, subject, true);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|t| t.location == subject));
    }
}
