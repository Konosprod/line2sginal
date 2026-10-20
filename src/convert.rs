//! Normalisation des images au format attendu par Signal.
//!
//! Règles reprises du créateur de packs de Signal Desktop :
//! image carrée d'au plus 512 px, 300 Kio au plus, et les APNG doivent boucler à l'infini.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use anyhow::{Context, Result};
use color_quant::NeuQuant;
use image::codecs::png::PngDecoder;
use image::imageops::{self, FilterType};
use image::{AnimationDecoder, RgbaImage};

pub const STICKER_SIZE: u32 = 512;
pub const MAX_BYTES: usize = 300 * 1024;
const MIN_SIDE: u32 = 64;

/// Tentatives successives pour passer sous `MAX_BYTES` :
/// (échelle, une image sur n gardée, palette 256 couleurs avec perte).
const ATTEMPTS: [(f32, usize, bool); 8] = [
    (1.0, 1, false),
    (1.0, 1, true),
    (0.85, 1, true),
    (0.72, 1, true),
    (0.72, 2, true),
    (0.6, 2, true),
    (0.5, 2, true),
    (0.5, 3, true),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FitMode {
    /// Garde la taille d'origine, centrée sur un carré : aucune perte, fichiers légers.
    #[default]
    Original,
    /// Agrandit le sticker pour occuper 512×512.
    Upscale,
}

#[derive(Debug, Clone)]
pub struct Converted {
    pub bytes: Arc<[u8]>,
    pub animated: bool,
    /// Côté du carré produit, en pixels.
    pub side: u32,
    /// Réductions appliquées pour tenir dans la limite de taille.
    pub note: Option<String>,
}

impl Converted {
    pub fn fits(&self) -> bool {
        self.bytes.len() <= MAX_BYTES
    }
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub image: RgbaImage,
    pub delay_ms: u32,
}

/// Renvoie `(nombre d'images, nombre de lectures)` si `data` est un APNG.
pub fn apng_info(data: &[u8]) -> Option<(u32, u32)> {
    if !data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let be = |b: &[u8]| u32::from_be_bytes(b.try_into().expect("4 octets"));
    let mut i = 8;
    while i + 8 <= data.len() {
        let len = be(&data[i..i + 4]) as usize;
        match &data[i + 4..i + 8] {
            b"acTL" => {
                let body = data.get(i + 8..i + 16)?;
                return Some((be(&body[..4]), be(&body[4..])));
            }
            b"IDAT" | b"IEND" => return None,
            _ => i += 12 + len,
        }
    }
    None
}

pub fn is_apng(data: &[u8]) -> bool {
    apng_info(data).is_some()
}

/// Décode un PNG ou un APNG en images RGBA complètes (déjà composées).
pub fn decode_frames(data: &[u8]) -> Result<Vec<Frame>> {
    if is_apng(data) {
        let decoder = PngDecoder::new(Cursor::new(data))?;
        let frames = decoder.apng()?.into_frames().collect_frames()?;
        return Ok(frames
            .into_iter()
            .map(|f| {
                let (num, den) = f.delay().numer_denom_ms();
                Frame {
                    delay_ms: (num as f64 / den.max(1) as f64).round() as u32,
                    image: clear_transparent(f.into_buffer()),
                }
            })
            .collect());
    }
    let image = image::load_from_memory(data)
        .context("image illisible")?
        .to_rgba8();
    Ok(vec![Frame {
        image: clear_transparent(image),
        delay_ms: 0,
    }])
}

/// Ramène tous les pixels invisibles à (0, 0, 0, 0) : la couleur sous un alpha nul
/// gonfle la compression et fausse la détection des zones modifiées.
fn clear_transparent(mut img: RgbaImage) -> RgbaImage {
    for p in img.pixels_mut().filter(|p| p[3] == 0) {
        p.0 = [0; 4];
    }
    img
}

/// Images réduites pour l'aperçu dans l'interface.
pub fn thumbnail_frames(data: &[u8], max_side: u32) -> Result<Vec<Frame>> {
    Ok(decode_frames(data)?
        .into_iter()
        .map(|f| {
            let (w, h) = f.image.dimensions();
            let ratio = (max_side as f32 / w.max(h) as f32).min(1.0);
            let (nw, nh) = scaled(w, h, ratio);
            Frame {
                image: imageops::resize(&f.image, nw, nh, FilterType::Triangle),
                delay_ms: f.delay_ms,
            }
        })
        .collect())
}

/// Convertit un PNG/APNG LINE en sticker Signal.
///
/// Si le résultat dépasse `MAX_BYTES`, des réductions de plus en plus fortes sont essayées ;
/// la dernière tentative est renvoyée même si elle reste trop lourde (voir [`Converted::fits`]).
pub fn convert(data: &[u8], fit: FitMode) -> Result<Converted> {
    let frames = decode_frames(data)?;
    let (w, h) = frames[0].image.dimensions();
    let side = match fit {
        FitMode::Upscale => STICKER_SIZE,
        FitMode::Original => w.max(h).clamp(MIN_SIDE, STICKER_SIZE),
    };

    let mut fitted: Option<(u32, Vec<Frame>)> = None;
    let mut last = None;
    for (scale, step, palette) in ATTEMPTS {
        if frames.len() == 1 && step > 1 {
            continue;
        }
        let canvas = ((side as f32 * scale).round() as u32).max(MIN_SIDE);
        if fitted.as_ref().map(|(c, _)| *c) != Some(canvas) {
            let resized = frames
                .iter()
                .map(|f| Frame {
                    image: fit_into(&f.image, canvas),
                    delay_ms: f.delay_ms,
                })
                .collect();
            fitted = Some((canvas, resized));
        }
        let (_, base) = fitted.as_ref().expect("rempli juste au-dessus");
        let out_frames = merge_identical(keep_one_in(base, step));
        let result = Converted {
            bytes: encode_png(&out_frames, palette)?.into(),
            animated: out_frames.len() > 1,
            side: canvas,
            note: describe(scale, step, palette, canvas),
        };
        if result.fits() {
            return Ok(result);
        }
        last = Some(result);
    }
    Ok(last.expect("au moins une tentative"))
}

fn describe(scale: f32, step: usize, palette: bool, canvas: u32) -> Option<String> {
    let mut parts = Vec::new();
    if palette {
        parts.push("palette 256 couleurs".to_string());
    }
    if step > 1 {
        parts.push(format!("1 image sur {step}"));
    }
    if scale < 1.0 {
        parts.push(format!("réduit à {canvas} px"));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

fn scaled(w: u32, h: u32, ratio: f32) -> (u32, u32) {
    (
        ((w as f32 * ratio).round() as u32).max(1),
        ((h as f32 * ratio).round() as u32).max(1),
    )
}

/// Redimensionne `img` pour tenir dans un carré `canvas`×`canvas` transparent, centré.
fn fit_into(img: &RgbaImage, canvas: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let (nw, nh) = scaled(w, h, canvas as f32 / w.max(h) as f32);
    let (nw, nh) = (nw.min(canvas), nh.min(canvas));
    let resized = if (nw, nh) == (w, h) {
        img.clone()
    } else {
        resize_premultiplied(img, nw, nh)
    };
    let mut out = RgbaImage::new(canvas, canvas);
    imageops::replace(
        &mut out,
        &resized,
        ((canvas - nw) / 2) as i64,
        ((canvas - nh) / 2) as i64,
    );
    out
}

/// Redimensionne en alpha prémultiplié pour éviter les halos sombres sur les bords.
fn resize_premultiplied(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let mut pre = img.clone();
    for p in pre.pixels_mut() {
        let a = p[3] as u32;
        for c in 0..3 {
            p[c] = ((p[c] as u32 * a + 127) / 255) as u8;
        }
    }
    let mut out = imageops::resize(&pre, w, h, FilterType::Lanczos3);
    for p in out.pixels_mut() {
        let a = p[3] as u32;
        // Un pixel totalement transparent devient (0, 0, 0, 0).
        for c in 0..3 {
            p[c] = (p[c] as u32 * 255 + a / 2)
                .checked_div(a)
                .map_or(0, |v| v.min(255) as u8);
        }
    }
    out
}

/// Garde une image sur `step` en cumulant les délais des images sautées.
fn keep_one_in(frames: &[Frame], step: usize) -> Vec<Frame> {
    frames
        .chunks(step)
        .map(|chunk| Frame {
            image: chunk[0].image.clone(),
            delay_ms: chunk.iter().map(|f| f.delay_ms).sum(),
        })
        .collect()
}

/// Fusionne les images consécutives identiques.
fn merge_identical(frames: Vec<Frame>) -> Vec<Frame> {
    let mut out: Vec<Frame> = Vec::with_capacity(frames.len());
    for frame in frames {
        match out.last_mut() {
            Some(last) if last.image == frame.image => last.delay_ms += frame.delay_ms,
            _ => out.push(frame),
        }
    }
    out
}

/// Rectangle `(x, y, largeur, hauteur)` englobant les pixels qui diffèrent entre `a` et `b`.
fn changed_rect(a: &RgbaImage, b: &RgbaImage) -> (u32, u32, u32, u32) {
    let (w, h) = a.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (x, y, p) in b.enumerate_pixels() {
        if a.get_pixel(x, y) != p {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x0 > x1 {
        (0, 0, 1, 1)
    } else {
        (x0, y0, x1 - x0 + 1, y1 - y0 + 1)
    }
}

fn delay_fraction(ms: u32) -> (u16, u16) {
    match u16::try_from(ms) {
        Ok(ms) => (ms, 1000),
        Err(_) => ((ms / 10).min(u16::MAX as u32) as u16, 100),
    }
}

/// Encode en PNG, ou en APNG bouclant à l'infini s'il y a plusieurs images.
///
/// Sans `lossy`, une palette n'est utilisée que si elle est exacte (255 couleurs visibles au plus).
/// Chaque image après la première n'encode que la zone modifiée depuis la précédente.
fn encode_png(frames: &[Frame], lossy: bool) -> Result<Vec<u8>> {
    let (w, h) = frames[0].image.dimensions();
    let animated = frames.len() > 1;
    let mut quantizer = if lossy {
        Some(Quantizer::lossy(frames))
    } else {
        Quantizer::exact(frames)
    };

    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Best);
    match &quantizer {
        Some(q) => {
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_palette(
                q.palette
                    .iter()
                    .flat_map(|c| [c[0], c[1], c[2]])
                    .collect::<Vec<_>>(),
            );
            encoder.set_trns(q.palette.iter().map(|c| c[3]).collect::<Vec<_>>());
            encoder.set_filter(png::FilterType::NoFilter);
        }
        None => {
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_adaptive_filter(png::AdaptiveFilterType::Adaptive);
        }
    }
    if animated {
        encoder.set_animated(frames.len() as u32, 0)?;
        encoder.set_dispose_op(png::DisposeOp::None)?;
        encoder.set_blend_op(png::BlendOp::Source)?;
    }

    let mut writer = encoder.write_header()?;
    let mut previous: Option<&RgbaImage> = None;
    for frame in frames {
        let (x, y, fw, fh) = previous.map_or((0, 0, w, h), |p| changed_rect(p, &frame.image));
        if animated {
            let (num, den) = delay_fraction(frame.delay_ms);
            writer.set_frame_delay(num, den)?;
            writer.set_frame_position(0, 0)?;
            writer.set_frame_dimension(fw, fh)?;
            writer.set_frame_position(x, y)?;
        }
        let sub = imageops::crop_imm(&frame.image, x, y, fw, fh).to_image();
        let data = match &mut quantizer {
            Some(q) => q.indices(&sub),
            None => sub.into_raw(),
        };
        writer.write_image_data(&data)?;
        previous = Some(&frame.image);
    }
    writer.finish()?;
    Ok(out)
}

/// Palette commune à toutes les images ; l'index 0 est réservé au transparent.
struct Quantizer {
    /// Absent pour une palette exacte : toutes les couleurs sont déjà dans `cache`.
    nq: Option<NeuQuant>,
    palette: Vec<[u8; 4]>,
    cache: HashMap<[u8; 4], u8>,
}

impl Quantizer {
    fn exact(frames: &[Frame]) -> Option<Self> {
        let mut palette = vec![[0u8; 4]];
        let mut cache = HashMap::new();
        for p in frames.iter().flat_map(|f| f.image.pixels()) {
            if p[3] == 0 || cache.contains_key(&p.0) {
                continue;
            }
            if palette.len() == 256 {
                return None;
            }
            cache.insert(p.0, palette.len() as u8);
            palette.push(p.0);
        }
        Some(Self {
            nq: None,
            palette,
            cache,
        })
    }

    fn lossy(frames: &[Frame]) -> Self {
        let total: usize = frames.iter().map(|f| f.image.len() / 4).sum();
        let stride = (total / 400_000).max(1);
        let mut pixels = Vec::new();
        for (i, p) in frames.iter().flat_map(|f| f.image.pixels()).enumerate() {
            if i % stride == 0 && p[3] > 0 {
                pixels.extend_from_slice(&p.0);
            }
        }
        if pixels.is_empty() {
            pixels.extend_from_slice(&[0, 0, 0, 255]);
        }
        let nq = NeuQuant::new(10, 255, &pixels);
        let mut palette = vec![[0u8; 4]];
        palette.extend(nq.color_map_rgba().as_chunks::<4>().0.iter().copied());
        Self {
            nq: Some(nq),
            palette,
            cache: HashMap::new(),
        }
    }

    fn indices(&mut self, img: &RgbaImage) -> Vec<u8> {
        let Self { nq, cache, .. } = self;
        img.pixels()
            .map(|p| {
                if p[3] == 0 {
                    0
                } else {
                    *cache
                        .entry(p.0)
                        .or_insert_with(|| nq.as_ref().map_or(0, |nq| nq.index_of(&p.0) as u8 + 1))
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// APNG « façon LINE » : images complètes, lu `plays` fois.
    fn make_apng(w: u32, h: u32, frames: &[RgbaImage], plays: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_animated(frames.len() as u32, plays).unwrap();
        enc.set_frame_delay(1, 20).unwrap();
        let mut writer = enc.write_header().unwrap();
        for f in frames {
            writer.write_image_data(f.as_raw()).unwrap();
        }
        writer.finish().unwrap();
        out
    }

    fn png_bytes(img: &RgbaImage) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// Bruit pseudo-aléatoire déterministe.
    fn noise(seed: u32) -> impl FnMut() -> u8 {
        let mut s = seed.wrapping_mul(2_654_435_761).max(1);
        move || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s >> 24) as u8
        }
    }

    fn moving_square(w: u32, h: u32, n: u32) -> Vec<RgbaImage> {
        (0..n)
            .map(|i| {
                RgbaImage::from_fn(w, h, |x, y| {
                    let inside = (i * 10..i * 10 + 60).contains(&x) && (50..110).contains(&y);
                    if inside {
                        Rgba([200, 30, 30, 255])
                    } else {
                        Rgba([0, 0, 0, 0])
                    }
                })
            })
            .collect()
    }

    #[test]
    fn static_upscaled_to_square() {
        let src = png_bytes(&RgbaImage::from_pixel(300, 200, Rgba([10, 200, 10, 255])));
        let c = convert(&src, FitMode::Upscale).unwrap();
        assert!(!c.animated && c.fits() && c.note.is_none());
        let img = image::load_from_memory(&c.bytes).unwrap();
        assert_eq!((img.width(), img.height()), (512, 512));
        // Bandes transparentes en haut et en bas.
        assert_eq!(img.to_rgba8().get_pixel(256, 0)[3], 0);
        assert_eq!(img.to_rgba8().get_pixel(256, 256)[3], 255);
    }

    #[test]
    fn static_original_size() {
        let src = png_bytes(&RgbaImage::from_pixel(300, 200, Rgba([10, 200, 10, 255])));
        let c = convert(&src, FitMode::Original).unwrap();
        assert_eq!(c.side, 300);
    }

    #[test]
    fn animated_loops_forever_and_is_square() {
        let frames = moving_square(370, 300, 10);
        let src = make_apng(370, 300, &frames, 3);
        assert_eq!(apng_info(&src), Some((10, 3)));

        let c = convert(&src, FitMode::Upscale).unwrap();
        assert!(c.animated && c.fits());
        let (count, plays) = apng_info(&c.bytes).unwrap();
        assert_eq!((count, plays), (10, 0));

        let decoded = decode_frames(&c.bytes).unwrap();
        assert_eq!(decoded.len(), 10);
        assert_eq!(decoded[0].image.dimensions(), (512, 512));
        assert_eq!(decoded[3].delay_ms, 50);
    }

    #[test]
    fn identical_frames_are_merged() {
        let frame = RgbaImage::from_pixel(100, 100, Rgba([1, 2, 3, 255]));
        let mut frames = moving_square(100, 100, 2);
        frames.insert(1, frames[0].clone());
        frames.push(frame);
        let c = convert(&make_apng(100, 100, &frames, 1), FitMode::Original).unwrap();
        let decoded = decode_frames(&c.bytes).unwrap();
        let delays: Vec<_> = decoded.iter().map(|f| f.delay_ms).collect();
        assert_eq!(delays, [100, 50, 50]);
    }

    #[test]
    fn heavy_animation_is_reduced_under_limit() {
        let mut rng = noise(7);
        let frames: Vec<_> = (0..12)
            .map(|_| {
                RgbaImage::from_fn(370, 300, |x, y| {
                    if (100..250).contains(&x) && (75..225).contains(&y) {
                        Rgba([rng(), rng(), rng(), 255])
                    } else {
                        Rgba([0, 0, 0, 0])
                    }
                })
            })
            .collect();
        let c = convert(&make_apng(370, 300, &frames, 1), FitMode::Upscale).unwrap();
        assert!(c.fits(), "{} octets", c.bytes.len());
        assert!(c.note.is_some());
        assert_eq!(apng_info(&c.bytes).unwrap().1, 0);
    }

    #[test]
    fn apng_info_ignores_plain_png() {
        let src = png_bytes(&RgbaImage::new(4, 4));
        assert_eq!(apng_info(&src), None);
        assert_eq!(apng_info(b"not a png"), None);
    }
}
