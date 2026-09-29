//! Cache de textures : décodage de tous les formats d'image, y compris animés (GIF, WebP, APNG).

use egui::{self, ColorImage, TextureHandle, TextureOptions};
use image::{AnimationDecoder, ImageFormat};
use std::collections::HashMap;
use std::io::Cursor;

use crate::assets::AssetSource;

pub struct Frame {
    pub texture: TextureHandle,
    pub delay: f32,
}

pub struct LoadedImage {
    pub frames: Vec<Frame>,
    pub size: egui::Vec2,
    total: f32,
}

impl LoadedImage {
    /// Texture à afficher au temps `t` (secondes) pour les images animées.
    pub fn frame_at(&self, t: f64) -> &TextureHandle {
        if self.frames.len() <= 1 || self.total <= 0.0 {
            return &self.frames[0].texture;
        }
        let mut t = (t % self.total as f64) as f32;
        for f in &self.frames {
            if t < f.delay {
                return &f.texture;
            }
            t -= f.delay;
        }
        &self.frames[0].texture
    }

    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }
}

#[derive(Default)]
pub struct ImageCache {
    map: HashMap<String, Result<LoadedImage, String>>,
}

impl ImageCache {
    pub fn get(
        &mut self,
        ctx: &egui::Context,
        assets: &dyn AssetSource,
        id: &str,
    ) -> Option<&LoadedImage> {
        if !self.map.contains_key(id) {
            let res = match assets.bytes(id) {
                Some(bytes) => load(ctx, id, &bytes),
                None => Err(format!("ressource introuvable : {id}")),
            };
            self.map.insert(id.to_string(), res);
        }
        self.map.get(id).and_then(|r| r.as_ref().ok())
    }

    pub fn error(&self, id: &str) -> Option<&str> {
        self.map
            .get(id)
            .and_then(|r| r.as_ref().err())
            .map(|s| s.as_str())
    }

    /// Oublie une image (après remplacement du fichier).
    pub fn invalidate(&mut self, id: &str) {
        self.map.remove(id);
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

fn load(ctx: &egui::Context, id: &str, bytes: &[u8]) -> Result<LoadedImage, String> {
    match decode(bytes, id) {
        Ok(frames) => Ok(upload(ctx, id, frames)),
        // Format non géré en Rust pur (ex: AVIF, HEIC, JPEG-XL, SVG...) : conversion via ffmpeg.
        Err(e) => match crate::ffmpeg::convert(bytes, "png") {
            Some(png) => decode(&png, "x.png")
                .map(|f| upload(ctx, id, f))
                .map_err(|e| e.to_string()),
            None => Err(format!("{id} : {e}")),
        },
    }
}

fn upload(ctx: &egui::Context, id: &str, frames: Vec<(ColorImage, f32)>) -> LoadedImage {
    let size = egui::vec2(frames[0].0.size[0] as f32, frames[0].0.size[1] as f32);
    let total = frames.iter().map(|f| f.1).sum();
    let frames = frames
        .into_iter()
        .enumerate()
        .map(|(i, (img, delay))| Frame {
            texture: ctx.load_texture(format!("{id}#{i}"), img, TextureOptions::LINEAR),
            delay,
        })
        .collect();
    LoadedImage {
        frames,
        size,
        total,
    }
}

type Frames = Vec<(ColorImage, f32)>;

fn decode(bytes: &[u8], name: &str) -> image::ImageResult<Frames> {
    // Certains formats (TGA...) n'ont pas de signature : on se base alors sur l'extension.
    let format =
        image::guess_format(bytes).or_else(|e| ImageFormat::from_path(name).map_err(|_| e))?;
    let animated = match format {
        ImageFormat::Gif => {
            collect(image::codecs::gif::GifDecoder::new(Cursor::new(bytes))?.into_frames())
        }
        ImageFormat::WebP => {
            let d = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))?;
            if d.has_animation() {
                collect(d.into_frames())
            } else {
                None
            }
        }
        ImageFormat::Png => {
            let d = image::codecs::png::PngDecoder::new(Cursor::new(bytes))?;
            if d.is_apng()? {
                collect(d.apng()?.into_frames())
            } else {
                None
            }
        }
        _ => None,
    };
    if let Some(frames) = animated.filter(|f| !f.is_empty()) {
        return Ok(frames);
    }
    let img = image::load_from_memory_with_format(bytes, format)?.to_rgba8();
    Ok(vec![(to_color_image(img), 0.0)])
}

fn collect(frames: image::Frames<'_>) -> Option<Frames> {
    let mut out = Vec::new();
    for f in frames {
        let f = f.ok()?;
        let (n, d) = f.delay().numer_denom_ms();
        let delay = if d == 0 {
            0.1
        } else {
            (n as f32 / d as f32 / 1000.0).max(0.02)
        };
        out.push((to_color_image(f.into_buffer()), delay));
    }
    Some(out)
}

fn to_color_image(img: image::RgbaImage) -> ColorImage {
    let size = [img.width() as usize, img.height() as usize];
    ColorImage::from_rgba_unmultiplied(size, img.as_raw())
}
