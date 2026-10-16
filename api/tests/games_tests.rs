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

/// Test creating a game
#[tokio::test]
async fn test_create_game() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "game_creator1").await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "name": "Test Game",
        }))
        .await;

    response.assert_status(axum::http::StatusCode::CREATED);

    let body = response.json::<serde_json::Value>();
    assert!(body.get("identifier").is_some());
    assert_eq!(body["name"], "Test Game");
    assert_eq!(body["status"], "NotStarted");

    test_db.cleanup().await;
}

/// Test listing games with pagination
#[tokio::test]
async fn test_list_games() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "game_lister").await;

    // Create a few games
    for _ in 0..3 {
        server
            .post("/api/games")
            .add_header("Authorization", user.auth_header())
            .json(&json!({
                "max_characters": 24,
                "character_pool": 24,
                "character_list": [],
            }))
            .await
            .assert_status(axum::http::StatusCode::CREATED);
    }

    // List games
    let response = server
        .get("/api/games")
        .add_header("Authorization", user.auth_header())
        .await;

    response.assert_status_ok();

    let body = response.json::<serde_json::Value>();
    assert!(body.get("games").is_some());
    assert!(body.get("pagination").is_some());

    let games = body["games"].as_array().unwrap();
    assert!(games.len() >= 3);

    test_db.cleanup().await;
}

/// Test getting a specific game
#[tokio::test]
async fn test_get_game() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "game_getter").await;

    // Create a game
    let create_response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "max_characters": 24,
            "character_pool": 24,
            "character_list": [],
        }))
        .await;

    let create_body = create_response.json::<serde_json::Value>();
    let game_id = create_body["identifier"].as_str().unwrap();

    // Get the game
    let get_response = server
        .get(&format!("/api/games/{}", game_id))
        .add_header("Authorization", user.auth_header())
        .await;

    get_response.assert_status_ok();

    let get_body = get_response.json::<serde_json::Value>();
    assert_eq!(get_body["identifier"].as_str().unwrap(), game_id);

    test_db.cleanup().await;
}

/// Test updating a game
#[tokio::test]
async fn test_update_game() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "game_updater").await;

    // Create a game
    let create_response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "max_characters": 24,
            "character_pool": 24,
            "character_list": [],
        }))
        .await;

    let create_body = create_response.json::<serde_json::Value>();
    let game_id = create_body["identifier"].as_str().unwrap();

    // Update the game
    let update_response = server
        .put(&format!("/api/games/{}", game_id))
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "identifier": game_id,
            "name": "Renamed Game",
            "private": false,
        }))
        .await;

    update_response.assert_status_ok();

    let update_body = update_response.json::<serde_json::Value>();
    assert_eq!(update_body["name"], "Renamed Game");
    assert_eq!(update_body["private"], false);

    test_db.cleanup().await;
}

/// Test deleting a game
#[tokio::test]
async fn test_delete_game() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "game_deleter").await;

    // Create a game
    let create_response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "max_characters": 24,
            "character_pool": 24,
            "character_list": [],
        }))
        .await;

    let create_body = create_response.json::<serde_json::Value>();
    let game_id = create_body["identifier"].as_str().unwrap();

    // Delete the game
    let delete_response = server
        .delete(&format!("/api/games/{}", game_id))
        .add_header("Authorization", user.auth_header())
        .await;

    delete_response.assert_status(axum::http::StatusCode::NO_CONTENT);

    // Try to get the deleted game - should return 404
    let get_response = server
        .get(&format!("/api/games/{}", game_id))
        .add_header("Authorization", user.auth_header())
        .await;

    get_response.assert_status(axum::http::StatusCode::NOT_FOUND);

    test_db.cleanup().await;
}

/// Test game display endpoint
#[tokio::test]
async fn test_game_display() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "game_displayer").await;

    // Create a game
    let create_response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "max_characters": 24,
            "character_pool": 24,
            "character_list": [],
        }))
        .await;

    let create_body = create_response.json::<serde_json::Value>();
    let game_id = create_body["identifier"].as_str().unwrap();

    // Get the game display
    let display_response = server
        .get(&format!("/api/games/{}/display", game_id))
        .add_header("Authorization", user.auth_header())
        .await;

    display_response.assert_status_ok();

    let display_body = display_response.json::<serde_json::Value>();
    assert!(display_body.get("identifier").is_some());
    assert!(display_body.get("status").is_some());

    test_db.cleanup().await;
}

/// Test game areas endpoint
#[tokio::test]
async fn test_game_areas() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "area_viewer").await;

    // Create a game
    let create_response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "max_characters": 24,
            "character_pool": 24,
            "character_list": [],
        }))
        .await;

    let create_body = create_response.json::<serde_json::Value>();
    let game_id = create_body["identifier"].as_str().unwrap();

    // Get game areas
    let areas_response = server
        .get(&format!("/api/games/{}/areas", game_id))
        .add_header("Authorization", user.auth_header())
        .await;

    areas_response.assert_status_ok();

    let areas_body = areas_response.json::<serde_json::Value>();
    let areas = areas_body.as_array().unwrap();

    // Should have 7 areas (Hub + Sector1..6)
    assert_eq!(areas.len(), 7);

    test_db.cleanup().await;
}

