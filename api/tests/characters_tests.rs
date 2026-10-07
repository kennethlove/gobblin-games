mod common;

use axum_test::TestServer;
use common::{TestDb, TestUser, create_test_router};
use serde_json::json;

/// Helper to create an authenticated test user
async fn create_authenticated_user(
    test_db: &TestDb,
    server: &TestServer,
    username: &str,
) -> TestUser {
    let test_user = TestUser::new(username);

    let response = server
        .post("/api/users")
        .json(&json!({
            "display_name": test_user.username,
            "email": test_user.email,
            "password": test_user.password,
        }))
        .await;

    response.assert_status(axum::http::StatusCode::CREATED);

    // Verify email
    test_db.verify_email(&test_user.email).await;

    // Authenticate to get tokens
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

/// Helper to create a game for testing. Auto-spawns 24 characters server-side
/// (see `api::games::create_game` in `api/src/games.rs:226-236`).
async fn create_test_game(server: &TestServer, user: &TestUser) -> String {
    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "max_characters": 24,
            "character_pool": 24,
            "character_list": [],
        }))
        .await;

    let body = response.json::<serde_json::Value>();
    body["identifier"].as_str().unwrap().to_string()
}

/// Helper to fetch the auto-spawned character roster for a game.
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

/// Helper to grab the first auto-spawned character identifier for a game.
async fn first_character_id(server: &TestServer, user: &TestUser, game_id: &str) -> String {
    let characters = fetch_characters(server, user, game_id).await;
    characters
        .first()
        .and_then(|t| t["identifier"].as_str())
        .map(|s| s.to_string())
        .expect("expected at least one auto-spawned character")
}

/// Verify that creating a game auto-spawns the full 24-character roster.
///
/// Replaces the old `POST /api/games/{id}/characters` test: manual
/// character creation is intentionally not exposed.
/// Every game starts with 24 server-generated characters which users edit in
/// place via `PUT /api/games/{id}/characters/{id}`.
#[tokio::test]
async fn test_game_auto_spawns_characters() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "character_creator1").await;
    let game_id = create_test_game(&server, &user).await;

    let characters = fetch_characters(&server, &user, &game_id).await;
    assert_eq!(
        characters.len(),
        24,
        "auto-spawn should populate 24 characters"
    );

    for t in &characters {
        assert!(t.get("identifier").is_some());
        assert!(t.get("name").is_some());
        assert!(t.get("team").is_some());
        let team = t["team"].as_u64().expect("team is u64");
        assert!((1..=8).contains(&team), "team {} out of range", team);
    }

    test_db.cleanup().await;
}

/// Test getting a single auto-spawned character.
#[tokio::test]
async fn test_get_character() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "character_getter").await;
    let game_id = create_test_game(&server, &user).await;
    let character_id = first_character_id(&server, &user, &game_id).await;

    let get_response = server
        .get(&format!(
            "/api/games/{}/characters/{}",
            game_id, character_id
        ))
        .add_header("Authorization", user.auth_header())
        .await;

    get_response.assert_status_ok();

    let get_body = get_response.json::<serde_json::Value>();
    assert_eq!(get_body["identifier"], character_id);
    assert!(get_body.get("name").is_some());
    assert!(get_body.get("team").is_some());

    test_db.cleanup().await;
}

/// Test updating an auto-spawned character via `EditCharacter`.
#[tokio::test]
async fn test_update_character() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "character_updater").await;
    let game_id = create_test_game(&server, &user).await;
    let character_id = first_character_id(&server, &user, &game_id).await;

    let update_response = server
        .put(&format!(
            "/api/games/{}/characters/{}",
            game_id, character_id
        ))
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "identifier": character_id,
            "name": "Nib (Updated)",
            "avatar": "",
            "game_identifier": game_id,
        }))
        .await;

    update_response.assert_status_ok();

    let get_response = server
        .get(&format!(
            "/api/games/{}/characters/{}",
            game_id, character_id
        ))
        .add_header("Authorization", user.auth_header())
        .await;
    get_response.assert_status_ok();
    let get_body = get_response.json::<serde_json::Value>();
    assert_eq!(get_body["name"], "Nib (Updated)");

    test_db.cleanup().await;
}

