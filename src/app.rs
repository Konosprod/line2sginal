//! Interface : import LINE, sélection des stickers, configuration et envoi du pack.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, RichText, Sense, Stroke,
    TextEdit, TextureHandle, TextureOptions, Vec2,
};

use line2signal::convert::{Converted, FitMode, MAX_BYTES};
use line2signal::line::{self, LinePack};
use line2signal::signal::upload::Credentials;
use line2signal::signal::{Pack, PackSticker, MAX_STICKERS};

use crate::credentials;
use crate::emoji::Emojis;
use crate::theme::{self, mix};
use crate::worker::{Msg, Preview, Worker, THUMBNAIL_SIDE};

const THUMB: f32 = THUMBNAIL_SIDE as f32;
/// Largeur d'une carte de sticker : vignette, marges et bordure.
const CARD_WIDTH: f32 = THUMB + 2.0 * 10.0 + 2.0 * 2.5;
const GRID_GAP: f32 = 14.0;
const DEFAULT_EMOJI: &str = "🙂";
/// Packs proposés en exemple sur l'écran d'import.
const EXAMPLES: [(&str, &str); 2] = [("11537", "Brown & Cony · animé"), ("789", "Sally · fixe")];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Import,
    Selection,
    Pack,
}

impl Step {
    const ALL: [Step; 3] = [Step::Import, Step::Selection, Step::Pack];

    fn color(self) -> Color32 {
        match self {
            Step::Import => theme::ORANGE,
            Step::Selection => theme::PINK,
            Step::Pack => theme::BLUE,
        }
    }

    fn labels(self) -> (&'static str, &'static str) {
        match self {
            Step::Import => ("Importe", "un pack LINE"),
            Step::Selection => ("Choisis", "tes stickers préférés"),
            Step::Pack => ("Envoie", "le pack dans Signal"),
        }
    }
}

struct Item {
    source: Arc<Vec<u8>>,
    animated: bool,
    selected: bool,
    emoji: String,
    preview: Preview,
    converted: Option<Result<Arc<Converted>, String>>,
}

#[derive(Default)]
struct UploadDialog {
    open: bool,
    loaded: bool,
    remember: bool,
    creds: Credentials,
}

