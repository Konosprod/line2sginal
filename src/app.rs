//! Interface : import LINE, sélection des stickers, configuration et envoi du pack.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, Color32, RichText, Sense, TextEdit, Vec2};

use line2signal::convert::{Converted, FitMode, MAX_BYTES};
use line2signal::line::{self, LinePack};
use line2signal::signal::upload::Credentials;
use line2signal::signal::{Pack, PackSticker, MAX_STICKERS};

use crate::credentials;
use crate::worker::{Msg, Preview, Worker, THUMBNAIL_SIDE};

const THUMB: f32 = THUMBNAIL_SIDE as f32;
const CARD_WIDTH: f32 = THUMB + 24.0;
const DEFAULT_EMOJI: &str = "🙂";
const EMOJIS: &[&str] = &[
    "😀", "😂", "🥰", "😍", "😘", "😉", "😎", "🤔", "😴", "😭", "😢", "😡", "😱", "😳", "🙄", "😅",
    "🙂", "🙏", "👍", "👎", "👏", "👋", "🙌", "💪", "❤", "💔", "✨", "🎉", "🔥", "💯", "❓", "❗",
    "💤", "🍺", "☕", "🐻", "🐰", "🐥", "🐱", "🐶",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Import,
    Selection,
    Pack,
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
        install_fonts(&cc.egui_ctx);
        Self {
            worker: Worker::new(cc.egui_ctx.clone()),
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
        problems.extend(self.build_pack().validate());
        problems
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some((text, fraction)) = &self.busy {
                ui.spinner();
                ui.label(text);
                if let Some(f) = fraction {
                    ui.add(
                        egui::ProgressBar::new(*f)
                            .desired_width(220.0)
                            .show_percentage(),
                    );
                }
            } else if let Some(error) = &self.error {
                if ui.small_button("✖").on_hover_text("Effacer").clicked() {
                    self.error = None;
                    return;
                }
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("⚠ {error}")).color(ui.visuals().error_fg_color),
                    )
                    .wrap(),
                );
            } else {
                let done = self.items.iter().filter(|i| i.converted.is_some()).count();
                if done < self.items.len() {
                    ui.spinner();
                    ui.label(format!("Conversion : {done} / {}", self.items.len()));
                } else {
                    ui.weak("Prêt.");
                }
            }
        });
    }

    fn steps(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.heading("LINE → Signal");
        ui.separator();
        for (step, label) in [
            (Step::Import, "1. Import"),
            (Step::Selection, "2. Sélection"),
            (Step::Pack, "3. Pack Signal"),
        ] {
            let enabled = step == Step::Import || !self.items.is_empty();
            ui.add_enabled_ui(enabled, |ui| {
                ui.selectable_value(&mut self.step, step, RichText::new(label).size(16.0));
            });
        }
    }

    fn import_panel(&mut self, ui: &mut egui::Ui) {
        let idle = self.busy.is_none();
        ui.heading("Importer un pack LINE");
        ui.label("Colle l'URL du pack sur le store LINE ou son identifiant numérique.");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let edit = ui.add(
                TextEdit::singleline(&mut self.input)
                    .hint_text("https://store.line.me/stickershop/product/11537/fr")
                    .desired_width(440.0),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let clicked = ui
                .add_enabled(idle, egui::Button::new("Télécharger"))
                .clicked();
            if idle && (enter || clicked) {
                self.start_fetch();
            }
        });
        ui.add_space(4.0);
        if ui
            .add_enabled(idle, egui::Button::new("Ouvrir un zip LINE local…"))
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Archive LINE", &["zip"])
                .pick_file()
            {
                self.error = None;
                self.busy = Some(("Lecture de l'archive…".into(), None));
                self.worker.open_zip(path);
            }
        }

        let Some(pack) = &self.line else { return };
        ui.add_space(12.0);
        ui.separator();
        egui::Grid::new("line-pack")
            .num_columns(2)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                ui.label("Pack");
                ui.label(pack.meta.package_id.to_string());
                ui.end_row();
                ui.label("Titre");
                ui.label(pack.title(&self.lang));
                ui.end_row();
                ui.label("Auteur");
                ui.label(pack.author(&self.lang));
                ui.end_row();
                ui.label("Stickers");
                let animated = self.items.iter().filter(|i| i.animated).count();
                ui.label(format!("{} dont {animated} animés", self.items.len()));
                ui.end_row();
            });
        ui.add_space(8.0);
        if ui.button("Choisir les stickers →").clicked() {
            self.step = Step::Selection;
        }
    }

    fn selection_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Tout").clicked() {
                self.items.iter_mut().for_each(|i| i.selected = true);
            }
            if ui.button("Aucun").clicked() {
                self.items.iter_mut().for_each(|i| i.selected = false);
            }
            ui.separator();
            let n = self.selected().count();
            let color = if n == 0 || n > MAX_STICKERS {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().text_color()
            };
            ui.colored_label(color, format!("{n} / {MAX_STICKERS} sélectionnés"));
            ui.separator();
            ui.label("Emoji :");
            ui.add(TextEdit::singleline(&mut self.default_emoji).desired_width(32.0));
            emoji_menu(ui, &mut self.default_emoji);
            if ui.button("Appliquer aux sélectionnés").clicked() {
                for item in self.items.iter_mut().filter(|i| i.selected) {
                    item.emoji = self.default_emoji.clone();
                }
            }
            ui.separator();
            if ui.button("Continuer →").clicked() {
                self.step = Step::Pack;
            }
        });
        ui.weak(
            "Clic : (dé)sélectionner · survol : animation · clic droit : définir comme couverture",
        );
        ui.separator();

        let time = ui.input(|i| i.time);
        let cover = self.effective_cover();
        let mut new_cover = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(10.0);
                    for (i, item) in self.items.iter_mut().enumerate() {
                        if sticker_card(ui, item, cover == Some(i), time) {
                            new_cover = Some(i);
                        }
                    }
                });
            });
        if let Some(c) = new_cover {
            self.cover = Some(c);
            self.items[c].selected = true;
        }
    }

    fn pack_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Configurer le pack Signal");
        ui.add_space(6.0);
        let mut restart = false;
        let time = ui.input(|i| i.time);
        let cover = self.effective_cover();
        egui::Grid::new("pack-form")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label("Titre");
                ui.add(TextEdit::singleline(&mut self.title).desired_width(380.0));
                ui.end_row();
                ui.label("Auteur");
                ui.add(TextEdit::singleline(&mut self.author).desired_width(380.0));
                ui.end_row();

                if let Some(line) = &self.line {
                    let languages = line.languages();
                    if languages.len() > 1 {
                        ui.label("Langue LINE");
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
                        ui.end_row();
                    }
                }

                ui.label("Taille");
                ui.horizontal(|ui| {
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
                ui.end_row();

                ui.label("Couverture");
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(96.0), Sense::hover());
                    if let Some(item) = cover.and_then(|c| self.items.get(c)) {
                        paint_preview(ui, rect, &item.preview, Some(time));
                    }
                    ui.weak("Clic droit sur un sticker dans « Sélection » pour la changer.");
                });
                ui.end_row();
            });
        if restart {
            self.restart_conversion();
        }

        ui.add_space(8.0);
        ui.separator();
        let problems = self.problems();
        if problems.is_empty() {
            ui.colored_label(Color32::from_rgb(60, 170, 90), "✔ Le pack est prêt.");
        } else {
            for p in problems.iter().take(8) {
                ui.colored_label(ui.visuals().warn_fg_color, format!("• {p}"));
            }
            if problems.len() > 8 {
                ui.weak(format!("… et {} autres", problems.len() - 8));
            }
        }

        ui.add_space(8.0);
        let ready = problems.is_empty() && self.busy.is_none();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(ready, egui::Button::new("Exporter le dossier…"))
                .clicked()
            {
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
            if ui
                .add_enabled(ready, egui::Button::new("Envoyer à Signal…"))
                .clicked()
            {
                self.upload.open = true;
            }
        });

        if let Some(path) = &self.exported {
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.strong("Dossier exporté");
                ui.label(RichText::new(path.display().to_string()).monospace());
                ui.label(
                    "Dans Signal Desktop : Fichier → Créer/téléverser un pack de stickers, \
                     ajoute les fichiers 001.png, 002.png… puis reprends les emojis listés dans pack.json.",
                );
            });
        }
        if let Some(url) = &self.result_url {
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.strong("Pack envoyé !");
                ui.add(egui::Label::new(RichText::new(url).monospace()).wrap());
                ui.horizontal(|ui| {
                    if ui.button("Copier le lien").clicked() {
                        ui.ctx().copy_text(url.clone());
                    }
                    if ui.button("Ouvrir").clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                });
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
        egui::Window::new("Envoyer à Signal")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(
                    "L'envoi se fait avec ton compte, via les identifiants de Signal Desktop.",
                );
                egui::CollapsingHeader::new("Où trouver ces identifiants ?").show(ui, |ui| {
                    ui.label("1. Dans Signal Desktop : Affichage → Outils de développement.");
                    ui.label("2. Dans l'onglet Console, tape :");
                    for line in [
                        "window.reduxStore.getState().items.uuid_id",
                        "window.reduxStore.getState().items.password",
                    ] {
                        ui.add(egui::Label::new(RichText::new(line).monospace()).selectable(true));
                    }
                    ui.label("3. Colle les deux valeurs ci-dessous.");
                });
                ui.add_space(6.0);
                egui::Grid::new("creds").num_columns(2).show(ui, |ui| {
                    ui.label("Identifiant (uuid_id)");
                    ui.add(TextEdit::singleline(&mut dialog.creds.username).desired_width(320.0));
                    ui.end_row();
                    ui.label("Mot de passe");
                    ui.add(
                        TextEdit::singleline(&mut dialog.creds.password)
                            .password(true)
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
                    .add_enabled(filled, egui::Button::new("Envoyer"))
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
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| self.status_bar(ui));
        egui::SidePanel::left("steps")
            .resizable(false)
            .exact_width(170.0)
            .show(ctx, |ui| self.steps(ui));
        egui::CentralPanel::default().show(ctx, |ui| match self.step {
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

/// Ajoute Noto Sans JP en repli des polices d'egui, pour les titres en japonais.
fn install_fonts(ctx: &egui::Context) {
    const NAME: &str = "noto-sans-jp";
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        NAME.into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/NotoSansJP-Regular.otf"
        ))),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push(NAME.into());
    }
    ctx.set_fonts(fonts);
}

/// Retire les guillemets copiés avec une valeur de la console.
fn unquote(s: &str) -> String {
    s.trim().trim_matches(['"', '\'']).to_string()
}

/// Carte d'un sticker ; renvoie `true` si l'utilisateur en fait la couverture.
fn sticker_card(ui: &mut egui::Ui, item: &mut Item, is_cover: bool, time: f64) -> bool {
    let mut make_cover = false;
    let stroke = if item.selected {
        ui.visuals().selection.stroke
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke
    };
    egui::Frame::group(ui.style())
        .stroke(stroke)
        .show(ui, |ui| {
            ui.set_width(CARD_WIDTH);
            ui.vertical_centered(|ui| {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(THUMB), Sense::click());
                paint_preview(ui, rect, &item.preview, resp.hovered().then_some(time));
                if !item.selected {
                    ui.painter().rect_filled(
                        rect,
                        4.0,
                        ui.visuals().panel_fill.gamma_multiply(0.7),
                    );
                }
                if resp.clicked() {
                    item.selected = !item.selected;
                }
                resp.context_menu(|ui| {
                    if ui.button("Définir comme couverture").clicked() {
                        make_cover = true;
                        ui.close();
                    }
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut item.selected, "");
                    ui.add(TextEdit::singleline(&mut item.emoji).desired_width(32.0));
                    emoji_menu(ui, &mut item.emoji);
                });
                ui.horizontal(|ui| {
                    size_label(ui, &item.converted);
                    if item.animated {
                        ui.weak("animé");
                    }
                    if is_cover {
                        ui.label("★").on_hover_text("Couverture du pack");
                    }
                });
            });
        });
    make_cover
}

