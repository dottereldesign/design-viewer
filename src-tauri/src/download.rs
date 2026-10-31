use crate::{
    content,
    db::{self, err, Result},
    library, sources,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Deserialize)]
pub struct Request {
    pub url: String,
    #[serde(default)]
    pub projects: Vec<i64>,
    #[serde(default)]
    pub tags: Vec<i64>,
}
#[derive(Clone, Serialize, Debug)]
pub struct Progress {
    pub phase: &'static str,
    pub percent: Option<f64>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed: Option<f64>,
    pub eta: Option<u64>,
}
impl Progress {
    fn phase(phase: &'static str) -> Self {
        Self {
            phase,
            percent: None,
            downloaded: 0,
            total: None,
            speed: None,
            eta: None,
        }
    }
}
#[derive(Serialize)]
pub struct Saved {
    pub id: i64,
    pub reused: bool,
}

pub fn normalize(input: &str) -> Result<String> {
    let mut url = url::Url::parse(db::valid_url(input)?).map_err(err)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Paste a website link without a username or password.".into());
    }
    url.set_fragment(None);
    if matches!(
        url.host_str(),
        Some("www.facebook.com" | "facebook.com" | "m.facebook.com")
    ) {
        if let Some(id) = url
            .query_pairs()
            .find(|(key, _)| key == "v")
            .map(|(_, value)| value.into_owned())
        {
            if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                return Ok(format!("https://www.facebook.com/watch/?v={id}"));
            }
        }
    }
    Ok(url.to_string())
}
pub fn progress(line: &str) -> Option<Progress> {
    let data: serde_json::Value = serde_json::from_str(line.strip_prefix("DV_PROGRESS:")?).ok()?;
    let downloaded = data["downloaded_bytes"].as_u64().unwrap_or(0);
    let total = data["total_bytes"]
        .as_u64()
        .or_else(|| data["total_bytes_estimate"].as_u64())
        .filter(|n| *n > 0);
    Some(Progress {
        phase: if data["status"] == "finished" {
            "processing"
        } else {
            "downloading"
        },
        percent: total.map(|n| (downloaded as f64 / n as f64 * 100.).clamp(0., 100.)),
        downloaded,
        total,
        speed: data["speed"].as_f64(),
        eta: data["eta"].as_f64().map(|n| n.max(0.).ceil() as u64),
    })
}
fn capture(request: &Request) -> content::Capture {
    content::Capture {
        destination: String::new(),
        thumbnail: None,
        resource: false,
        source: "Videos".into(),
        files: vec![],
        url: String::new(),
        name: String::new(),
        source_url: request.url.clone(),
        projects: request.projects.clone(),
        tags: request.tags.clone(),
    }
}
fn labels(path: &Path, request: &Request) -> Result<()> {
    let db = db::connect(path)?;
    for (table, ids) in [("projects", &request.projects), ("tags", &request.tags)] {
        for id in ids {
            let exists: bool = db
                .query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?)"),
                    [id],
                    |r| r.get(0),
                )
                .map_err(err)?;
            if !exists {
                return Err("A selected project or tag no longer exists.".into());
            }
        }
    }
    Ok(())
}
pub fn existing(path: &Path, request: &Request) -> Result<Option<Saved>> {
    labels(path, request)?;
    let mut db = db::connect(path)?;
    let records = {
        let mut stmt = db.prepare("SELECT id,path,source_url FROM items WHERE kind='video' AND source_url!='' AND (source='Videos' OR source LIKE 'Videos/%')").map_err(err)?;
        let records = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        records
    };
    for (id, file, source) in records {
        if normalize(&source).ok() == Some(request.url.clone()) && Path::new(&file).is_file() {
            let tx = db.transaction().map_err(err)?;
            tx.execute("UPDATE items SET archived=0,missing=0 WHERE id=?", [id])
                .map_err(err)?;
            content::attach(&tx, id, &capture(request))?;
            tx.commit().map_err(err)?;
            return Ok(Some(Saved { id, reused: true }));
        }
    }
    Ok(None)
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn hidden(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
}
fn stop(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        let mut taskkill = Command::new("taskkill.exe");
        hidden(&mut taskkill);
        let _ = taskkill
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}
fn error_message(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("login")
        || lower.contains("log in")
        || lower.contains("private")
        || lower.contains("cookies")
    {
        "This video requires a login or is private. Try a public video link.".into()
    } else if lower.contains("unsupported url") {
        "This website or link is not supported by yt-dlp.".into()
    } else if lower.contains("timed out")
        || lower.contains("resolve")
        || lower.contains("connection")
    {
        "Could not connect to the video website. Check your connection and try again.".into()
    } else {
        "The website did not provide a downloadable video. It may be removed, restricted, or temporarily blocked. Try again or use another link.".into()
    }
}
pub fn run(
    db_path: &Path,
    downloader: &Path,
    ffmpeg: &Path,
    mut request: Request,
    cancelled: &AtomicBool,
    notify: &mut impl FnMut(Progress),
) -> Result<Saved> {
    request.url = normalize(&request.url)?;
    if let Some(saved) = existing(db_path, &request)? {
        return Ok(saved);
    }
    if !downloader.is_file() {
        return Err("yt-dlp is missing. Reinstall the updated Design app.".into());
    }
    let root = sources::root(&db::connect(db_path)?, "Videos")?
        .canonicalize()
        .map_err(err)?;
    let staging = db_path
        .parent()
        .ok_or("Missing data folder")?
        .join("video-downloads");
    fs::create_dir_all(&staging).map_err(err)?;
    let staging = staging.canonicalize().map_err(err)?;
    let job = staging.join(content::nonce().to_string());
    fs::create_dir(&job).map_err(err)?;
    let job = job.canonicalize().map_err(err)?;
    if !job.starts_with(&staging) {
        return Err("Download staging must stay inside the app data folder.".into());
    }
    let job = Temporary(job);
    notify(Progress::phase("connecting"));
    let mut command = Command::new(downloader);
    hidden(&mut command);
    command
        .args([
            "--ignore-config",
            "--no-playlist",
            "--no-cache-dir",
            "--encoding",
            "utf-8",
            "--newline",
            "--no-colors",
            "--progress",
            "--socket-timeout",
            "20",
            "--retries",
            "2",
            "--fragment-retries",
            "2",
            "--windows-filenames",
            "--trim-filenames",
            "160",
            "--merge-output-format",
            "mp4",
            "--remux-video",
            "mp4",
            "--write-info-json",
            "--progress-template",
            "download:DV_PROGRESS:%(progress)j",
            "--print",
            "after_move:DV_FILE:%(filepath)s",
            "--ffmpeg-location",
        ])
        .arg(ffmpeg)
        .arg("-P")
        .arg(&job.0)
        .args(["-o", "%(id)s.%(ext)s", "--"])
        .arg(&request.url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not start yt-dlp: {e}"))?;
    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out_tx = tx.clone();
    let out = std::thread::spawn(move || {
        for line in BufReader::new(stdout)
            .lines()
            .map_while(std::result::Result::ok)
        {
            if out_tx.send(line).is_err() {
                break;
            }
        }
    });
    let errors = std::thread::spawn(move || {
        for line in BufReader::new(stderr)
            .lines()
            .map_while(std::result::Result::ok)
        {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let started = Instant::now();
    let mut filepath = None;
    let mut last_error = String::new();
    let status = loop {
        if cancelled.load(Ordering::SeqCst) || started.elapsed() > Duration::from_secs(60 * 60) {
            stop(&mut child);
            let _ = out.join();
            let _ = errors.join();
            return Err(if cancelled.load(Ordering::SeqCst) {
                "Download cancelled."
            } else {
                "Download timed out. Try again."
            }
            .into());
        }
        if let Ok(line) = rx.recv_timeout(Duration::from_millis(150)) {
            consume(&line, &mut filepath, &mut last_error, notify);
        }
        if let Some(status) = child.try_wait().map_err(err)? {
            break status;
        }
    };
    let _ = out.join();
    let _ = errors.join();
    for line in rx.try_iter() {
        consume(&line, &mut filepath, &mut last_error, notify);
    }
    if !status.success() {
        return Err(error_message(&last_error));
    }
    if cancelled.load(Ordering::SeqCst) {
        return Err("Download cancelled.".into());
    }
    notify(Progress::phase("saving"));
    let file = PathBuf::from(filepath.ok_or("yt-dlp did not return a video file.")?)
        .canonicalize()
        .map_err(err)?;
    if !file.starts_with(&job.0)
        || library::kind(&file) != Some("video")
        || fs::metadata(&file).map_err(err)?.len() == 0
    {
        return Err("No playable video was downloaded.".into());
    }
    let info = file.with_extension("info.json");
    let title = fs::read(&info)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|info| {
            info["title"]
                .as_str()
                .map(|s| s.chars().take(300).collect::<String>())
        })
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Downloaded video".into());
    import(db_path, &root, &file, &title, &request)
}
fn consume(
    line: &str,
    file: &mut Option<String>,
    error: &mut String,
    notify: &mut impl FnMut(Progress),
) {
    if let Some(progress) = progress(line) {
        notify(progress);
    } else if let Some(path) = line.strip_prefix("DV_FILE:") {
        *file = Some(path.into());
    } else if line.contains("ERROR:") {
        *error = line.chars().take(1500).collect();
    }
}
pub fn import(
    db_path: &Path,
    root: &Path,
    file: &Path,
    title: &str,
    request: &Request,
) -> Result<Saved> {
    labels(db_path, request)?;
    let directory = root.join("Downloads");
    fs::create_dir_all(&directory).map_err(err)?;
    let directory = directory.canonicalize().map_err(err)?;
    if !directory.starts_with(root) {
        return Err("Videos/Downloads must be inside the Videos folder.".into());
    }
    let destination = directory.join(format!(
        "{}-{}",
        content::nonce(),
        file.file_name().unwrap_or_default().to_string_lossy()
    ));
    let temporary = destination.with_extension("pending");
    fs::copy(file, &temporary).map_err(err)?;
    fs::rename(&temporary, &destination).map_err(err)?;
    let modified = fs::metadata(&destination)
        .map_err(err)?
        .modified()
        .map_err(err)?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let mut db = db::connect(db_path)?;
    let tx = db.transaction().map_err(err)?;
    tx.execute("INSERT INTO items(path,name,kind,source,modified,added,source_url) VALUES(?,?,'video','Videos/Downloads',?,?,?) ON CONFLICT(path) DO UPDATE SET name=excluded.name,source_url=excluded.source_url,archived=0,missing=0",params![destination.to_string_lossy(),title,modified,db::now(),request.url]).map_err(err)?;
    let id = tx
        .query_row(
            "SELECT id FROM items WHERE path=?",
            [destination.to_string_lossy()],
            |r| r.get(0),
        )
        .map_err(err)?;
    content::attach(&tx, id, &capture(request))?;
    tx.commit().map_err(err)?;
    Ok(Saved { id, reused: false })
}
