//! Travaux en arrière-plan (réseau, conversion, envoi) ; l'interface ne bloque jamais.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use rayon::prelude::*;

use line2signal::convert::{self, Converted, FitMode};
use line2signal::line::{self, LinePack};
use line2signal::signal::upload::{self, Credentials};
use line2signal::signal::{export, Pack};

pub const THUMBNAIL_SIDE: u32 = 128;

/// Images d'aperçu d'un sticker, avec leur durée en millisecondes.
pub type Preview = Vec<(TextureHandle, u32)>;

pub enum Msg {
    Progress(String, Option<f32>),
    Loaded(Box<LinePack>),
    /// Numéro du pack chargé, index du sticker, aperçu.
    Preview(u64, usize, Preview),
    Converted(u64, usize, Result<Arc<Converted>, String>),
    Exported(Result<PathBuf, String>),
    Uploaded(Result<String, String>),
    Failed(String),
}

#[derive(Clone)]
struct Reporter {
    tx: Sender<Msg>,
    ctx: egui::Context,
}

impl Reporter {
    fn send(&self, msg: Msg) {
        let _ = self.tx.send(msg);
        self.ctx.request_repaint();
    }
}

pub struct Worker {
    reporter: Reporter,
    rx: Receiver<Msg>,
    /// Numéro de la conversion en cours ; les résultats plus anciens sont ignorés.
    generation: Arc<AtomicU64>,
}

impl Worker {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        Self {
            reporter: Reporter { tx, ctx },
            rx,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn poll(&self) -> Vec<Msg> {
        self.rx.try_iter().collect()
    }

    fn spawn(&self, job: impl FnOnce(Reporter) + Send + 'static) {
        let reporter = self.reporter.clone();
        thread::spawn(move || job(reporter));
    }

    pub fn fetch(&self, id: u64) {
        self.spawn(move |r| {
            let progress = |text: &str, fraction| r.send(Msg::Progress(text.into(), fraction));
            match line::fetch_pack(id, &progress) {
                Ok(pack) => r.send(Msg::Loaded(Box::new(pack))),
                Err(e) => r.send(Msg::Failed(format!("{e:#}"))),
            }
        });
    }

    pub fn open_zip(&self, path: PathBuf) {
        self.spawn(move |r| {
            r.send(Msg::Progress("Lecture de l'archive…".into(), None));
            match std::fs::read(&path)
                .map_err(anyhow::Error::from)
                .and_then(|b| line::read_zip(&b))
            {
                Ok(pack) => r.send(Msg::Loaded(Box::new(pack))),
                Err(e) => r.send(Msg::Failed(format!("{}: {e:#}", path.display()))),
            }
        });
    }

    pub fn previews(&self, serial: u64, sources: Vec<Arc<Vec<u8>>>) {
        self.spawn(move |r| {
            sources.par_iter().enumerate().for_each(|(i, data)| {
                let Ok(frames) = convert::thumbnail_frames(data, THUMBNAIL_SIDE) else {
                    return;
                };
                let preview = frames
                    .into_iter()
                    .enumerate()
                    .map(|(n, f)| {
                        let size = [f.image.width() as usize, f.image.height() as usize];
                        let image = ColorImage::from_rgba_unmultiplied(size, f.image.as_raw());
                        let texture = r.ctx.load_texture(
                            format!("sticker-{serial}-{i}-{n}"),
                            image,
                            TextureOptions::LINEAR,
                        );
                        (texture, f.delay_ms)
                    })
                    .collect();
                r.send(Msg::Preview(serial, i, preview));
            });
        });
    }

    /// Lance la conversion de `sources` et renvoie son numéro de génération.
    pub fn convert(&self, sources: Vec<Arc<Vec<u8>>>, fit: FitMode) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let current = self.generation.clone();
        self.spawn(move |r| {
            sources.par_iter().enumerate().for_each(|(i, data)| {
                if current.load(Ordering::SeqCst) != generation {
                    return;
                }
                let result = convert::convert(data, fit)
                    .map(Arc::new)
                    .map_err(|e| format!("{e:#}"));
                r.send(Msg::Converted(generation, i, result));
            });
        });
        generation
    }

    pub fn export(&self, pack: Pack, dir: PathBuf) {
        self.spawn(move |r| {
            r.send(Msg::Progress("Export du dossier…".into(), None));
            r.send(Msg::Exported(
                export::export_dir(&pack, &dir).map_err(|e| format!("{e:#}")),
            ));
        });
    }

    pub fn upload(&self, pack: Pack, creds: Credentials) {
        self.spawn(move |r| {
            r.send(Msg::Progress("Connexion à Signal…".into(), None));
            let progress = |done: usize, total: usize| {
                r.send(Msg::Progress(
                    format!("Envoi des stickers : {done}/{total}"),
                    Some(done as f32 / total.max(1) as f32),
                ))
            };
            let result = upload::upload(&pack, &creds, &progress).map_err(|e| format!("{e:#}"));
            r.send(Msg::Uploaded(result));
        });
    }
}
