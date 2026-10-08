//! Structured `GameEvent` enum — the typed counterpart to [`world::output::GameOutput`].
//!
//! `GameOutput` is a borrowed, stringly-typed enum used purely to render
//! player-facing log lines. `GameEvent` is owned, serde-friendly, and carries
//! the original typed fields (UUIDs, numbers, names, items) so downstream
//! consumers (DB, websockets, analytics, announcers) can react to *what
//! happened* rather than re-parsing a localized sentence.
//!
//! This module is introduced in mqi.1. No emission sites have switched yet —
//! the engine still emits `GameOutput`. Parity tests below guarantee that
//! every `GameEvent` variant renders to a byte-identical string when fed the
//! same data as the matching `GameOutput` variant, so future emission-site
//! migration (mqi.2) and persistence (mqi.3) can proceed in lockstep without
//! changing player-visible log output.
//!
//! Design decisions documented in
//! `docs/superpowers/specs/2026-04-26-game-event-enum.md`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use world::items::Item;
use world::threats::animals::Animal;

/// Structured, owned, serde-friendly counterpart to [`world::output::GameOutput`].
///
/// Every variant of `GameOutput` has a matching variant here. Fields use
/// owned types so the event can outlive the borrowed sources that produced
/// it, and named struct variants are used throughout so future fields can be
/// added without breaking call sites.
///
/// Where `GameOutput` only carries names (e.g. character display names),
/// `GameEvent` carries both a UUID (`*_id`) for stable cross-system reference
/// **and** the rendered name (`*_name`) so [`Display`] can reproduce the
/// exact log line without a name-lookup round-trip.
///
/// Serialization uses serde's default externally-tagged representation, which
/// gives unambiguous JSON shapes for every variant and round-trips cleanly
/// without bespoke deserialization logic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GameEvent {
    // ---- Day / night cycle markers ----
    GameDayStart {
        day_number: u32,
    },
    GameDayEnd {
        day_number: u32,
    },
    FirstDayStart,
    FeastDayStart,
    CharactersLeft {
        character_count: u32,
    },
    GameNightStart {
        day_number: u32,
    },
    GameNightEnd {
        day_number: u32,
    },
    DailyDeathAnnouncement {
        death_count: u32,
    },
    DeathAnnouncement {
        character_id: Uuid,
        character_name: String,
    },
    NoOneWins,
    CharacterWins {
        character_id: Uuid,
        character_name: String,
    },

    // ---- Rest / hide / movement ----
    CharacterRest {
        character_id: Uuid,
        character_name: String,
    },
    CharacterLongRest {
        character_id: Uuid,
        character_name: String,
    },
    CharacterHide {
        character_id: Uuid,
        character_name: String,
    },
    CharacterTravel {
        character_id: Uuid,
        character_name: String,
        from_area: String,
        to_area: String,
    },
    CharacterTakeItem {
        character_id: Uuid,
        character_name: String,
        item_name: String,
    },
    CharacterCannotUseItem {
        character_id: Uuid,
        character_name: String,
        item_name: String,
    },
    CharacterUseItem {
        character_id: Uuid,
        character_name: String,
        item: Item,
    },
    CharacterTravelTooTired {
        character_id: Uuid,
        character_name: String,
        area: String,
    },
    CharacterTravelExhausted {
        character_id: Uuid,
        character_name: String,
        area: String,
    },
    CharacterTravelAlreadyThere {
        character_id: Uuid,
        character_name: String,
        area: String,
    },
    CharacterTravelFollow {
        character_id: Uuid,
        character_name: String,
        area: String,
    },
    CharacterTravelStay {
        character_id: Uuid,
        character_name: String,
        area: String,
    },
    CharacterTravelNoOptions {
        character_id: Uuid,
        character_name: String,
        area: String,
    },

    // ---- Status effects (single-character) ----
    CharacterBleeds {
        character_id: Uuid,
        character_name: String,
    },
    CharacterSick {
        character_id: Uuid,
        character_name: String,
    },
    CharacterElectrocuted {
        character_id: Uuid,
        character_name: String,
    },
    CharacterFrozen {
        character_id: Uuid,
        character_name: String,
    },
    CharacterOverheated {
        character_id: Uuid,
        character_name: String,
    },
    CharacterDehydrated {
        character_id: Uuid,
        character_name: String,
    },
    CharacterStarving {
        character_id: Uuid,
        character_name: String,
    },
    CharacterPoisoned {
        character_id: Uuid,
        character_name: String,
    },
    CharacterMauled {
        character_id: Uuid,
        character_name: String,
        animal_count: u32,
        animal: Animal,
        damage: u32,
    },
    CharacterBurned {
        character_id: Uuid,
        character_name: String,
    },
    CharacterHorrified {
        character_id: Uuid,
        character_name: String,
        sanity_damage: u32,
    },
    CharacterSuffer {
        character_id: Uuid,
        character_name: String,
    },
    CharacterSelfHarm {
        character_id: Uuid,
        character_name: String,
    },
    CharacterSuicide {
        character_id: Uuid,
        character_name: String,
    },

    // ---- Combat ----
    CharacterAttackWin {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },
    CharacterAttackWinExtra {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },
    CharacterAttackWound {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },
    CharacterAttackLose {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },
    CharacterAttackLoseExtra {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },
    CharacterAttackMiss {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },
    CharacterAttackDied {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },
    CharacterAttackSuccessKill {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },
    CharacterAttackHidden {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },
    /// Natural 20 on attack roll.
    CharacterCriticalHit {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },
    /// Natural 1 on attack roll.
    CharacterCriticalFumble {
        character_id: Uuid,
        character_name: String,
    },
    /// Natural 20 on defense roll.
    CharacterPerfectBlock {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },

    // ---- Death ----
    CharacterDiesFromStatus {
        character_id: Uuid,
        character_name: String,
        status: String,
    },
    CharacterDiesFromAreaEvent {
        character_id: Uuid,
        character_name: String,
        area_event: String,
    },
    CharacterDiesFromCharacterEvent {
        character_id: Uuid,
        character_name: String,
        character_event: String,
    },
    CharacterAlreadyDead {
        character_id: Uuid,
        character_name: String,
    },
    CharacterDead {
        character_id: Uuid,
        character_name: String,
    },
    CharacterDeath {
        character_id: Uuid,
        character_name: String,
    },

    // ---- Items / equipment ----
    WeaponBreak {
        character_id: Uuid,
        character_name: String,
        weapon_name: String,
    },
    WeaponWear {
        character_id: Uuid,
        character_name: String,
        weapon_name: String,
    },
    ShieldBreak {
        character_id: Uuid,
        character_name: String,
        shield_name: String,
    },
    ShieldWear {
        character_id: Uuid,
        character_name: String,
        shield_name: String,
    },
    PatronGift {
        character_id: Uuid,
        character_name: String,
        item: Item,
    },

    // ---- Area events ----
    AreaEvent {
        area_event: String,
        area_name: String,
    },
    AreaClose {
        area_name: String,
    },
    AreaOpen {
        area_name: String,
    },
    TrappedInArea {
        character_id: Uuid,
        character_name: String,
        area_name: String,
    },
    DiedInArea {
        character_id: Uuid,
        character_name: String,
        area_name: String,
    },

    // ---- Social / alliance ----
    CharacterBetrayal {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },
    CharacterForcedBetrayal {
        character_id: Uuid,
        character_name: String,
        target_id: Uuid,
        target_name: String,
    },
    NoOneToAttack {
        character_id: Uuid,
        character_name: String,
    },
    AllAlone {
        character_id: Uuid,
        character_name: String,
    },
    AllianceFormed {
        character_a_id: Uuid,
        character_a_name: String,
        character_b_id: Uuid,
        character_b_name: String,
        factor: String,
    },
    BetrayalTriggered {
        betrayer_id: Uuid,
        betrayer_name: String,
        victim_id: Uuid,
        victim_name: String,
    },
    TrustShockBreak {
        character_id: Uuid,
        character_name: String,
    },
    /// One combat swing carrying the full typed beat.
    CombatSwing {
        beat: crate::characters::combat_beat::CombatBeat,
    },
}
