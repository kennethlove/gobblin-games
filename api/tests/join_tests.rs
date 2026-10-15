//! Roster selection and join-game flow: create a game with chosen owned
//! goblins, join before start (owner-only, one per player, 24 cap,
//! per-team capacity, fewest-team balance), bot-fill to 24 at start.

mod common;

use axum_test::TestServer;
use common::{TestDb, TestUser, create_test_router};
use serde_json::json;

async fn create_authenticated_user(
    test_db: &TestDb,
    server: &TestServer,
    username: &str,
) -> TestUser {
    let test_user = TestUser::new(username);

    server
        .post("/api/users")
        .json(&json!({
            "display_name": test_user.username,
            "email": test_user.email,
            "password": test_user.password,
        }))
        .await
        .assert_status(axum::http::StatusCode::CREATED);

    test_db.verify_email(&test_user.email).await;

    let auth_response = server
        .post("/api/users/authenticate")
        .json(&json!({
            "email": test_user.email,
            "password": test_user.password,
        }))
        .await;

    let body = auth_response.json::<serde_json::Value>();
    let access_token = body["access_token"].as_str().unwrap().to_string();
    let refresh_token = body["refresh_token"].as_str().unwrap().to_string();

    test_user.with_tokens(access_token, refresh_token)
}

/// Create an owned persistent goblin and return its identifier.
async fn create_owned_goblin(server: &TestServer, user: &TestUser, name: &str) -> String {
    let response = server
        .post("/api/characters")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": name, "clan_name": "Test Clan" }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn fetch_characters(
    server: &TestServer,
    user: &TestUser,
    game_id: &str,
) -> Vec<serde_json::Value> {
    let response = server
        .get(&format!("/api/games/{}/characters?limit=24", game_id))
        .add_header("Authorization", user.auth_header())
        .await;
    response.assert_status_ok();
    let body = response.json::<serde_json::Value>();
    body["characters"].as_array().cloned().unwrap_or_default()
}

fn team_counts(characters: &[serde_json::Value]) -> std::collections::BTreeMap<u64, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for character in characters {
        let team = character["team"].as_u64().expect("team is u64");
        *counts.entry(team).or_insert(0) += 1;
    }
    counts
}

