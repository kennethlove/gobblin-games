//! Persist gate end-to-end: only state-dramatic payloads land in the
//! message table; clock bookkeeping (phase starts/ends) is server-log-only.
//! The full stream is still available in server logs (tracing debug) and
//! the partition is unit-tested in `api::games::persist`.

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

#[tokio::test]
async fn store_only_important() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "persist_gate").await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Gate Game" }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    // Start the game, then run at least one real phase cycle.
    for _ in 0..2 {
        server
            .put(&format!("/api/games/{}/next", game_id))
            .add_header("Authorization", user.auth_header())
            .await
            .assert_status_ok();
    }

    let logs = server
        .get(&format!("/api/games/{}/log", game_id))
        .add_header("Authorization", user.auth_header())
        .await;
    logs.assert_status_ok();
    let entries = logs.json::<serde_json::Value>();
    let entries = entries.as_array().expect("log is a JSON array");
    assert!(!entries.is_empty(), "the cycle must have produced messages");

    let kinds: Vec<&str> = entries
        .iter()
        .filter_map(|e| e["payload"]["type"].as_str())
        .collect();

    // Persist tier: the day's CycleStart bookmark landed.
    assert!(
        kinds.contains(&"CycleStart"),
        "CycleStart must persist; got kinds: {kinds:?}"
    );

    // Server-log-only tier: clock bookkeeping never reaches storage.
    for dropped in ["PhaseStarted", "PhaseEnded", "CycleEnd"] {
        assert!(
            !kinds.contains(&dropped),
            "{dropped} must not be persisted; got kinds: {kinds:?}"
        );
    }

    test_db.cleanup().await;
}

#[tokio::test]
async fn day_summary_in_log() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "day_summary").await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Summary Game" }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    // Two full game days.
    for _ in 0..2 {
        server
            .put(&format!("/api/games/{}/next", game_id))
            .add_header("Authorization", user.auth_header())
            .await
            .assert_status_ok();
    }

    // Every day that produced messages has exactly one compiled digest,
    // with one entry per goblin (default roster = 24).
    let logs = server
        .get(&format!("/api/games/{}/log", game_id))
        .add_header("Authorization", user.auth_header())
        .await;
    logs.assert_status_ok();
    let entries = logs.json::<serde_json::Value>();
    let entries = entries.as_array().expect("log is a JSON array");

    let summary_days: Vec<u32> = entries
        .iter()
        .filter_map(|e| {
            if e["payload"]["type"] == "DaySummary" {
                e["payload"]["day"].as_u64().map(|d| d as u32)
            } else {
                None
            }
        })
        .collect();
    assert!(
        !summary_days.is_empty(),
        "at least one DaySummary must be stored after two full days"
    );

    for day in summary_days {
        let day_logs = server
            .get(&format!("/api/games/{}/log/{}", game_id, day))
            .add_header("Authorization", user.auth_header())
            .await;
        day_logs.assert_status_ok();
        let day_entries = day_logs.json::<serde_json::Value>();
        let day_entries = day_entries.as_array().expect("day log is an array");
        let summaries: Vec<_> = day_entries
            .iter()
            .filter(|e| e["payload"]["type"] == "DaySummary")
            .collect();
        assert_eq!(summaries.len(), 1, "exactly one DaySummary for day {day}");
        let goblins = summaries[0]["payload"]["goblins"]
            .as_array()
            .expect("goblins array");
        assert_eq!(goblins.len(), 24, "one entry per goblin on day {day}");
        // Rollup sanity: roster snapshot adds up.
        let rollup = &summaries[0]["payload"]["rollup"];
        assert_eq!(
            rollup["survivors"].as_u64().unwrap() + rollup["fallen"].as_u64().unwrap(),
            24,
            "survivors + fallen = roster on day {day}"
        );
    }

    test_db.cleanup().await;
}

/// Every stored row must be Persist-tier — the gate's read-side contract.
#[tokio::test]
async fn log_returns_important_only() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "important_only").await;
    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Important Only" }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    for _ in 0..2 {
        server
            .put(&format!("/api/games/{}/next", game_id))
            .add_header("Authorization", user.auth_header())
            .await
            .assert_status_ok();
    }

    let logs = server
        .get(&format!("/api/games/{}/log", game_id))
        .add_header("Authorization", user.auth_header())
        .await;
    logs.assert_status_ok();
    let entries = logs.json::<serde_json::Value>();
    let entries = entries.as_array().expect("log is a JSON array");
    assert!(!entries.is_empty());

    for entry in entries {
        let payload: shared::messages::MessagePayload =
            serde_json::from_value(entry["payload"].clone())
                .expect("stored payload must deserialize");
        assert_eq!(
            payload.importance(),
            shared::messages::Importance::Persist,
            "server-log-only payload reached storage: {:?}",
            payload.kind()
        );
    }

    test_db.cleanup().await;
}
