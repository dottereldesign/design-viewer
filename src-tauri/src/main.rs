#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod browser;
mod clipboard;
mod content;
mod db;
mod download;
mod library;
mod resources;
mod sources;
mod thumbnail;
mod history;
use db::{err, Result};
use notify::Watcher;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};

#[derive(Clone)]
struct State {
    db: PathBuf,
    cache: PathBuf,
    ffmpeg: PathBuf,
    downloader: PathBuf,
    downloading: Arc<AtomicBool>,
    download_cancel: Arc<AtomicBool>,
    scanning: Arc<AtomicBool>,
    media_slots: Arc<tokio::sync::Semaphore>,
    content_lock: Arc<std::sync::Mutex<()>>,
    index_progress: Arc<std::sync::Mutex<Option<content::DuplicateProgress>>>,
}
#[tauri::command]
async fn get_library(state: tauri::State<'_, State>) -> Result<db::Library> {
    let s = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || db::load(&s.db, s.scanning.load(Ordering::SeqCst)))
        .await
        .map_err(err)?
}
#[tauri::command]
async fn update(change: db::Change, state: tauri::State<'_, State>) -> Result<()> {
    let path = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db::change(&path, change))
        .await
        .map_err(err)?
}
#[tauri::command]
async fn get_media(id: i64, mode: String, state: tauri::State<'_, State>) -> Result<String> {
    let s = state.inner().clone();
    let _permit = s.media_slots.clone().acquire_owned().await.map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || {
        library::media(&s.db, &s.cache, &s.ffmpeg, id, &mode)
    })
    .await
    .map_err(err)?
}