pub struct App {
    worker: Worker,
    emojis: Emojis,
    icon: TextureHandle,
    step: Step,
    input: String,
    busy: Option<(String, Option<f32>)>,
    error: Option<String>,
    /// Pack LINE chargé ; ses stickers ont été déplacés dans `items`.
    line: Option<LinePack>,
    /// Incrémenté à chaque chargement, pour ignorer les aperçus d'un pack précédent.
    serial: u64,
    items: Vec<Item>,
    cover: Option<usize>,
    title: String,
    author: String,
    lang: String,
    default_emoji: String,
    fit: FitMode,
    generation: u64,
    upload: UploadDialog,
    exported: Option<PathBuf>,
    result_url: Option<String>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        theme::apply(ctx);
        Self {
            worker: Worker::new(ctx.clone()),
            emojis: Emojis::load(ctx),
            icon: load_icon(ctx),
            step: Step::Import,
            input: String::new(),
            busy: None,
            error: None,
            line: None,
            serial: 0,
            items: Vec::new(),
            cover: None,
            title: String::new(),
            author: String::new(),
            lang: String::new(),
            default_emoji: DEFAULT_EMOJI.into(),
            fit: FitMode::default(),
            generation: 0,
            upload: UploadDialog::default(),
            exported: None,
            result_url: None,
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Progress(text, fraction) => self.busy = Some((text, fraction)),
            Msg::Loaded(pack) => self.load(*pack),
            Msg::Preview(serial, i, preview) => {
                if let Some(item) = self.items.get_mut(i).filter(|_| serial == self.serial) {
                    item.preview = preview;
                }
            }
            Msg::Converted(generation, i, result) => {
                if let Some(item) = self
                    .items
                    .get_mut(i)
                    .filter(|_| generation == self.generation)
                {
                    item.converted = Some(result);
                }
            }
            Msg::Exported(result) => {
                self.busy = None;
                match result {
                    Ok(path) => self.exported = Some(path),
                    Err(e) => self.error = Some(e),
                }
            }
            Msg::Uploaded(result) => {
                self.busy = None;
                match result {
                    Ok(url) => self.result_url = Some(url),
                    Err(e) => self.error = Some(e),
                }
            }
            Msg::Failed(e) => {
                self.busy = None;
                self.error = Some(e);
            }
        }
    }

    fn load(&mut self, mut pack: LinePack) {
        self.busy = None;
        self.serial += 1;
        self.items = std::mem::take(&mut pack.stickers)
            .into_iter()
            .map(|s| Item {
                source: Arc::new(s.data),
                animated: s.animated,
                selected: true,
                emoji: self.default_emoji.clone(),
                preview: Vec::new(),
                converted: None,
            })
            .collect();
        self.cover = Some(0);
        self.lang = pack.default_language();
        self.title = pack.title(&self.lang);
        self.author = pack.author(&self.lang);
        self.line = Some(pack);
        self.exported = None;
        self.result_url = None;
        self.worker.previews(self.serial, self.sources());
        self.restart_conversion();
        self.step = Step::Selection;
    }

    fn sources(&self) -> Vec<Arc<Vec<u8>>> {
        self.items.iter().map(|i| i.source.clone()).collect()
    }

    fn restart_conversion(&mut self) {
        for item in &mut self.items {
            item.converted = None;
        }
        self.generation = self.worker.convert(self.sources(), self.fit);
    }

    fn start_fetch(&mut self) {
        match line::parse_input(&self.input) {
            Ok(id) => {
                self.error = None;
                self.busy = Some((format!("Pack {id}…"), None));
                self.worker.fetch(id);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn selected(&self) -> impl Iterator<Item = (usize, &Item)> {
        self.items.iter().enumerate().filter(|(_, i)| i.selected)
    }

    /// Couverture choisie si elle est sélectionnée, sinon le premier sticker sélectionné.
    fn effective_cover(&self) -> Option<usize> {
        self.cover
            .filter(|&c| self.items.get(c).is_some_and(|i| i.selected))
            .or_else(|| self.selected().next().map(|(i, _)| i))
    }

    /// Pack Signal à partir des stickers sélectionnés ; ceux non convertis ont des données vides.
    fn build_pack(&self) -> Pack {
        let cover = self.effective_cover();
        let mut pack = Pack {
            title: self.title.clone(),
            author: self.author.clone(),
            stickers: Vec::new(),
            cover: 0,
        };
        for (i, item) in self.selected() {
            if Some(i) == cover {
                pack.cover = pack.stickers.len();
            }
            let data = match &item.converted {
                Some(Ok(c)) => c.bytes.clone(),
                _ => Arc::from(&[][..]),
            };
            pack.stickers.push(PackSticker {
                data,
                emoji: item.emoji.clone(),
            });
        }
        pack
    }

    fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let pending = self
            .selected()
            .filter(|(_, i)| i.converted.is_none())
            .count();
        if pending > 0 {
            problems.push(format!("Conversion en cours ({pending} restants)…"));
        }
        for (n, (_, item)) in self.selected().enumerate() {
            if let Some(Err(e)) = &item.converted {
                problems.push(format!(
                    "Sticker n°{} : conversion impossible ({e}).",
                    n + 1
                ));
            }
        }
        for (n, (_, item)) in self.selected().enumerate() {
            let emoji = item.emoji.trim();
            if !emoji.is_empty() && !self.emojis.is_emoji(emoji) {
                problems.push(format!(
                    "Le sticker n°{} a « {emoji} » comme emoji, qui n'en est pas un.",
                    n + 1
                ));
            }
        }
        problems.extend(self.build_pack().validate());
        problems
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some((text, fraction)) = &self.busy {
                ui.add(egui::Spinner::new().color(theme::PINK));
                ui.label(text);
                if let Some(f) = fraction {
                    ui.add(
                        egui::ProgressBar::new(*f)
                            .desired_width(240.0)
                            .fill(theme::PINK)
                            .corner_radius(99),
                    );
                }
            } else if let Some(error) = &self.error {
                ui.label(RichText::new("⚠").color(theme::RED).size(18.0));
                ui.add(egui::Label::new(RichText::new(error).color(theme::RED)).wrap());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("✖").on_hover_text("Effacer").clicked() {
                        self.error = None;
                    }
                });
            } else {
                let done = self.items.iter().filter(|i| i.converted.is_some()).count();
                if done < self.items.len() {
                    ui.add(egui::Spinner::new().color(theme::BLUE));
                    ui.label(format!(
                        "Conversion des stickers : {done} / {}",
                        self.items.len()
                    ));
                    ui.add(
                        egui::ProgressBar::new(done as f32 / self.items.len() as f32)
                            .desired_width(240.0)
                            .fill(theme::BLUE)
                            .corner_radius(99),
                    );
                } else if self.items.is_empty() {
                    ui.label(RichText::new("Prêt à importer !").color(theme::MUTED));
                } else {
                    ui.label(RichText::new("✔").color(theme::GREEN));
                    ui.label(
                        RichText::new(format!("{} stickers convertis", self.items.len()))
                            .color(theme::MUTED),
                    );
                }
            }
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        theme::vertical_gradient(
            ui.painter(),
            ui.max_rect(),
            &[theme::ORANGE, theme::PINK, theme::PURPLE],
        );
        egui::Frame::new()
            .inner_margin(Margin::symmetric(16, 22))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.icon).fit_to_exact_size(Vec2::splat(52.0)));
                    ui.vertical(|ui| {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("LINE → Signal")
                                .family(theme::heavy())
                                .size(21.0)
                                .color(Color32::WHITE),
                        );
                        ui.label(
                            RichText::new("des stickers plein de joie")
                                .size(13.5)
                                .color(Color32::from_white_alpha(220)),
                        );
                    });
                });
                ui.add_space(30.0);
                for (n, step) in Step::ALL.into_iter().enumerate() {
                    let enabled = step == Step::Import || !self.items.is_empty();
                    if step_button(ui, n + 1, step, self.step == step, enabled).clicked() {
                        self.step = step;
                    }
                    ui.add_space(4.0);
                }
            });
    }

    fn import_panel(&mut self, ui: &mut egui::Ui) {
        let idle = self.busy.is_none();
        ui.label(theme::title("Des stickers LINE… dans Signal !"));
        ui.label(theme::subtitle(
            "Colle l'adresse d'un pack du store LINE, choisis tes stickers préférés, et hop : direction Signal.",
        ));
        ui.add_space(12.0);

        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(theme::section("Adresse ou numéro du pack", theme::ORANGE));
            ui.horizontal(|ui| {
                let edit = ui.add(
                    TextEdit::singleline(&mut self.input)
                        .hint_text("https://store.line.me/stickershop/product/11537/fr")
                        .font(FontId::proportional(17.0))
                        .margin(Vec2::new(12.0, 9.0))
                        .desired_width((ui.available_width() - 160.0).max(200.0)),
                );
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let clicked = ui
                    .add_enabled(idle, theme::primary("Télécharger", theme::ORANGE))
                    .clicked();
                if idle && (enter || clicked) {
                    self.start_fetch();
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Pour essayer :").color(theme::MUTED));
                for (i, (id, label)) in EXAMPLES.iter().enumerate() {
                    let color = [theme::PINK, theme::PURPLE][i % 2];
                    let chip = theme::chip_button(&format!("{id} · {label}"), color);
                    if ui.add_enabled(idle, chip).clicked() {
                        self.input = id.to_string();
                        self.start_fetch();
                    }
                }
            });
            ui.add_space(2.0);
            let zip = ui.add_enabled(
                idle,
                egui::Link::new("ou ouvrir un zip LINE déjà téléchargé…"),
            );
            if zip.clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Archive LINE", &["zip"])
                    .pick_file()
                {
                    self.error = None;
                    self.busy = Some(("Lecture de l'archive…".into(), None));
                    self.worker.open_zip(path);
                }
            }
        });
        ui.add_space(12.0);

        match &self.line {
            Some(pack) => {
                let time = ui.input(|i| i.time);
                let mut next = false;
                theme::card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(112.0), Sense::hover());
                        if let Some(item) = self.effective_cover().and_then(|c| self.items.get(c)) {
                            paint_preview(ui, rect, &item.preview, Some(time));
                        }
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(pack.title(&self.lang))
                                    .family(theme::heavy())
                                    .size(22.0)
                                    .color(theme::INK),
                            );
                            ui.label(
                                RichText::new(format!("par {}", pack.author(&self.lang)))
                                    .color(theme::MUTED),
                            );
                            ui.horizontal(|ui| {
                                theme::chip(
                                    ui,
                                    format!("{} stickers", self.items.len()),
                                    theme::BLUE,
                                );
                                let animated = self.items.iter().filter(|i| i.animated).count();
                                if animated > 0 {
                                    theme::chip(ui, format!("{animated} animés"), theme::PINK);
                                } else {
                                    theme::chip(ui, "fixes", theme::GREEN);
                                }
                                theme::chip(
                                    ui,
                                    format!("pack {}", pack.meta.package_id),
                                    theme::PURPLE,
                                );
                            });
                            next = ui
                                .add(theme::primary("Choisir les stickers →", theme::PINK))
                                .clicked();
                        });
                    });
                });
                if next {
                    self.step = Step::Selection;
                }
            }
            None => {
                ui.columns(3, |cols| {
                    for (col, step) in cols.iter_mut().zip(Step::ALL) {
                        let (head, tail) = step.labels();
                        theme::tinted_card(step.color()).show(col, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(theme::section(head, step.color()));
                            ui.label(RichText::new(tail).color(theme::INK));
                        });
                    }
                });
            }
        }
    }

    fn selection_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(theme::title("Choisis tes stickers"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(theme::primary("Continuer →", theme::BLUE)).clicked() {
                    self.step = Step::Pack;
                }
            });
        });
        ui.label(theme::subtitle(
            "Clic : sélectionner · survol : animation · clic droit : couverture",
        ));
        ui.add_space(8.0);
        theme::card()
            .inner_margin(Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Tout").clicked() {
                        self.items.iter_mut().for_each(|i| i.selected = true);
                    }
                    if ui.button("Aucun").clicked() {
                        self.items.iter_mut().for_each(|i| i.selected = false);
                    }
                    let n = self.selected().count();
                    let color = if n == 0 || n > MAX_STICKERS {
                        theme::RED
                    } else {
                        theme::PINK
                    };
                    theme::chip(ui, format!("{n} / {MAX_STICKERS}"), color).on_hover_text(format!(
                        "{n} stickers sélectionnés, {MAX_STICKERS} au maximum dans un pack Signal"
                    ));
                    ui.separator();
                    ui.label("Emoji pour tous");
                    self.emojis.picker(ui, &mut self.default_emoji, 22.0);
                    if ui.button("Appliquer").clicked() {
                        for item in self.items.iter_mut().filter(|i| i.selected) {
                            item.emoji = self.default_emoji.clone();
                        }
                    }
                });
            });
        ui.add_space(10.0);

        let time = ui.input(|i| i.time);
        let cover = self.effective_cover();
        let mut new_cover = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(Margin::same(6))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(GRID_GAP);
                        // Les cartes ont une largeur fixe : on calcule soi-même le nombre de colonnes.
                        let width = ui.available_width();
                        let columns = ((width + GRID_GAP) / (CARD_WIDTH + GRID_GAP))
                            .floor()
                            .max(1.0);
                        let used = columns * CARD_WIDTH + (columns - 1.0) * GRID_GAP;
                        let indent = ((width - used) / 2.0).max(0.0);
                        let columns = columns as usize;
                        let mut items = self.items.iter_mut().enumerate().peekable();
                        while items.peek().is_some() {
                            ui.horizontal(|ui| {
                                ui.add_space(indent - GRID_GAP);
                                for (i, item) in items.by_ref().take(columns) {
                                    if sticker_card(ui, item, &self.emojis, cover == Some(i), time)
                                    {
                                        new_cover = Some(i);
                                    }
                                }
                            });
                        }
                    });
            });
        if let Some(c) = new_cover {
            self.cover = Some(c);
            self.items[c].selected = true;
        }
    }

    fn pack_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(theme::title("Ton pack Signal"));
        ui.label(theme::subtitle(
            "Dernières retouches avant le grand départ.",
        ));
        ui.add_space(8.0);
        self.results(ui);

        let mut restart = false;
        let time = ui.input(|i| i.time);
        let cover = self.effective_cover();
        ui.horizontal_top(|ui| {
            let form_width = (ui.available_width() - 250.0).max(320.0);
            theme::card().show(ui, |ui| {
                ui.set_width(form_width - 36.0);
                ui.vertical(|ui| {
                    ui.label(theme::section("Titre", theme::BLUE));
                    ui.add(
                        TextEdit::singleline(&mut self.title)
                            .font(FontId::proportional(17.0))
                            .margin(Vec2::new(10.0, 7.0))
                            .desired_width(f32::INFINITY),
                    );
                    ui.label(theme::section("Auteur", theme::BLUE));
                    ui.add(
                        TextEdit::singleline(&mut self.author)
                            .font(FontId::proportional(17.0))
                            .margin(Vec2::new(10.0, 7.0))
                            .desired_width(f32::INFINITY),
                    );
                    if let Some(line) = &self.line {
                        let languages = line.languages();
                        if languages.len() > 1 {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Langue du pack LINE").color(theme::MUTED));
                                let mut lang = self.lang.clone();
                                egui::ComboBox::from_id_salt("lang")
                                    .selected_text(&lang)
                                    .show_ui(ui, |ui| {
                                        for l in &languages {
                                            ui.selectable_value(&mut lang, l.clone(), l);
                                        }
                                    });
                                if lang != self.lang {
                                    self.title = line.title(&lang);
                                    self.author = line.author(&lang);
                                    self.lang = lang;
                                }
                            });
                        }
                    }
                    ui.add_space(4.0);
                    ui.label(theme::section("Taille des stickers", theme::BLUE));
                    let before = self.fit;
                    ui.radio_value(
                        &mut self.fit,
                        FitMode::Original,
                        "Taille d'origine (recommandé)",
                    )
                    .on_hover_text("Aucune perte de qualité et des fichiers légers.");
                    ui.radio_value(&mut self.fit, FitMode::Upscale, "Agrandir à 512 px")
                        .on_hover_text(
                            "Peut obliger à réduire les couleurs ou les images des animations.",
                        );
                    restart = before != self.fit;
                });
            });
            theme::card().show(ui, |ui| {
                ui.set_width(180.0);
                ui.vertical_centered(|ui| {
                    ui.label(theme::section("Couverture", theme::PINK));
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(150.0), Sense::hover());
                    if let Some(item) = cover.and_then(|c| self.items.get(c)) {
                        paint_preview(ui, rect, &item.preview, Some(time));
                    }
                    ui.label(
                        RichText::new(
                            "Clic droit sur un sticker de l'étape « Choisis » pour la changer.",
                        )
                        .size(13.0)
                        .color(theme::MUTED),
                    );
                });
            });
        });
        if restart {
            self.restart_conversion();
        }

        ui.add_space(12.0);
        let problems = self.problems();
        if problems.is_empty() {
            theme::tinted_card(theme::GREEN).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let total: usize = self
                    .build_pack()
                    .stickers
                    .iter()
                    .map(|s| s.data.len())
                    .sum();
                ui.horizontal(|ui| {
                    ui.label(theme::section("✔ Tout est prêt !", theme::GREEN));
                    ui.label(
                        RichText::new(format!(
                            "{} stickers, {} Kio au total",
                            self.selected().count(),
                            total.div_ceil(1024)
                        ))
                        .color(theme::MUTED),
                    );
                });
            });
        } else {
            theme::tinted_card(theme::ORANGE).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(theme::section("Encore un petit effort", theme::ORANGE));
                for p in problems.iter().take(8) {
                    ui.label(RichText::new(format!("• {p}")).color(theme::INK));
                }
                if problems.len() > 8 {
                    ui.label(
                        RichText::new(format!("… et {} autres", problems.len() - 8))
                            .color(theme::MUTED),
                    );
                }
            });
        }

        ui.add_space(12.0);
        let ready = problems.is_empty() && self.busy.is_none();
        ui.horizontal(|ui| {
            let send =
                theme::primary("Envoyer à Signal", theme::BLUE).min_size(Vec2::new(220.0, 48.0));
            if ui.add_enabled(ready, send).clicked() {
                self.upload.open = true;
            }
            let export = theme::primary("Exporter un dossier", theme::GREEN)
                .min_size(Vec2::new(220.0, 48.0));
            if ui.add_enabled(ready, export).clicked() {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Dossier de destination")
                    .pick_folder()
                {
                    self.error = None;
                    self.exported = None;
                    self.busy = Some(("Export du dossier…".into(), None));
                    self.worker.export(self.build_pack(), dir);
                }
            }
        });
    }

    /// Résultat du dernier envoi ou export, affiché en haut de l'écran du pack.
    fn results(&self, ui: &mut egui::Ui) {
        if let Some(url) = &self.result_url {
            egui::Frame::new()
                .fill(theme::PURPLE)
                .corner_radius(18)
                .inner_margin(Margin::same(20))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if let Some(party) = self.emojis.image("🎉", 28.0) {
                            ui.add(party);
                        }
                        ui.label(
                            RichText::new("Pack envoyé ! Il n'y a plus qu'à l'ouvrir dans Signal.")
                                .family(theme::heavy())
                                .size(20.0)
                                .color(Color32::WHITE),
                        );
                    });
                    egui::Frame::new()
                        .fill(Color32::WHITE)
                        .corner_radius(10)
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add(
                                egui::Label::new(RichText::new(url).monospace().color(theme::INK))
                                    .wrap(),
                            );
                        });
                    ui.horizontal(|ui| {
                        let button = |text: &str| {
                            egui::Button::new(
                                RichText::new(text)
                                    .color(theme::PURPLE)
                                    .family(theme::heavy()),
                            )
                            .fill(Color32::WHITE)
                            .stroke(Stroke::NONE)
                            .corner_radius(12)
                        };
                        if ui.add(button("Copier le lien")).clicked() {
                            ui.ctx().copy_text(url.clone());
                        }
                        if ui.add(button("Ouvrir dans Signal")).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                        }
                    });
                });
        }
        if let Some(path) = &self.exported {
            theme::tinted_card(theme::GREEN).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(theme::section("Dossier exporté !", theme::GREEN));
                ui.label(RichText::new(path.display().to_string()).monospace());
                ui.label(
                    "Dans Signal Desktop : Fichier → Créer/téléverser un pack de stickers, \
                     ajoute les fichiers 001.png, 002.png… puis reprends les emojis listés dans pack.json.",
                );
            });
        }
    }

    fn upload_window(&mut self, ctx: &egui::Context) {
        if !self.upload.open {
            return;
        }
        if !self.upload.loaded {
            self.upload.loaded = true;
            if let Some(creds) = credentials::load() {
                self.upload.creds = creds;
                self.upload.remember = true;
            }
        }

        let mut open = true;
        let mut send = false;
        let dialog = &mut self.upload;
        // L'aide défile pour que les champs et le bouton restent visibles dans une petite fenêtre.
        let help_height = (ctx.screen_rect().height() - 380.0).max(120.0);
        egui::Window::new(theme::section("Envoyer à Signal", theme::BLUE))
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(
                    "L'envoi se fait avec ton compte, via les identifiants de Signal Desktop.",
                );
                let empty = dialog.creds.username.is_empty() && dialog.creds.password.is_empty();
                egui::CollapsingHeader::new("Où trouver ces identifiants ?")
                    .default_open(empty)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(help_height)
                            .show(ui, credentials_help);
                    });
                ui.add_space(6.0);
                egui::Grid::new("creds")
                    .num_columns(2)
                    .spacing([12.0, 10.0])
                    .show(ui, |ui| {
                        ui.label("Identifiant (uuid_id)");
                        ui.add(
                            TextEdit::singleline(&mut dialog.creds.username)
                                .margin(Vec2::new(10.0, 6.0))
                                .desired_width(320.0),
                        );
                        ui.end_row();
                        ui.label("Mot de passe");
                        ui.add(
                            TextEdit::singleline(&mut dialog.creds.password)
                                .password(true)
                                .margin(Vec2::new(10.0, 6.0))
                                .desired_width(320.0),
                        );
                        ui.end_row();
                    });
                ui.checkbox(
                    &mut dialog.remember,
                    "Mémoriser dans le trousseau du système",
                );
                ui.add_space(6.0);
                let filled = !dialog.creds.username.trim().is_empty()
                    && !dialog.creds.password.trim().is_empty();
                send = ui
                    .add_enabled(filled, theme::primary("Envoyer", theme::BLUE))
                    .clicked();
            });
        if !open {
            self.upload.open = false;
        }
        if send {
            let creds = Credentials {
                username: unquote(&self.upload.creds.username),
                password: unquote(&self.upload.creds.password),
            };
            if self.upload.remember {
                if let Err(e) = credentials::save(&creds) {
                    self.error = Some(format!("Identifiants non mémorisés : {e}"));
                }
            } else {
                credentials::forget();
            }
            self.upload.open = false;
            self.result_url = None;
            self.busy = Some(("Connexion à Signal…".into(), None));
            self.worker.upload(self.build_pack(), creds);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for msg in self.worker.poll() {
            self.handle(msg);
        }
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(theme::CARD)
                    .inner_margin(Margin::symmetric(16, 8))
                    .stroke(theme::stroke(1.0, theme::BORDER)),
            )
            .show(ctx, |ui| self.status_bar(ui));
        egui::SidePanel::left("steps")
            .resizable(false)
            .exact_width(240.0)
            .frame(egui::Frame::new())
            .show(ctx, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(28, 22)),
            )
            .show(ctx, |ui| match self.step {
                Step::Import => self.import_panel(ui),
                Step::Selection => self.selection_panel(ui),
                Step::Pack => {
                    egui::ScrollArea::vertical()
                        .show(ui, |ui| self.pack_panel(ui))
                        .inner
                }
            });
        self.upload_window(ctx);
    }
}