/// Test publishing a game
#[tokio::test]
async fn test_publish_game() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "game_publisher").await;

    // Create a game
    let create_response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "max_characters": 24,
            "character_pool": 24,
            "character_list": [],
        }))
        .await;

    let create_body = create_response.json::<serde_json::Value>();
    let game_id = create_body["identifier"].as_str().unwrap();

    // Publish the game
    let publish_response = server
        .put(&format!("/api/games/{}/publish", game_id))
        .add_header("Authorization", user.auth_header())
        .await;

    publish_response.assert_status_ok();

    let publish_body = publish_response.json::<serde_json::Value>();
    assert_eq!(publish_body["published"], true);

    test_db.cleanup().await;
}

/// Test unpublishing a game
#[tokio::test]
async fn test_unpublish_game() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "game_unpublisher").await;

    // Create and publish a game
    let create_response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "max_characters": 24,
            "character_pool": 24,
            "character_list": [],
        }))
        .await;

    let create_body = create_response.json::<serde_json::Value>();
    let game_id = create_body["identifier"].as_str().unwrap();

    server
        .put(&format!("/api/games/{}/publish", game_id))
        .add_header("Authorization", user.auth_header())
        .await
        .assert_status_ok();

    // Unpublish the game
    let unpublish_response = server
        .put(&format!("/api/games/{}/unpublish", game_id))
        .add_header("Authorization", user.auth_header())
        .await;

    unpublish_response.assert_status_ok();

    let unpublish_body = unpublish_response.json::<serde_json::Value>();
    assert_eq!(unpublish_body["published"], false);

    test_db.cleanup().await;
}

/// Test unauthorized access to games
#[tokio::test]
async fn test_unauthorized_game_access() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    // Try to create a game without authentication
    let response = server
        .post("/api/games")
        .json(&json!({
            "max_characters": 24,
            "character_pool": 24,
            "character_list": [],
        }))
        .await;

    response.assert_status(axum::http::StatusCode::UNAUTHORIZED);

    test_db.cleanup().await;
}

/// Test that an unstarted game returns at least the current period summary (Day 0/Day).
#[tokio::test]
async fn timeline_summary_includes_current_period_even_when_empty() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "timeline_user_1").await;

    // Create a game (status = NotStarted, day = 0)
    let create_response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "max_characters": 24,
            "character_pool": 24,
            "character_list": [],
        }))
        .await;
    create_response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = create_response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    // Hit the timeline-summary endpoint
    let response = server
        .get(&format!("/api/games/{}/timeline-summary", game_id))
        .add_header("Authorization", user.auth_header())
        .await;

    response.assert_status_ok();
    let body = response.json::<serde_json::Value>();
    let summaries = body["periods"]
        .as_array()
        .expect("response.periods should be an array");

    // summarize_periods always seeds at least the current period, even with no messages.
    assert!(
        !summaries.is_empty(),
        "expected at least one PeriodSummary for the current period"
    );

    let current = &summaries[0];
    assert_eq!(current["day"], 0, "current period day should be 0");
    assert_eq!(
        current["phase"], "06",
        "current period phase should be the day-start hour"
    );
    assert_eq!(
        current["event_count"], 0,
        "current period should have zero events for an unstarted game"
    );

    test_db.cleanup().await;
}

/// Test the endpoint returns 404 for a game that does not exist.
#[tokio::test]
async fn timeline_summary_returns_404_for_missing_game() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "timeline_user_2").await;

    let bogus_id = uuid::Uuid::new_v4();
    let response = server
        .get(&format!("/api/games/{}/timeline-summary", bogus_id))
        .add_header("Authorization", user.auth_header())
        .await;

    response.assert_status(axum::http::StatusCode::NOT_FOUND);

    test_db.cleanup().await;
}

/// Create a game with shifted day/night boundaries; config must persist
/// and round-trip through the detail endpoint (gobblin-games-1mb.3).
#[tokio::test]
async fn test_create_game_with_shifted_day_night_hours() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "nightowl").await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "name": "Night Falls Early",
            "day_start_hour": 8,
            "nightfall_hour": 18,
        }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let created = response.json::<serde_json::Value>();
    let identifier = created["identifier"].as_str().unwrap().to_string();

    // Round-trip through the detail endpoint: config survives persistence.
    let detail = server
        .get(&format!("/api/games/{identifier}"))
        .add_header("Authorization", user.auth_header())
        .await;
    detail.assert_status(axum::http::StatusCode::OK);
    let body = detail.json::<serde_json::Value>();
    assert_eq!(body["config"]["day_start_hour"], 8);
    assert_eq!(body["config"]["nightfall_hour"], 18);

    test_db.cleanup().await;
}