/// Test deleting an auto-spawned character.
#[tokio::test]
async fn test_delete_character() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "character_deleter").await;
    let game_id = create_test_game(&server, &user).await;
    let character_id = first_character_id(&server, &user, &game_id).await;

    let delete_response = server
        .delete(&format!(
            "/api/games/{}/characters/{}",
            game_id, character_id
        ))
        .add_header("Authorization", user.auth_header())
        .await;

    delete_response.assert_status(axum::http::StatusCode::NO_CONTENT);

    let get_response = server
        .get(&format!(
            "/api/games/{}/characters/{}",
            game_id, character_id
        ))
        .add_header("Authorization", user.auth_header())
        .await;

    get_response.assert_status(axum::http::StatusCode::NOT_FOUND);

    let remaining = fetch_characters(&server, &user, &game_id).await;
    assert_eq!(
        remaining.len(),
        23,
        "deleting one should leave 23 characters"
    );

    test_db.cleanup().await;
}

/// Test that the auto-spawn roster covers every team 1..=8.
#[tokio::test]
async fn test_auto_spawn_team_coverage() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "multi_character_creator").await;
    let game_id = create_test_game(&server, &user).await;

    let characters = fetch_characters(&server, &user, &game_id).await;
    assert_eq!(characters.len(), 24);

    let mut teams: Vec<u64> = characters
        .iter()
        .map(|t| t["team"].as_u64().expect("team is u64"))
        .collect();
    teams.sort_unstable();
    let unique: std::collections::BTreeSet<_> = teams.iter().copied().collect();
    assert_eq!(
        unique,
        (1..=8u64).collect(),
        "every team 1..=8 should be represented"
    );

    test_db.cleanup().await;
}

/// The auto-spawn must spread the 24-character
/// roster evenly: exactly 3 characters in each team 1..=8.
#[tokio::test]
async fn test_auto_spawn_three_characters_per_team() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "three_per_team").await;
    let game_id = create_test_game(&server, &user).await;

    let characters = fetch_characters(&server, &user, &game_id).await;
    assert_eq!(
        characters.len(),
        24,
        "auto-spawn should populate 24 characters"
    );

    let mut per_team: std::collections::BTreeMap<u64, usize> = std::collections::BTreeMap::new();
    for character in &characters {
        let team = character["team"].as_u64().expect("team is u64");
        *per_team.entry(team).or_insert(0) += 1;
    }

    assert_eq!(per_team.len(), 8, "teams must all lie within 1..=8");
    for team in 1..=8u64 {
        assert_eq!(
            per_team.get(&team),
            Some(&3),
            "team {} should have exactly 3 characters",
            team
        );
    }

    test_db.cleanup().await;
}

/// A full spawn satisfies the readiness rule from `schemas/game.surql`
/// (24 characters AND 8 distinct teams) through the real query paths:
/// `fn::get_detail_game` behind `GET /api/games/{id}`,
/// `fn::get_list_games` behind `GET /api/games`, and the internal
/// `fn::get_full_game` used by quickstart and websocket loads.
#[tokio::test]
async fn test_full_spawn_marks_game_ready() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "game_ready_checker").await;
    let game_id = create_test_game(&server, &user).await;

    // fn::get_detail_game via GET /api/games/{id}.
    let detail_response = server
        .get(&format!("/api/games/{}", game_id))
        .add_header("Authorization", user.auth_header())
        .await;
    detail_response.assert_status_ok();
    let detail_body = detail_response.json::<serde_json::Value>();
    assert_eq!(
        detail_body["ready"],
        json!(true),
        "fn::get_detail_game should mark a full spawn ready"
    );

    // fn::get_list_games via GET /api/games.
    let list_response = server
        .get("/api/games")
        .add_header("Authorization", user.auth_header())
        .await;
    list_response.assert_status_ok();
    let list_body = list_response.json::<serde_json::Value>();
    let listed_game = list_body["games"]
        .as_array()
        .expect("games should be an array")
        .iter()
        .find(|game| game["identifier"] == game_id)
        .expect("created game should appear in the list");
    assert_eq!(
        listed_game["ready"],
        json!(true),
        "fn::get_list_games should mark a full spawn ready"
    );

    // fn::get_full_game via the same internal query `api::games::get_full_game` runs.
    let mut full_game = test_db
        .db
        .query("SELECT * FROM fn::get_full_game($identifier)")
        .bind(("identifier", game_id.clone()))
        .await
        .expect("fn::get_full_game should run");
    let full_game_rows: Vec<serde_json::Value> = full_game
        .take(0)
        .expect("fn::get_full_game result should deserialize");
    assert_eq!(
        full_game_rows.len(),
        1,
        "fn::get_full_game should find the game"
    );
    assert_eq!(
        full_game_rows[0]["ready"],
        json!(true),
        "fn::get_full_game should mark a full spawn ready"
    );

    test_db.cleanup().await;
}

