//! Récupération des packs de stickers LINE.

mod extract;
mod fetch;

pub use extract::read_zip;
pub use fetch::fetch_pack;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use anyhow::{bail, Result};
use regex::Regex;
use serde::Deserialize;

/// Contenu de `productInfo.meta`.
#[derive(Debug, Clone, Deserialize)]
pub struct Meta {
    #[serde(rename = "packageId")]
    pub package_id: u64,
    #[serde(default)]
    pub title: BTreeMap<String, String>,
    #[serde(default)]
    pub author: BTreeMap<String, String>,
    #[serde(default)]
    pub stickers: Vec<MetaSticker>,
    #[serde(rename = "hasAnimation", default)]
    pub has_animation: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetaSticker {
    pub id: u64,
}

#[derive(Debug, Clone)]
pub struct LineSticker {
    pub id: u64,
    /// PNG ou APNG d'origine.
    pub data: Vec<u8>,
    pub animated: bool,
}

#[derive(Debug, Clone)]
pub struct LinePack {
    pub meta: Meta,
    pub stickers: Vec<LineSticker>,
}

impl LinePack {
    /// Langues disponibles pour le titre, triées.
    pub fn languages(&self) -> Vec<String> {
        self.meta.title.keys().cloned().collect()
    }

    /// Langue par défaut : français, puis anglais, puis japonais, puis la première venue.
    pub fn default_language(&self) -> String {
        ["fr", "en", "ja"]
            .iter()
            .find(|l| self.meta.title.contains_key(**l))
            .map(|l| l.to_string())
            .or_else(|| self.meta.title.keys().next().cloned())
            .unwrap_or_else(|| "en".into())
    }

    pub fn title(&self, lang: &str) -> String {
        localized(&self.meta.title, lang)
    }

    pub fn author(&self, lang: &str) -> String {
        localized(&self.meta.author, lang)
    }
}

fn localized(map: &BTreeMap<String, String>, lang: &str) -> String {
    map.get(lang)
        .or_else(|| map.get("en"))
        .or_else(|| map.values().next())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Extrait l'ID du pack d'une saisie utilisateur : ID nu ou URL du store LINE.
pub fn parse_input(input: &str) -> Result<u64> {
    static URL_RE: OnceLock<Regex> = OnceLock::new();
    let input = input.trim();
    if let Ok(id) = input.parse() {
        return Ok(id);
    }
    let re = URL_RE.get_or_init(|| {
        Regex::new(r"(?:stickershop/product|S/sticker|products/\d+/\d+/\d+|/product)/(\d+)")
            .expect("regex valide")
    });
    match re.captures(input) {
        Some(c) => Ok(c[1].parse()?),
        None => bail!("« {input} » n'est ni un ID ni une URL de pack LINE"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bare_id() {
        assert_eq!(parse_input(" 11537 ").unwrap(), 11537);
    }

    #[test]
    fn parse_store_urls() {
        for url in [
            "https://store.line.me/stickershop/product/11537/fr",
            "https://store.line.me/stickershop/product/11537",
            "https://line.me/S/sticker/11537/?lang=fr&ref=lsh_stickerDetail",
            "http://dl.stickershop.line.naver.jp/products/0/0/1/11537/iphone/stickers@2x.zip",
        ] {
            assert_eq!(parse_input(url).unwrap(), 11537, "{url}");
        }
    }

    #[test]
    fn parse_invalid() {
        assert!(parse_input("https://example.com/foo").is_err());
        assert!(parse_input("").is_err());
    }

    #[test]
    fn localized_fallbacks() {
        let map: BTreeMap<_, _> = [
            ("en".to_string(), "Hello".to_string()),
            ("ja".into(), "こんにちは".into()),
        ]
        .into();
        assert_eq!(localized(&map, "ja"), "こんにちは");
        assert_eq!(localized(&map, "fr"), "Hello");
        assert_eq!(localized(&BTreeMap::new(), "fr"), "");
    }
}
