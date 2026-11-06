//! Regression test for the ADVANCE perf fix: `PUT /api/games/{id}/next`
//! must stay fast as the game ages (128-goblin worst case, 60 steps), and
//! the `message` table must stay capped near `MAX_MESSAGES` (10000) via
//! the game's `message_count` counter + rotation.
//!
//! Prints each advance so latency growth over game age stays visible in
//! `--nocapture` runs.

mod common;

use axum_test::TestServer;
use common::{TestDb, TestUser, create_test_router};
use serde_json::json;
use std::time::Instant;

const TEAM_COUNT: u32 = 16;
const GOBLINS_PER_TEAM: u32 = 8;

/// Acceptance bound from the issue: advance well under a couple seconds
/// at day 30 with the full roster. Observed ~1.1-1.2s; 2500ms leaves CI
/// headroom while still catching the unbounded-count regression (the
/// counter scan alone grew past that by day 30 before the fix).
const DAY_30_BUDGET_MS: u128 = 2500;

async fn create_authenticated_user(test_db: &TestDb, server: &TestServer) -> TestUser {
    let test_user = TestUser::new("advance_probe");
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
async fn advance_latency_and_message_cap_hold() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);
    let user = create_authenticated_user(&test_db, &server).await;

    let response = server
        .post("/api/games")
        .add_header("Authorization", user.auth_header())
        .json(&json!({
            "name": "Advance Probe",
            "team_count": TEAM_COUNT,
            "goblins_per_team": GOBLINS_PER_TEAM,
        }))
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    let game_id = response.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    let mut day_30_ms: Option<u128> = None;

    for step in 0..60 {
        // Keep the game alive across the whole span: revive dead goblins
        // and un-finish the game directly in the test DB so the loop
        // measures the advance path over a 60-day age span, not
        // game-over rules.
        test_db
            .db
            .query(
                "UPDATE character SET blood = 1000, status = 'Healthy', \
                 attributes.luck = 50, hunger = 0, thirst = 0, \
                 wounds = [], afflictions = [] , stamina = max_stamina \
                 WHERE blood = 0 OR status != 'Healthy' OR attributes.luck > 100",
            )
            .await
            .expect("revive characters");
        test_db
            .db
            .query("UPDATE game SET status = 'InProgress' WHERE status = 'Finished'")
            .await
            .expect("reset game status");

        let t = Instant::now();
        let resp = server
            .put(&format!("/api/games/{}/next", game_id))
            .add_header("Authorization", user.auth_header())
            .await;
        let ms = t.elapsed().as_millis();
        resp.assert_status_ok();
        let body = resp.json::<serde_json::Value>();
        if body.is_null() {
            println!("step {step}: {ms}ms -> game finished (null)");
            break;
        }
        let day = body["day"].as_u64().unwrap_or(0);
        let status = body["status"].as_str().unwrap_or("?").to_string();
        let chars = body["characters"].as_array().map_or(0, |c| c.len());
        if day == 30 && day_30_ms.is_none() {
            day_30_ms = Some(ms);
        }
        println!("step {step:2}: {ms:6}ms day={day} status={status} chars={chars}");
    }

    // The counter + rotation must keep the table near MAX_MESSAGES
    // instead of growing ~1000 rows/day without bound.
    let mut counter = test_db
        .db
        .query("SELECT message_count FROM type::record('game', $id)")
        .bind(("id", game_id.clone()))
        .await
        .expect("counter query");
    let counter_rows: Vec<serde_json::Value> = counter.take(0).expect("counter rows");
    let mut count = test_db
        .db
        .query("RETURN count(SELECT id FROM message WHERE string::starts_with(subject, $id))")
        .bind(("id", game_id.clone()))
        .await
        .expect("message count query");
    let counted: Option<u64> = count.take(0).expect("count");
    println!("FINAL message_count={counter_rows:?} actual_messages={counted:?}");
    assert!(
        counted.unwrap_or(0) <= 11_000,
        "rotation must cap messages near MAX_MESSAGES (10000), got {counted:?}"
    );

    // Day-30 advance must meet the issue's latency budget.
    let day_30 = day_30_ms.expect("game must reach day 30");
    assert!(
        day_30 <= DAY_30_BUDGET_MS,
        "day-30 advance took {day_30}ms, budget is {DAY_30_BUDGET_MS}ms"
    );

    test_db.cleanup().await;
}
