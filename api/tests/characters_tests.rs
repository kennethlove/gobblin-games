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
/// Replaces the old `POST /api/games/{id}/characters` test: per
/// `hangrier_games-0jl`, manual character creation is intentionally not exposed.
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
        assert!(t.get("clan").is_some());
        let clan = t["clan"].as_u64().expect("clan is u64");
        assert!((1..=12).contains(&clan), "clan {} out of range", clan);
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
    assert!(get_body.get("clan").is_some());

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

/// Test that the auto-spawn roster covers every clan 1..=12.
#[tokio::test]
async fn test_auto_spawn_clan_coverage() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "multi_character_creator").await;
    let game_id = create_test_game(&server, &user).await;

    let characters = fetch_characters(&server, &user, &game_id).await;
    assert_eq!(characters.len(), 24);

    let mut clans: Vec<u64> = characters
        .iter()
        .map(|t| t["clan"].as_u64().expect("clan is u64"))
        .collect();
    clans.sort_unstable();
    let unique: std::collections::BTreeSet<_> = clans.iter().copied().collect();
    assert_eq!(
        unique,
        (1..=12u64).collect(),
        "every clan 1..=12 should be represented"
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