fn load_icon(ctx: &egui::Context) -> TextureHandle {
    let image = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .expect("icône PNG valide")
        .resize(128, 128, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    ctx.load_texture("icon", image, TextureOptions::LINEAR)
}

/// Commandes qui relancent Signal Desktop avec les outils de développement.
const SIGNAL_DEVTOOLS_COMMANDS: &[(&str, &str)] = if cfg!(target_os = "windows") {
    &[(
        "",
        r#""%LOCALAPPDATA%\Programs\signal-desktop\Signal.exe" --enable-dev-tools"#,
    )]
} else if cfg!(target_os = "macos") {
    &[("", "open -a Signal --args --enable-dev-tools")]
} else {
    &[
        (
            "Flatpak",
            "flatpak run org.signal.Signal --enable-dev-tools",
        ),
        ("Paquet .deb", "signal-desktop --enable-dev-tools"),
    ]
};

/// En production, le menu des outils de développement de Signal Desktop n'existe qu'avec
/// `--enable-dev-tools`, et la console n'accède qu'à l'objet `SignalDebug`.
fn credentials_help(ui: &mut egui::Ui) {
    let code = |ui: &mut egui::Ui, text: &str| {
        egui::Frame::new()
            .fill(theme::mix(theme::BG, theme::ORANGE, 0.08))
            .corner_radius(8)
            .inner_margin(Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(text).monospace()).selectable(true));
            });
    };
    ui.label("1. Quitte complètement Signal Desktop, puis relance-le depuis un terminal :");
    for (label, command) in SIGNAL_DEVTOOLS_COMMANDS {
        if !label.is_empty() {
            ui.label(RichText::new(*label).color(theme::MUTED).size(13.0));
        }
        code(ui, command);
    }
    let shortcut = if cfg!(target_os = "macos") {
        "Cmd+Option+I"
    } else {
        "Ctrl+Shift+I"
    };
    ui.label(format!(
        "2. Ouvre les outils de développement : menu Affichage, ou {shortcut}."
    ));
    ui.label("3. Dans l'onglet Console, tape ces deux lignes une par une :");
    code(ui, "SignalDebug.getStorageItem('uuid_id')");
    code(ui, "SignalDebug.getStorageItem('password')");
    ui.label(
        RichText::new(
            "Si la console refuse le collage, tape d'abord « allow pasting ». \
             Ces identifiants donnent accès à ton compte : ne les partage avec personne.",
        )
        .size(13.0)
        .color(theme::MUTED),
    );
    ui.label("4. Copie les deux valeurs ci-dessous.");
}

