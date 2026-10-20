use std::collections::HashMap;
use std::io::{Cursor, Read};

use anyhow::{bail, Context, Result};
use zip::ZipArchive;

use super::{LinePack, LineSticker, Meta};
use crate::convert::is_apng;

/// Lit un zip LINE (`stickers@2x.zip` ou `stickerpack@2x.zip`).
///
/// Les stickers sont rendus dans l'ordre de `productInfo.meta`. La version animée
/// (`animation@2x/{id}@2x.png`) est préférée à la version fixe (`{id}@2x.png`).
pub fn read_zip(bytes: &[u8]) -> Result<LinePack> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).context("archive zip invalide")?;
    let mut files = HashMap::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        if file.is_dir() {
            continue;
        }
        let mut buf = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut buf)?;
        files.insert(file.name().to_string(), buf);
    }

    let meta: Meta = serde_json::from_slice(
        files
            .get("productInfo.meta")
            .context("productInfo.meta absent du zip")?,
    )
    .context("productInfo.meta invalide")?;

    let mut stickers = Vec::new();
    for s in &meta.stickers {
        let data = files
            .remove(&format!("animation@2x/{}@2x.png", s.id))
            .or_else(|| files.remove(&format!("{}@2x.png", s.id)));
        if let Some(data) = data {
            stickers.push(LineSticker {
                id: s.id,
                animated: is_apng(&data),
                data,
            });
        }
    }
    if stickers.is_empty() {
        bail!("aucun sticker trouvé dans le zip");
    }
    Ok(LinePack { meta, stickers })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn png_bytes() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn reads_stickers_in_meta_order() {
        let meta = r#"{"packageId":42,"title":{"en":"Test"},"author":{"en":"Me"},
            "stickers":[{"id":2},{"id":1},{"id":3}],"hasAnimation":false}"#;
        let mut out = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut out);
            let opts = SimpleFileOptions::default();
            zip.start_file("productInfo.meta", opts).unwrap();
            zip.write_all(meta.as_bytes()).unwrap();
            for name in ["1@2x.png", "2@2x.png", "1_key@2x.png", "tab_on@2x.png"] {
                zip.start_file(name, opts).unwrap();
                zip.write_all(&png_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        let pack = read_zip(out.get_ref()).unwrap();
        assert_eq!(pack.meta.package_id, 42);
        let ids: Vec<_> = pack.stickers.iter().map(|s| s.id).collect();
        assert_eq!(ids, [2, 1]);
        assert!(!pack.stickers[0].animated);
    }
}
