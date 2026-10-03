//! Stamina-as-combat-resource integration tests (PR1 backend).
//! See docs/superpowers/specs/2026-05-03-stamina-combat-resource-design.md.

use game::characters::Character;
use game::games::Game;
use shared::messages::{MessagePayload, StaminaBand};

#[test]
fn per_phase_loop_emits_stamina_band_changed_when_band_crosses() {
    // Character starts at 19/100 stamina (Exhausted). After one phase of idle
    // recovery (+5) they reach 24/100 (Winded). We expect a
    // StaminaBandChanged Exhausted -> Winded message in the game log.
    let mut g = Game::default();
    let mut t = Character::new("Tester".to_string(), None, None);
    t.stamina = 19;
    t.max_stamina = 100;
    let id = t.identifier.clone();
    g.characters.push(t);

    let _ = g.run_phase(shared::messages::Phase::Day);

    let crossed = g.messages.iter().any(|m| {
        matches!(&m.payload,
            MessagePayload::StaminaBandChanged { character, from, to }
                if character.identifier == id
                    && *from == StaminaBand::Exhausted
                    && *to == StaminaBand::Winded
        )
    });
    assert!(
        crossed,
        "expected Exhausted -> Winded StaminaBandChanged event for {id}"
    );
}

#[test]
fn fresh_character_emits_no_band_change_when_recovery_keeps_band() {
    // Character at 100/100 stamina stays Fresh after a phase (capped at max).
    // Verify NO StaminaBandChanged event fires for them.
    let mut g = Game::default();
    let mut t = Character::new("Steady".to_string(), None, None);
    t.stamina = 100;
    t.max_stamina = 100;
    let id = t.identifier.clone();
    g.characters.push(t);

    let _ = g.run_phase(shared::messages::Phase::Day);

    let crossed = g.messages.iter().any(|m| {
        matches!(&m.payload, MessagePayload::StaminaBandChanged { character, .. } if character.identifier == id)
    });
    assert!(
        !crossed,
        "Fresh character should not emit a StaminaBandChanged event"
    );
}
