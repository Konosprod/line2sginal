//! Thème clair et coloré, polices et petits composants visuels réutilisables.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText,
    Shadow, Stroke, TextStyle, Theme, Vec2,
};

pub const BG: Color32 = Color32::from_rgb(0xFF, 0xF6, 0xEC);
pub const CARD: Color32 = Color32::WHITE;
pub const BORDER: Color32 = Color32::from_rgb(0xF0, 0xE2, 0xD0);
pub const INK: Color32 = Color32::from_rgb(0x2B, 0x2D, 0x42);
pub const MUTED: Color32 = Color32::from_rgb(0x86, 0x8B, 0xA4);

pub const ORANGE: Color32 = Color32::from_rgb(0xFF, 0x8A, 0x3D);
pub const PINK: Color32 = Color32::from_rgb(0xFF, 0x5D, 0x8F);
pub const PURPLE: Color32 = Color32::from_rgb(0x8E, 0x6C, 0xEF);
pub const BLUE: Color32 = Color32::from_rgb(0x3B, 0x7B, 0xF6);
pub const GREEN: Color32 = Color32::from_rgb(0x22, 0xB5, 0x73);
pub const YELLOW: Color32 = Color32::from_rgb(0xFF, 0xC1, 0x3D);
pub const RED: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4D);

const HEAVY: &str = "heavy";

/// Police des titres et des boutons principaux (Nunito ExtraBold).
pub fn heavy() -> FontFamily {
    FontFamily::Name(HEAVY.into())
}

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    // Le même style pour les deux thèmes : l'appli reste claire même si le système est sombre.
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.set_style_of(Theme::Light, style());
    ctx.set_style_of(Theme::Dark, style());
}

/// Nunito pour l'interface, puis les polices d'egui (symboles), puis Noto Sans JP (japonais).
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        (
            "nunito",
            &include_bytes!("../assets/fonts/Nunito-SemiBold.ttf")[..],
        ),
        (
            "nunito-heavy",
            &include_bytes!("../assets/fonts/Nunito-ExtraBold.ttf")[..],
        ),
        (
            "noto-sans-jp",
            &include_bytes!("../assets/fonts/NotoSansJP-Regular.otf")[..],
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    let defaults = fonts.families[&FontFamily::Proportional].clone();
    let chain = |first: &str| {
        let mut chain = vec![first.to_string()];
        chain.extend(defaults.iter().cloned());
        chain.push("noto-sans-jp".into());
        chain
    };
    fonts
        .families
        .insert(FontFamily::Proportional, chain("nunito"));
    fonts.families.insert(heavy(), chain("nunito-heavy"));
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("noto-sans-jp".into());
    ctx.set_fonts(fonts);
}

fn style() -> egui::Style {
    let mut style = egui::Style {
        visuals: visuals(),
        ..Default::default()
    };
    style.text_styles = [
        (TextStyle::Heading, FontId::new(28.0, heavy())),
        (TextStyle::Body, FontId::proportional(15.5)),
        (TextStyle::Button, FontId::proportional(15.5)),
        (TextStyle::Small, FontId::proportional(12.5)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    let spacing = &mut style.spacing;
    spacing.item_spacing = Vec2::new(10.0, 10.0);
    spacing.button_padding = Vec2::new(14.0, 7.0);
    spacing.interact_size.y = 30.0;
    spacing.window_margin = Margin::same(20);
    spacing.menu_margin = Margin::same(10);
    style
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.extreme_bg_color = Color32::WHITE;
    v.faint_bg_color = mix(BG, ORANGE, 0.08);
    v.code_bg_color = mix(BG, ORANGE, 0.08);
    v.hyperlink_color = BLUE;
    v.warn_fg_color = ORANGE;
    v.error_fg_color = RED;
    v.selection.bg_fill = mix(Color32::WHITE, PINK, 0.25);
    v.selection.stroke = stroke(1.5, PINK);
    v.text_cursor.stroke = stroke(2.0, PINK);
    v.window_corner_radius = CornerRadius::same(18);
    v.menu_corner_radius = CornerRadius::same(14);
    v.window_stroke = stroke(1.0, BORDER);
    v.window_shadow = shadow(8, 28, 40);
    v.popup_shadow = shadow(6, 18, 30);

    let w = &mut v.widgets;
    for state in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        state.corner_radius = CornerRadius::same(10);
        state.fg_stroke = stroke(1.0, INK);
    }
    w.noninteractive.bg_fill = CARD;
    w.noninteractive.weak_bg_fill = CARD;
    w.noninteractive.bg_stroke = stroke(1.0, BORDER);
    w.inactive.bg_fill = mix(Color32::WHITE, ORANGE, 0.12);
    w.inactive.weak_bg_fill = Color32::WHITE;
    w.inactive.bg_stroke = stroke(1.0, mix(BORDER, ORANGE, 0.15));
    w.hovered.bg_fill = mix(Color32::WHITE, ORANGE, 0.22);
    w.hovered.weak_bg_fill = mix(Color32::WHITE, ORANGE, 0.1);
    w.hovered.bg_stroke = stroke(1.5, ORANGE);
    w.active.bg_fill = mix(Color32::WHITE, PINK, 0.3);
    w.active.weak_bg_fill = mix(Color32::WHITE, PINK, 0.18);
    w.active.bg_stroke = stroke(1.5, PINK);
    w.open.weak_bg_fill = mix(Color32::WHITE, ORANGE, 0.1);
    w.open.bg_stroke = stroke(1.5, ORANGE);
    v
}

fn shadow(offset: i8, blur: u8, alpha: u8) -> Shadow {
    Shadow {
        offset: [0, offset],
        blur,
        spread: 0,
        color: Color32::from_black_alpha(alpha),
    }
}

pub fn stroke(width: f32, color: Color32) -> Stroke {
    Stroke::new(width, color)
}

/// Mélange opaque de `a` vers `b`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// Carte blanche arrondie à ombre douce.
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(18)
        .inner_margin(Margin::same(18))
        .stroke(stroke(1.0, BORDER))
        .shadow(shadow(4, 18, 14))
}