/// Retire les guillemets copiés avec une valeur de la console.
fn unquote(s: &str) -> String {
    s.trim().trim_matches(['"', '\'']).to_string()
}

/// Étape de la barre latérale : pastille numérotée, titre et sous-titre.
fn step_button(
    ui: &mut egui::Ui,
    number: usize,
    step: Step,
    active: bool,
    enabled: bool,
) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 58.0), sense);
    let painter = ui.painter();
    let alpha = if enabled { 1.0 } else { 0.45 };
    if active {
        painter.rect_filled(rect, 16.0, Color32::WHITE);
    } else if resp.hovered() {
        painter.rect_filled(rect, 16.0, Color32::from_white_alpha(40));
    }
    if enabled && !active && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let center = egui::pos2(rect.left() + 30.0, rect.center().y);
    let (circle, digit) = if active {
        (step.color(), Color32::WHITE)
    } else {
        (Color32::from_white_alpha(60), Color32::WHITE)
    };
    painter.circle_filled(center, 16.0, circle.gamma_multiply(alpha));
    painter.text(
        center,
        Align2::CENTER_CENTER,
        number.to_string(),
        FontId::new(17.0, theme::heavy()),
        digit.gamma_multiply(alpha),
    );
    let (head, tail) = step.labels();
    let (head_color, tail_color) = if active {
        (theme::INK, theme::MUTED)
    } else {
        (Color32::WHITE, Color32::from_white_alpha(210))
    };
    let x = rect.left() + 58.0;
    painter.text(
        egui::pos2(x, rect.center().y - 9.0),
        Align2::LEFT_CENTER,
        head,
        FontId::new(18.0, theme::heavy()),
        head_color.gamma_multiply(alpha),
    );
    painter.text(
        egui::pos2(x, rect.center().y + 11.0),
        Align2::LEFT_CENTER,
        tail,
        FontId::proportional(13.0),
        tail_color.gamma_multiply(alpha),
    );
    resp
}

