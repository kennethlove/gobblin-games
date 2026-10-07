mod common;

use axum::http::StatusCode;
use axum_test::TestServer;
use common::{TestDb, TestUser, create_test_router};
use serde_json::json;

/// Helper to create an authenticated test user (same flow as
/// `characters_tests.rs`).
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
        .assert_status(StatusCode::CREATED);

    test_db.verify_email(&test_user.email).await;

    let auth_response = server
        .post("/api/users/authenticate")
        .json(&json!({
            "email": test_user.email,
            "password": test_user.password,
        }))
        .await;
    auth_response.assert_status_ok();

    let body = auth_response.json::<serde_json::Value>();
    let access_token = body["access_token"].as_str().unwrap().to_string();
    let refresh_token = body["refresh_token"].as_str().unwrap().to_string();

    test_user.with_tokens(access_token, refresh_token)
}

/// Cookie header carrying the session JWT and a known CSRF token, as the
/// browser would send after login.
fn session_cookie(user: &TestUser, csrf: &str) -> String {
    format!(
        "gg_session={}; gg_csrf={}",
        user.access_token.as_ref().unwrap(),
        csrf
    )
}

/// Unauthenticated visitors are sent to the login page instead of seeing
/// the goblins they don't have.
#[tokio::test]
async fn test_my_goblins_page_requires_auth() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    server
        .get("/my-goblins")
        .await
        .assert_status_see_other()
        .assert_header("location", "/auth?tab=login");

    // No session cookie → the CSRF check bounces the create post to /auth.
    server
        .post("/my-goblins")
        .form(&json!({ "csrf_token": "guess" }))
        .await
        .assert_status_see_other()
        .assert_header("location", "/auth");

    test_db.cleanup().await;
}

/// The page renders only the caller's own characters — API-created ones
/// included — with clan, status, and avatar URL when present.
#[tokio::test]
async fn test_my_goblins_page_renders_owned_characters() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let alice = create_authenticated_user(&test_db, &server, "goblins_alice").await;
    let bob = create_authenticated_user(&test_db, &server, "goblins_bob").await;

    let alice_char = server
        .post("/api/characters")
        .add_header("Authorization", alice.auth_header())
        .json(&json!({ "name": "Alice Gob One", "clan_name": "Clan of the Bog" }))
        .await;
    alice_char.assert_status(StatusCode::CREATED);
    let alice_id = alice_char.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    server
        .post("/api/characters")
        .add_header("Authorization", alice.auth_header())
        .json(&json!({ "name": "Alice Gob Two" }))
        .await
        .assert_status(StatusCode::CREATED);
    server
        .post("/api/characters")
        .add_header("Authorization", bob.auth_header())
        .json(&json!({ "name": "Bob Gob" }))
        .await
        .assert_status(StatusCode::CREATED);

    // Give Alice's first goblin an avatar path; the page must expose it
    // through the storage backend's public URL.
    test_db
        .db
        .query("UPDATE character SET avatar = $avatar WHERE identifier = $identifier")
        .bind(("avatar", format!("avatars/{alice_id}.png")))
        .bind(("identifier", alice_id.clone()))
        .await
        .expect("avatar update");

    let csrf = api::cookies::generate_csrf_token();
    let page = server
        .get("/my-goblins")
        .add_header("Cookie", session_cookie(&alice, &csrf))
        .await;
    page.assert_status_ok();
    page.assert_text_contains("My Goblins");
    page.assert_text_contains("Alice Gob One");
    page.assert_text_contains("Clan of the Bog");
    page.assert_text_contains("Alice Gob Two");
    page.assert_text_contains("/uploads/avatars/");
    assert!(
        !page.text().contains("Bob Gob"),
        "Alice's page must not list Bob's goblin"
    );

    let page = server
        .get("/my-goblins")
        .add_header("Cookie", session_cookie(&bob, &csrf))
        .await;
    page.assert_status_ok();
    page.assert_text_contains("Bob Gob");
    assert!(
        !page.text().contains("Alice Gob One"),
        "Bob's page must not list Alice's goblins"
    );

    test_db.cleanup().await;
}

