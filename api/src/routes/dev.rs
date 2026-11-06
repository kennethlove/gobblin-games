use api::AppState;
use axum::Form;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect};
use std::collections::HashMap;

/// Dev-only: bypass email verification without Mailpit.
/// Only active when ENV=development.
/// POST /dev/verify-email with Form { email: "..." }
pub async fn dev_verify_email_handler(
    State(state): State<AppState>,
    Form(form): Form<HashMap<String, String>>,
) -> impl IntoResponse {
    let email = match form.get("email") {
        Some(e) => e.trim().to_lowercase(),
        None => {
            return crate::redirect_with_flash("/auth?tab=login", "Missing email", None);
        }
    };

    let env_mode = std::env::var("ENV").unwrap_or_else(|_| "production".to_string());
    if env_mode != "development" {
        return Redirect::to("/").into_response();
    }

    if state
        .db
        .use_ns(&state.namespace)
        .use_db(&state.database)
        .await
        .is_err()
    {
        return crate::redirect_with_flash("/auth?tab=login", "Database error", None);
    }

    match state
        .db
        .query("UPDATE user SET email_verified = true WHERE email = $email")
        .bind(("email", email.clone()))
        .await
    {
        Ok(_) => crate::redirect_with_flash(
            "/auth?tab=login",
            "Email verified! You can now sign in.",
            None,
        ),
        Err(e) => {
            tracing::error!("Dev verify failed for {}: {}", email, e);
            crate::redirect_with_flash("/auth?tab=login", "Verification failed. Try again.", None)
        }
    }
}