/// Test the per-character log endpoint.
#[tokio::test]
async fn test_character_log() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "character_logger").await;
    let game_id = create_test_game(&server, &user).await;
    let character_id = first_character_id(&server, &user, &game_id).await;

    let log_response = server
        .get(&format!(
            "/api/games/{}/characters/{}/log",
            game_id, character_id
        ))
        .add_header("Authorization", user.auth_header())
        .await;

    log_response.assert_status_ok();

    let log_body = log_response.json::<serde_json::Value>();
    assert!(log_body.is_array());

    test_db.cleanup().await;
}

/// Test that a character detail response includes the items relationship.
#[tokio::test]
async fn test_character_items() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "character_item_tester").await;
    let game_id = create_test_game(&server, &user).await;
    let character_id = first_character_id(&server, &user, &game_id).await;

    let get_response = server
        .get(&format!(
            "/api/games/{}/characters/{}",
            game_id, character_id
        ))
        .add_header("Authorization", user.auth_header())
        .await;

    get_response.assert_status_ok();

    let get_body = get_response.json::<serde_json::Value>();
    assert!(get_body.get("items").is_some());

    test_db.cleanup().await;
}

/// Test that updating a character with an empty name fails validation.
#[tokio::test]
async fn test_update_character_validation() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "character_validator").await;
    let game_id = create_test_game(&server, &user).await;
    let character_id = first_character_id(&server, &user, &game_id).await;

    let response = server
        .put(&format!(
            "/api/games/{}/characters/{}",
            game_id, character_id
        ))
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "identifier": character_id,
            "name": "",
            "avatar": "",
            "game_identifier": game_id,
        }))
        .await;

    assert!(
        response.status_code() == axum::http::StatusCode::BAD_REQUEST
            || response.status_code() == axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400/422 for empty name, got {}",
        response.status_code()
    );

    test_db.cleanup().await;
}

// ── Owned character CRUD: /api/characters ──────────────────────────────

/// Create a standalone owned character via `POST /api/characters` and
/// return the created character JSON (asserts 201).
async fn create_standalone_character(
    server: &TestServer,
    user: &TestUser,
    body: serde_json::Value,
) -> serde_json::Value {
    let response = server
        .post("/api/characters")
        .add_header("Authorization", user.auth_header())
        .json(&body)
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    response.json::<serde_json::Value>()
}

/// Fetch `GET /api/characters/{id}` as the given user and assert 200.
async fn get_owned_character(
    server: &TestServer,
    user: &TestUser,
    identifier: &str,
) -> serde_json::Value {
    let response = server
        .get(&format!("/api/characters/{}", identifier))
        .add_header("Authorization", user.auth_header())
        .await;
    response.assert_status_ok();
    response.json::<serde_json::Value>()
}

