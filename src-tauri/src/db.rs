use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub type Result<T> = std::result::Result<T, String>;
pub fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Item {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub source: String,
    pub modified: i64,
    pub added: i64,
    pub missing: bool,
    pub tags: Vec<i64>,
    pub projects: Vec<i64>,
    #[serde(default)]
    pub favourites: Vec<i64>,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub thumbnail: String,
    #[serde(default)]
    pub category: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Label {
    pub id: i64,
    pub name: String,
    pub count: i64,
}
#[derive(Serialize, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub undo_label: Option<String>,
    #[serde(default)]
    pub undo_count: usize,
    #[serde(default)]
    pub redo_label: Option<String>,
    #[serde(default)]
    pub redo_count: usize,
    #[serde(default)]
    pub categories: Vec<String>,
    pub items: Vec<Item>,
    pub projects: Vec<Label>,
    pub tags: Vec<Label>,
    pub sources: Vec<String>,
    pub root: String,
    pub scanning: bool,
    #[serde(default)]
    pub recent_projects: Vec<i64>,
    #[serde(default)]
    pub source_paths: Vec<(String, String)>,
}

pub fn connect(path: &Path) -> Result<Connection> {
    let db = Connection::open(path).map_err(err)?;
    db.busy_timeout(Duration::from_secs(15)).map_err(err)?;
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
        .map_err(err)?;
    Ok(db)
}
pub fn init(path: &Path) -> Result<()> {
    let db = connect(path)?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS organisation_history(id INTEGER PRIMARY KEY,label TEXT NOT NULL,patches TEXT NOT NULL); CREATE TABLE IF NOT EXISTS organisation_redo(id INTEGER PRIMARY KEY,label TEXT NOT NULL,patches TEXT NOT NULL);").map_err(err)?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS categories(name TEXT PRIMARY KEY); CREATE TABLE IF NOT EXISTS item_categories(item_id INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,name TEXT NOT NULL);").map_err(err)?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS items(id INTEGER PRIMARY KEY,path TEXT UNIQUE NOT NULL,name TEXT NOT NULL,kind TEXT NOT NULL,source TEXT NOT NULL DEFAULT '',modified INTEGER NOT NULL DEFAULT 0,added INTEGER NOT NULL,missing INTEGER NOT NULL DEFAULT 0);
      CREATE TABLE IF NOT EXISTS projects(id INTEGER PRIMARY KEY,name TEXT UNIQUE NOT NULL);
      CREATE TABLE IF NOT EXISTS tags(id INTEGER PRIMARY KEY,name TEXT UNIQUE NOT NULL);
      CREATE TABLE IF NOT EXISTS item_projects(item_id INTEGER REFERENCES items(id) ON DELETE CASCADE,label_id INTEGER REFERENCES projects(id) ON DELETE CASCADE,PRIMARY KEY(item_id,label_id));
      CREATE TABLE IF NOT EXISTS item_tags(item_id INTEGER REFERENCES items(id) ON DELETE CASCADE,label_id INTEGER REFERENCES tags(id) ON DELETE CASCADE,PRIMARY KEY(item_id,label_id));
      CREATE INDEX IF NOT EXISTS idx_items_kind ON items(kind);
      CREATE INDEX IF NOT EXISTS idx_projects_label ON item_projects(label_id);
      CREATE INDEX IF NOT EXISTS idx_tags_label ON item_tags(label_id);").map_err(err)?;
    for (table, column, definition) in [
        ("items", "thumbnail", "TEXT NOT NULL DEFAULT ''"),
        ("items", "source_url", "TEXT NOT NULL DEFAULT ''"),
        ("items", "archived", "INTEGER NOT NULL DEFAULT 0"),
        ("item_projects", "favourite", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        let exists: bool = db
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(err)?
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(err)?
            .filter_map(|r| r.ok())
            .any(|name| name == column);
        if !exists {
            db.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition}"
            ))
            .map_err(err)?;
        }
    }
    db.execute_batch("CREATE TABLE IF NOT EXISTS fingerprints(item_id INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,size INTEGER NOT NULL,modified INTEGER NOT NULL,file_hash TEXT NOT NULL,pixel_hash TEXT);
      CREATE INDEX IF NOT EXISTS idx_file_hash ON fingerprints(file_hash);
      CREATE INDEX IF NOT EXISTS idx_pixel_hash ON fingerprints(pixel_hash);
      CREATE TABLE IF NOT EXISTS duplicate_recovery(item_id INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,original_path TEXT NOT NULL,recovery_path TEXT NOT NULL,keeper_id INTEGER NOT NULL,removed_at INTEGER NOT NULL);").map_err(err)?;
    // Captures is a storage folder, rather than an organisation category.
    db.execute_batch("UPDATE item_categories SET name='' WHERE name='Captures' COLLATE NOCASE; DELETE FROM categories WHERE name='Captures' COLLATE NOCASE;").map_err(err)?;
    // Migrate the former Photography source into normal category assignments once.
    // Run inside the app so this updates its own library, including normal launches.
    if setting(&db, "photography_category_migrated").is_none() {
        let tx = db.unchecked_transaction().map_err(err)?;
        tx.execute("INSERT OR IGNORE INTO categories(name) VALUES('Photography')", []).map_err(err)?;
        tx.execute("INSERT INTO item_categories(item_id,name)
          SELECT id,'Photography' FROM items
          WHERE kind!='link' AND archived=0 AND (source='Photography' OR source LIKE 'Photography/%')
          ON CONFLICT(item_id) DO UPDATE SET name=excluded.name WHERE item_categories.name=''", []).map_err(err)?;
        set_setting(&tx, "photography_category_migrated", "1")?;
        tx.commit().map_err(err)?;
    }
    if !setting(&db, "seeded").is_some() {
        for name in ["Psychology & Wellbeing", "School", "Sports"] {
            db.execute("INSERT OR IGNORE INTO tags(name) VALUES(?)", [name])
                .map_err(err)?;
        }
        set_setting(&db, "seeded", "1")?;
    }
    Ok(())
}
pub fn setting(db: &Connection, key: &str) -> Option<String> {
    db.query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
        r.get(0)
    })
    .ok()
}
pub fn set_setting(db: &Connection, key: &str, value: &str) -> Result<()> {
    db.execute(
        "INSERT INTO settings VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )
    .map_err(err)?;
    Ok(())
}
pub fn touch_project(db: &Connection, project: i64) -> Result<()> {
    let mut recent: Vec<i64> = setting(db, "recent_projects")
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    recent.retain(|id| *id != project);
    recent.insert(0, project);
    recent.truncate(10);
    set_setting(
        db,
        "recent_projects",
        &serde_json::to_string(&recent).map_err(err)?,
    )
}
pub fn load(path: &Path, scanning: bool) -> Result<Library> {
    let db = connect(path)?;
    let mut stmt = db
        .prepare("SELECT id,path,name,kind,source,modified,added,missing,source_url,thumbnail FROM items WHERE archived=0 ORDER BY id")
        .map_err(err)?;
    let mut items = stmt
        .query_map([], |r| {
            Ok(Item {
                id: r.get(0)?,
                path: r.get(1)?,
                name: r.get(2)?,
                kind: r.get(3)?,
                source: r.get(4)?,
                modified: r.get(5)?,
                added: r.get(6)?,
                missing: r.get(7)?,
                tags: vec![],
                projects: vec![],
                favourites: vec![],
                source_url: r.get(8)?,
                thumbnail: r.get(9)?,
                category: None,
            })
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    let lookup: std::collections::HashMap<i64, usize> =
        items.iter().enumerate().map(|(n, i)| (i.id, n)).collect();
    for (table, is_tag) in [("item_tags", true), ("item_projects", false)] {
        let mut s = db
            .prepare(&format!("SELECT item_id,label_id FROM {table}"))
            .map_err(err)?;
        let rows = s
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .map_err(err)?;
        for row in rows {
            let (id, label) = row.map_err(err)?;
            if let Some(n) = lookup.get(&id) {
                if is_tag {
                    items[*n].tags.push(label)
                } else {
                    items[*n].projects.push(label)
                }
            }
        }
    }
    {
        let mut stmt = db
            .prepare("SELECT item_id,label_id FROM item_projects WHERE favourite=1")
            .map_err(err)?;
        for row in stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .map_err(err)?
        {
            let (id, project) = row.map_err(err)?;
            if let Some(n) = lookup.get(&id) {
                items[*n].favourites.push(project);
            }
        }
    }
    let overrides = {
        let mut stmt = db.prepare("SELECT item_id,name FROM item_categories").map_err(err)?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).map_err(err)?;
        rows.collect::<std::result::Result<std::collections::HashMap<_,_>,_>>().map_err(err)?
    };
    let mut categories = {
        let mut stmt = db.prepare("SELECT name FROM categories").map_err(err)?;
        let rows = stmt.query_map([], |r| r.get::<_,String>(0)).map_err(err)?;
        rows.collect::<std::result::Result<Vec<_>,_>>().map_err(err)?
    };
    for item in &mut items {
        if item.kind != "link" {
            let initial = default_category(&item.source);
            if !initial.is_empty() { categories.push(initial.clone()); }
            let mut category = overrides.get(&item.id).cloned().unwrap_or(initial);
            if category.eq_ignore_ascii_case("Captures") { category.clear(); }
            if !category.is_empty() { categories.push(category.clone()); }
            item.category = Some(category);
        }
    }
    categories.retain(|name| !name.eq_ignore_ascii_case("Captures"));
    categories.sort(); categories.dedup();
    let source_paths: Vec<(String, String)> = if setting(&db, "source_layout").is_some() {
        crate::sources::roots(&db)
            .into_iter()
            .map(|(name, path)| (name, path.to_string_lossy().into_owned()))
            .collect()
    } else {
        vec![]
    };
    let mut sources: Vec<String> = source_paths.iter().map(|(name, _)| name.clone()).collect();
    for item in items
        .iter()
        .filter(|i| i.kind != "link" && !i.source.is_empty())
    {
        let mut parent = String::new();
        for part in item.source.split('/') {
            if !parent.is_empty() {
                parent.push('/');
            }
            parent.push_str(part);
            sources.push(parent.clone());
        }
    }
    sources.sort();
    sources.dedup();
    let labels = |table: &str, join: &str| -> Result<Vec<Label>> {
        let mut s = db.prepare(&format!("SELECT t.id,t.name,COUNT(i.id) FROM {table} t LEFT JOIN {join} j ON t.id=j.label_id LEFT JOIN items i ON i.id=j.item_id AND i.archived=0 GROUP BY t.id ORDER BY t.id")).map_err(err)?;
        let rows = s
            .query_map([], |r| {
                Ok(Label {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    count: r.get(2)?,
                })
            })
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        Ok(rows)
    };
    Ok(Library {
        redo_label: db.query_row("SELECT label FROM organisation_redo ORDER BY id DESC LIMIT 1",[],|r|r.get(0)).ok(),
        redo_count: db.query_row("SELECT count(*) FROM organisation_redo",[],|r|r.get(0)).map_err(err)?,
        undo_label: db.query_row("SELECT label FROM organisation_history ORDER BY id DESC LIMIT 1",[],|r|r.get(0)).ok(),
        undo_count: db.query_row("SELECT count(*) FROM organisation_history",[],|r|r.get(0)).map_err(err)?,
        categories,
        items,
        projects: labels("projects", "item_projects")?,
        tags: labels("tags", "item_tags")?,
        sources,
        root: setting(&db, "root").unwrap_or_default(),
        scanning,
        recent_projects: setting(&db, "recent_projects")
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default(),
        source_paths,
    })
}
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn default_category(source: &str) -> String {
    if source == "Photography" || source.starts_with("Photography/") {
        return "Photography".to_owned();
    }
    let name=source.strip_prefix("Design/").unwrap_or("").split('/').next().unwrap_or("");
    if name.eq_ignore_ascii_case("Captures") {String::new()} else {name.to_owned()}
}
fn assign_category(db: &Connection, id: i64, name: &str) -> Result<()> {
    if !name.is_empty() { valid_name(name)?; }
    let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM items WHERE id=? AND kind!='link' AND archived=0)",[id],|r|r.get(0)).map_err(err)?;
    if !exists { return Err("Only references still in your library can change category.".into()); }
    if !name.is_empty() { db.execute("INSERT OR IGNORE INTO categories(name) VALUES(?)",[name]).map_err(err)?; }
    db.execute("INSERT INTO item_categories(item_id,name) VALUES(?,?) ON CONFLICT(item_id) DO UPDATE SET name=excluded.name",params![id,name]).map_err(err)?;
    Ok(())
}
fn valid_name(name: &str) -> Result<&str> {
    let s = name.trim();
    if s.is_empty() || s.len() > 300 {
        Err("Enter a name (up to 300 characters).".into())
    } else {
        Ok(s)
    }
}
pub fn valid_url(url: &str) -> Result<&str> {
    let s = url.trim();
    if !(s.starts_with("https://") || s.starts_with("http://"))
        || s.contains(char::is_whitespace)
        || !url::Url::parse(s)
            .map(|u| u.host_str().is_some_and(|h| !h.is_empty()))
            .unwrap_or(false)
    {
        Err("Enter a complete http:// or https:// URL.".into())
    } else {
        Ok(s)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub action: String,
    #[serde(default)]
    pub ids: Vec<i64>,
    pub target: Option<i64>,
    pub name: Option<String>,
    pub value: Option<String>,
}
pub fn change(path: &Path, c: Change) -> Result<()> {
    let mut db = connect(path)?;
    let tx = db.transaction().map_err(err)?;
    if c.action=="undo_organisation" {crate::history::undo(&tx)?;tx.commit().map_err(err)?;return Ok(());}
    if c.action=="redo_organisation" {crate::history::redo(&tx)?;tx.commit().map_err(err)?;return Ok(());}
    let undo=crate::history::snapshot(&tx,&c)?;
    let target = c.target.unwrap_or(0);
    let name = c.name.as_deref().unwrap_or("");
    let value = c.value.as_deref().unwrap_or("");
    match c.action.as_str() {
        "create_category" => {
            tx.execute("INSERT OR IGNORE INTO categories(name) VALUES(?)",[valid_name(name)?]).map_err(err)?;
        }
        "set_category" => {
            if c.ids.is_empty() { return Err("Select at least one reference.".into()); }
            for id in c.ids { assign_category(&tx,id,value.trim())?; }
        }
        "restore_categories" => {
            let previous: Vec<(i64,String)> = serde_json::from_str(value).map_err(err)?;
            for (id,category) in previous { assign_category(&tx,id,&category)?; }
        }
        "create_project" | "create_tag" | "rename_project" | "rename_tag" | "delete_project"
        | "delete_tag" => {
            let table = if c.action.ends_with("project") {
                "projects"
            } else {
                "tags"
            };
            if c.action.starts_with("create") {
                tx.execute(
                    &format!("INSERT INTO {table}(name) VALUES(?)"),
                    [valid_name(name)?],
                )
                .map_err(err)?;
            } else if c.action.starts_with("rename") {
                tx.execute(
                    &format!("UPDATE {table} SET name=? WHERE id=?"),
                    params![valid_name(name)?, target],
                )
                .map_err(err)?;
            } else {
                tx.execute(&format!("DELETE FROM {table} WHERE id=?"), [target])
                    .map_err(err)?;
            }
        }
        "add_project" | "remove_project" | "add_tag" | "remove_tag" => {
            if c.action == "add_project" && !c.ids.is_empty() {
                touch_project(&tx, target)?;
            }
            let table = if c.action.ends_with("project") {
                "item_projects"
            } else {
                "item_tags"
            };
            for id in c.ids {
                if c.action.starts_with("add") {
                    tx.execute(
                        &format!("INSERT OR IGNORE INTO {table}(item_id,label_id) VALUES(?,?)"),
                        params![id, target],
                    )
                    .map_err(err)?;
                } else {
                    tx.execute(
                        &format!("DELETE FROM {table} WHERE item_id=? AND label_id=?"),
                        params![id, target],
                    )
                    .map_err(err)?;
                }
            }
        }
        "create_link" => {
            tx.execute(
                "INSERT INTO items(path,name,kind,source,added) VALUES(?,?,'link','',?)",
                params![valid_url(value)?, valid_name(name)?, now()],
            )
            .map_err(err)?;
            let id = tx.last_insert_rowid();
            if target > 0 {
                touch_project(&tx, target)?;
                tx.execute(
                    "INSERT INTO item_projects(item_id,label_id) VALUES(?,?)",
                    params![id, target],
                )
                .map_err(err)?;
            }
        }
        "pin_project" | "unpin_project" => {
            for id in c.ids {
                tx.execute(
                    "UPDATE item_projects SET favourite=? WHERE item_id=? AND label_id=?",
                    params![c.action == "pin_project", id, target],
                )
                .map_err(err)?;
            }
        }
        "set_source_url" => {
            if !value.trim().is_empty() {
                valid_url(value)?;
            }
            tx.execute(
                "UPDATE items SET source_url=? WHERE id=?",
                params![value.trim(), target],
            )
            .map_err(err)?;
        }
        "edit_link" => {
            tx.execute(
                "UPDATE items SET name=?,path=? WHERE id=? AND kind='link'",
                params![valid_name(name)?, valid_url(value)?, target],
            )
            .map_err(err)?;
        }
        "set_link_destination" => {
            let source = match value {
                "resources" => "Resources",
                "fonts" => "Fonts",
                "codepen" => "CodePen",
                "brand-guidelines" => "Brand Guidelines",
                "links" => "",
                _ => return Err("Choose a link collection.".into()),
            };
            tx.execute(
                "UPDATE items SET source=? WHERE id=? AND kind='link'",
                params![source, target],
            )
            .map_err(err)?;
        }
        "delete_links" => {
            for id in c.ids {
                tx.execute("DELETE FROM items WHERE id=? AND kind='link'", [id])
                    .map_err(err)?;
            }
        }
        "remove_items" | "restore_items" => {
            for id in c.ids {
                tx.execute("UPDATE items SET archived=? WHERE id=? AND NOT EXISTS(SELECT 1 FROM duplicate_recovery WHERE item_id=items.id)", params![c.action == "remove_items", id]).map_err(err)?;
            }
        }
        _ => return Err("Unknown change".into()),
    }
    if let Some((label,patches))=undo {crate::history::record(&tx,&label,patches)?;}
    tx.commit().map_err(err)?;
    Ok(())
}

#[derive(Serialize)]
pub struct RemovedItem {
    pub id: i64,
    pub name: String,
    pub path: String,
}
pub fn removed_items(path: &Path) -> Result<Vec<RemovedItem>> {
    let db = connect(path)?;
    let mut stmt = db.prepare("SELECT id,name,path FROM items WHERE archived=1 AND NOT EXISTS(SELECT 1 FROM duplicate_recovery WHERE item_id=items.id) ORDER BY id DESC").map_err(err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(RemovedItem {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
            })
        })
        .map_err(err)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)
}

