pub mod auth;
pub mod dev;
pub mod games;

// Re-export all handler functions so main.rs can access them via `use routes::*;`
pub use auth::{
    auth_handler, check_email_handler, email_verified_handler, login_post_handler, logout_handler,
    register_post_handler, resend_verification_handler, verify_email_handler,
};
pub use dev::dev_verify_email_handler;
// My Goblins page handlers live in the library so integration tests can
// mount them; re-exported here with the other page handlers for main.rs.
pub use api::pages::{
    my_goblins_create_handler, my_goblins_delete_handler, my_goblins_edit_handler,
    my_goblins_handler,
};
pub use games::{
    account_handler, account_settings_handler, create_game_handler, create_game_post_handler,
    game_areas_handler, game_character_detail_handler, game_characters_handler,
    game_detail_handler, game_log_handler, games_list_handler, home_handler, timeline_handler,
};