/// Collect the identifiers from `GET /api/characters` for a user.
async fn list_owned_identifiers(
    server: &TestServer,
    user: &TestUser,
) -> std::collections::BTreeSet<String> {
    let response = server
        .get("/api/characters")
        .add_header("Authorization", user.auth_header())
        .await;
    response.assert_status_ok();
    let body = response.json::<serde_json::Value>();
    body["characters"]
        .as_array()
        .expect("characters should be an array")
        .iter()
        .filter_map(|c| c["identifier"].as_str().map(str::to_string))
        .collect()
}

/// POST /api/characters with no fields generates both names, and both
/// round-trip unchanged through the detail endpoint.
#[tokio::test]
async fn test_create_owned_character_generates_names_when_omitted() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);
    let user = create_authenticated_user(&test_db, &server, "owned_default_names").await;

    let created = create_standalone_character(&server, &user, json!({})).await;

    let identifier = created["identifier"].as_str().expect("identifier");
    let name = created["name"].as_str().expect("generated name");
    let clan = created["clan_name"].as_str().expect("generated clan_name");
    assert!(!name.is_empty(), "omitted name should be generated");
    assert!(!clan.is_empty(), "omitted clan_name should be generated");

    let fetched = get_owned_character(&server, &user, identifier).await;
    assert_eq!(fetched["name"], created["name"]);
    assert_eq!(fetched["clan_name"], created["clan_name"]);

    test_db.cleanup().await;
}

/// A blank clan_name is generated just like an omitted one; supplied
/// names round-trip through creation and detail. Team stays unassigned
/// until a game assigns one at join.
#[tokio::test]
async fn test_create_owned_character_round_trips_supplied_names() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);
    let user = create_authenticated_user(&test_db, &server, "owned_supplied_names").await;

    let created = create_standalone_character(
        &server,
        &user,
        json!({
            "name": "Nibbles the Lesser",
            "clan_name": "Clan of the Twilight the Darkness",
        }),
    )
    .await;
    assert_eq!(created["name"], "Nibbles the Lesser");
    assert_eq!(created["clan_name"], "Clan of the Twilight the Darkness");
    assert_eq!(
        created["team"], 0,
        "team is assigned at game join, not creation"
    );

    let identifier = created["identifier"].as_str().expect("identifier");
    let fetched = get_owned_character(&server, &user, identifier).await;
    assert_eq!(fetched["name"], "Nibbles the Lesser");
    assert_eq!(fetched["clan_name"], "Clan of the Twilight the Darkness");

    let blank_clan = create_standalone_character(
        &server,
        &user,
        json!({ "name": "Grub", "clan_name": "   " }),
    )
    .await;
    assert_eq!(blank_clan["name"], "Grub");
    assert!(
        !blank_clan["clan_name"]
            .as_str()
            .expect("clan_name")
            .is_empty(),
        "blank clan_name should be auto-generated"
    );

    test_db.cleanup().await;
}

/// GET /api/characters returns exactly the caller's own characters —
/// never another player's.
#[tokio::test]
async fn test_list_owned_characters_returns_only_mine() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);
    let alice = create_authenticated_user(&test_db, &server, "owned_alice").await;
    let bob = create_authenticated_user(&test_db, &server, "owned_bob").await;

    let a1 = create_standalone_character(&server, &alice, json!({ "name": "Alice One" })).await;
    let a2 = create_standalone_character(&server, &alice, json!({ "name": "Alice Two" })).await;
    let b1 = create_standalone_character(&server, &bob, json!({ "name": "Bob One" })).await;

    let alice_ids = list_owned_identifiers(&server, &alice).await;
    let expected_alice: std::collections::BTreeSet<String> = [a1, a2]
        .iter()
        .map(|c| c["identifier"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        alice_ids, expected_alice,
        "list should only contain Alice's characters"
    );

    let bob_ids = list_owned_identifiers(&server, &bob).await;
    let expected_bob: std::collections::BTreeSet<String> = [b1]
        .iter()
        .map(|c| c["identifier"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        bob_ids, expected_bob,
        "list should only contain Bob's characters"
    );

    test_db.cleanup().await;
}

/// Every /api/characters endpoint requires authentication (401 without
/// a token, not 403: no identity is presented at all).
#[tokio::test]
async fn test_owned_character_endpoints_require_auth() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);
    let some_id = "00000000-0000-0000-0000-000000000000";

    server
        .get("/api/characters")
        .await
        .assert_status(axum::http::StatusCode::UNAUTHORIZED);
    server
        .post("/api/characters")
        .json(&json!({}))
        .await
        .assert_status(axum::http::StatusCode::UNAUTHORIZED);
    server
        .get(&format!("/api/characters/{}", some_id))
        .await
        .assert_status(axum::http::StatusCode::UNAUTHORIZED);
    server
        .put(&format!("/api/characters/{}", some_id))
        .json(&json!({ "name": "Nope" }))
        .await
        .assert_status(axum::http::StatusCode::UNAUTHORIZED);
    server
        .delete(&format!("/api/characters/{}", some_id))
        .await
        .assert_status(axum::http::StatusCode::UNAUTHORIZED);

    test_db.cleanup().await;
}