#[tokio::test]
async fn test_create_with_chosen_roster_skips_bots() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "roster_picker").await;
    let goblin = create_owned_goblin(&server, &user, "Stinky").await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Chosen Roster", "characters": [goblin] }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    // Only the chosen goblin — no bots at creation.
    let characters = fetch_characters(&server, &user, &game_id).await;
    assert_eq!(characters.len(), 1, "chosen roster must not auto-spawn");
    // First entry takes the fewest team: slot 1.
    let teams = team_counts(&characters);
    assert_eq!(teams.keys().copied().collect::<Vec<_>>(), vec![1]);

    // Creators may field several of their own goblins at once…
    let second = create_owned_goblin(&server, &user, "Second").await;
    let third = create_owned_goblin(&server, &user, "Third").await;
    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Creator Dozen", "characters": [second, third] }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let creator_game = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();
    let creator_roster = fetch_characters(&server, &user, &creator_game).await;
    assert_eq!(creator_roster.len(), 2);
    assert_eq!(
        team_counts(&creator_roster)
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![1, 2]
    );

    // …and keep adding after creation (the one-per-player gate skips the
    // creator; one active game per character still applies).
    let fourth = create_owned_goblin(&server, &user, "Fourth").await;
    server
        .post(&format!("/api/games/{}/join", creator_game))
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "character_id": fourth }))
        .await
        .assert_status_ok();
    let creator_roster = fetch_characters(&server, &user, &creator_game).await;
    assert_eq!(creator_roster.len(), 3);
    assert_eq!(
        team_counts(&creator_roster)
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_start_bot_fills_to_24_with_three_per_team() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "bot_fill").await;
    let goblin = create_owned_goblin(&server, &user, "Stinky").await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Fill Me", "characters": [goblin] }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    server
        .put(&format!("/api/games/{}/next", game_id))
        .add_header("Authorization", user.auth_header())
        .await
        .assert_status_ok();

    let characters = fetch_characters(&server, &user, &game_id).await;
    assert_eq!(characters.len(), 24, "start must bot-fill to 24");
    let counts = team_counts(&characters);
    assert_eq!(counts.len(), 8, "all eight teams must be represented");
    for (team, count) in &counts {
        assert_eq!(*count, 3, "team {team} should hold exactly 3 goblins");
    }

    // Ready rule (24 + 8 distinct teams) now holds.
    let detail = server
        .get(&format!("/api/games/{}", game_id))
        .add_header("Authorization", user.auth_header())
        .await;
    detail.assert_status_ok();
    assert_eq!(detail.json::<serde_json::Value>()["ready"], json!(true));

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_join_before_start_and_gates() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let alice = create_authenticated_user(&test_db, &server, "join_alice").await;
    let bob = create_authenticated_user(&test_db, &server, "join_bob").await;

    let alice_goblin = create_owned_goblin(&server, &alice, "Stinky").await;
    let alice_spare = create_owned_goblin(&server, &alice, "Spare").await;
    let bob_goblin = create_owned_goblin(&server, &bob, "Billy Slick").await;
    let bob_spare = create_owned_goblin(&server, &bob, "Bob Spare").await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", alice.auth_header())
        .json(&json!({ "name": "Join Test", "characters": [alice_goblin.clone()] }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();
    // Unlisted: Bob can open Alice's private game by URL …
    server
        .get(&format!("/api/games/{}", game_id))
        .add_header("Authorization", bob.auth_header())
        .await
        .assert_status_ok();
    // … but it must not show up in his game list.
    let listing = server
        .get("/api/games")
        .add_header("Authorization", bob.auth_header())
        .await;
    listing.assert_status_ok();
    let listed = listing.json::<serde_json::Value>()["games"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !listed.iter().any(|g| g["identifier"] == game_id),
        "private games must stay out of other players' lists"
    );

    // Bob joins with his own goblin → balanced onto team 2.
    server
        .post(&format!("/api/games/{}/join", game_id))
        .add_header("Authorization", bob.auth_header())
        .json(&json!({ "character_id": bob_goblin }))
        .await
        .assert_status_ok();

    let characters = fetch_characters(&server, &alice, &game_id).await;
    assert_eq!(characters.len(), 2);
    let teams = team_counts(&characters);
    assert_eq!(teams.keys().copied().collect::<Vec<_>>(), vec![1, 2]);

    // A joined participant keeps the private game in their own list …
    let joined_listing = server
        .get("/api/games")
        .add_header("Authorization", bob.auth_header())
        .await;
    joined_listing.assert_status_ok();
    let joined_listed = joined_listing.json::<serde_json::Value>()["games"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        joined_listed.iter().any(|g| g["identifier"] == game_id),
        "a joined player must see the private game in their own list"
    );

    // … and reaches every participation surface for it.
    let bob_roster = fetch_characters(&server, &bob, &game_id).await;
    assert_eq!(bob_roster.len(), 2, "joined player reads the roster");
    server
        .get(&format!("/api/games/{}/log", game_id))
        .add_header("Authorization", bob.auth_header())
        .await
        .assert_status_ok();

    // … but it stays invisible to a player who never joined.
    let stranger = create_authenticated_user(&test_db, &server, "nosy").await;
    let stranger_listing = server
        .get("/api/games")
        .add_header("Authorization", stranger.auth_header())
        .await;
    stranger_listing.assert_status_ok();
    let stranger_listed = stranger_listing.json::<serde_json::Value>()["games"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !stranger_listed.iter().any(|g| g["identifier"] == game_id),
        "joined private games must stay out of non-participants' lists"
    );

    // One goblin per player per game.
    server
        .post(&format!("/api/games/{}/join", game_id))
        .add_header("Authorization", bob.auth_header())
        .json(&json!({ "character_id": bob_spare }))
        .await
        .assert_status(axum::http::StatusCode::CONFLICT);

    // One active game per character: Alice's spare is fine here, but her
    // in-game goblin cannot enter a second game.
    server
        .post(&format!("/api/games/{}/join", game_id))
        .add_header("Authorization", alice.auth_header())
        .json(&json!({ "character_id": alice_goblin.clone() }))
        .await
        .assert_status(axum::http::StatusCode::CONFLICT);

    // Ownership gate: Bob cannot enter Alice's goblin.
    let response = server
        .post("/api/games")
        .add_header("Authorization", alice.auth_header())
        .json(&json!({ "name": "Second Game" }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_two = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();
    server
        .post(&format!("/api/games/{}/join", game_two))
        .add_header("Authorization", bob.auth_header())
        .json(&json!({ "character_id": alice_spare }))
        .await
        .assert_status(axum::http::StatusCode::FORBIDDEN);

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_join_rejects_started_and_full_games() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let alice = create_authenticated_user(&test_db, &server, "gate_alice").await;
    let bob = create_authenticated_user(&test_db, &server, "gate_bob").await;
    let bob_goblin = create_owned_goblin(&server, &bob, "Billy Slick").await;

    // Started game: fill via bot-fallback creation, then start.
    let response = server
        .post("/api/games")
        .add_header("Authorization", alice.auth_header())
        .json(&json!({ "name": "Already Running" }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();
    server
        .put(&format!("/api/games/{}/next", game_id))
        .add_header("Authorization", alice.auth_header())
        .await
        .assert_status_ok();

    // Full roster (24 bots) rejects a join even before start elsewhere…
    server
        .post(&format!("/api/games/{}/join", game_id))
        .add_header("Authorization", bob.auth_header())
        .json(&json!({ "character_id": bob_goblin }))
        .await
        .assert_status(axum::http::StatusCode::CONFLICT);

    // …and a not-started but full game rejects on the cap.
    let response = server
        .post("/api/games")
        .add_header("Authorization", alice.auth_header())
        .json(&json!({ "name": "Full House" }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let full_game = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();
    server
        .post(&format!("/api/games/{}/join", full_game))
        .add_header("Authorization", bob.auth_header())
        .json(&json!({ "character_id": bob_goblin }))
        .await
        .assert_status(axum::http::StatusCode::CONFLICT);

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_create_with_foreign_character_fails_cleanly() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let alice = create_authenticated_user(&test_db, &server, "clean_alice").await;
    let bob = create_authenticated_user(&test_db, &server, "clean_bob").await;
    let bob_goblin = create_owned_goblin(&server, &bob, "Billy Slick").await;

    server
        .post("/api/games")
        .add_header("Authorization", alice.auth_header())
        .json(&json!({ "name": "Ghost Game", "characters": [bob_goblin] }))
        .await
        .assert_status(axum::http::StatusCode::FORBIDDEN);

    // The rejected create leaves no game row behind.
    let listing = server
        .get("/api/games")
        .add_header("Authorization", alice.auth_header())
        .await;
    listing.assert_status_ok();
    let games = listing.json::<serde_json::Value>()["games"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !games.iter().any(|g| g["name"] == "Ghost Game"),
        "failed create must not leak a game row"
    );

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_entry_resets_per_game_state_and_items() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "entry_reset").await;
    let goblin = create_owned_goblin(&server, &user, "Stinky").await;

    // Dirty the per-game state and leave a stale item behind.
    let stale_item = world::items::Item::new_random(None);
    let stale_id = stale_item.identifier.to_string();
    let item_body = serde_json::to_value(&stale_item).unwrap();
    test_db
        .db
        .query("UPSERT type::record('item', $id) CONTENT $body")
        .bind(("id", stale_id.clone()))
        .bind(("body", item_body))
        .await
        .unwrap();
    test_db
        .db
        .query("RELATE $char->owns->$item")
        .bind((
            "char",
            surrealdb_types::RecordId::new("character", goblin.clone()),
        ))
        .bind((
            "item",
            surrealdb_types::RecordId::new("item", stale_id.clone()),
        ))
        .await
        .unwrap();
    test_db
        .db
        .query(
            "UPDATE character SET hunger = 5, thirst = 7, stamina = 9, \
             max_stamina = 42, allies = [$ally], statistics.day_killed = 3, \
             statistics.game = 'old-game' WHERE identifier = $id",
        )
        .bind(("id", goblin.clone()))
        .bind(("ally", uuid::Uuid::new_v4().to_string()))
        .await
        .unwrap();
    // A stale psychotic break in the brain must not survive the join.
    test_db
        .db
        .query(
            "UPDATE character SET brain.psychotic_break = 'Berserk' \
             WHERE identifier = $id",
        )
        .bind(("id", goblin.clone()))
        .await
        .unwrap();
    let dirty = server
        .get(&format!("/api/characters/{}", goblin))
        .add_header("Authorization", user.auth_header())
        .await;
    dirty.assert_status_ok();
    assert_eq!(
        dirty.json::<serde_json::Value>()["brain"]["psychotic_break"],
        json!("Berserk"),
        "sanity check: the dirty brain state loads through the api"
    );

    // Entry: creating the game with this goblin runs the join path.
    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Fresh Start", "characters": [goblin.clone()] }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    // Per-game state is pristine; identity and progress survive.
    let detail = server
        .get(&format!("/api/characters/{}", goblin))
        .add_header("Authorization", user.auth_header())
        .await;
    detail.assert_status_ok();
    let body = detail.json::<serde_json::Value>();
    assert_eq!(body["hunger"], json!(0));
    assert_eq!(body["thirst"], json!(0));
    // Pristine stamina is trait-coherent: a curse like Asthmatic shrinks
    // the fresh pool at entry, so expect the rolled traits' value, not a
    // hardcoded 100.
    let rolled: Vec<characters::traits::Trait> =
        serde_json::from_value(body["traits"].clone()).expect("traits array");
    let fresh_max = characters::lifecycle::health::max_stamina_for_traits(&rolled);
    assert_eq!(body["stamina"], json!(fresh_max));
    assert_eq!(body["max_stamina"], json!(fresh_max));
    assert!(
        body.get("allies")
            .is_none_or(|a| a.as_array().is_some_and(|arr| arr.is_empty())),
        "allies must be empty after entry"
    );
    assert_eq!(body["statistics"]["day_killed"], serde_json::Value::Null);
    assert_eq!(body["statistics"]["game"], json!(game_id));
    assert_eq!(
        body["brain"]["psychotic_break"],
        serde_json::Value::Null,
        "entry must clear a stale psychotic break"
    );
    assert_eq!(body["team"], json!(1));
    // Fresh items per game: exactly the new starter, stale one gone.
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "entry leaves one fresh starter item");
    assert!(
        items.iter().all(|i| i["identifier"] != stale_id.as_str()),
        "stale item from the previous game must be gone"
    );

    // A dead goblin cannot enter.
    let doomed = create_owned_goblin(&server, &user, "Doomed").await;
    test_db
        .db
        .query("UPDATE character SET status = 'Dead' WHERE identifier = $id")
        .bind(("id", doomed.clone()))
        .await
        .unwrap();
    server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Doomed Game", "characters": [doomed] }))
        .await
        .assert_status(axum::http::StatusCode::CONFLICT);

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_custom_roster_size() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "roster_config").await;

    // 4 teams × 2 goblins = 8-slot roster, bot-filled at creation.
    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Small Arena", "team_count": 4, "goblins_per_team": 2 }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    let characters = fetch_characters(&server, &user, &game_id).await;
    assert_eq!(characters.len(), 8, "cap = teams × goblins per team");
    let counts = team_counts(&characters);
    assert_eq!(counts.len(), 4, "exactly 4 teams represented");
    assert!(
        counts.values().all(|&c| c == 2),
        "exactly 2 goblins per team: {counts:?}"
    );

    // Ready rule follows the config: full 8-slot roster with 4 teams.
    let detail = server
        .get(&format!("/api/games/{}", game_id))
        .add_header("Authorization", user.auth_header())
        .await;
    detail.assert_status_ok();
    assert_eq!(detail.json::<serde_json::Value>()["ready"], json!(true));

    // The cap rejects further joins.
    let bob = create_authenticated_user(&test_db, &server, "roster_config_bob").await;
    let bob_goblin = create_owned_goblin(&server, &bob, "Billy Slick").await;
    server
        .post(&format!("/api/games/{}/join", game_id))
        .add_header("Authorization", bob.auth_header())
        .json(&json!({ "character_id": bob_goblin }))
        .await
        .assert_status(axum::http::StatusCode::CONFLICT);

    // Out-of-range team counts are rejected.
    server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "One Team", "team_count": 1 }))
        .await
        .assert_status(axum::http::StatusCode::BAD_REQUEST);

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_max_goblins_per_player() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let alice = create_authenticated_user(&test_db, &server, "limit_alice").await;
    let bob = create_authenticated_user(&test_db, &server, "limit_bob").await;

    let alice_goblin = create_owned_goblin(&server, &alice, "Stinky").await;
    let b1 = create_owned_goblin(&server, &bob, "Billy Slick").await;
    let b2 = create_owned_goblin(&server, &bob, "Slick Two").await;
    let b3 = create_owned_goblin(&server, &bob, "Slick Three").await;
    let b4 = create_owned_goblin(&server, &bob, "Slick Four").await;

    // Creator allows each joining player three goblins.
    let response = server
        .post("/api/games")
        .add_header("Authorization", alice.auth_header())
        .json(&json!({
            "name": "Three Each",
            "max_goblins_per_player": 3,
            "characters": [alice_goblin],
        }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    for goblin in [&b1, &b2, &b3] {
        server
            .post(&format!("/api/games/{}/join", game_id))
            .add_header("Authorization", bob.auth_header())
            .json(&json!({ "character_id": goblin }))
            .await
            .assert_status_ok();
    }

    // Fourth goblin exceeds the allowance.
    server
        .post(&format!("/api/games/{}/join", game_id))
        .add_header("Authorization", bob.auth_header())
        .json(&json!({ "character_id": b4 }))
        .await
        .assert_status(axum::http::StatusCode::CONFLICT);

    let characters = fetch_characters(&server, &alice, &game_id).await;
    assert_eq!(characters.len(), 4, "creator's 1 + Bob's 3");

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_roster_ceiling() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "roster_ceiling").await;

    // The roster size ceilings both the per-player allowance…
    server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Too Generous", "team_count": 2, "goblins_per_team": 1, "max_goblins_per_player": 3 }))
        .await
        .assert_status(axum::http::StatusCode::BAD_REQUEST);

    // …and the creator's chosen list (a 2-slot roster takes 2, not 3).
    let g1 = create_owned_goblin(&server, &user, "One").await;
    let g2 = create_owned_goblin(&server, &user, "Two").await;
    let g3 = create_owned_goblin(&server, &user, "Three").await;
    server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Chosen Over", "team_count": 2, "goblins_per_team": 1, "characters": [g1, g2, g3] }))
        .await
        .assert_status(axum::http::StatusCode::BAD_REQUEST);

    // 10 × 5 = exactly 50 is allowed and fills to the brim.
    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Maxed", "team_count": 10, "goblins_per_team": 5 }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    // (The shared helper pages at ?limit=24 — ask for more here.)
    let listing = server
        .get(&format!("/api/games/{}/characters?limit=64", game_id))
        .add_header("Authorization", user.auth_header())
        .await;
    listing.assert_status_ok();
    let characters = listing.json::<serde_json::Value>()["characters"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(characters.len(), 50, "ceiling caps at 50 exactly");
    let counts = team_counts(&characters);
    assert_eq!(counts.len(), 10);
    assert!(counts.values().all(|&c| c == 5), "{counts:?}");

    test_db.cleanup().await;
}

/// Games are private by default: a create that doesn't opt into public
/// listing stores `private = true`, and the game stays out of other players'
/// lists until the creator publishes it.
#[tokio::test]
async fn test_create_game_is_private_by_default() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let owner = create_authenticated_user(&test_db, &server, "priv_owner").await;
    let stranger = create_authenticated_user(&test_db, &server, "priv_stranger").await;

    // No privacy field in the payload → must default to private.
    let create = server
        .post("/api/games")
        .add_header("Authorization", owner.auth_header())
        .json(&json!({ "name": "Private By Default" }))
        .await;
    create.assert_status(axum::http::StatusCode::CREATED);
    let created = create.json::<serde_json::Value>();
    let game_id = created["identifier"].as_str().unwrap().to_string();
    assert_eq!(
        created["private"], true,
        "a game created without opting into public must be private"
    );

    // … and it must not be listed for anyone but the creator.
    let stranger_listing = server
        .get("/api/games")
        .add_header("Authorization", stranger.auth_header())
        .await;
    stranger_listing.assert_status_ok();
    let listed = stranger_listing.json::<serde_json::Value>()["games"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !listed.iter().any(|g| g["identifier"] == game_id),
        "a default-private game must stay out of strangers' lists"
    );

    test_db.cleanup().await;
}

/// A persisted `game` row that omits `private` entirely hydrates to private
/// via the schema default (`DEFINE FIELD private ... DEFAULT true`). This is
/// the back-compat guarantee for pre-flag rows.
#[tokio::test]
async fn test_missing_private_flag_hydrates_to_private() {
    let test_db = TestDb::new().await;

    // Create a raw game row that omits `private` entirely. The schema's
    // `DEFINE FIELD private ... DEFAULT true` must materialize it as true.
    // (The `created_by VALUE $auth` field evaluates to null in this
    // root-scoped session, which is fine — we only assert `private`.)
    test_db
        .db
        .query(
            "CREATE game CONTENT {
                identifier: 'hydrate-test',
                name: 'Hydration',
                status: 'NotStarted',
                team_count: 8,
                goblins_per_team: 3,
                max_goblins_per_player: 1
            }",
        )
        .await
        .expect("failed to create raw game row");

    let mut result = test_db
        .db
        .query("SELECT private FROM game WHERE identifier = 'hydrate-test'")
        .await
        .expect("failed to read raw game row");
    let row: Option<serde_json::Value> = result.take(0).expect("failed to decode hydration row");
    let private = row
        .as_ref()
        .and_then(|r| r.get("private"))
        .expect("row should carry a private field");
    assert_eq!(
        private,
        &serde_json::json!(true),
        "a game row without an explicit private flag must hydrate to private"
    );

    test_db.cleanup().await;
}

/// Public means LISTABLE, not open-join: a private game is invisible until
/// published; publishing only makes it appear in listings. Entering it still
/// goes through the game's own join endpoint (the game URL IS the invite
/// link) — there is no join shortcut from the list.
#[tokio::test]
async fn test_public_game_is_listable_but_join_goes_through_the_link() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let owner = create_authenticated_user(&test_db, &server, "pub_owner").await;
    let stranger = create_authenticated_user(&test_db, &server, "pub_stranger").await;

    let owner_goblin = create_owned_goblin(&server, &owner, "Stinky").await;
    let stranger_goblin = create_owned_goblin(&server, &stranger, "Billy Slick").await;

    // Create (private by default).
    let create = server
        .post("/api/games")
        .add_header("Authorization", owner.auth_header())
        .json(&json!({ "name": "Public Game", "characters": [owner_goblin] }))
        .await;
    create.assert_status(axum::http::StatusCode::CREATED);
    let game_id = create.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    // Before publish: not listed for the stranger (private → link is secret).
    let before = server
        .get("/api/games")
        .add_header("Authorization", stranger.auth_header())
        .await;
    let before_listed = before.json::<serde_json::Value>()["games"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !before_listed.iter().any(|g| g["identifier"] == game_id),
        "an unpublished game must stay out of strangers' lists"
    );

    // Publish → public = listable.
    server
        .put(&format!("/api/games/{}/publish", game_id))
        .add_header("Authorization", owner.auth_header())
        .await
        .assert_status_ok();

    let after = server
        .get("/api/games")
        .add_header("Authorization", stranger.auth_header())
        .await;
    let after_listed = after.json::<serde_json::Value>()["games"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        after_listed.iter().any(|g| g["identifier"] == game_id),
        "a published (public) game must be listable to everyone"
    );

    // Entry still goes through the game's join endpoint — the link. There is
    // no separate list-join affordance; the listing only surfaces the game.
    server
        .post(&format!("/api/games/{}/join", game_id))
        .add_header("Authorization", stranger.auth_header())
        .json(&json!({ "character_id": stranger_goblin }))
        .await
        .assert_status_ok();

    test_db.cleanup().await;
}

