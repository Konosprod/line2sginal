//! Emojis en couleur, lus dans la police Noto Color Emoji.
//!
//! egui ne dessine que des contours monochromes : on récupère donc nous-mêmes l'image PNG
//! du glyphe (table CBDT). rustybuzz assemble les séquences (tons de peau, ZWJ, drapeaux).

use std::cell::RefCell;
use std::collections::HashMap;

use eframe::egui::containers::menu::{MenuButton, MenuConfig};
use eframe::egui::{
    self, load::SizedTexture, ColorImage, PopupCloseBehavior, RichText, TextureFilter,
    TextureHandle, TextureOptions, Vec2,
};
use rustybuzz::ttf_parser::{GlyphId, RasterImageFormat};

use crate::theme;

const FONT: &[u8] = include_bytes!("../assets/fonts/NotoColorEmoji.ttf");
/// Côté des textures ; les emojis s'affichent au plus à 36 px.
const TEXTURE_SIDE: u32 = 72;

const CATEGORIES: &[(&str, &str)] = &[
    (
        "😀",
        "😀 😃 😄 😁 😆 😅 😂 🤣 🥲 😊 😇 🙂 🙃 😉 😌 😍 🥰 😘 😋 😛 😜 🤪 😝 🤗 🤭 🤫 🤔 \
         🫡 🤐 🤨 😐 😑 😶 😏 😒 🙄 😬 😮‍💨 😴 😪 🤤 😷 🤒 🤕 🤢 🤮 🥵 🥶 🥴 😵 🤯 🤠 🥳 😎 🤓 \
         🧐 😕 😟 🙁 😮 😯 😲 😳 🥺 🥹 😦 😨 😰 😥 😢 😭 😱 😖 😣 😞 😓 😩 😫 🥱 😤 😡 😠 🤬 \
         😈 💀 💩 🤡 👻 👽 🤖",
    ),
    (
        "👍",
        "👋 🤚 ✋ 🖖 👌 🤌 🤏 ✌️ 🤞 🫶 🤟 🤘 🤙 👈 👉 👆 👇 ☝️ 👍 👎 ✊ 👊 🤛 🤜 👏 🙌 👐 \
         🤲 🤝 🙏 💪 🫂 🙇 🤷 🤦 🙋 🙅 🙆 💁 🏃 💃 🕺",
    ),
    (
        "❤️",
        "❤️ 🧡 💛 💚 💙 💜 🤎 🖤 🤍 💔 ❣️ 💕 💞 💓 💗 💖 💘 💝 💋 💯 💢 💥 💫 💦 💨 💤 ✨ 🔥 \
         ⭐ 🌟",
    ),
    (
        "🐻",
        "🐶 🐱 🐭 🐹 🐰 🦊 🐻 🐼 🐨 🐯 🦁 🐮 🐷 🐸 🐵 🙈 🙉 🙊 🐔 🐧 🐦 🐤 🐥 🦆 🦉 🐺 🐴 🦄 \
         🐝 🐛 🦋 🐌 🐞 🐢 🐍 🐙 🐬 🐳 🦈 🐊 🦖 🐘 🦒 🐑 🦔",
    ),
    (
        "🍰",
        "🍎 🍓 🍒 🍑 🍍 🥝 🍉 🍌 🍋 🥑 🌽 🥕 🍞 🧀 🍳 🍔 🍟 🍕 🌭 🌮 🍣 🍜 🍙 🍰 🎂 🍩 🍪 🍫 \
         🍬 🍭 🍦 ☕ 🍵 🍺 🍻 🥂 🍷 🍹",
    ),
    (
        "🎉",
        "🎉 🎊 🎈 🎁 🎀 🏆 🥇 ⚽ 🏀 🎮 🎲 🎵 🎶 🎤 🎧 📷 📱 💻 ⏰ 💡 💰 💌 📚 ✏️ 📌 🔔 🚀 ✈️ \
         🚗 🏠 🌈 ☀️ 🌙 ⛄ 🌸 🌹 🌻 🍀",
    ),
    (
        "✅",
        "✅ ❌ ❓ ❗ ‼️ ⁉️ ⚠️ 🚫 🆗 🆒 🆕 🔝 ➡️ ⬅️ ⬆️ ⬇️ 🔄 ➕ ➖ ✖️ ➗ ♻️ 💲 🔴 🟢 🔵",
    ),
];

/// Glyphe unique qui représente `text` dans la police, s'il en existe un.
fn glyph(face: &rustybuzz::Face<'_>, text: &str) -> Option<GlyphId> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    let shaped = rustybuzz::shape(face, &[], buffer);
    match shaped.glyph_infos() {
        [info] if info.glyph_id != 0 => Some(GlyphId(info.glyph_id as u16)),
        _ => None,
    }
}

/// Image RGBA du glyphe couleur de `text`.
fn rasterize(face: &rustybuzz::Face<'_>, text: &str) -> Option<image::RgbaImage> {
    let raster = face.glyph_raster_image(glyph(face, text)?, u16::MAX)?;
    if raster.format != RasterImageFormat::PNG {
        return None;
    }
    let image = image::load_from_memory(raster.data).ok()?;
    Some(
        image
            .resize(
                TEXTURE_SIDE,
                TEXTURE_SIDE,
                image::imageops::FilterType::Lanczos3,
            )
            .to_rgba8(),
    )
}

