use crate::db::{self, err, Result};
use image::ImageDecoder;
use std::{borrow::Cow, path::Path};

fn decode(path: &Path) -> Result<image::DynamicImage> {
    let mut decoder = image::ImageReader::open(path)
        .map_err(err)?
        .with_guessed_format()
        .map_err(err)?
        .into_decoder()
        .map_err(err)?;
    let orientation = decoder.orientation().map_err(err)?;
    let mut image = image::DynamicImage::from_decoder(decoder).map_err(err)?;
    image.apply_orientation(orientation);
    Ok(image)
}

// Kept separate from writing the system clipboard so tests never replace it.
pub fn image_data(
    db_path: &Path,
    cache: &Path,
    ffmpeg: &Path,
    id: i64,
) -> Result<arboard::ImageData<'static>> {
    let db = db::connect(db_path)?;
    let (path, kind): (String, String) = db
        .query_row(
            "SELECT path,kind FROM items WHERE id=? AND archived=0",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(err)?;
    if kind != "image" && kind != "animation" {
        return Err("Choose an image to copy.".into());
    }
    let image = match decode(Path::new(&path)) {
        Ok(image) => image,
        Err(_) => {
            std::fs::create_dir_all(cache).map_err(err)?;
            let temporary = cache.join(format!("clipboard-{}.png", crate::content::nonce()));
            let result = crate::library::run_ffmpeg(
                ffmpeg,
                &[
                    "-y",
                    "-i",
                    &path,
                    "-frames:v",
                    "1",
                    &temporary.to_string_lossy(),
                ],
                30,
            )
            .and_then(|_| decode(&temporary));
            let _ = std::fs::remove_file(&temporary);
            result?
        }
    }
    .into_rgba8();
    Ok(arboard::ImageData {
        width: image.width() as usize,
        height: image.height() as usize,
        bytes: Cow::Owned(image.into_raw()),
    })
}