fn scan_background(app: tauri::AppHandle, s: State) {
    if s.scanning.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = app.emit("scan-started", ());
    tauri::async_runtime::spawn_blocking(move || {
        let result = (|| library::scan_all(&s.db))();
        s.scanning.store(false, Ordering::SeqCst);
        match result {
            Ok(_) => {
                let _ = app.emit("library-changed", ());
                let _guard = s.content_lock.lock();
                let _ = content::index_with_progress(&s.db, &mut |mut progress| {
                    progress.phase = "indexing";
                    if let Ok(mut current) = s.index_progress.lock() {
                        *current = Some(progress);
                    }
                });
                if let Ok(mut current) = s.index_progress.lock() {
                    *current = None;
                }
            }
            Err(e) => {
                let _ = app.emit("scan-error", e);
            }
        }
    });
}
#[tauri::command]
fn rescan(app: tauri::AppHandle, state: tauri::State<'_, State>) {
    scan_background(app, state.inner().clone());
}
#[tauri::command]
fn set_root(path: String, app: tauri::AppHandle, state: tauri::State<'_, State>) -> Result<()> {
    if state.scanning.load(Ordering::SeqCst) {
        return Err("Wait for the current scan to finish.".into());
    }
    let root = PathBuf::from(path).canonicalize().map_err(err)?;
    if !root.is_dir() {
        return Err("Choose a folder".into());
    }
    app.asset_protocol_scope()
        .allow_directory(&root, true)
        .map_err(err)?;
    let db = db::connect(&state.db)?;
    db::set_setting(&db, "root", &root.to_string_lossy())?;
    sources::ensure(&db)?;
    for (_, path) in sources::roots(&db) {
        app.asset_protocol_scope()
            .allow_directory(path, true)
            .map_err(err)?;
    }
    scan_background(app, state.inner().clone());
    Ok(())
}
#[tauri::command]
async fn backup(
    path: String,
    restore: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, State>,
) -> Result<()> {
    let db = state.db.clone();
    let db_path = db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if restore {
            db::import(&db, PathBuf::from(path))
        } else {
            db::export(&db, PathBuf::from(path))
        }
    })
    .await
    .map_err(err)??;
    if restore {
        for item in db::load(&db_path, false)?.items {
            if item.kind != "link" {
                app.asset_protocol_scope()
                    .allow_file(&item.path)
                    .map_err(err)?;
            }
        }
    }
    Ok(())
}
#[tauri::command]
fn data_folder(state: tauri::State<'_, State>) -> String {
    state.db.parent().unwrap().to_string_lossy().into_owned()
}
#[tauri::command]
fn removed_items(state: tauri::State<'_, State>) -> Result<Vec<db::RemovedItem>> {
    db::removed_items(&state.db)
}
#[tauri::command]
async fn capture_items(
    input: content::Capture,
    state: tauri::State<'_, State>,
) -> Result<content::CaptureResult> {
    let s = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = s.content_lock.lock().map_err(err)?;
        content::capture(&s.db, input)
    })
    .await
    .map_err(err)?
}
#[tauri::command]
async fn capture_screen(ratio: f64, window: tauri::WebviewWindow, app: tauri::AppHandle, state: tauri::State<'_, State>) -> Result<Option<content::CaptureFile>> {
    if !ratio.is_finite() || !(ratio == 0.0 || (0.2..=5.0).contains(&ratio)) { return Err("Invalid capture ratio".into()); }
    let data = state.db.parent().unwrap().to_path_buf();
    window.hide().map_err(err)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(250));
        #[cfg(windows)] { thumbnail::capture(&data, ratio) }
        #[cfg(not(windows))] { let _ = data; Err::<Option<content::CaptureFile>,String>("Screen capture currently requires Windows.".into()) }
    }).await.map_err(err);
    let _ = window.show();
    let _ = window.set_focus();
    let file = result??;
    if let Some(file) = &file { app.asset_protocol_scope().allow_file(&file.path).map_err(err)?; }
    Ok(file)
}
#[tauri::command]
async fn crop_thumbnail(path: String, x: u32, y: u32, width: u32, height: u32, app: tauri::AppHandle, state: tauri::State<'_, State>) -> Result<content::CaptureFile> {
    let data = state.db.parent().unwrap().to_path_buf();
    let file = tauri::async_runtime::spawn_blocking(move || thumbnail::crop(&data,&path,x,y,width,height)).await.map_err(err)??;
    app.asset_protocol_scope().allow_file(&file.path).map_err(err)?;
    Ok(file)
}
#[tauri::command]
async fn replace_thumbnail(id: i64, file: content::CaptureFile, state: tauri::State<'_, State>) -> Result<()> {
    let s = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = s.content_lock.lock().map_err(err)?;
        thumbnail::replace(&s.db,id,&file)
    }).await.map_err(err)?
}
#[tauri::command]
async fn download_video(input: download::Request, on_progress: tauri::ipc::Channel<download::Progress>, app: tauri::AppHandle, state: tauri::State<'_, State>) -> Result<download::Saved> {
    let s=state.inner().clone();
    if s.downloading.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).is_err() { return Err("A video is already downloading.".into()); }
    s.download_cancel.store(false,Ordering::SeqCst);
    let active=s.downloading.clone();
    let result=tauri::async_runtime::spawn_blocking(move||download::run(&s.db,&s.downloader,&s.ffmpeg,input,&s.download_cancel,&mut |progress|{let _=on_progress.send(progress);})).await.map_err(err);
    active.store(false,Ordering::SeqCst);
    let saved=result??;
    let _=app.emit("library-changed",());
    Ok(saved)
}
#[tauri::command]
fn cancel_video_download(state: tauri::State<'_,State>) { state.download_cancel.store(true,Ordering::SeqCst); }
#[tauri::command]
async fn read_clipboard(app: tauri::AppHandle, state: tauri::State<'_, State>) -> Result<content::ClipboardContent> {
    let data = state.db.parent().unwrap().to_path_buf();
    let result = tauri::async_runtime::spawn_blocking(move || content::clipboard(&data))
        .await
        .map_err(err)??;
    // Grant preview access only to the files the user explicitly pasted.
    for file in &result.files {
        app.asset_protocol_scope().allow_file(&file.path).map_err(err)?;
    }
    Ok(result)
}
#[tauri::command]
async fn copy_image(id: i64, state: tauri::State<'_, State>) -> Result<()> {
    let s = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let image = clipboard::image_data(&s.db, &s.cache, &s.ffmpeg, id)?;
        arboard::Clipboard::new()
            .map_err(err)?
            .set_image(image)
            .map_err(err)
    })
    .await
    .map_err(err)?
}
#[tauri::command]
fn stage_begin(name: String, app: tauri::AppHandle, state: tauri::State<'_, State>) -> Result<content::CaptureFile> {
    let file = content::stage_begin(state.db.parent().unwrap(), &name)?;
    app.asset_protocol_scope().allow_file(&file.path).map_err(err)?;
    Ok(file)
}
#[tauri::command]
fn stage_chunk(request: tauri::ipc::Request<'_>, state: tauri::State<'_, State>) -> Result<()> {
    let token = request
        .headers()
        .get("x-capture-token")
        .and_then(|h| h.to_str().ok())
        .ok_or("Missing capture token")?;
    match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => {
            content::stage_chunk(state.db.parent().unwrap(), token, bytes)
        }
        _ => Err("Expected binary file data".into()),
    }
}
#[tauri::command]
fn discard_capture(paths: Vec<String>, state: tauri::State<'_, State>) -> Result<()> {
    let staging = state.db.parent().unwrap().join("staging");
    let Ok(staging) = staging.canonicalize() else {
        return Ok(());
    };
    for path in paths {
        if let Ok(path) = PathBuf::from(path).canonicalize() {
            if path.parent() == Some(staging.as_path()) && path.is_file() {
                std::fs::remove_file(path).map_err(err)?;
            }
        }
    }
    Ok(())
}
#[tauri::command]
async fn find_duplicates(
    on_progress: tauri::ipc::Channel<content::DuplicateProgress>,
    state: tauri::State<'_, State>,
) -> Result<content::DuplicateScan> {
    let s = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _ = on_progress.send(content::DuplicateProgress::waiting());
        let _guard = loop {
            match s.content_lock.try_lock() {
                Ok(guard) => break guard,
                Err(std::sync::TryLockError::Poisoned(e)) => return Err(err(e)),
                Err(std::sync::TryLockError::WouldBlock) => {
                    if let Some(progress) = s.index_progress.lock().map_err(err)?.clone() {
                        let _ = on_progress.send(progress);
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        };
        content::duplicates_with_progress(&s.db, &mut |progress| {
            let _ = on_progress.send(progress);
        })
    })
    .await
    .map_err(err)?
}
#[tauri::command]
async fn remove_duplicates(
    keeper: i64,
    ids: Vec<i64>,
    state: tauri::State<'_, State>,
) -> Result<usize> {
    let s = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = s.content_lock.lock().map_err(err)?;
        content::remove_duplicates(&s.db, s.db.parent().unwrap(), keeper, ids)
    })
    .await
    .map_err(err)?
}
#[tauri::command]
fn duplicate_recovery(state: tauri::State<'_, State>) -> Result<Vec<content::Recovery>> {
    content::recovery(&state.db)
}
#[tauri::command]
async fn restore_duplicate(id: i64, state: tauri::State<'_, State>) -> Result<()> {
    let s = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = s.content_lock.lock().map_err(err)?;
        content::restore(&s.db, id)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn open_external_url(url: String) -> Result<()> {
    browser::open(&url)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data = app.path().app_local_data_dir()?;
            #[cfg(debug_assertions)]
            let data = std::env::var_os("DESIGN_VIEWER_TEST_DATA")
                .map(PathBuf::from)
                .unwrap_or(data);
            std::fs::create_dir_all(&data)?;
            let db_path = data.join("library.sqlite");
            db::init(&db_path)?;
            let db = db::connect(&db_path)?;
            resources::seed(&db)?;
            if db::setting(&db, "root").is_none() {
                if let Some(root) = library::default_root() {
                    db::set_setting(&db, "root", &root.to_string_lossy())?;
                }
            }
            if db::setting(&db, "root").is_some() {
                sources::ensure(&db)?;
                for (_, root) in sources::roots(&db) {
                    app.asset_protocol_scope().allow_directory(root, true)?;
                }
            }
            // Restored references may live outside the current source folder.
            for item in db::load(&db_path, false)?.items {
                if item.kind != "link" {
                    let _ = app.asset_protocol_scope().allow_file(&item.path);
                }
            }
            let thumbnails = data.join("link-thumbnails");
            std::fs::create_dir_all(&thumbnails)?;
            app.asset_protocol_scope().allow_directory(&thumbnails, true)?;
            let cache = data.join("cache");
            std::fs::create_dir_all(&cache)?;
            app.asset_protocol_scope().allow_directory(&cache, true)?;
            let ffmpeg = app
                .path()
                .resource_dir()?
                .join("binaries")
                .join("ffmpeg.exe");
            let state = State {
                db: db_path,
                cache,
                downloader: app.path().resource_dir()?.join("binaries").join("yt-dlp.exe"),
                downloading: Arc::new(AtomicBool::new(false)),
                download_cancel: Arc::new(AtomicBool::new(false)),
                ffmpeg,
                scanning: Arc::new(AtomicBool::new(false)),
                media_slots: Arc::new(tokio::sync::Semaphore::new(3)),
                content_lock: Arc::new(std::sync::Mutex::new(())),
                index_progress: Arc::new(std::sync::Mutex::new(None)),
            };
            app.manage(state.clone());
            scan_background(app.handle().clone(), state.clone());
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let (tx, rx) = std::sync::mpsc::channel();
                let Ok(mut watcher) =
                    notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                        if let Ok(e) = event {
                            if matches!(
                                e.kind,
                                notify::EventKind::Create(_)
                                    | notify::EventKind::Modify(_)
                                    | notify::EventKind::Remove(_)
                            ) {
                                let _ = tx.send(());
                            }
                        }
                    })
                else {
                    return;
                };
                let mut watched = Vec::<PathBuf>::new();
                loop {
                    if let Ok(db) = db::connect(&state.db) {
                        let roots: Vec<PathBuf> = sources::roots(&db)
                            .into_iter()
                            .map(|(_, path)| path)
                            .collect();
                        for old in watched.iter().filter(|old| !roots.contains(old)) {
                            let _ = watcher.unwatch(old);
                        }
                        watched.retain(|old| roots.contains(old));
                        for root in roots {
                            if !watched.contains(&root)
                                && watcher
                                    .watch(&root, notify::RecursiveMode::Recursive)
                                    .is_ok()
                            {
                                watched.push(root);
                            }
                        }
                    }
                    if rx.recv_timeout(Duration::from_secs(2)).is_ok() {
                        // Collapse bursts of file-copy events; a quiet interval starts one scan.
                        while rx.recv_timeout(Duration::from_millis(700)).is_ok() {}
                        scan_background(handle.clone(), state.clone());
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_external_url,
            get_library,
            update,
            get_media,
            rescan,
            set_root,
            backup,
            data_folder,
            removed_items,
            capture_items,
            capture_screen,
            crop_thumbnail,
            replace_thumbnail,
            download_video,
            cancel_video_download,
            read_clipboard,
            copy_image,
            stage_begin,
            stage_chunk,
            discard_capture,
            find_duplicates,
            remove_duplicates,
            duplicate_recovery,
            restore_duplicate
        ])
        .run(tauri::generate_context!())
        .expect("Could not start Design");
}

#[cfg(test)]
mod tests;
