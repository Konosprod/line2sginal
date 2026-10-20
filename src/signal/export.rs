//! Export d'un dossier à importer avec le créateur de packs de Signal Desktop.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::json;

use super::Pack;

/// Écrit les stickers numérotés, `cover.png` et un `pack.json` récapitulatif dans un
/// sous-dossier de `dir` nommé d'après le titre du pack. Renvoie ce sous-dossier.
pub fn export_dir(pack: &Pack, dir: &Path) -> Result<PathBuf> {
    let folder = dir.join(folder_name(&pack.title));
    fs::create_dir_all(&folder).with_context(|| format!("création de {}", folder.display()))?;

    let mut entries = Vec::new();
    for (i, sticker) in pack.stickers.iter().enumerate() {
        let file = format!("{:03}.png", i + 1);
        fs::write(folder.join(&file), &sticker.data)?;
        entries.push(json!({ "file": file, "emoji": sticker.emoji.trim() }));
    }
    fs::write(folder.join("cover.png"), &pack.stickers[pack.cover].data)?;

    let summary = json!({
        "title": pack.title.trim(),
        "author": pack.author.trim(),
        "cover": "cover.png",
        "stickers": entries,
    });
    fs::write(
        folder.join("pack.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    Ok(folder)
}

fn folder_name(title: &str) -> String {
    let name: String = title
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let name = name.trim_end_matches(['.', ' ']);
    if name.is_empty() {
        "pack-signal".to_string()
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::PackSticker;

    #[test]
    fn sanitizes_folder_name() {
        assert_eq!(
            folder_name("Brown & Cony: Special?"),
            "Brown & Cony_ Special_"
        );
        assert_eq!(folder_name(" ... "), "pack-signal");
    }

    #[test]
    fn writes_files() {
        let dir = std::env::temp_dir().join(format!("line2signal-test-{}", std::process::id()));
        let pack = Pack {
            title: "Test".into(),
            author: "Moi".into(),
            stickers: vec![
                PackSticker {
                    data: vec![1].into(),
                    emoji: "😀".into(),
                },
                PackSticker {
                    data: vec![2].into(),
                    emoji: "🐻".into(),
                },
            ],
            cover: 1,
        };
        let folder = export_dir(&pack, &dir).unwrap();
        assert_eq!(fs::read(folder.join("002.png")).unwrap(), [2]);
        assert_eq!(fs::read(folder.join("cover.png")).unwrap(), [2]);
        let summary: serde_json::Value =
            serde_json::from_slice(&fs::read(folder.join("pack.json")).unwrap()).unwrap();
        assert_eq!(summary["stickers"][0]["emoji"], "😀");
        fs::remove_dir_all(&dir).unwrap();
    }
}
