use crate::{
    db::{self, err, Result},
    library,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Clone, Debug)]
pub struct Fingerprint {
    pub file: String,
    pub pixels: Option<String>,
    pub size: i64,
    pub modified: i64,
}
fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(err)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(err)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn fingerprint(path: &Path) -> Result<Fingerprint> {
    let before = fs::metadata(path).map_err(err)?;
    let modified = before
        .modified()
        .map_err(err)?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let file = hash_file(path)?;
    let pixels = if library::kind(path) == Some("image") {
        image::ImageReader::open(path)
            .ok()
            .and_then(|r| r.with_guessed_format().ok())
            .and_then(|r| r.decode().ok())
            .map(|image| {
                let mut hash = Sha256::new();
                hash.update(image.width().to_le_bytes());
                hash.update(image.height().to_le_bytes());
                hash.update(image.to_rgba8().as_raw());
                format!("{:x}", hash.finalize())
            })
    } else {
        None
    };
    let after = fs::metadata(path).map_err(err)?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err("A file changed while checking duplicates. Try again.".into());
    }
    Ok(Fingerprint {
        file,
        pixels,
        size: before.len() as i64,
        modified,
    })
}
fn store(db: &Connection, id: i64, fp: &Fingerprint) -> Result<()> {
    db.execute("INSERT INTO fingerprints VALUES(?,?,?,?,?) ON CONFLICT(item_id) DO UPDATE SET size=excluded.size,modified=excluded.modified,file_hash=excluded.file_hash,pixel_hash=excluded.pixel_hash",params![id,fp.size,fp.modified,fp.file,fp.pixels]).map_err(err)?;
    Ok(())
}
pub fn index(db_path: &Path) -> Result<usize> {
    index_with_progress(db_path, &mut |_| {})
}

#[derive(Clone, Serialize, Debug)]
pub struct DuplicateProgress {
    pub phase: &'static str,
    pub checked: usize,
    pub total: usize,
    pub current: String,
    pub elapsed_ms: u64,
    pub remaining_seconds: Option<u64>,
}
impl DuplicateProgress {
    pub fn waiting() -> Self {
        Self {
            phase: "waiting",
            checked: 0,
            total: 0,
            current: String::new(),
            elapsed_ms: 0,
            remaining_seconds: None,
        }
    }
}

