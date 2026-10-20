use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use reqwest::StatusCode;

use super::{read_zip, LinePack, Meta};

/// Serveurs essayés dans l'ordre. Le premier ne répond correctement qu'en HTTP
/// (son certificat ne couvre pas ce nom d'hôte).
const MIRRORS: [&str; 2] = [
    "http://dl.stickershop.line.naver.jp/products/0/0/1/{id}/iphone/",
    "https://stickershop.line-scdn.net/stickershop/v1/product/{id}/iphone/",
];

/// Télécharge (ou relit depuis le cache) le pack `id`.
///
/// `progress` reçoit un message et, si connue, la fraction téléchargée.
pub fn fetch_pack(id: u64, progress: &dyn Fn(&str, Option<f32>)) -> Result<LinePack> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent(concat!("line2signal/", env!("CARGO_PKG_VERSION")))
        .build()?;

    progress("Lecture des informations du pack…", None);
    let meta = cached(id, "productInfo.meta", || {
        download(&client, id, "productInfo.meta", &|_| {})
    })?;
    let meta: Meta = serde_json::from_slice(&meta).context("productInfo.meta invalide")?;

    let file = if meta.has_animation {
        "stickerpack@2x.zip"
    } else {
        "stickers@2x.zip"
    };
    let label = format!("Téléchargement de {file}…");
    let zip = cached(id, file, || {
        download(&client, id, file, &|fraction| progress(&label, fraction))
    })?;
    progress("Lecture de l'archive…", None);
    read_zip(&zip)
}

fn download(
    client: &Client,
    id: u64,
    file: &str,
    progress: &dyn Fn(Option<f32>),
) -> Result<Vec<u8>> {
    let mut errors = Vec::new();
    for base in MIRRORS {
        let url = format!("{}{file}", base.replace("{id}", &id.to_string()));
        match download_url(client, &url, progress) {
            Ok(bytes) => return Ok(bytes),
            Err(e) => errors.push(e),
        }
    }
    let not_found = errors.iter().all(|e| {
        e.downcast_ref::<reqwest::Error>()
            .and_then(|e| e.status())
            .is_some_and(|s| s == StatusCode::NOT_FOUND || s == StatusCode::FORBIDDEN)
    });
    if not_found {
        return Err(anyhow!("le pack LINE {id} est introuvable"));
    }
    let details: Vec<_> = errors.iter().map(|e| format!("{e:#}")).collect();
    Err(anyhow!(
        "téléchargement de {file} impossible : {}",
        details.join(" ; ")
    ))
}

fn download_url(client: &Client, url: &str, progress: &dyn Fn(Option<f32>)) -> Result<Vec<u8>> {
    let mut resp = client.get(url).send()?.error_for_status()?;
    let total = resp.content_length();
    let mut buf = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let n = resp.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        progress(total.map(|t| buf.len() as f32 / t as f32));
    }
    Ok(buf)
}

fn cache_path(id: u64, file: &str) -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "line2signal")?;
    Some(dirs.cache_dir().join(id.to_string()).join(file))
}

/// Relit `file` depuis le cache disque, sinon le télécharge et l'y enregistre.
fn cached(id: u64, file: &str, fetch: impl FnOnce() -> Result<Vec<u8>>) -> Result<Vec<u8>> {
    let path = cache_path(id, file);
    if let Some(bytes) = path.as_ref().and_then(|p| fs::read(p).ok()) {
        return Ok(bytes);
    }
    let bytes = fetch()?;
    if let Some(path) = path {
        // Le cache est facultatif : une erreur d'écriture n'empêche pas l'import.
        let _ = path
            .parent()
            .map(fs::create_dir_all)
            .transpose()
            .and_then(|_| fs::write(&path, &bytes));
    }
    Ok(bytes)
}
