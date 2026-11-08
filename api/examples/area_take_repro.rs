// WHY: local repro of prod "failed to take areas" (unit value, expected
// string or map). Correlation: any game with >=1 area->items->item edge
// fails typed take; games with 0 edges pass. Bisects which record breaks
// SerdeWrapper<AreaDetails> / SerdeWrapper<Item> deserialization.
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb_types::SerdeWrapper;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ident = std::env::args()
        .nth(1)
        .expect("usage: area_take_repro <game-identifier>");
    let db: Surreal<Any> = surrealdb::engine::any::connect("ws://127.0.0.1:8000").await?;
    db.signin(surrealdb::opt::auth::Root {
        username: "root".to_string(),
        password: "root".to_string(),
    })
    .await?;
    db.use_ns("gobblin-games").use_db("games").await?;

    let full = r#"SELECT (
    SELECT *, ->items->item[*] AS items
    FROM ->areas->area
) AS areas FROM game WHERE identifier = $identifier;"#;
    let no_items =
        "SELECT (SELECT * FROM ->areas->area) AS areas FROM game WHERE identifier = $identifier;";
    let items_only = "SELECT (SELECT ->items->item[*] AS items FROM ->areas->area) AS areas FROM game WHERE identifier = $identifier;";

    for (label, q) in [
        ("full", full),
        ("no-items-projection", no_items),
        ("items-only", items_only),
    ] {
        let mut res = db.query(q).bind(("identifier", ident.clone())).await?;
        let taken: Result<Vec<Vec<SerdeWrapper<areas::AreaDetails>>>, _> = res.take("areas");
        match taken {
            Ok(v) => println!("{label}: OK ({} areas)", v.first().map_or(0, Vec::len)),
            Err(e) => println!("{label}: ERR {e}"),
        }
    }

    // Native row (what the SDK sees) — navigate it for record ids.
    use surrealdb::types::ToSql;
    use surrealdb::types::Value as V;
    let mut res = db.query(full).bind(("identifier", ident.clone())).await?;
    let row: V = res.take(0)?;
    let row = match row {
        V::Array(mut a) if !a.is_empty() => a.swap_remove(0),
        other => other,
    };
    let areas: Vec<V> = match row.get("areas") {
        V::Array(a) => a.iter().cloned().collect(),
        _ => vec![],
    };
    let as_id = |v: &V| match v {
        V::String(s) => s.to_string(),
        V::RecordId(t) => t.to_sql(),
        _ => String::new(),
    };

    // Take each area record alone as AreaDetails.
    for area in &areas {
        let id = as_id(area.get("id"));
        if id.is_empty() {
            continue;
        }
        let mut one = db.query(format!("SELECT * FROM {id};")).await?;
        match one.take::<Option<SerdeWrapper<areas::AreaDetails>>>(0) {
            Ok(_) => println!("area {id}: OK"),
            Err(e) => println!("area {id}: ERR {e}"),
        }
        // Take each of its items alone as Item.
        let items: Vec<V> = match area.get("items") {
            V::Array(a) => a.iter().cloned().collect(),
            _ => vec![],
        };
        for item in &items {
            let iid = as_id(item.get("id"));
            if iid.is_empty() {
                continue;
            }
            let mut two = db.query(format!("SELECT * FROM {iid};")).await?;
            match two.take::<Option<SerdeWrapper<world::items::Item>>>(0) {
                Ok(_) => println!("  item {iid}: OK"),
                Err(e) => {
                    println!("  item {iid}: ERR {e}");
                    let mut three = db.query(format!("SELECT * FROM {iid};")).await?;
                    let raw: V = three.take(0)?;
                    let json =
                        serde_json::to_string(&serde_json::to_value(&raw)?).unwrap_or_default();
                    println!("    raw: {json}");
                }
            }
        }
    }

    Ok(())
}
