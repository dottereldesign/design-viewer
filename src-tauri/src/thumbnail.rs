use crate::{content, db::{self, err, Result}};
use std::path::Path;
use rusqlite::params;

pub fn crop(data: &Path, path: &str, x: u32, y: u32, width: u32, height: u32) -> Result<content::CaptureFile> {
    let mut decoder = image::ImageReader::open(path).map_err(err)?.with_guessed_format().map_err(err)?.into_decoder().map_err(err)?;
    use image::ImageDecoder;
    let orientation = decoder.orientation().map_err(err)?;
    let mut image = image::DynamicImage::from_decoder(decoder).map_err(err)?;
    image.apply_orientation(orientation);
    if width == 0 || height == 0 || x.checked_add(width).is_none_or(|n| n > image.width()) || y.checked_add(height).is_none_or(|n| n > image.height()) {
        return Err("Choose an area inside the image.".into());
    }
    let file = content::stage_begin(data, "Thumbnail.png")?;
    if let Err(error) = image.crop_imm(x, y, width, height).thumbnail(width.min(1600),height.min(1600)).save(&file.path) {
        let _ = std::fs::remove_file(&file.path); return Err(err(error));
    }
    Ok(file)
}

pub fn replace(db_path: &Path, id: i64, file: &content::CaptureFile) -> Result<()> {
    let db = db::connect(db_path)?;
    let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM items WHERE id=? AND kind='link' AND archived=0)", [id], |r| r.get(0)).map_err(err)?;
    if !exists { return Err("This link is no longer in the library.".into()); }
    let image = image::open(&file.path).map_err(err)?;
    let directory = db_path.parent().unwrap().join("link-thumbnails");
    std::fs::create_dir_all(&directory).map_err(err)?;
    let path = directory.join(format!("{}.png", content::nonce()));
    image.thumbnail(image.width().min(1600),image.height().min(1600)).save(&path).map_err(err)?;
    if let Err(error) = db.execute("UPDATE items SET thumbnail=? WHERE id=? AND kind='link' AND archived=0", params![path.to_string_lossy(),id]) {
        let _ = std::fs::remove_file(path); return Err(err(error));
    }
    Ok(())
}

#[cfg(windows)]
pub fn capture(data: &Path, ratio: f64) -> Result<Option<content::CaptureFile>> {
    use std::os::windows::process::CommandExt;
    let file = content::stage_begin(data, "Screenshot.png")?;
    let script = data.join(format!("capture-{}.ps1",content::nonce()));
    std::fs::write(&script, include_str!("screen-capture.ps1")).map_err(err)?;
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-STA", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script).arg("-OutputPath").arg(&file.path).arg("-Ratio").arg(ratio.to_string())
        .creation_flags(0x08000000).output().map_err(err);
    let _ = std::fs::remove_file(script);
    let result = (|| {
        let output = output?;
        if !output.status.success() { return Err(format!("Screen capture failed: {}", String::from_utf8_lossy(&output.stderr))); }
        if std::fs::metadata(&file.path).map(|m| m.len()).unwrap_or(0) == 0 { return Ok(None); }
        image::image_dimensions(&file.path).map_err(err)?;
        Ok(Some(file.clone()))
    })();
    if !matches!(result, Ok(Some(_))) { let _ = std::fs::remove_file(&file.path); }
    result
}