/// Per-team capacity: a team already at `goblins_per_team` must not take
/// another goblin even when the total roster still has room — otherwise
/// the ready rule (full roster plus every team represented) can never
/// fire. The roster is skewed directly to the shape a racing join used
/// to produce: team 1 full, team 2 empty, total under the cap.
#[tokio::test]
async fn test_join_redirects_away_from_a_full_team() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let alice = create_authenticated_user(&test_db, &server, "team_cap_alice").await;
    let bob = create_authenticated_user(&test_db, &server, "team_cap_bob").await;
    let a1 = create_owned_goblin(&server, &alice, "Stinky").await;
    let a2 = create_owned_goblin(&server, &alice, "Second").await;
    let a3 = create_owned_goblin(&server, &alice, "Third").await;
    let b1 = create_owned_goblin(&server, &bob, "Billy Slick").await;
    let b2 = create_owned_goblin(&server, &bob, "Bob Spare").await;

    // 2 teams × 2 goblins = 4 slots.
    let response = server
        .post("/api/games")
        .add_header("Authorization", alice.auth_header())
        .json(&json!({
            "name": "Small Teams",
            "team_count": 2,
            "goblins_per_team": 2,
            "characters": [a1.clone()],
        }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    // Alice's second goblin joins and takes team 2 (fewest).
    server
        .post(&format!("/api/games/{}/join", game_id))
        .add_header("Authorization", alice.auth_header())
        .json(&json!({ "character_id": a2.clone() }))
        .await
        .assert_status_ok();

    // Pin both onto team 1: full team, empty team 2, total 2 < 4.
    for id in [&a1, &a2] {
        test_db
            .db
            .query("UPDATE character SET team = 1 WHERE identifier = $id")
            .bind(("id", id.clone()))
            .await
            .unwrap();
    }

    // Bob's join must land on the team with space, not overfill team 1.
    server
        .post(&format!("/api/games/{}/join", game_id))
        .add_header("Authorization", bob.auth_header())
        .json(&json!({ "character_id": b1.clone() }))
        .await
        .assert_status_ok();
    let characters = fetch_characters(&server, &alice, &game_id).await;
    let counts = team_counts(&characters);
    assert_eq!(
        counts.get(&1),
        Some(&2),
        "a full team must not gain members"
    );
    assert_eq!(
        counts.get(&2),
        Some(&1),
        "the join lands on the team with space"
    );

    // Alice's third goblin (creator exemption) takes the last slot, so
    // the roster is a true 2-per-team fill and the ready rule fires.
    server
        .post(&format!("/api/games/{}/join", game_id))
        .add_header("Authorization", alice.auth_header())
        .json(&json!({ "character_id": a3 }))
        .await
        .assert_status_ok();
    let characters = fetch_characters(&server, &alice, &game_id).await;
    let counts = team_counts(&characters);
    assert_eq!(counts.get(&1), Some(&2));
    assert_eq!(counts.get(&2), Some(&2));
    let detail = server
        .get(&format!("/api/games/{}", game_id))
        .add_header("Authorization", alice.auth_header())
        .await;
    detail.assert_status_ok();
    assert_eq!(
        detail.json::<serde_json::Value>()["ready"],
        json!(true),
        "an even fill fires the ready rule"
    );

    // The total cap still rejects once every slot is taken.
    server
        .post(&format!("/api/games/{}/join", game_id))
        .add_header("Authorization", bob.auth_header())
        .json(&json!({ "character_id": b2 }))
        .await
        .assert_status(axum::http::StatusCode::CONFLICT);

    test_db.cleanup().await;
}

/// The creator's per-player exemption must not bypass the per-team cap:
/// a creator may field every slot with their own goblins, but each team
/// still stops at `goblins_per_team`.
#[tokio::test]
async fn test_creator_join_exemption_is_still_capped_per_team() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let alice = create_authenticated_user(&test_db, &server, "creator_cap").await;
    let g1 = create_owned_goblin(&server, &alice, "Stinky").await;
    let g2 = create_owned_goblin(&server, &alice, "Second").await;
    let g3 = create_owned_goblin(&server, &alice, "Third").await;
    let g4 = create_owned_goblin(&server, &alice, "Fourth").await;
    let g5 = create_owned_goblin(&server, &alice, "Fifth").await;

    // 2 × 2 = 4 slots; the default one-per-player gate skips the
    // creator, so all four joins are hers to make.
    let response = server
        .post("/api/games")
        .add_header("Authorization", alice.auth_header())
        .json(&json!({
            "name": "Creator Fills",
            "team_count": 2,
            "goblins_per_team": 2,
            "characters": [g1],
        }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    for goblin in [&g2, &g3, &g4] {
        server
            .post(&format!("/api/games/{}/join", game_id))
            .add_header("Authorization", alice.auth_header())
            .json(&json!({ "character_id": goblin }))
            .await
            .assert_status_ok();
    }

    let characters = fetch_characters(&server, &alice, &game_id).await;
    assert_eq!(characters.len(), 4);
    let counts = team_counts(&characters);
    for (team, count) in &counts {
        assert_eq!(*count, 2, "team {team} must stop at goblins_per_team");
    }

    // Fifth goblin: every team full, every slot taken — the creator's
    // exemption does not extend past the roster.
    server
        .post(&format!("/api/games/{}/join", game_id))
        .add_header("Authorization", alice.auth_header())
        .json(&json!({ "character_id": g5 }))
        .await
        .assert_status(axum::http::StatusCode::CONFLICT);

    test_db.cleanup().await;
}

/// `create_character` (bot fill) trusts the caller's team index — the
/// requested team must still have space. A full request redirects to
/// the fewest team with room; once every team is full the roster is
/// full.
#[tokio::test]
async fn test_bot_fill_team_request_redirects_when_team_is_full() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let alice = create_authenticated_user(&test_db, &server, "bot_team_cap").await;
    let g1 = create_owned_goblin(&server, &alice, "Stinky").await;
    let g2 = create_owned_goblin(&server, &alice, "Second").await;

    // 2 × 2 game with both goblins in: team 1 and team 2 hold one each.
    let response = server
        .post("/api/games")
        .add_header("Authorization", alice.auth_header())
        .json(&json!({
            "name": "Bot Capacity",
            "team_count": 2,
            "goblins_per_team": 2,
            "characters": [g1.clone(), g2.clone()],
        }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    // Pin both onto team 1: full team 1, empty team 2, total 2 < 4.
    for id in [&g1, &g2] {
        test_db
            .db
            .query("UPDATE character SET team = 1 WHERE identifier = $id")
            .bind(("id", id.clone()))
            .await
            .unwrap();
    }

    // Requesting the full team 1 (index 0) must redirect to team 2.
    let bot = api::characters::create_character(None, &game_id, &test_db.db, 0)
        .await
        .expect("bot fill into a full team must redirect");
    assert_eq!(bot.team, 2, "full team must not take the bot");

    // Requesting a team that still has room keeps it.
    let bot = api::characters::create_character(None, &game_id, &test_db.db, 1)
        .await
        .expect("bot fill into a roomy team");
    assert_eq!(bot.team, 2);

    // Every team full: no room anywhere, the roster rejects the bot.
    let err = api::characters::create_character(None, &game_id, &test_db.db, 0)
        .await
        .expect_err("a full roster must reject the bot");
    assert!(matches!(err, api::AppError::GameFull(_)));

    let characters = fetch_characters(&server, &alice, &game_id).await;
    assert_eq!(characters.len(), 4, "no goblin beyond the roster cap");
    let counts = team_counts(&characters);
    assert_eq!(counts.get(&1), Some(&2), "team 1 stays exactly full");
    assert_eq!(counts.get(&2), Some(&2), "team 2 takes the redirected bots");

    test_db.cleanup().await;
}

/// The ready rule is the capacity contract's payoff: it must fire only
/// on a true even fill (full roster AND every team represented), never
/// on an overfilled spread.
#[tokio::test]
async fn test_ready_rule_requires_even_fill() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "ready_even").await;

    // 3 × 2 = 6, bot-filled at creation: 2 per team, ready.
    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Even Fill", "team_count": 3, "goblins_per_team": 2 }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    let characters = fetch_characters(&server, &user, &game_id).await;
    assert_eq!(characters.len(), 6);
    let counts = team_counts(&characters);
    assert_eq!(counts.len(), 3, "every team represented");
    assert!(counts.values().all(|&c| c == 2), "{counts:?}");
    let detail = server
        .get(&format!("/api/games/{}", game_id))
        .add_header("Authorization", user.auth_header())
        .await;
    detail.assert_status_ok();
    assert_eq!(detail.json::<serde_json::Value>()["ready"], json!(true));

    // Skew onto team 1 (the overfill shape): the roster is still full
    // at 6, but team 2 is empty — uneven, so not ready.
    let skew_ids: Vec<String> = characters
        .iter()
        .filter(|c| c["team"].as_u64() == Some(2))
        .filter_map(|c| c["identifier"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(skew_ids.len(), 2, "team 2 holds both skew donors");
    for id in skew_ids {
        test_db
            .db
            .query("UPDATE character SET team = 1 WHERE identifier = $id")
            .bind(("id", id))
            .await
            .unwrap();
    }
    let detail = server
        .get(&format!("/api/games/{}", game_id))
        .add_header("Authorization", user.auth_header())
        .await;
    detail.assert_status_ok();
    assert_eq!(
        detail.json::<serde_json::Value>()["ready"],
        json!(false),
        "a full but uneven roster must not be ready"
    );

    test_db.cleanup().await;
}

/// `fewest_team_with_space` picks the fewest team with room and reports
/// `GameFull` once every team is at `goblins_per_team`.
#[tokio::test]
async fn test_fewest_team_with_space_rejects_when_all_teams_are_full() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "with_space").await;

    // Full 2 × 2 roster, bot-filled at creation.
    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "All Full", "team_count": 2, "goblins_per_team": 2 }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let full_game = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    let err = api::characters::fewest_team_with_space(&test_db.db, &full_game, 2, 2).await;
    assert!(matches!(err, Err(api::AppError::GameFull(_))));

    // A partial roster picks the fewest team with room.
    let goblin = create_owned_goblin(&server, &user, "Stinky").await;
    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "name": "Roomy",
            "team_count": 3,
            "goblins_per_team": 3,
            "characters": [goblin],
        }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let roomy_game = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();
    let team = api::characters::fewest_team_with_space(&test_db.db, &roomy_game, 3, 3)
        .await
        .expect("roomy roster has space");
    assert_eq!(team, 2, "fewest team with room, not the occupied team 1");

    test_db.cleanup().await;
}