/// Create, edit, and delete round-trip through the page's own form posts:
/// each mutation redirects (POST/Redirect/GET) and the list reflects it.
#[tokio::test]
async fn test_my_goblins_page_create_edit_delete_round_trip() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "goblins_page_user").await;
    let csrf = api::cookies::generate_csrf_token();
    let cookie = session_cookie(&user, &csrf);

    // Create through the page form (blank fields exercise generation
    // only at the API level; here we supply names to assert on).
    server
        .post("/my-goblins")
        .add_header("Cookie", cookie.clone())
        .form(&json!({
            "name": "Page Goblin",
            "clan_name": "Page Clan",
            "csrf_token": csrf,
        }))
        .await
        .assert_status_see_other()
        .assert_header("location", "/my-goblins");

    server
        .get("/my-goblins")
        .add_header("Cookie", cookie.clone())
        .await
        .assert_status_ok()
        .assert_text_contains("Page Goblin");

    // Locate the created goblin through the API list.
    let list = server
        .get("/api/characters")
        .add_header("Authorization", user.auth_header())
        .await;
    list.assert_status_ok();
    let characters = list.json::<serde_json::Value>()["characters"]
        .as_array()
        .cloned()
        .unwrap();
    assert_eq!(characters.len(), 1, "page create should add one goblin");
    let identifier = characters[0]["identifier"].as_str().unwrap().to_string();

    // Edit through the page form, addressed by the name slug (the delete
    // below proves the UUID path still works).
    server
        .post("/my-goblins/page-goblin")
        .add_header("Cookie", cookie.clone())
        .form(&json!({
            "name": "Renamed Goblin",
            "clan_name": "Renamed Clan",
            "csrf_token": csrf,
        }))
        .await
        .assert_status_see_other()
        .assert_header("location", "/my-goblins");

    let page = server
        .get("/my-goblins")
        .add_header("Cookie", cookie.clone())
        .await;
    page.assert_status_ok();
    page.assert_text_contains("Renamed Goblin");
    assert!(
        !page.text().contains("Page Goblin"),
        "old name should be gone after the edit"
    );

    // Delete through the page form.
    server
        .post(&format!("/my-goblins/{identifier}/delete"))
        .add_header("Cookie", cookie.clone())
        .form(&json!({ "csrf_token": csrf }))
        .await
        .assert_status_see_other()
        .assert_header("location", "/my-goblins");

    let page = server
        .get("/my-goblins")
        .add_header("Cookie", cookie.clone())
        .await;
    page.assert_status_ok();
    page.assert_text_contains("No goblins yet");
    assert!(
        !page.text().contains("Renamed Goblin"),
        "deleted goblin should be gone from the page"
    );

    test_db.cleanup().await;
}

/// Avatar action: the page's per-goblin upload form posts to the
/// owned-character avatar route, and the page renders the new URL.
#[tokio::test]
async fn test_my_goblins_avatar_upload_action() {
    let test_db = TestDb::new().await;
    let router = create_test_router(test_db.app_state());
    let server = TestServer::new(router);

    let user = create_authenticated_user(&test_db, &server, "goblins_avatar_user").await;

    let created = server
        .post("/api/characters")
        .add_header("Authorization", user.auth_header())
        .json(&json!({ "name": "Avy Gob" }))
        .await;
    created.assert_status(StatusCode::CREATED);
    let identifier = created.json::<serde_json::Value>()["identifier"]
        .as_str()
        .unwrap()
        .to_string();

    // Minimal valid PNG: magic bytes plus padding for the 12-byte
    // minimum in `validate_image_format`.
    let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
    let form = axum_test::multipart::MultipartForm::new().add_part(
        "avatar",
        axum_test::multipart::Part::bytes(png.to_vec())
            .file_name("gob.png")
            .mime_type("image/png"),
    );
    server
        .post(&format!("/api/characters/{identifier}/avatar"))
        .add_header("Authorization", user.auth_header())
        .multipart(form)
        .await
        .assert_status_ok();

    let csrf = api::cookies::generate_csrf_token();
    let page = server
        .get("/my-goblins")
        .add_header("Cookie", session_cookie(&user, &csrf))
        .await;
    page.assert_status_ok();
    page.assert_text_contains(format!("/uploads/avatars/{identifier}.png"));

    // The test storage backend writes into `test_uploads/` (relative to
    // the test process CWD); drop the artifact so it doesn't linger.
    let _ = std::fs::remove_file(format!("test_uploads/avatars/{identifier}.png"));

    test_db.cleanup().await;
}