/// The owner can edit name and clan_name; a blank clan_name regenerates,
/// omitted fields keep their values, and a blank name is rejected.
#[tokio::test]
async fn test_update_owned_character_edits_name_and_clan() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);
    let user = create_authenticated_user(&test_db, &server, "owned_editor").await;

    let created = create_standalone_character(
        &server,
        &user,
        json!({ "name": "Original", "clan_name": "Clan of the Bog" }),
    )
    .await;
    let identifier = created["identifier"].as_str().unwrap().to_string();

    let response = server
        .put(&format!("/api/characters/{}", identifier))
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "name": "Renamed Goblin",
            "clan_name": "Clan of the Moon",
        }))
        .await;
    response.assert_status_ok();
    let updated = response.json::<serde_json::Value>();
    assert_eq!(updated["name"], "Renamed Goblin");
    assert_eq!(updated["clan_name"], "Clan of the Moon");

    // Blank clan_name regenerates; omitted name keeps its value.
    let response = server
        .put(&format!("/api/characters/{}", identifier))
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "clan_name": "" }))
        .await;
    response.assert_status_ok();
    let updated = response.json::<serde_json::Value>();
    assert_eq!(
        updated["name"], "Renamed Goblin",
        "omitted name must not change"
    );
    assert!(
        !updated["clan_name"].as_str().expect("clan_name").is_empty(),
        "blank clan_name should regenerate"
    );

    // A name cannot be blanked.
    server
        .put(&format!("/api/characters/{}", identifier))
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "   " }))
        .await
        .assert_status(axum::http::StatusCode::BAD_REQUEST);

    test_db.cleanup().await;
}

/// Another player cannot update or delete a character they do not own,
/// on either the top-level or the game-scoped routes, and the character
/// survives untouched.
#[tokio::test]
async fn test_foreign_character_update_and_delete_rejected() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);
    let alice = create_authenticated_user(&test_db, &server, "owned_victim").await;
    let bob = create_authenticated_user(&test_db, &server, "owned_attacker").await;

    let created =
        create_standalone_character(&server, &alice, json!({ "name": "Untouchable" })).await;
    let identifier = created["identifier"].as_str().unwrap().to_string();

    server
        .put(&format!("/api/characters/{}", identifier))
        .add_header("Authorization", bob.auth_header())
        .json(&json!({ "name": "Stolen" }))
        .await
        .assert_status(axum::http::StatusCode::FORBIDDEN);

    server
        .delete(&format!("/api/characters/{}", identifier))
        .add_header("Authorization", bob.auth_header())
        .await
        .assert_status(axum::http::StatusCode::FORBIDDEN);

    let fetched = get_owned_character(&server, &alice, identifier.as_str()).await;
    assert_eq!(
        fetched["name"], "Untouchable",
        "foreign update must not change the character"
    );
    assert!(
        list_owned_identifiers(&server, &alice)
            .await
            .contains(&identifier),
        "foreign delete must not remove the character"
    );

    test_db.cleanup().await;
}