/// Carte d'un sticker ; renvoie `true` si l'utilisateur en fait la couverture.
fn sticker_card(
    ui: &mut egui::Ui,
    item: &mut Item,
    emojis: &Emojis,
    is_cover: bool,
    time: f64,
) -> bool {
    let mut make_cover = false;
    let frame = if item.selected {
        theme::card().stroke(theme::stroke(2.5, theme::PINK))
    } else {
        theme::card()
            .stroke(theme::stroke(2.5, theme::BORDER))
            .shadow(egui::Shadow::NONE)
    };
    frame.inner_margin(Margin::same(10)).show(ui, |ui| {
        ui.set_width(THUMB);
        ui.vertical(|ui| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(THUMB), Sense::click());
            paint_preview(ui, rect, &item.preview, resp.hovered().then_some(time));
            let painter = ui.painter();
            if !item.selected {
                painter.rect_filled(rect, 12.0, Color32::from_white_alpha(150));
            }
            // Pastille de sélection, en haut à droite.
            let badge = rect.right_top() + Vec2::new(-12.0, 12.0);
            if item.selected {
                painter.circle_filled(badge, 11.0, theme::PINK);
                painter.text(
                    badge,
                    Align2::CENTER_CENTER,
                    "✔",
                    FontId::proportional(13.0),
                    Color32::WHITE,
                );
            } else {
                painter.circle(
                    badge,
                    10.0,
                    Color32::WHITE,
                    theme::stroke(1.5, theme::BORDER),
                );
            }
            if is_cover {
                let star = rect.left_top() + Vec2::new(12.0, 12.0);
                painter.circle_filled(star, 11.0, theme::YELLOW);
                painter.text(
                    star,
                    Align2::CENTER_CENTER,
                    "★",
                    FontId::proportional(14.0),
                    Color32::WHITE,
                );
            }
            if item.animated {
                let tag = egui::Rect::from_min_size(
                    rect.left_bottom() + Vec2::new(6.0, -24.0),
                    Vec2::new(22.0, 18.0),
                );
                painter.rect_filled(tag, 6.0, theme::PURPLE.gamma_multiply(0.85));
                painter.text(
                    tag.center(),
                    Align2::CENTER_CENTER,
                    "▶",
                    FontId::proportional(10.0),
                    Color32::WHITE,
                );
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let resp = resp.on_hover_text(if is_cover {
                "Couverture du pack"
            } else {
                "Clic droit : définir comme couverture"
            });
            if resp.clicked() {
                item.selected = !item.selected;
            }
            resp.context_menu(|ui| {
                if ui.button("★ Définir comme couverture").clicked() {
                    make_cover = true;
                    ui.close();
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                emojis.picker(ui, &mut item.emoji, 22.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    size_chip(ui, &item.converted);
                });
            });
        });
    });
    make_cover
}