#[derive(Serialize, Deserialize)]
pub struct Backup {
    pub version: u32,
    pub library: Library,
}
pub fn export(path: &Path, destination: PathBuf) -> Result<()> {
    let data = Backup {
        version: 1,
        library: load(path, false)?,
    };
    std::fs::write(destination, serde_json::to_vec_pretty(&data).map_err(err)?).map_err(err)
}
pub fn import(path: &Path, source: PathBuf) -> Result<()> {
    let backup: Backup =
        serde_json::from_slice(&std::fs::read(source).map_err(err)?).map_err(err)?;
    if backup.version != 1 {
        return Err("Unsupported backup version".into());
    }
    let mut db = connect(path)?;
    let tx = db.transaction().map_err(err)?;
    let mut projects = std::collections::HashMap::new();
    for category in backup.library.categories {
        tx.execute("INSERT OR IGNORE INTO categories(name) VALUES(?)",[valid_name(&category)?]).map_err(err)?;
    }
    let mut tags = std::collections::HashMap::new();
    for (table, labels, map) in [
        ("projects", backup.library.projects, &mut projects),
        ("tags", backup.library.tags, &mut tags),
    ] {
        for label in labels {
            tx.execute(
                &format!("INSERT OR IGNORE INTO {table}(name) VALUES(?)"),
                [valid_name(&label.name)?],
            )
            .map_err(err)?;
            let id: i64 = tx
                .query_row(
                    &format!("SELECT id FROM {table} WHERE name=?"),
                    [&label.name],
                    |r| r.get(0),
                )
                .map_err(err)?;
            map.insert(label.id, id);
        }
    }
    for item in backup.library.items {
        if !["image", "animation", "video", "link"].contains(&item.kind.as_str()) {
            return Err("Invalid item type in backup".into());
        }
        if item.kind == "link" {
            valid_url(&item.path)?;
        }
        let missing = item.kind != "link" && !Path::new(&item.path).is_file();
        if !item.source_url.is_empty() {
            valid_url(&item.source_url)?;
        }
        tx.execute("INSERT OR IGNORE INTO items(path,name,kind,source,modified,added,missing,source_url,thumbnail) VALUES(?,?,?,?,?,?,?,?,?)",params![item.path,item.name,item.kind,item.source,item.modified,item.added,missing,item.source_url,item.thumbnail]).map_err(err)?;
        let id: i64 = tx
            .query_row("SELECT id FROM items WHERE path=?", [&item.path], |r| {
                r.get(0)
            })
            .map_err(err)?;
        if item.kind != "link" {
            if let Some(category) = &item.category {
                // Archived items keep their category too when merging a backup.
                if !category.is_empty() { valid_name(category)?; tx.execute("INSERT OR IGNORE INTO categories(name) VALUES(?)",[category]).map_err(err)?; }
                tx.execute("INSERT INTO item_categories(item_id,name) VALUES(?,?) ON CONFLICT(item_id) DO UPDATE SET name=excluded.name",params![id,category]).map_err(err)?;
            }
        }
        if item.kind == "link"
            && (["Resources", "Fonts", "CodePen", "Brand Guidelines"].contains(&item.source.as_str()) || item.source.starts_with("Resources/"))
        {
            tx.execute(
                "UPDATE items SET source=? WHERE id=? AND kind='link'",
                params![item.source, id],
            )
            .map_err(err)?;
        }
        if !item.source_url.is_empty() {
            tx.execute(
                "UPDATE items SET source_url=? WHERE id=? AND source_url=''",
                params![item.source_url, id],
            )
            .map_err(err)?;
        }
        if !item.thumbnail.is_empty() {
            tx.execute("UPDATE items SET thumbnail=? WHERE id=? AND thumbnail=''", params![item.thumbnail,id]).map_err(err)?;
        }
        for (table, ids, map) in [
            ("item_projects", item.projects, &projects),
            ("item_tags", item.tags, &tags),
        ] {
            for old in ids {
                if let Some(label) = map.get(&old) {
                    tx.execute(
                        &format!("INSERT OR IGNORE INTO {table}(item_id,label_id) VALUES(?,?)"),
                        params![id, label],
                    )
                    .map_err(err)?;
                }
            }
        }
        for old in item.favourites {
            if let Some(project) = projects.get(&old) {
                tx.execute(
                    "UPDATE item_projects SET favourite=1 WHERE item_id=? AND label_id=?",
                    params![id, project],
                )
                .map_err(err)?;
            }
        }
    }
    tx.execute("DELETE FROM organisation_history",[]).map_err(err)?;
    tx.execute("DELETE FROM organisation_redo",[]).map_err(err)?;
    tx.commit().map_err(err)?;
    Ok(())
}
