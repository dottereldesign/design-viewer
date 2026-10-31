use crate::db::{self, err, Backup, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs, io::{Read, Write}, path::{Path, PathBuf}, sync::atomic::{AtomicBool, Ordering}, time::{Duration, Instant}};

#[derive(Clone, Serialize)]
pub struct Progress {
    pub phase: &'static str,
    pub copied_bytes: u64,
    pub total_bytes: u64,
    pub completed_files: usize,
    pub total_files: usize,
    pub current: String,
}
#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub path: String,
    pub files: usize,
    pub links: usize,
    pub missing: Vec<String>,
    pub cancelled: bool,
}
#[derive(Serialize)]
struct Entry { path: String, bytes: u64, sha256: String }
struct CopyJob { source: PathBuf, relative: String, size: u64 }

fn safe_name(name: &str) -> String {
    let value: String = name.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) {'_'} else {c}).take(110).collect();
    let value=value.trim().trim_matches('.');
    if value.is_empty() { "file".into() } else {value.to_owned()}
}
fn escape(value: &str) -> String { value.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;").replace('\'',"&#39;") }
fn csv(value: &str) -> String {
    let value=if value.starts_with(['=', '+', '-', '@', '\t', '\r', '\n']) {format!("'{value}")} else {value.to_owned()};
    format!("\"{}\"",value.replace('"',"\"\""))
}

// Portable backups resolve relative paths against the JSON's own directory,
// never the working directory. Reject traversal and links outside the bundle.
pub fn resolve_relative(base: &Path, value: &str) -> Result<String> {
    let path=Path::new(value);
    if path.is_absolute() || path.components().any(|p| !matches!(p,std::path::Component::Normal(_))) {
        return Err("Invalid path in portable backup.".into());
    }
    let joined=base.join(path);
    if joined.exists() && !joined.canonicalize().map_err(err)?.starts_with(base.canonicalize().map_err(err)?) {
        return Err("A portable backup file points outside its folder.".into());
    }
    Ok(joined.to_string_lossy().into_owned())
}

pub fn export(db_path: &Path, destination: &Path, name: &str, cancel: &AtomicBool, progress: &mut impl FnMut(Progress)) -> Result<ExportResult> {
    let destination=destination.canonicalize().map_err(err)?;
    if !destination.is_dir() {return Err("Choose a backup folder.".into());}
    let conn=db::connect(db_path)?;
    for (_,root) in crate::sources::roots(&conn) {
        if let Ok(root)=root.canonicalize() {if destination.starts_with(root) {return Err("Choose a backup location outside your media folders.".into());}}
    }
    if destination.starts_with(db_path.parent().ok_or("Missing data folder")?.canonicalize().map_err(err)?) {
        return Err("Choose a backup location outside Design's app data.".into());
    }
    let output=destination.join(format!("{}-{}",safe_name(name),db::now()));
    fs::create_dir(&output).map_err(err)?;
    fs::write(output.join("INCOMPLETE.txt"),"This backup did not finish. Keep your originals. A complete backup has library.json and manifest.json.\n").map_err(err)?;
    let result=run(&conn,&output,cancel,progress);
    result.map_err(|e|format!("{e}\nPartial backup: {}",output.display()))
}

fn run(conn: &rusqlite::Connection, output: &Path, cancel: &AtomicBool, progress: &mut impl FnMut(Progress)) -> Result<ExportResult> {
    let mut status=Progress{phase:"preparing",copied_bytes:0,total_bytes:0,completed_files:0,total_files:0,current:String::new()};
    progress(status.clone());
    // SQLite's backup API takes a consistent snapshot, including live WAL data.
    let snapshot=output.join("metadata.snapshot.sqlite");
    conn.backup("main",&snapshot,None).map_err(err)?;
    let mut library=db::load(&snapshot,false)?;
    for suffix in ["", "-wal", "-shm"] {let path=output.join(format!("metadata.snapshot.sqlite{suffix}"));if path.exists(){fs::remove_file(path).map_err(err)?;}}
    let mut jobs=vec![];
    let mut missing=vec![];
    let mut links=0;
    for item in &mut library.items {
        if cancel.load(Ordering::SeqCst) {return Ok(ExportResult{path:output.to_string_lossy().into_owned(),files:0,links,missing,cancelled:true});}
        if item.kind=="link" {links+=1;} else {
            let source=PathBuf::from(&item.path);
            let group=match item.source.split('/').next().unwrap_or("") {"Photography"=>"Photography","Videos"=>"Videos",_=>"Design"};
            let file_name=safe_name(source.file_name().and_then(|s|s.to_str()).unwrap_or(&item.name));
            let relative=format!("Media/{group}/{:08}-{file_name}",item.id);
            match fs::metadata(&source) {
                Ok(meta) if meta.is_file()=>{jobs.push(CopyJob{source,relative:relative.clone(),size:meta.len()});item.missing=false;},
                _=>{missing.push(format!("{} — original unavailable",item.name));item.missing=true;}
            }
            item.path=relative;
        }
        if !item.thumbnail.is_empty() {
            let source=PathBuf::from(&item.thumbnail);
            let file_name=safe_name(source.file_name().and_then(|s|s.to_str()).unwrap_or("thumbnail.png"));
            let relative=format!("Thumbnails/{:08}-{file_name}",item.id);
            match fs::metadata(&source) {
                Ok(meta) if meta.is_file()=>jobs.push(CopyJob{source,relative:relative.clone(),size:meta.len()}),
                _=>missing.push(format!("{} — thumbnail unavailable",item.name)),
            }
            item.thumbnail=relative;
        }
    }
    for group in ["Design","Photography","Videos"] {fs::create_dir_all(output.join("Media").join(group)).map_err(err)?;}
    library.root="Media/Design".into();
    library.source_paths=["Design","Photography","Videos"].into_iter().map(|name|(name.to_owned(),format!("Media/{name}"))).collect();
    library.undo_label=None;library.undo_count=0;library.redo_label=None;library.redo_count=0;library.scanning=false;
    status.total_files=jobs.len();status.total_bytes=jobs.iter().map(|job|job.size).sum();status.phase="copying";
    progress(status.clone());
    let mut entries=vec![];
    let mut last=Instant::now();
    let mut buffer=vec![0u8;1024*1024];
    for job in jobs {
        status.current=job.source.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let target=output.join(&job.relative);
        fs::create_dir_all(target.parent().unwrap()).map_err(err)?;
        let temporary=target.with_extension("part");
        let mut source=fs::File::open(&job.source).map_err(|e|format!("Could not read {}: {e}",status.current))?;
        let before=source.metadata().map_err(err)?;
        let mut writer=fs::OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(err)?;
        let mut hash=Sha256::new();
        let mut bytes=0;
        loop {
            if cancel.load(Ordering::SeqCst) {
                drop(writer);fs::remove_file(&temporary).map_err(err)?;
                status.phase="cancelled";progress(status.clone());
                return Ok(ExportResult{path:output.to_string_lossy().into_owned(),files:status.completed_files,links,missing,cancelled:true});
            }
            let read=source.read(&mut buffer).map_err(err)?;
            if read==0 {break;}
            writer.write_all(&buffer[..read]).map_err(err)?;hash.update(&buffer[..read]);
            bytes+=read as u64;status.copied_bytes+=read as u64;
            if last.elapsed()>=Duration::from_millis(100) {progress(status.clone());last=Instant::now();}
        }
        let after=source.metadata().map_err(err)?;
        if bytes!=job.size || before.len()!=after.len() || before.modified().ok()!=after.modified().ok() {return Err(format!("{} changed during export. Try again.",status.current));}
        writer.sync_all().map_err(err)?;drop(writer);
        fs::rename(&temporary,&target).map_err(err)?;
        entries.push(Entry{path:job.relative,bytes,sha256:format!("{:x}",hash.finalize())});
        status.completed_files+=1;progress(status.clone());
    }
    if cancel.load(Ordering::SeqCst) {
        status.phase="cancelled";progress(status.clone());
        return Ok(ExportResult{path:output.to_string_lossy().into_owned(),files:status.completed_files,links,missing,cancelled:true});
    }
    status.phase="finishing";status.current.clear();progress(status.clone());
    let mut html=String::from("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Design saved links</title><style>body{max-width:900px;margin:40px auto;padding:0 20px;font:16px system-ui;line-height:1.6}li{margin:14px 0}small{color:#666}</style><h1>Saved links</h1><p>All saved links from your Design library.</p><ul>");
    let mut table=String::from("Name,Collection,URL,Source URL\r\n");
    for item in library.items.iter().filter(|i|i.kind=="link") {
        let name=escape(&item.name);let url=escape(&item.path);let collection=escape(&item.source);
        if db::valid_url(&item.path).is_ok() {html.push_str(&format!("<li><a href=\"{url}\">{name}</a><br><small>{collection} · {url}</small></li>"));}
        else {html.push_str(&format!("<li>{name}<br><small>{url}</small></li>"));}
        table.push_str(&format!("{},{},{},{}\r\n",csv(&item.name),csv(&item.source),csv(&item.path),csv(&item.source_url)));
    }
    html.push_str("</ul></html>");
    fs::write(output.join("links.html"),html).map_err(err)?;
    fs::write(output.join("links.csv"),table).map_err(err)?;
    fs::write(output.join("manifest.json"),serde_json::to_vec_pretty(&serde_json::json!({"version":1,"files":entries,"missing":missing,"links":links})).map_err(err)?).map_err(err)?;
    fs::write(output.join("README.txt"),"DESIGN BACKUP\n\nMedia/ contains your original images, animations and videos.\nThumbnails/ contains saved link thumbnails.\nOpen links.html in any browser, or links.csv in a spreadsheet.\nlibrary.json retains projects, tags, categories and pins.\nmanifest.json lists SHA-256 checksums and unavailable files.\n\nCopy this entire folder to your external drive or another PC.\nTo restore in Design, keep the folder in its permanent location, then use Settings > Import metadata and choose library.json. Media stays in this folder.\n\nA backup with INCOMPLETE.txt did not finish. Review missing entries in manifest.json before deleting any originals.\n").map_err(err)?;
    fs::write(output.join("library.json.part"),serde_json::to_vec_pretty(&Backup{version:1,portable:true,library}).map_err(err)?).map_err(err)?;
    fs::rename(output.join("library.json.part"),output.join("library.json")).map_err(err)?;
    fs::remove_file(output.join("INCOMPLETE.txt")).map_err(err)?;
    status.phase="complete";progress(status);
    Ok(ExportResult{path:output.to_string_lossy().into_owned(),files:entries.len(),links,missing,cancelled:false})
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn fixture() -> (tempfile::TempDir,PathBuf,PathBuf) {
        let temp=tempfile::tempdir().unwrap();
        let data=temp.path().join("data");fs::create_dir(&data).unwrap();
        let media=temp.path().join("media");fs::create_dir_all(media.join("Design")).unwrap();
        let destination=temp.path().join("exports");fs::create_dir(&destination).unwrap();
        let path=data.join("library.sqlite");db::init(&path).unwrap();let conn=db::connect(&path).unwrap();
        db::set_setting(&conn,"root",media.join("Design").to_str().unwrap()).unwrap();crate::sources::ensure(&conn).unwrap();
        for (id,group,kind) in [(1,"Design","image"),(2,"Photography","animation"),(3,"Videos","video")] {
            let source=media.join(group).join("same-name.dat");fs::write(&source,format!("original bytes {id}")).unwrap();
            conn.execute("INSERT INTO items(id,path,name,kind,source,added) VALUES(?,?,?,?,?,?)",params![id,source.to_str().unwrap(),format!("Reference {id}"),kind,group,id]).unwrap();
        }
        let thumbnail=data.join("link-thumbnail.png");fs::write(&thumbnail,b"thumbnail bytes").unwrap();
        conn.execute("INSERT INTO items(id,path,name,kind,source,added,thumbnail) VALUES(4,'https://example.com/?a=1&b=2','=SUM(1,2) <script>','link','Resources/Photography',4,?)",[thumbnail.to_str().unwrap()]).unwrap();
        conn.execute("INSERT INTO items(id,path,name,kind,source,added,missing) VALUES(5,?,'Missing reference','image','Design',5,1)",[media.join("Design/missing.png").to_str().unwrap()]).unwrap();
        conn.execute_batch("INSERT INTO projects(id,name) VALUES(7,'Keep project');INSERT INTO item_projects(item_id,label_id,favourite) VALUES(1,7,1);INSERT INTO item_tags(item_id,label_id) VALUES(1,1);INSERT INTO categories(name) VALUES('My category');INSERT INTO item_categories(item_id,name) VALUES(1,'My category');").unwrap();
        (temp,path,destination)
    }
    #[test]
    fn library_export_is_portable_verified_and_restores_after_folder_moves() {
        let (temp,path,destination)=fixture();let mut events=vec![];
        let result=export(&path,&destination,"Design Backup",&AtomicBool::new(false),&mut |p|events.push(p)).unwrap();
        assert_eq!(result.files,4);assert_eq!(result.links,1);assert_eq!(result.missing.len(),1);assert!(!result.cancelled);
        let root=PathBuf::from(&result.path);assert!(!root.join("INCOMPLETE.txt").exists());
        let manifest:serde_json::Value=serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
        for entry in manifest["files"].as_array().unwrap() {
            let data=fs::read(root.join(entry["path"].as_str().unwrap())).unwrap();
            assert_eq!(format!("{:x}",Sha256::digest(&data)),entry["sha256"].as_str().unwrap());
        }
        assert!(fs::read_to_string(root.join("links.html")).unwrap().contains("&lt;script&gt;"));
        assert!(!fs::read_to_string(root.join("links.html")).unwrap().contains("<script>"));
        assert!(fs::read_to_string(root.join("links.csv")).unwrap().contains("\"'=SUM"));
        assert_eq!(events.last().unwrap().phase,"complete");assert_eq!(events.last().unwrap().copied_bytes,events.last().unwrap().total_bytes);
        let moved=temp.path().join("moved backup");fs::rename(&root,&moved).unwrap();
        let restored=temp.path().join("restore");fs::create_dir(&restored).unwrap();let restored=restored.join("library.sqlite");db::init(&restored).unwrap();
        db::connect(&restored).unwrap().execute("INSERT INTO items(path,name,kind,source,added) VALUES('https://example.com/?a=1&b=2','Pre-seeded name','link','Resources',0)",[]).unwrap();
        db::import(&restored,moved.join("library.json")).unwrap();
        let lib=db::load(&restored,false).unwrap();assert_eq!(lib.items.len(),5);
        let first=lib.items.iter().find(|i|i.name=="Reference 1").unwrap();
        assert_eq!(fs::read(&first.path).unwrap(),b"original bytes 1");assert!(Path::new(&first.path).starts_with(moved.canonicalize().unwrap()));
        assert_eq!(first.category.as_deref(),Some("My category"));assert_eq!(first.projects.len(),1);assert_eq!(first.tags.len(),1);assert_eq!(first.favourites.len(),1);
        let link=lib.items.iter().find(|i|i.kind=="link").unwrap();assert_eq!(link.name,"=SUM(1,2) <script>");assert_eq!(link.source,"Resources/Photography");assert!(Path::new(&link.thumbnail).is_file());
        assert!(lib.items.iter().find(|i|i.name=="Missing reference").unwrap().missing);
        assert!(Path::new(&lib.root).starts_with(moved.canonicalize().unwrap()));
        let another=export(&path,&destination,"Design Backup",&AtomicBool::new(false),&mut |_|{}).unwrap();assert_ne!(another.path,result.path);
    }
    #[test]
    fn library_export_cancel_keeps_originals_and_marks_partial_folder() {
        let (_temp,path,destination)=fixture();let cancel=AtomicBool::new(false);
        let result=export(&path,&destination,"Cancelled",&cancel,&mut |p|{if p.phase=="copying" {cancel.store(true,Ordering::SeqCst);}}).unwrap();
        assert!(result.cancelled);assert!(Path::new(&result.path).join("INCOMPLETE.txt").exists());assert!(!Path::new(&result.path).join("library.json").exists());
        for item in db::load(&path,false).unwrap().items.iter().filter(|i|i.kind!="link"&&!i.missing) {assert!(Path::new(&item.path).exists());}
    }
    #[test]
    fn library_export_rejects_nested_destinations_and_portable_path_traversal() {
        let (temp,path,_destination)=fixture();
        assert!(export(&path,&temp.path().join("media/Design"),"Bad",&AtomicBool::new(false),&mut |_|{}).is_err());
        assert!(resolve_relative(temp.path(),"../escape.png").is_err());assert!(resolve_relative(temp.path(),temp.path().to_str().unwrap()).is_err());
    }
    #[test]
    fn library_export_detects_source_changes_and_never_marks_failed_copy_complete() {
        let (temp,path,destination)=fixture();let source=temp.path().join("media/Design/same-name.dat");let mut changed=false;
        let result=export(&path,&destination,"Changed",&AtomicBool::new(false),&mut |p| {if p.phase=="copying"&&!changed {changed=true;fs::write(&source,b"new bytes of a different length").unwrap();}});
        assert!(result.unwrap_err().contains("changed during export"));
        let root=fs::read_dir(&destination).unwrap().next().unwrap().unwrap().path();assert!(root.join("INCOMPLETE.txt").exists());assert!(!root.join("library.json").exists());
    }
}
