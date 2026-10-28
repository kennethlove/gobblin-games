//! k5n: legacy rows written before `private` existed.
//!
//! Both hydration shapes must agree with the schema (`DEFAULT true`):
//! a row with the column *missing* and a row with `private: null`.

use crate::games::Game;

fn legacy_row() -> serde_json::Value {
    serde_json::json!({
        "identifier": "g1",
        "name": "Legacy",
        "status": "NotStarted"
    })
}

#[test]
fn missing_private_column_hydrates_as_private() {
    let row = legacy_row();
    match serde_json::from_value::<Game>(row) {
        Ok(game) => assert!(
            game.private,
            "missing private must hydrate as private=true (schema DEFAULT true)"
        ),
        Err(err) => panic!("missing private failed to hydrate: {err}"),
    }
}

#[test]
fn null_private_column_hydrates_as_private() {
    let mut row = legacy_row();
    row["private"] = serde_json::Value::Null;
    match serde_json::from_value::<Game>(row) {
        Ok(game) => assert!(
            game.private,
            "null private must hydrate as private=true (schema DEFAULT true)"
        ),
        Err(err) => panic!("null private failed to hydrate: {err}"),
    }
}
