use crate::db::{self, err, Result};
use rusqlite::{params, Connection};
use std::path::PathBuf;

pub const NAMES: [&str; 3] = ["Design", "Photography", "Videos"];

fn key(name: &str) -> Result<&'static str> {
    match name {
        "" | "Design" => Ok("root"),
        "Photography" => Ok("root_photography"),
        "Videos" => Ok("root_videos"),
        _ => Err("Choose Design, Photography or Videos.".into()),
    }
}

pub fn root(db: &Connection, name: &str) -> Result<PathBuf> {
    db::setting(db, key(name)?)
        .map(PathBuf::from)
        .ok_or_else(|| "Choose your Design folder in Settings first.".into())
}

pub fn roots(db: &Connection) -> Vec<(String, PathBuf)> {
    NAMES
        .into_iter()
        .filter_map(|name| root(db, name).ok().map(|path| (name.to_owned(), path)))
        .collect()
}

pub fn ensure(db: &Connection) -> Result<()> {
    let design = root(db, "Design")?.canonicalize().map_err(err)?;
    let parent = design
        .parent()
        .ok_or("Choose a Design folder inside a directory.")?;
    for name in ["Photography", "Videos"] {
        if db::setting(db, key(name)?).is_none() {
            let path = parent.join(name);
            std::fs::create_dir_all(&path).map_err(err)?;
            db::set_setting(
                db,
                key(name)?,
                &path.canonicalize().map_err(err)?.to_string_lossy(),
            )?;
        }
    }
    if db::setting(db, "source_layout").is_none() {
        let records = {
            let mut stmt = db
                .prepare("SELECT id,path,source FROM items WHERE kind!='link'")
                .map_err(err)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })
                .map_err(err)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(err)?
        };
        let tx = db.unchecked_transaction().map_err(err)?;
        for (id, path, old) in records {
            let path = PathBuf::from(path);
            if path.canonicalize().unwrap_or(path).starts_with(&design) {
                let source = if old.is_empty() {
                    "Design".to_owned()
                } else {
                    format!("Design/{old}")
                };
                tx.execute("UPDATE items SET source=? WHERE id=?", params![source, id])
                    .map_err(err)?;
            }
        }
        db::set_setting(&tx, "source_layout", "1")?;
        tx.commit().map_err(err)?;
    }
    Ok(())
}
