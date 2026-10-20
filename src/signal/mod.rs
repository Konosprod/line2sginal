//! Construction, export et envoi de packs de stickers Signal.

pub mod crypto;
pub mod export;
pub mod manifest;
pub mod upload;

use std::sync::Arc;

use crate::convert::MAX_BYTES;

pub const MAX_STICKERS: usize = 200;

#[derive(Debug, Clone)]
pub struct PackSticker {
    /// PNG ou APNG déjà converti.
    pub data: Arc<[u8]>,
    pub emoji: String,
}

#[derive(Debug, Clone)]
pub struct Pack {
    pub title: String,
    pub author: String,
    pub stickers: Vec<PackSticker>,
    /// Index dans `stickers` du sticker servant de couverture.
    pub cover: usize,
}

impl Pack {
    /// Liste les problèmes qui empêcheraient Signal d'accepter le pack.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.title.trim().is_empty() {
            problems.push("Le titre est vide.".to_string());
        }
        if self.author.trim().is_empty() {
            problems.push("L'auteur est vide.".to_string());
        }
        match self.stickers.len() {
            0 => problems.push("Aucun sticker sélectionné.".to_string()),
            n if n > MAX_STICKERS => problems.push(format!(
                "{n} stickers sélectionnés, Signal en accepte {MAX_STICKERS} au plus."
            )),
            _ => {}
        }
        for (i, s) in self.stickers.iter().enumerate() {
            if s.emoji.trim().is_empty() {
                problems.push(format!("Le sticker n°{} n'a pas d'emoji.", i + 1));
            }
            if s.data.len() > MAX_BYTES {
                problems.push(format!(
                    "Le sticker n°{} pèse {} Kio (limite : {} Kio).",
                    i + 1,
                    s.data.len() / 1024,
                    MAX_BYTES / 1024
                ));
            }
        }
        if self.cover >= self.stickers.len() && !self.stickers.is_empty() {
            problems.push("La couverture ne fait pas partie des stickers.".to_string());
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(n: usize) -> Pack {
        Pack {
            title: "Titre".into(),
            author: "Auteur".into(),
            stickers: (0..n)
                .map(|_| PackSticker {
                    data: vec![0; 10].into(),
                    emoji: "🙂".into(),
                })
                .collect(),
            cover: 0,
        }
    }

    #[test]
    fn valid_pack() {
        assert!(pack(3).validate().is_empty());
    }

    #[test]
    fn detects_problems() {
        let mut p = pack(MAX_STICKERS + 1);
        p.title = " ".into();
        p.stickers[1].emoji.clear();
        p.stickers[2].data = vec![0; MAX_BYTES + 1].into();
        assert_eq!(p.validate().len(), 4);
        assert_eq!(pack(0).validate().len(), 1);
    }
}