/// Games created without hour payload default to 6/20 in stored config.
#[tokio::test]
async fn test_create_game_defaults_day_night_hours() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "dawnlover").await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Default Hours" }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let created = response.json::<serde_json::Value>();
    let identifier = created["identifier"].as_str().unwrap().to_string();

    let detail = server
        .get(&format!("/api/games/{identifier}"))
        .add_header("Authorization", user.auth_header())
        .await;
    let body = detail.json::<serde_json::Value>();
    assert_eq!(body["config"]["day_start_hour"], 6);
    assert_eq!(body["config"]["nightfall_hour"], 20);

    test_db.cleanup().await;
}

/// Invalid day/night hours (odd, out of range, or equal) are rejected 400.
#[tokio::test]
async fn test_create_game_rejects_invalid_day_night_hours() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "badhours").await;

    for payload in [
        json!({"name": "Odd", "day_start_hour": 7}),
        json!({"name": "Late", "day_start_hour": 24}),
        json!({"name": "Same", "day_start_hour": 12, "nightfall_hour": 12}),
    ] {
        let response = server
            .post("/api/games")
            .add_header("Authorization", user.auth_header())
            .json(&payload)
            .await;
        response.assert_status(axum::http::StatusCode::BAD_REQUEST);
    }

    test_db.cleanup().await;
}

/// Read the arena snapshot stored on a game row.
async fn fetch_arena_snapshot(test_db: &TestDb, game_identifier: &str) -> serde_json::Value {
    let mut result = test_db
        .db
        .query("SELECT arena FROM game WHERE identifier = $id")
        .bind(("id", game_identifier.to_string()))
        .await
        .expect("failed to read game row");
    let row: Option<serde_json::Value> = result.take(0).expect("failed to decode game row");
    row.expect("game row must exist")["arena"].clone()
}

/// Creating a game with no arena choice stores the default preset
/// snapshot on the game row and instantiates the usual seven areas.
#[tokio::test]
async fn test_create_game_without_arena_choice_uses_default_preset() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "arena_default").await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Default Arena Game" }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let created = response.json::<serde_json::Value>();
    let identifier = created["identifier"].as_str().unwrap();

    let arena = fetch_arena_snapshot(&test_db, identifier).await;
    assert_eq!(arena["id"], "classic-hex");
    assert_eq!(arena["areas"].as_array().unwrap().len(), 7);

    // Areas are instantiated from the snapshot: seven relation edges.
    let mut result = test_db
        .db
        .query("SELECT count() AS n FROM areas GROUP ALL")
        .await
        .expect("failed to count areas");
    let row: Option<serde_json::Value> = result.take(0).expect("failed to decode area count");
    assert_eq!(row.expect("area count row")["n"], 7);

    test_db.cleanup().await;
}

/// Creating a game with the default preset id explicitly chosen works
/// and stores the same snapshot.
#[tokio::test]
async fn test_create_game_with_default_preset_id_works() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "arena_explicit").await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Explicit Arena Game", "arena": "classic-hex" }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let created = response.json::<serde_json::Value>();
    let identifier = created["identifier"].as_str().unwrap();

    let arena = fetch_arena_snapshot(&test_db, identifier).await;
    assert_eq!(arena["id"], "classic-hex");

    test_db.cleanup().await;
}

/// The default preset's sub-tile atmosphere rides each area record:
/// six sub-tiles per area, in `SubTriangle` index order, with terrain
/// and feature names hydrated for display.
#[tokio::test]
async fn test_created_area_records_carry_sub_tiles() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "arena_sub-tile").await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Sub-Hex Game" }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);

    let mut result = test_db
        .db
        .query("SELECT sub_tiles FROM area")
        .await
        .expect("failed to read area records");
    let rows: Vec<serde_json::Value> = result.take(0).expect("failed to decode area rows");
    assert_eq!(rows.len(), 7, "seven area records");
    for row in &rows {
        let subs = row["sub_tiles"]
            .as_array()
            .unwrap_or_else(|| panic!("area record must carry sub-tiles: {row}"));
        assert_eq!(subs.len(), 6);
        assert!(subs.iter().all(|s| {
            s["terrain"].is_string() && !s["feature"].as_str().unwrap_or("").is_empty()
        }));
    }

    test_db.cleanup().await;
}

/// An unknown arena preset id is rejected with a 400 that names the
/// bad id and lists the available presets.
#[tokio::test]
async fn test_create_game_rejects_unknown_arena_preset() {
    let test_db = TestDb::new().await;
    let app_state = test_db.app_state();
    let router = create_test_router(app_state);
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "arena_bad").await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Bad Arena Game", "arena": "volcano" }))
        .await;
    response.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let message = response.text();
    assert!(
        message.contains("volcano"),
        "error should name the id: {message}"
    );
    assert!(
        message.contains("classic-hex"),
        "error should list available presets: {message}"
    );

    test_db.cleanup().await;
}