fn size_chip(ui: &mut egui::Ui, converted: &Option<Result<Arc<Converted>, String>>) {
    match converted {
        None => {
            ui.add(egui::Spinner::new().size(14.0).color(theme::BLUE));
        }
        Some(Ok(c)) => {
            let color = match (c.fits(), &c.note) {
                (false, _) => theme::RED,
                (true, Some(_)) => theme::ORANGE,
                (true, None) => theme::GREEN,
            };
            let mut tip = format!("{} × {} px", c.side, c.side);
            if let Some(note) = &c.note {
                tip.push_str(&format!("\nRéductions : {note}"));
            }
            if !c.fits() {
                tip.push_str(&format!("\nDépasse la limite de {} Kio", MAX_BYTES / 1024));
            }
            theme::chip(ui, format!("{} Kio", c.bytes.len().div_ceil(1024)), color)
                .on_hover_text(tip);
        }
        Some(Err(e)) => {
            theme::chip(ui, "erreur", theme::RED).on_hover_text(e);
        }
    }
}

/// Dessine l'aperçu dans `rect` ; anime si `time` est fourni.
fn paint_preview(ui: &mut egui::Ui, rect: egui::Rect, preview: &Preview, time: Option<f64>) {
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(12),
        mix(theme::BG, Color32::WHITE, 0.4),
    );
    if preview.is_empty() {
        ui.put(rect, egui::Spinner::new().color(theme::PINK));
        return;
    }
    let index = match time {
        Some(t) if preview.len() > 1 => {
            ui.ctx().request_repaint();
            frame_at(preview, t)
        }
        _ => 0,
    };
    let texture = &preview[index].0;
    let size = texture.size_vec2();
    let scale = ((rect.width() - 8.0) / size.x).min((rect.height() - 8.0) / size.y);
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    ui.painter().image(
        texture.id(),
        egui::Rect::from_center_size(rect.center(), size * scale),
        uv,
        Color32::WHITE,
    );
}

