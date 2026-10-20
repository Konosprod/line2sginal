//! Manifest protobuf du pack (`protos/Stickers.proto` de Signal Desktop).

use prost::Message;

use super::Pack;

#[derive(Clone, PartialEq, Message)]
pub struct StickerPack {
    #[prost(string, optional, tag = "1")]
    pub title: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub author: Option<String>,
    #[prost(message, optional, tag = "3")]
    pub cover: Option<Sticker>,
    #[prost(message, repeated, tag = "4")]
    pub stickers: Vec<Sticker>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Sticker {
    #[prost(uint32, optional, tag = "1")]
    pub id: Option<u32>,
    #[prost(string, optional, tag = "2")]
    pub emoji: Option<String>,
}

/// Les identifiants de stickers sont leurs positions dans l'ordre d'envoi.
pub fn build(pack: &Pack) -> StickerPack {
    let sticker = |i: usize| Sticker {
        id: Some(i as u32),
        emoji: Some(pack.stickers[i].emoji.trim().to_string()),
    };
    StickerPack {
        title: Some(pack.title.trim().to_string()),
        author: Some(pack.author.trim().to_string()),
        cover: Some(sticker(pack.cover)),
        stickers: (0..pack.stickers.len()).map(sticker).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::PackSticker;

    #[test]
    fn encodes_and_decodes() {
        let pack = Pack {
            title: " Mon pack ".into(),
            author: "Moi".into(),
            stickers: ["😀", "🐻"]
                .iter()
                .map(|e| PackSticker {
                    data: Vec::new().into(),
                    emoji: e.to_string(),
                })
                .collect(),
            cover: 1,
        };
        let bytes = build(&pack).encode_to_vec();
        let decoded = StickerPack::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.title.as_deref(), Some("Mon pack"));
        assert_eq!(decoded.stickers.len(), 2);
        assert_eq!(decoded.stickers[1].emoji.as_deref(), Some("🐻"));
        assert_eq!(decoded.cover.unwrap().id, Some(1));
    }
}
