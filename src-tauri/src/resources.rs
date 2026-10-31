use crate::db::{self, err, Result};
use rusqlite::{params, Connection};
use serde::Deserialize;

#[derive(Deserialize)]
struct Resource {
    name: String,
    url: String,
    category: String,
}
pub fn seed(db: &Connection) -> Result<()> {
    seed_batch(db, "resources_seeded", include_str!("../resources.json"))?;
    seed_batch(db, "photography_resources_seeded", include_str!("../photography-resources.json"))
}
fn seed_batch(db: &Connection, key: &str, catalog: &str) -> Result<()> {
    if db::setting(db, key).is_some() {
        return Ok(());
    }
    let resources: Vec<Resource> =
        serde_json::from_str(catalog).map_err(err)?;
    let tx = db.unchecked_transaction().map_err(err)?;
    for resource in resources {
        tx.execute("INSERT INTO items(path,name,kind,source,added) VALUES(?,?,'link',?,?) ON CONFLICT(path) DO UPDATE SET source=excluded.source WHERE items.kind='link' AND items.source=''", params![resource.url, resource.name, format!("Resources/{}", resource.category), db::now()]).map_err(err)?;
    }
    db::set_setting(&tx, key, "1")?;
    tx.commit().map_err(err)
}