fn frame_at(preview: &Preview, time: f64) -> usize {
    let total: u64 = preview
        .iter()
        .map(|(_, d)| u64::from(*d))
        .sum::<u64>()
        .max(1);
    let mut ms = (time * 1000.0) as u64 % total;
    for (i, (_, delay)) in preview.iter().enumerate() {
        if ms < u64::from(*delay) {
            return i;
        }
        ms -= u64::from(*delay);
    }
    preview.len() - 1
}

#[cfg(test)]
mod screenshots {
    use super::*;
    use egui_kittest::Harness;

    fn click(harness: &mut Harness<'_, App>, pos: egui::Pos2) {
        for pressed in [None, Some(true), Some(false)] {
            harness.input_mut().events.push(match pressed {
                None => egui::Event::PointerMoved(pos),
                Some(pressed) => egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            });
            harness.step();
        }
    }

    /// Captures des écrans : `SCREENSHOT_DIR=… cargo test screenshots -- --ignored`.
    #[test]
    #[ignore = "accès réseau et rendu wgpu"]
    fn screenshots() {
        let dir =
            PathBuf::from(std::env::var("SCREENSHOT_DIR").unwrap_or_else(|_| "screenshots".into()));
        std::fs::create_dir_all(&dir).unwrap();
        let pack = line::fetch_pack(11537, &|_, _| {}).unwrap();
        for (size, suffix) in [([1100.0, 760.0], ""), ([900.0, 600.0], "-etroit")] {
            let mut harness = Harness::builder()
                .with_size(size)
                .wgpu()
                .build_eframe(|cc| App::new(cc));
            let shoot = |harness: &mut Harness<'_, App>, name: &str| {
                harness.run_steps(3);
                let file = dir.join(format!("{name}{suffix}.png"));
                harness.render().unwrap().save(file).unwrap();
            };
            shoot(&mut harness, "0-vide");

            harness.state_mut().input = "https://store.line.me/stickershop/product/11537/fr".into();
            harness.state_mut().load(pack.clone());
            for _ in 0..1200 {
                harness.step();
                let app = harness.state();
                if app
                    .items
                    .iter()
                    .all(|i| !i.preview.is_empty() && i.converted.is_some())
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            {
                let app = harness.state_mut();
                for i in [3, 7, 8] {
                    app.items[i].selected = false;
                }
                app.items[2].emoji = "🐥".into();
                app.cover = Some(1);
            }
            for (name, step) in [
                ("1-import", Step::Import),
                ("2-selection", Step::Selection),
                ("3-pack", Step::Pack),
            ] {
                harness.state_mut().step = step;
                shoot(&mut harness, name);
            }
            harness.state_mut().result_url =
                Some("https://signal.art/addstickers/#pack_id=0123&pack_key=abcd".into());
            shoot(&mut harness, "4-envoye");
            harness.state_mut().upload.open = true;
            shoot(&mut harness, "5-dialogue");

            // Palette d'emojis ouverte depuis la barre d'outils, puis saisie d'un autre emoji.
            if suffix.is_empty() {
                harness.state_mut().upload.open = false;
                harness.state_mut().step = Step::Selection;
                harness.run_steps(3);
                click(&mut harness, egui::pos2(678.0, 142.0));
                shoot(&mut harness, "6-palette");
                click(&mut harness, egui::pos2(800.0, 487.0));
                harness
                    .input_mut()
                    .events
                    .push(egui::Event::Text("🐱".into()));
                harness.step();
                shoot(&mut harness, "7-autre");
                assert_eq!(harness.state().default_emoji, "🐱");
            }
        }
    }
}