pub struct Emojis {
    face: rustybuzz::Face<'static>,
    ctx: egui::Context,
    textures: RefCell<HashMap<String, Option<TextureHandle>>>,
}

impl Emojis {
    pub fn load(ctx: &egui::Context) -> Self {
        Self {
            face: rustybuzz::Face::from_slice(FONT, 0).expect("police emoji valide"),
            ctx: ctx.clone(),
            textures: RefCell::default(),
        }
    }

    /// Vrai si `text` est exactement un emoji connu de la police.
    pub fn is_emoji(&self, text: &str) -> bool {
        self.texture(text).is_some()
    }

    fn texture(&self, text: &str) -> Option<SizedTexture> {
        let key = text.trim();
        let mut textures = self.textures.borrow_mut();
        let texture = textures.entry(key.to_string()).or_insert_with(|| {
            let image = rasterize(&self.face, key)?;
            let size = [image.width() as usize, image.height() as usize];
            let image = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            let options = TextureOptions {
                mipmap_mode: Some(TextureFilter::Linear),
                ..TextureOptions::LINEAR
            };
            Some(
                self.ctx
                    .load_texture(format!("emoji-{key}"), image, options),
            )
        });
        texture.as_ref().map(SizedTexture::from_handle)
    }

    /// Image en couleur de `emoji`, si la police le connaît.
    pub fn image(&self, emoji: &str, size: f32) -> Option<egui::Image<'static>> {
        self.texture(emoji)
            .map(|t| egui::Image::new(t).fit_to_exact_size(Vec2::splat(size)))
    }

    /// Bouton affichant `value` ; un clic ouvre la palette.
    pub fn picker(&self, ui: &mut egui::Ui, value: &mut String, size: f32) {
        let button = match self.image(value, size) {
            Some(image) => egui::Button::image(image),
            None => egui::Button::new(RichText::new("?").size(size * 0.8).color(theme::RED)),
        };
        // Les menus se ferment par défaut au premier clic, même dans le champ « Autre emoji ».
        let config = MenuConfig::new().close_behavior(PopupCloseBehavior::CloseOnClickOutside);
        MenuButton::from_button(button)
            .config(config)
            .ui(ui, |ui| self.palette(ui, value));
    }

    fn palette(&self, ui: &mut egui::Ui, value: &mut String) {
        let tab_id = ui.id().with("emoji-tab");
        let mut tab = ui.data(|d| d.get_temp::<usize>(tab_id)).unwrap_or(0);
        ui.set_width(9.0 * 38.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, (icon, _)) in CATEGORIES.iter().enumerate() {
                if let Some(image) = self.image(icon, 22.0) {
                    if ui
                        .add(egui::Button::image(image).selected(i == tab))
                        .clicked()
                    {
                        tab = i;
                    }
                }
            }
        });
        ui.data_mut(|d| d.insert_temp(tab_id, tab));
        ui.separator();

        egui::ScrollArea::vertical()
            .max_height(220.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(4.0);
                    for emoji in CATEGORIES[tab].1.split_whitespace() {
                        let Some(image) = self.image(emoji, 28.0) else {
                            continue;
                        };
                        let selected = emoji == value.trim();
                        if ui.add(egui::Button::image(image).frame(selected)).clicked() {
                            *value = emoji.to_string();
                            ui.close();
                        }
                    }
                });
            });
        ui.separator();
        // Saisie libre : appliquée dès que le texte est un emoji reconnu.
        let custom_id = ui.id().with("emoji-custom");
        let mut custom = ui
            .data(|d| d.get_temp::<String>(custom_id))
            .unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label("Autre emoji :");
            let edit = ui.add(
                egui::TextEdit::singleline(&mut custom)
                    .hint_text("colle-le ici")
                    .desired_width(110.0),
            );
            if edit.changed() && self.is_emoji(&custom) {
                *value = custom.trim().to_string();
            }
            if !custom.trim().is_empty() {
                match self.image(&custom, 26.0) {
                    Some(image) => {
                        ui.add(image);
                    }
                    None => {
                        ui.label(RichText::new("pas un emoji").color(theme::RED).size(13.0));
                    }
                }
            }
        });
        ui.data_mut(|d| d.insert_temp(custom_id, custom));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> rustybuzz::Face<'static> {
        rustybuzz::Face::from_slice(FONT, 0).unwrap()
    }

    #[test]
    fn single_emojis_and_sequences() {
        let face = face();
        for emoji in ["🙂", "❤️", "❤", "👍🏽", "🏳️‍🌈", "🇫🇷", "😮‍💨", " 🐻 "]
        {
            assert!(glyph(&face, emoji).is_some(), "{emoji}");
            let image = rasterize(&face, emoji).unwrap();
            assert_eq!(image.width(), TEXTURE_SIDE);
        }
        for text in ["", "ab", "🙂🙂", "é"] {
            assert!(glyph(&face, text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn palette_is_fully_supported() {
        let face = face();
        for (icon, list) in CATEGORIES {
            assert!(glyph(&face, icon).is_some(), "{icon}");
            for emoji in list.split_whitespace() {
                assert!(glyph(&face, emoji).is_some(), "{emoji}");
            }
        }
    }
}
