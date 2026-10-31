use crate::db::{self, err, Result};
use rusqlite::params;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn kind(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "bmp" | "tiff" | "tif" | "ico" | "avif" => Some("image"),
        "gif" => Some("animation"),
        "webp" => {
            let mut header = [0u8; 32];
            let animated = fs::File::open(path)
                .and_then(|mut f| f.read(&mut header))
                .map(|n| n >= 21 && &header[12..16] == b"VP8X" && header[20] & 2 != 0)
                .unwrap_or(false);
            Some(if animated { "animation" } else { "image" })
        }
        "mp4" | "mov" | "webm" | "m4v" | "mkv" | "avi" => Some("video"),
        _ => None,
    }
}
pub fn scan(db_path: &Path, root: &Path) -> Result<usize> {
    scan_named(db_path, root, "")
}
pub fn scan_all(db_path: &Path) -> Result<usize> {
    let db = db::connect(db_path)?;
    let roots = crate::sources::roots(&db);
    if roots.is_empty() {
        return Err("Choose your Design folder first".into());
    }
    roots.into_iter().try_fold(0, |count, (name, root)| {
        scan_named(db_path, &root, &name).map(|n| count + n)
    })
}
fn scan_named(db_path: &Path, root: &Path, prefix: &str) -> Result<usize> {
    if !root.is_dir() {
        return Err("The source folder is unavailable. Choose an existing folder.".into());
    }
    let root = root.canonicalize().map_err(err)?;
    let mut records = vec![];
    for entry in walkdir::WalkDir::new(&root).follow_links(false) {
        let entry = entry.map_err(err)?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Some(kind) = kind(path) else { continue };
        let metadata = entry.metadata().map_err(err)?;
        let modified = metadata
            .modified()
            .unwrap_or(std::time::UNIX_EPOCH)
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let relative = path.strip_prefix(&root).map_err(err)?;
        let relative_source = relative
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .replace('\\', "/");
        let source = match (prefix.is_empty(), relative_source.is_empty()) {
            (true, _) => relative_source,
            (false, true) => prefix.to_owned(),
            (false, false) => format!("{prefix}/{relative_source}"),
        };
        records.push((
            path.to_string_lossy().to_string(),
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            kind,
            source,
            modified,
        ));
    }
    let mut db = db::connect(db_path)?;
    let tx = db.transaction().map_err(err)?;
    for (path, name, kind, source, modified) in &records {
        tx.execute("INSERT INTO items(path,name,kind,source,modified,added,missing) VALUES(?,?,?,?,?,?,0) ON CONFLICT(path) DO UPDATE SET kind=excluded.kind,source=excluded.source,modified=excluded.modified,missing=0",params![path,name,kind,source,modified,db::now()]).map_err(err)?;
    }
    let existing: Vec<(i64, String)> = {
        let mut stmt = tx
            .prepare("SELECT id,path FROM items WHERE kind!='link'")
            .map_err(err)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        rows
    };
    for (id, path) in existing {
        tx.execute(
            "UPDATE items SET missing=? WHERE id=?",
            params![!Path::new(&path).is_file(), id],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)?;
    Ok(records.len())
}

pub(crate) fn run_ffmpeg(ffmpeg: &Path, args: &[&str], timeout: u64) -> Result<()> {
    let mut command = Command::new(ffmpeg);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not start FFmpeg: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait().map_err(err)? {
            Some(status) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(
                        "This file could not be decoded. You can reveal the original in Explorer."
                            .into(),
                    )
                }
            }
            None => {
                if start.elapsed() > Duration::from_secs(timeout) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("Preview generation timed out.".into());
                }
                std::thread::sleep(Duration::from_millis(60));
            }
        }
    }
}
pub fn media(db_path: &Path, cache: &Path, ffmpeg: &Path, id: i64, mode: &str) -> Result<String> {
    let db = db::connect(db_path)?;
    let (path, kind, modified): (String, String, i64) = db
        .query_row(
            "SELECT path,kind,modified FROM items WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(err)?;
    if kind == "link" {
        return Err("Links have no local media".into());
    }
    let source = Path::new(&path);
    if !source.is_file() {
        return Err("Original file is missing".into());
    }
    if mode == "original" && !path.to_lowercase().ends_with(".mov") {
        return Ok(path);
    }
    if !["thumbnail", "preview", "original", "compatible"].contains(&mode) {
        return Err("Unknown media mode".into());
    }
    let meta = fs::metadata(source).map_err(err)?;
    let suffix = if mode == "thumbnail" { "jpg" } else { "mp4" };
    let target = cache.join(format!("v1-{id}-{modified}-{}-{mode}.{suffix}", meta.len()));
    if target.is_file() {
        return Ok(target.to_string_lossy().into_owned());
    }
    fs::create_dir_all(cache).map_err(err)?;
    // Each request writes its own temporary file; an interrupted decode never becomes a cache hit.
    let temp = cache.join(format!("tmp-{id}-{mode}-{}.{suffix}", db::now()));
    let output = temp.to_string_lossy();
    let result = if mode == "thumbnail" {
        let decoded = if kind != "video" {
            image::ImageReader::open(source)
                .ok()
                .and_then(|r| r.with_guessed_format().ok())
                .and_then(|r| r.decode().ok())
        } else {
            None
        };
        if let Some(image) = decoded {
            let small = image
                .thumbnail(image.width().min(720), image.height().min(540))
                .to_rgba8();
            let mut canvas = image::RgbImage::new(small.width(), small.height());
            for (x, y, p) in small.enumerate_pixels() {
                let a = p[3] as u16;
                canvas.put_pixel(
                    x,
                    y,
                    image::Rgb([
                        ((p[0] as u16 * a + 255 * (255 - a)) / 255) as u8,
                        ((p[1] as u16 * a + 255 * (255 - a)) / 255) as u8,
                        ((p[2] as u16 * a + 255 * (255 - a)) / 255) as u8,
                    ]),
                );
            }
            let file = fs::File::create(&temp).map_err(err)?;
            image::codecs::jpeg::JpegEncoder::new_with_quality(file, 82)
                .encode_image(&canvas)
                .map_err(err)
        } else {
            run_ffmpeg(
                ffmpeg,
                &[
                    "-v",
                    "error",
                    "-y",
                    "-i",
                    &path,
                    "-frames:v",
                    "1",
                    "-vf",
                    "scale=720:540:force_original_aspect_ratio=decrease",
                    "-q:v",
                    "3",
                    &output,
                ],
                45,
            )
        }
    } else {
        let mut args = vec!["-v", "error", "-y", "-i", &path];
        if mode == "preview" {
            args.extend(["-t", "8"]);
        }
        args.extend([
            "-an",
            "-vf",
            "scale=1280:960:force_original_aspect_ratio=decrease:force_divisible_by=2,fps=24",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "25",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            &output,
        ]);
        run_ffmpeg(ffmpeg, &args, if mode == "preview" { 60 } else { 180 })
    };
    if let Err(e) = result {
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    if let Err(e) = fs::rename(&temp, &target) {
        let _ = fs::remove_file(&temp);
        if !target.is_file() {
            return Err(err(e));
        }
    }
    Ok(target.to_string_lossy().into_owned())
}
pub fn default_root() -> Option<PathBuf> {
    let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .join("Design");
    if dev.is_dir() {
        return Some(dev);
    }
    let adjacent = std::env::current_exe().ok()?.parent()?.join("Design");
    if adjacent.is_dir() {
        Some(adjacent)
    } else {
        None
    }
}