/// Carte teintée de `color`.
pub fn tinted_card(color: Color32) -> egui::Frame {
    card()
        .fill(mix(Color32::WHITE, color, 0.12))
        .stroke(stroke(1.0, mix(Color32::WHITE, color, 0.35)))
}

/// Gros bouton plein, texte blanc.
pub fn primary(text: &str, color: Color32) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(text)
            .color(Color32::WHITE)
            .family(heavy())
            .size(16.0),
    )
    .fill(color)
    .stroke(Stroke::NONE)
    .corner_radius(14)
    .min_size(Vec2::new(0.0, 40.0))
}

/// Pastille colorée cliquable.
pub fn chip_button(text: &str, color: Color32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).color(mix(color, INK, 0.25)).size(14.0))
        .fill(mix(Color32::WHITE, color, 0.16))
        .stroke(Stroke::NONE)
        .corner_radius(99)
}

/// Pastille colorée.
pub fn chip(ui: &mut egui::Ui, text: impl Into<String>, color: Color32) -> egui::Response {
    egui::Frame::new()
        .fill(mix(Color32::WHITE, color, 0.16))
        .corner_radius(99)
        .inner_margin(Margin::symmetric(9, 2))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text.into())
                    .color(mix(color, INK, 0.3))
                    .size(13.0),
            )
        })
        .response
}

pub fn title(text: &str) -> RichText {
    RichText::new(text).family(heavy()).size(30.0).color(INK)
}

pub fn subtitle(text: &str) -> RichText {
    RichText::new(text).size(16.0).color(MUTED)
}

pub fn section(text: &str, color: Color32) -> RichText {
    RichText::new(text).family(heavy()).size(18.0).color(color)
}

/// Dégradé vertical passant par `colors`, du haut vers le bas.
pub fn vertical_gradient(painter: &egui::Painter, rect: egui::Rect, colors: &[Color32]) {
    let mut mesh = egui::Mesh::default();
    let last = (colors.len() - 1) as f32;
    for (i, color) in colors.iter().enumerate() {
        let y = egui::lerp(rect.top()..=rect.bottom(), i as f32 / last);
        mesh.colored_vertex(egui::pos2(rect.left(), y), *color);
        mesh.colored_vertex(egui::pos2(rect.right(), y), *color);
        if i > 0 {
            let n = (i * 2) as u32;
            mesh.add_triangle(n - 2, n - 1, n);
            mesh.add_triangle(n - 1, n, n + 1);
        }
    }
    painter.add(mesh);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_are_available() {
        let ctx = egui::Context::default();
        apply(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let text = "動くブラウン＆コニー＆サリー スペシャル · Sélection → ✔ ★ ▶ ✖ ⚠";
        for font in [
            FontId::proportional(14.0),
            FontId::new(14.0, heavy()),
            FontId::monospace(14.0),
        ] {
            let missing: String = text
                .chars()
                .filter(|&c| !ctx.fonts(|f| f.has_glyph(&font, c)))
                .collect();
            assert!(missing.is_empty(), "{font:?} : {missing}");
        }
    }
}