fn emoji_menu(ui: &mut egui::Ui, target: &mut String) {
    ui.menu_button("▾", |ui| {
        ui.set_max_width(250.0);
        ui.horizontal_wrapped(|ui| {
            for emoji in EMOJIS {
                if ui.button(RichText::new(*emoji).size(18.0)).clicked() {
                    *target = emoji.to_string();
                    ui.close();
                }
            }
        });
    });
}

fn size_label(ui: &mut egui::Ui, converted: &Option<Result<Arc<Converted>, String>>) {
    match converted {
        None => {
            ui.add(egui::Spinner::new().size(12.0));
        }
        Some(Ok(c)) => {
            let color = if c.fits() {
                ui.visuals().weak_text_color()
            } else {
                ui.visuals().error_fg_color
            };
            let mut tip = format!("{} × {} px", c.side, c.side);
            if let Some(note) = &c.note {
                tip.push_str(&format!("\nRéductions : {note}"));
            }
            if !c.fits() {
                tip.push_str(&format!("\nDépasse la limite de {} Kio", MAX_BYTES / 1024));
            }
            ui.colored_label(color, format!("{} Kio", c.bytes.len().div_ceil(1024)))
                .on_hover_text(tip);
        }
        Some(Err(e)) => {
            ui.colored_label(ui.visuals().error_fg_color, "erreur")
                .on_hover_text(e);
        }
    }
}

/// Dessine l'aperçu dans `rect` ; anime si `time` est fourni.
fn paint_preview(ui: &mut egui::Ui, rect: egui::Rect, preview: &Preview, time: Option<f64>) {
    ui.painter()
        .rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    if preview.is_empty() {
        ui.put(rect, egui::Spinner::new());
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
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
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
mod tests {
    use super::*;

    #[test]
    fn japanese_glyphs_are_available() {
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let title = "動くブラウン＆コニー＆サリー スペシャル";
        for font in [
            egui::FontId::proportional(14.0),
            egui::FontId::monospace(14.0),
        ] {
            assert!(ctx.fonts(|f| f.has_glyphs(&font, title)));
        }
    }
}