pub fn index_with_progress(
    db_path: &Path,
    progress: &mut impl FnMut(DuplicateProgress),
) -> Result<usize> {
    let db = db::connect(db_path)?;
    let records: Vec<(i64, String, Option<i64>, Option<i64>)> = {
        let mut stmt=db.prepare("SELECT i.id,i.path,f.size,f.modified FROM items i LEFT JOIN fingerprints f ON f.item_id=i.id WHERE i.kind!='link' AND i.archived=0 AND i.missing=0").map_err(err)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        rows
    };
    let mut skipped = 0;
    let total = records.len();
    let started = Instant::now();
    let mut last_sent = Instant::now();
    progress(DuplicateProgress {
        phase: "checking",
        total,
        ..DuplicateProgress::waiting()
    });
    for (position, (id, path, size, modified)) in records.into_iter().enumerate() {
        // Report the file before decoding/hashing it, including slow individual files.
        let elapsed_ms = started.elapsed().as_millis() as u64;
        if position == 0 || last_sent.elapsed().as_millis() >= 100 {
            progress(DuplicateProgress {
                phase: "checking",
                checked: position,
                total,
                current: Path::new(&path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                elapsed_ms,
                remaining_seconds: if position >= 3 && elapsed_ms >= 1500 {
                    Some(
                        ((elapsed_ms as f64 / position as f64 * (total - position) as f64) / 1000.0)
                            .ceil() as u64,
                    )
                } else {
                    None
                },
            });
            last_sent = Instant::now();
        }
        let Ok(meta) = fs::metadata(&path) else {
            skipped += 1;
            continue;
        };
        let time = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|t| t.as_millis() as i64);
        if size == Some(meta.len() as i64) && modified == time {
            continue;
        }
        match fingerprint(Path::new(&path)) {
            Ok(fp) => store(&db, id, &fp)?,
            Err(_) => {
                skipped += 1;
                db.execute("DELETE FROM fingerprints WHERE item_id=?", [id])
                    .map_err(err)?;
            }
        }
    }
    progress(DuplicateProgress {
        phase: "grouping",
        checked: total,
        total,
        current: String::new(),
        elapsed_ms: started.elapsed().as_millis() as u64,
        remaining_seconds: None,
    });
    Ok(skipped)
}
fn matching(db: &Connection, fp: &Fingerprint, root: &Path) -> Result<Option<i64>> {
    let mut stmt=db.prepare("SELECT i.id,i.path FROM fingerprints f JOIN items i ON i.id=f.item_id WHERE i.archived=0 AND i.missing=0 AND (f.file_hash=? OR (? IS NOT NULL AND f.pixel_hash=?)) ORDER BY i.id").map_err(err)?;
    for row in stmt
        .query_map(params![fp.file, fp.pixels, fp.pixels], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(err)?
    {
        let (id, path) = row.map_err(err)?;
        if !Path::new(&path)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(&path))
            .starts_with(root)
        {
            continue;
        }
        if let Ok(current) = fingerprint(Path::new(&path)) {
            if current.file == fp.file || (current.pixels.is_some() && current.pixels == fp.pixels)
            {
                return Ok(Some(id));
            }
            store(db, id, &current)?;
        }
    }
    Ok(None)
}
#[derive(Serialize)]
pub struct DuplicateScan {
    pub groups: Vec<Vec<i64>>,
    pub skipped: usize,
}
pub fn duplicates(db_path: &Path) -> Result<DuplicateScan> {
    duplicates_with_progress(db_path, &mut |_| {})
}
pub fn duplicates_with_progress(
    db_path: &Path,
    progress: &mut impl FnMut(DuplicateProgress),
) -> Result<DuplicateScan> {
    let skipped = index_with_progress(db_path, progress)?;
    let db = db::connect(db_path)?;
    let mut stmt=db.prepare("SELECT f.item_id,f.file_hash,f.pixel_hash FROM fingerprints f JOIN items i ON i.id=f.item_id WHERE i.archived=0 AND i.missing=0 ORDER BY i.id").map_err(err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    let mut key_to_group = HashMap::<String, usize>::new();
    let mut groups: Vec<HashSet<i64>> = vec![];
    for (id, file, pixels) in rows {
        let mut keys = vec![format!("file:{file}")];
        if let Some(p) = pixels {
            keys.push(format!("pixels:{p}"));
        }
        let known: Vec<usize> = keys
            .iter()
            .filter_map(|k| key_to_group.get(k).copied())
            .collect();
        let group = known.first().copied().unwrap_or_else(|| {
            groups.push(HashSet::new());
            groups.len() - 1
        });
        for other in known {
            if other != group {
                let moved = std::mem::take(&mut groups[other]);
                groups[group].extend(moved);
                for value in key_to_group.values_mut() {
                    if *value == other {
                        *value = group
                    }
                }
            }
        }
        groups[group].insert(id);
        for key in keys {
            key_to_group.insert(key, group);
        }
    }
    let groups = groups
        .into_iter()
        .filter(|g| g.len() > 1)
        .map(|g| {
            let mut ids: Vec<i64> = g.into_iter().collect();
            ids.sort();
            ids
        })
        .collect();
    Ok(DuplicateScan { groups, skipped })
}
#[derive(Serialize, Deserialize, Clone)]
pub struct CaptureFile {
    pub path: String,
    pub name: String,
}
#[derive(Deserialize)]
pub struct Capture {
    #[serde(default)]
    pub destination: String,
    #[serde(default)]
    pub thumbnail: Option<CaptureFile>,
    #[serde(default)]
    pub resource: bool,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub files: Vec<CaptureFile>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub projects: Vec<i64>,
    #[serde(default)]
    pub tags: Vec<i64>,
}
#[derive(Serialize)]
pub struct CaptureResult {
    pub destination: Option<&'static str>,
    pub ids: Vec<i64>,
    pub added: usize,
    pub reused: usize,
}
pub fn nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
pub(crate) fn attach(db: &Connection, id: i64, c: &Capture) -> Result<()> {
    for project in &c.projects {
        db::touch_project(db, *project)?;
        db.execute(
            "INSERT OR IGNORE INTO item_projects(item_id,label_id) VALUES(?,?)",
            params![id, project],
        )
        .map_err(err)?;
    }
    for tag in &c.tags {
        db.execute(
            "INSERT OR IGNORE INTO item_tags(item_id,label_id) VALUES(?,?)",
            params![id, tag],
        )
        .map_err(err)?;
    }
    if !c.source_url.trim().is_empty() {
        db.execute(
            "UPDATE items SET source_url=? WHERE id=? AND source_url=''",
            params![c.source_url.trim(), id],
        )
        .map_err(err)?;
    }
    Ok(())
}
// Remove a newly written thumbnail if metadata saving fails at any point.
struct PendingThumbnail(Option<PathBuf>);
impl Drop for PendingThumbnail {
    fn drop(&mut self) { if let Some(path) = &self.0 { let _ = fs::remove_file(path); } }
}
pub fn capture(db_path: &Path, c: Capture) -> Result<CaptureResult> {
    if !c.source_url.trim().is_empty() {
        db::valid_url(&c.source_url)?;
    }
    let mut db = db::connect(db_path)?;
    // Validate all labels before writing files or adding any metadata.
    for (table, ids) in [("projects", &c.projects), ("tags", &c.tags)] {
        for id in ids {
            let found: bool = db
                .query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?)"),
                    [id],
                    |r| r.get(0),
                )
                .map_err(err)?;
            if !found {
                return Err("A selected project or tag no longer exists.".into());
            }
        }
    }
    let mut result = CaptureResult {
        destination: None,
        ids: vec![],
        added: 0,
        reused: 0,
    };
    if c.files.is_empty() {
        let (destination, collection) = match c.destination.as_str() {
            "fonts" => ("fonts", "Fonts"),
            "codepen" => ("codepen", "CodePen"),
            "brand-guidelines" => ("brand-guidelines", "Brand Guidelines"),
            "resources" => ("resources", "Resources"),
            "links" | "" => if c.resource { ("resources", "Resources") } else { ("links", "") },
            _ => return Err("Unknown link collection".into()),
        };
        result.destination = Some(destination);
        let url = url::Url::parse(db::valid_url(&c.url)?)
            .map_err(err)?
            .to_string();
        let mut thumbnail = PendingThumbnail(None);
        if let Some(file) = &c.thumbnail {
            let mut decoder = image::ImageReader::open(&file.path).map_err(err)?.with_guessed_format().map_err(err)?.into_decoder().map_err(err)?;
            use image::ImageDecoder;
            let orientation = decoder.orientation().map_err(err)?;
            let mut image = image::DynamicImage::from_decoder(decoder).map_err(err)?;
            image.apply_orientation(orientation);
            let directory = db_path.parent().ok_or("Missing data folder")?.join("link-thumbnails");
            fs::create_dir_all(&directory).map_err(err)?;
            let path = directory.join(format!("{}.png", nonce()));
            thumbnail.0 = Some(path.clone());
            image.thumbnail(image.width().min(1600), image.height().min(1600)).save(&path).map_err(err)?;
        }
        let mut stmt = db
            .prepare("SELECT id,path FROM items WHERE kind='link'")
            .map_err(err)?;
        let mut existing = None;
        for row in stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(err)?
        {
            let (id, path) = row.map_err(err)?;
            if url::Url::parse(&path).ok().map(|u| u.to_string()) == Some(url.clone()) {
                existing = Some(id);
                break;
            }
        }
        drop(stmt);
        let tx = db.transaction().map_err(err)?;
        let id = if let Some(id) = existing {
            result.reused = 1;
            id
        } else {
            let name = if c.name.trim().is_empty() {
                url::Url::parse(&url)
                    .unwrap()
                    .host_str()
                    .unwrap_or("Link")
                    .to_owned()
            } else {
                c.name.trim().to_owned()
            };
            tx.execute(
                "INSERT INTO items(path,name,kind,source,added) VALUES(?,?,'link','',?)",
                params![url, name, db::now()],
            )
            .map_err(err)?;
            result.added = 1;
            tx.last_insert_rowid()
        };
        tx.execute("UPDATE items SET archived=0 WHERE id=?", [id])
            .map_err(err)?;
        if collection == "Resources" {
            tx.execute(
                "UPDATE items SET source='Resources' WHERE id=? AND source NOT LIKE 'Resources/%'",
                [id],
            )
            .map_err(err)?;
        } else {
            tx.execute("UPDATE items SET source=? WHERE id=?", params![collection, id])
                .map_err(err)?;
        }
        if let Some(path) = &thumbnail.0 {
            tx.execute("UPDATE items SET thumbnail=? WHERE id=?", params![path.to_string_lossy(), id]).map_err(err)?;
        }
        attach(&tx, id, &c)?;
        tx.commit().map_err(err)?;
        thumbnail.0.take();
        result.ids.push(id);
        return Ok(result);
    }
    index(db_path)?;
    let root = crate::sources::root(&db, &c.source)?
        .canonicalize()
        .map_err(err)?;
    let destination = root.join("Captures");
    fs::create_dir_all(&destination).map_err(err)?;
    let destination = destination.canonicalize().map_err(err)?;
    if !destination.starts_with(&root) {
        return Err("Captures must be inside your source folder.".into());
    }
    for input in &c.files {
        let path = PathBuf::from(&input.path).canonicalize().map_err(err)?;
        let kind = library::kind(&path).ok_or("Unsupported file type")?;
        let fp = fingerprint(&path)?;
        if let Some(id) = matching(&db, &fp, &root)? {
            let tx = db.transaction().map_err(err)?;
            attach(&tx, id, &c)?;
            tx.commit().map_err(err)?;
            result.ids.push(id);
            result.reused += 1;
            continue;
        }
        let original = Path::new(&input.name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Capture.png");
        let target = destination.join(format!("{}-{original}", nonce()));
        let temporary = target.with_extension("pending");
        fs::copy(&path, &temporary).map_err(err)?;
        if hash_file(&temporary)? != fp.file {
            let _ = fs::remove_file(&temporary);
            return Err("The source file changed during capture. Try again.".into());
        }
        fs::rename(&temporary, &target).map_err(err)?;
        let target = target.canonicalize().map_err(err)?;
        let modified = fs::metadata(&target)
            .map_err(err)?
            .modified()
            .map_err(err)?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let name = if c.files.len() == 1 && !c.name.trim().is_empty() {
            c.name.trim().to_owned()
        } else {
            Path::new(original)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        };
        let tx = db.transaction().map_err(err)?;
        let source = if db::setting(&tx, "source_layout").is_some() {
            format!(
                "{}/Captures",
                if c.source.is_empty() {
                    "Design"
                } else {
                    &c.source
                }
            )
        } else {
            "Captures".to_owned()
        };
        tx.execute("INSERT INTO items(path,name,kind,source,modified,added,source_url) VALUES(?,?,?,?,?,?,?) ON CONFLICT(path) DO NOTHING",params![target.to_string_lossy(),name,kind,source,modified,db::now(),c.source_url.trim()]).map_err(err)?;
        let id: i64 = tx
            .query_row(
                "SELECT id FROM items WHERE path=?",
                [target.to_string_lossy().as_ref()],
                |r| r.get(0),
            )
            .map_err(err)?;
        // The file watcher may have indexed it first; capture metadata wins.
        tx.execute(
            "UPDATE items SET name=?,source_url=? WHERE id=?",
            params![name, c.source_url.trim(), id],
        )
        .map_err(err)?;
        attach(&tx, id, &c)?;
        store(&tx, id, &Fingerprint { modified, ..fp })?;
        tx.commit().map_err(err)?;
        result.ids.push(id);
        result.added += 1;
    }
    result.ids.sort();
    result.ids.dedup();
    Ok(result)
}
pub fn stage_begin(data: &Path, name: &str) -> Result<CaptureFile> {
    let name = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid file name")?;
    let folder = data.join("staging");
    fs::create_dir_all(&folder).map_err(err)?;
    let extension = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ![
        "jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff", "ico", "avif", "mp4", "mov",
        "webm", "m4v", "mkv", "avi",
    ]
    .contains(&extension.as_str())
    {
        return Err("Unsupported file type".into());
    }
    let path = folder.join(format!("{}.{extension}", nonce()));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(err)?;
    Ok(CaptureFile {
        path: path.to_string_lossy().to_string(),
        name: name.to_owned(),
    })
}
pub fn stage_chunk(data: &Path, token: &str, bytes: &[u8]) -> Result<()> {
    if token.contains(['/', '\\', ':']) || token.is_empty() || bytes.len() > 4 * 1024 * 1024 {
        return Err("Invalid capture chunk".into());
    }
    let path = data.join("staging").join(token);
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(err)?;
    file.write_all(bytes).map_err(err)
}
#[derive(Serialize)]
pub struct ClipboardContent {
    pub files: Vec<CaptureFile>,
    pub url: String,
}
pub fn clipboard(data: &Path) -> Result<ClipboardContent> {
    let mut clipboard = arboard::Clipboard::new().map_err(err)?;
    if let Ok(paths) = clipboard.get().file_list() {
        if !paths.is_empty() {
            return clipboard_files(paths);
        }
    }
    if let Ok(image) = clipboard.get_image() {
        let staged = stage_begin(data, &format!("Clipboard-{}.png", db::now()))?;
        let buffer = image::RgbaImage::from_raw(
            image.width as u32,
            image.height as u32,
            image.bytes.into_owned(),
        )
        .ok_or("Invalid clipboard image")?;
        buffer.save(&staged.path).map_err(err)?;
        return Ok(ClipboardContent {
            files: vec![staged],
            url: String::new(),
        });
    }
    let text = clipboard
        .get_text()
        .map_err(|_| "Copy image/video files, an image, or a website URL first.".to_string())?;
    if !text.trim().starts_with("http://") && !text.trim().starts_with("https://") {
        let paths: Vec<PathBuf> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let line = line.trim().trim_matches('"');
                url::Url::parse(line)
                    .ok()
                    .and_then(|url| url.to_file_path().ok())
                    .unwrap_or_else(|| PathBuf::from(line))
            })
            .collect();
        if !paths.is_empty() && paths.iter().all(|path| path.is_absolute()) {
            return clipboard_files(paths);
        }
    }
    Ok(ClipboardContent {
        files: vec![],
        url: db::valid_url(&text)?.to_owned(),
    })
}
pub fn clipboard_files(paths: Vec<PathBuf>) -> Result<ClipboardContent> {
    let mut files = Vec::new();
    for path in paths {
        let path = path
            .canonicalize()
            .map_err(|_| format!("File is unavailable: {}", path.display()))?;
        if !path.is_file() || library::kind(&path).is_none() {
            return Err(format!(
                "Unsupported file: {}. Choose images, GIFs or videos.",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        if !files
            .iter()
            .any(|file: &CaptureFile| file.path == path.to_string_lossy())
        {
            files.push(CaptureFile {
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                path: path.to_string_lossy().into_owned(),
            });
        }
    }
    Ok(ClipboardContent {
        files,
        url: String::new(),
    })
}
fn move_file(source: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        return Err("The destination already exists.".into());
    }
    if fs::rename(source, destination).is_ok() {
        return Ok(());
    }
    let mut input = fs::File::open(source).map_err(err)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(err)?;
    std::io::copy(&mut input, &mut output).map_err(err)?;
    output.sync_all().map_err(err)?;
    drop(output);
    drop(input);
    if hash_file(source)? != hash_file(destination)? {
        return Err("Copy verification failed. The original was kept.".into());
    }
    fs::remove_file(source).map_err(err)
}
pub fn remove_duplicates(db_path: &Path, data: &Path, keeper: i64, ids: Vec<i64>) -> Result<usize> {
    if ids.contains(&keeper) {
        return Err("The retained copy cannot be removed.".into());
    }
    let mut db = db::connect(db_path)?;
    let kept: String = db
        .query_row(
            "SELECT path FROM items WHERE id=? AND archived=0 AND kind!='link'",
            [keeper],
            |r| r.get(0),
        )
        .map_err(err)?;
    let kept_fp = fingerprint(Path::new(&kept))?;
    let mut removed = 0;
    let folder = data.join("removed-duplicates");
    fs::create_dir_all(&folder).map_err(err)?;
    for id in ids.into_iter().collect::<HashSet<_>>() {
        if id == keeper {
            return Err("The retained copy cannot be removed.".into());
        }
        let original: String = db
            .query_row(
                "SELECT path FROM items WHERE id=? AND archived=0 AND kind!='link'",
                [id],
                |r| r.get(0),
            )
            .map_err(err)?;
        let fp = fingerprint(Path::new(&original))?;
        if fp.file != kept_fp.file && !(fp.pixels.is_some() && fp.pixels == kept_fp.pixels) {
            return Err("These files are no longer identical. Scan again.".into());
        }
        let recovery = folder.join(format!(
            "{id}-{}-{}",
            nonce(),
            Path::new(&original)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        let tx = db.transaction().map_err(err)?;
        tx.execute("INSERT INTO item_projects(item_id,label_id,favourite) SELECT ?,label_id,favourite FROM item_projects WHERE item_id=? ON CONFLICT(item_id,label_id) DO UPDATE SET favourite=MAX(favourite,excluded.favourite)",params![keeper,id]).map_err(err)?;
        tx.execute("INSERT OR IGNORE INTO item_tags(item_id,label_id) SELECT ?,label_id FROM item_tags WHERE item_id=?",params![keeper,id]).map_err(err)?;
        tx.execute("UPDATE items SET source_url=(SELECT source_url FROM items WHERE id=?) WHERE id=? AND source_url=''",params![id,keeper]).map_err(err)?;
        tx.execute(
            "INSERT INTO duplicate_recovery VALUES(?,?,?,?,?)",
            params![id, original, recovery.to_string_lossy(), keeper, db::now()],
        )
        .map_err(err)?;
        tx.execute("UPDATE items SET archived=1 WHERE id=?", [id])
            .map_err(err)?;
        move_file(Path::new(&original), &recovery)?;
        if let Err(e) = tx.commit() {
            let _ = move_file(&recovery, Path::new(&original));
            return Err(err(e));
        }
        removed += 1;
    }
    Ok(removed)
}
#[derive(Serialize)]
pub struct Recovery {
    pub id: i64,
    pub name: String,
    pub path: String,
}
pub fn recovery(db_path: &Path) -> Result<Vec<Recovery>> {
    let db = db::connect(db_path)?;
    let mut stmt=db.prepare("SELECT r.item_id,i.name,r.original_path FROM duplicate_recovery r JOIN items i ON i.id=r.item_id ORDER BY r.removed_at DESC").map_err(err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Recovery {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
            })
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(rows)
}
pub fn restore(db_path: &Path, id: i64) -> Result<()> {
    let mut db = db::connect(db_path)?;
    let (original, recovered): (String, String) = db
        .query_row(
            "SELECT original_path,recovery_path FROM duplicate_recovery WHERE item_id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(err)?;
    if Path::new(&original).exists() {
        return Err("A file already exists at the original location.".into());
    }
    let tx = db.transaction().map_err(err)?;
    tx.execute("UPDATE items SET archived=0,missing=0 WHERE id=?", [id])
        .map_err(err)?;
    tx.execute("DELETE FROM duplicate_recovery WHERE item_id=?", [id])
        .map_err(err)?;
    move_file(Path::new(&recovered), Path::new(&original))?;
    if let Err(e) = tx.commit() {
        let _ = move_file(Path::new(&original), Path::new(&recovered));
        return Err(err(e));
    }
    Ok(())
}