/// The game-scoped mutation routes answer honestly for foreign callers:
/// 403 (previously a misleading 500 for updates and a false 204 for
/// deletes, because the schema silently skips unauthorized rows).
#[tokio::test]
async fn test_foreign_game_scoped_character_mutation_rejected() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);
    let alice = create_authenticated_user(&test_db, &server, "game_owner").await;
    let bob = create_authenticated_user(&test_db, &server, "game_attacker").await;

    let game_id = create_test_game(&server, &alice).await;
    let character_id = first_character_id(&server, &alice, &game_id).await;

    server
        .put(&format!(
            "/api/games/{}/characters/{}",
            game_id, character_id
        ))
        .add_header("Authorization", bob.auth_header())
        .json(&json!({
            "identifier": character_id,
            "name": "Bob was here",
            "avatar": "",
            "game_identifier": game_id,
        }))
        .await
        .assert_status(axum::http::StatusCode::FORBIDDEN);

    server
        .delete(&format!(
            "/api/games/{}/characters/{}",
            game_id, character_id
        ))
        .add_header("Authorization", bob.auth_header())
        .await
        .assert_status(axum::http::StatusCode::FORBIDDEN);

    let remaining = fetch_characters(&server, &alice, &game_id).await;
    assert_eq!(
        remaining.len(),
        24,
        "foreign delete must not remove the character"
    );

    test_db.cleanup().await;
}

/// The owner can delete their own character: 204, gone from detail and
/// from the list.
#[tokio::test]
async fn test_delete_owned_character() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);
    let user = create_authenticated_user(&test_db, &server, "owned_deleter").await;

    let created = create_standalone_character(&server, &user, json!({})).await;
    let identifier = created["identifier"].as_str().unwrap().to_string();

    server
        .delete(&format!("/api/characters/{}", identifier))
        .add_header("Authorization", user.auth_header())
        .await
        .assert_status(axum::http::StatusCode::NO_CONTENT);

    server
        .get(&format!("/api/characters/{}", identifier))
        .add_header("Authorization", user.auth_header())
        .await
        .assert_status(axum::http::StatusCode::NOT_FOUND);

    assert!(
        list_owned_identifiers(&server, &user).await.is_empty(),
        "deleted character should leave the list"
    );

    test_db.cleanup().await;
}

/// An owned character that is in a game follows the editable/NotStarted
/// rule: editable while the game is NotStarted, rejected with 409 once
/// the game has started.
#[tokio::test]
async fn test_owned_edit_respects_started_game() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);
    let user = create_authenticated_user(&test_db, &server, "owned_in_game").await;

    let game_id = create_test_game(&server, &user).await;
    let character_id = first_character_id(&server, &user, &game_id).await;

    // NotStarted: the owner may edit.
    server
        .put(&format!("/api/characters/{}", character_id))
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Prepped" }))
        .await
        .assert_status_ok();

    // Start the game directly through the root connection.
    test_db
        .db
        .query("UPDATE game SET status = 'InProgress' WHERE identifier = $identifier")
        .bind(("identifier", game_id.clone()))
        .await
        .expect("start the game");

    // Started: the edit is rejected and nothing changes.
    server
        .put(&format!("/api/characters/{}", character_id))
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Too Late" }))
        .await
        .assert_status(axum::http::StatusCode::CONFLICT);

    let fetched = get_owned_character(&server, &user, &character_id).await;
    assert_eq!(fetched["name"], "Prepped");

    test_db.cleanup().await;
}

/// Creation validates field lengths: an over-long name or clan_name is
/// rejected with 400 and nothing is written.
#[tokio::test]
async fn test_create_owned_character_validates_lengths() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);
    let user = create_authenticated_user(&test_db, &server, "owned_validator").await;

    server
        .post("/api/characters")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "x".repeat(51) }))
        .await
        .assert_status(axum::http::StatusCode::BAD_REQUEST);

    server
        .post("/api/characters")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "clan_name": "y".repeat(101) }))
        .await
        .assert_status(axum::http::StatusCode::BAD_REQUEST);

    assert!(
        list_owned_identifiers(&server, &user).await.is_empty(),
        "rejected creations must not persist"
    );

    test_db.cleanup().await;
}
