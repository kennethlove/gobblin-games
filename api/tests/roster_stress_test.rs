//! 128-goblin roster stress through the real API/DB path: creation
//! bot-fill, ready rule, phase stepping (processing + save_game), and
//! the detail read (get_full_game + template data).
//!
//! Uses the in-memory SurrealDB harness — no external services.
//! Re-run: `cargo test -p api --test roster_stress_test -- --nocapture`.
//! Engine-only numbers: `cargo test -p game --test roster_stress_test`
//! and `just bench-stress`.

mod common;

use axum_test::TestServer;
use common::{TestDb, TestUser, create_test_router};
use serde_json::json;
use std::time::Instant;

const TEAM_COUNT: u32 = 16;
const GOBLINS_PER_TEAM: u32 = 8;
const ROSTER: u64 = (TEAM_COUNT * GOBLINS_PER_TEAM) as u64; // 128

/// Per-phase wall-clock ceiling. Generous enough to never flake on a
/// loaded machine, tight enough to catch a pathological regression
/// (e.g. accidental O(n^2) spawn or per-character query in a loop).
const PHASE_BUDGET_MS: u128 = 10_000;

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

async fn fetch_character_count(server: &TestServer, user: &TestUser, game_id: &str) -> u64 {
    // The endpoint caps limit at 100 and paginates in Rust, so page
    // through — 128 needs two requests.
    let mut total = 0;
    for offset in [0, 100] {
        let response = server
            .get(&format!(
                "/api/games/{}/characters?limit=100&offset={}",
                game_id, offset
            ))
            .add_header("Authorization", user.auth_header())
            .await;
        response.assert_status_ok();
        let body = response.json::<serde_json::Value>();
        total += body["characters"].as_array().map_or(0, |c| c.len() as u64);
    }
    total
}

#[tokio::test]
async fn roster_stress_128_end_to_end() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "roster_stress").await;

    // Creation bot-fills the full 16 x 8 = 128 roster.
    let t_fill = Instant::now();
    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "name": "Stress Arena",
            "team_count": TEAM_COUNT,
            "goblins_per_team": GOBLINS_PER_TEAM,
        }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let fill_ms = t_fill.elapsed().as_millis();
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(
        fetch_character_count(&server, &user, &game_id).await,
        ROSTER,
        "creation must bot-fill to 128"
    );

    // Read path: get_full_game + detail data at 128.
    let t_read = Instant::now();
    let detail = server
        .get(&format!("/api/games/{}", game_id))
        .add_header("Authorization", user.auth_header())
        .await;
    detail.assert_status_ok();
    let read_ms = t_read.elapsed().as_millis();
    assert_eq!(
        detail.json::<serde_json::Value>()["ready"],
        json!(true),
        "128 with 16 distinct teams satisfies the ready rule"
    );

    // Processing + persistence: phase steps at 128 (run_full_day inside,
    // then save_game UPSERTs characters/items/messages).
    let mut phase_ms = Vec::new();
    for _ in 0..2 {
        let t_step = Instant::now();
        server
            .put(&format!("/api/games/{}/next", game_id))
            .add_header("Authorization", user.auth_header())
            .await
            .assert_status_ok();
        phase_ms.push(t_step.elapsed().as_millis());
    }

    // Roster unchanged after stepping (no dupes, no drops).
    assert_eq!(
        fetch_character_count(&server, &user, &game_id).await,
        ROSTER
    );

    println!("roster_stress[128]: create+fill={fill_ms}ms read={read_ms}ms phases={phase_ms:?}");
    for (i, ms) in phase_ms.iter().enumerate() {
        assert!(
            *ms <= PHASE_BUDGET_MS,
            "phase {i} took {ms}ms, over the {PHASE_BUDGET_MS}ms budget"
        );
    }

    test_db.cleanup().await;
}
