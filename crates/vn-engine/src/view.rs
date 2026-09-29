//! Affichage du jeu dans egui. Utilisé à la fois par l'aperçu de l'éditeur et par le jeu compilé.

use egui::{self, Align2, Color32, FontId, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::assets::AssetSource;
use crate::audio::Audio;
use crate::images::ImageCache;
use crate::video::VideoPlayer;
use vn_core::model::{Node, Project};
use vn_core::runtime::{Runtime, Waiting};
use vn_core::script::Effect;

pub struct GameView {
    pub images: ImageCache,
    pub audio: Audio,
    /// Vitesse d'apparition du texte (caractères / seconde).
    pub text_speed: f32,
    video: Option<(String, Result<VideoPlayer, String>)>,
    shake: Option<(f32, f32, f32)>,
    flash: Option<([u8; 3], f32, f32)>,
    toasts: Vec<(String, f64)>,
    line: (String, usize),
    line_start: f64,
}

impl Default for GameView {
    fn default() -> Self {
        Self::new()
    }
}

fn c(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

impl GameView {
    pub fn new() -> Self {
        Self {
            images: ImageCache::default(),
            audio: Audio::new(),
            text_speed: 45.0,
            video: None,
            shake: None,
            flash: None,
            toasts: Vec::new(),
            line: (String::new(), usize::MAX),
            line_start: 0.0,
        }
    }

    pub fn toast(&mut self, ui: &egui::Ui, text: impl Into<String>) {
        let now = ui.input(|i| i.time);
        self.toasts.push((text.into(), now + 3.0));
    }

    /// Dessine le jeu dans tout l'espace disponible et gère les entrées du joueur.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        rt: &mut Runtime,
        assets: &dyn AssetSource,
    ) {
        let ctx = ui.ctx().clone();
        let (now, dt) = ui.input(|i| (i.time, i.stable_dt.min(0.1)));
        rt.update(project, dt);

        for effect in rt.take_effects() {
            match effect {
                Effect::Shake { intensity, seconds } => {
                    self.shake = Some((intensity, seconds, seconds))
                }
                Effect::Flash { color, seconds } => self.flash = Some((color, seconds, seconds)),
                Effect::Notify(t) => self.toasts.push((t, now + 3.0)),
                Effect::PlaySound { asset, volume } => {
                    self.audio.play_sound(&asset, volume, assets)
                }
                Effect::Log(_) => {}
            }
        }
        self.audio.sync_music(rt.stage.music.as_ref(), assets);

        // --- Zone de jeu au bon ratio (bandes noires autour) ---
        let (outer, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        let painter = ui.painter_at(outer);
        painter.rect_filled(outer, 0.0, Color32::BLACK);
        let ratio = project.resolution[0].max(1) as f32 / project.resolution[1].max(1) as f32;
        let mut size = Vec2::new(outer.width(), outer.width() / ratio);
        if size.y > outer.height() {
            size = Vec2::new(outer.height() * ratio, outer.height());
        }
        let mut rect = Rect::from_center_size(outer.center(), size);
        if let Some((intensity, left, total)) = &mut self.shake {
            let a = *intensity * (*left / *total) * rect.height() / 720.0;
            rect = rect.translate(Vec2::new(
                (now * 57.0).sin() as f32 * a,
                (now * 43.0).cos() as f32 * a,
            ));
            *left -= dt;
            if *left <= 0.0 {
                self.shake = None;
            }
        }
        let s = rect.height() / 720.0; // échelle des éléments d'interface
        let t = now;

        // --- Décor ---
        painter.rect_filled(rect, 0.0, c(rt.stage.background_color));
        if let Some(bg) = rt.stage.background.clone() {
            match self.images.get(&ctx, assets, &bg) {
                Some(img) => {
                    let tex = img.frame_at(t).id();
                    painter.image(tex, cover(rect, img.size), full_uv(), Color32::WHITE);
                }
                None => {
                    let err = self.images.error(&bg).unwrap_or("").to_string();
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        err,
                        FontId::proportional(14.0),
                        Color32::LIGHT_RED,
                    );
                }
            }
        }

        // --- Personnages ---
        for sc in rt.stage.characters.clone() {
            let ch = project.character(&sc.id);
            let sprite = ch.and_then(|ch| {
                ch.sprites
                    .get(&sc.expression)
                    .or_else(|| ch.sprites.values().next())
                    .cloned()
            });
            let x = rect.left() + rect.width() * sc.position.x();
            let h = rect.height() * 0.85;
            match sprite
                .as_deref()
                .and_then(|id| self.images.get(&ctx, assets, id))
            {
                Some(img) => {
                    let w = h * img.size.x / img.size.y.max(1.0);
                    let r = Rect::from_min_max(
                        Pos2::new(x - w / 2.0, rect.bottom() - h),
                        Pos2::new(x + w / 2.0, rect.bottom()),
                    );
                    painter.image(img.frame_at(t).id(), r, full_uv(), Color32::WHITE);
                }
                None => {
                    // Silhouette provisoire tant qu'aucun sprite n'est défini.
                    let col = ch.map(|ch| c(ch.color)).unwrap_or(Color32::GRAY);
                    let w = h * 0.35;
                    let body = Rect::from_min_max(
                        Pos2::new(x - w / 2.0, rect.bottom() - h * 0.75),
                        Pos2::new(x + w / 2.0, rect.bottom()),
                    );
                    painter.rect_filled(body, 40.0 * s, col.gamma_multiply(0.7));
                    painter.circle_filled(
                        Pos2::new(x, rect.bottom() - h * 0.85),
                        h * 0.12,
                        col.gamma_multiply(0.7),
                    );
                    let name = ch.map(|c| c.name.clone()).unwrap_or(sc.id.clone());
                    painter.text(
                        body.center(),
                        Align2::CENTER_CENTER,
                        name,
                        FontId::proportional(20.0 * s),
                        Color32::WHITE,
                    );
                }
            }
        }

        // --- Vidéo ---
        let mut advance_clicked = response.clicked();
        if let Waiting::Video { asset, skippable } = rt.waiting.clone() {
            let key = format!("{}#{}", rt.scene, rt.index);
            if self.video.as_ref().map(|v| &v.0) != Some(&key) {
                let player = match assets.file_path(&asset) {
                    Some(path) => {
                        VideoPlayer::start(&path, project.resolution, self.audio.new_player())
                    }
                    None => Err(format!("vidéo introuvable : {asset}")),
                };
                if let Err(e) = &player {
                    self.toasts.push((format!("⚠ {e}"), now + 4.0));
                }
                self.video = Some((key, player));
            }
            let done = match &mut self.video.as_mut().unwrap().1 {
                Ok(player) => {
                    painter.rect_filled(rect, 0.0, Color32::BLACK);
                    if let Some(tex) = player.texture(&ctx) {
                        painter.image(tex.id(), rect, full_uv(), Color32::WHITE);
                    }
                    if skippable {
                        painter.text(
                            rect.right_bottom() - Vec2::splat(10.0 * s),
                            Align2::RIGHT_BOTTOM,
                            "Cliquer pour passer ⏭",
                            FontId::proportional(14.0 * s),
                            Color32::from_white_alpha(160),
                        );
                    }
                    player.finished || (skippable && advance_clicked)
                }
                Err(_) => true,
            };
            advance_clicked = false;
            if done {
                self.video = None;
                rt.video_finished(project);
            }
        } else {
            self.video = None;
        }

        // --- Dialogue ---
        let line_key = (rt.scene.clone(), rt.index);
        if self.line != line_key {
            self.line = line_key;
            self.line_start = now;
        }
        let mut fully_shown = true;
        if rt.waiting == Waiting::Dialogue {
            if let Some(Node::Dialogue { speaker, text }) = rt.current_node(project) {
                let text = interpolate(text, rt);
                let total = text.chars().count();
                let shown = (((now - self.line_start) as f32) * self.text_speed) as usize;
                fully_shown = shown >= total;
                let visible: String = text.chars().take(shown.min(total)).collect();
                let bx = Rect::from_min_max(
                    Pos2::new(rect.left() + 30.0 * s, rect.bottom() - 200.0 * s),
                    Pos2::new(rect.right() - 30.0 * s, rect.bottom() - 20.0 * s),
                );
                painter.rect_filled(bx, 14.0 * s, Color32::from_black_alpha(200));
                painter.rect_stroke(
                    bx,
                    14.0 * s,
                    Stroke::new(2.0 * s, Color32::from_white_alpha(60)),
                    StrokeKind::Inside,
                );
                if let Some(sp) = speaker.as_deref().filter(|s| !s.is_empty()) {
                    let (name, col) = match project.character(sp) {
                        Some(ch) => (ch.name.clone(), c(ch.color)),
                        None => (sp.to_string(), Color32::WHITE),
                    };
                    let g = painter.layout_no_wrap(name, FontId::proportional(26.0 * s), col);
                    let tag = Rect::from_min_size(
                        bx.left_top() + Vec2::new(20.0 * s, -22.0 * s),
                        g.size() + Vec2::new(28.0 * s, 10.0 * s),
                    );
                    painter.rect_filled(tag, 8.0 * s, Color32::from_black_alpha(230));
                    painter.galley(tag.min + Vec2::new(14.0 * s, 5.0 * s), g, col);
                }
                let g = painter.layout(
                    visible,
                    FontId::proportional(28.0 * s),
                    Color32::WHITE,
                    bx.width() - 60.0 * s,
                );
                painter.galley(bx.min + Vec2::new(30.0 * s, 32.0 * s), g, Color32::WHITE);
                if fully_shown {
                    let blink = ((now * 3.0).sin() * 0.5 + 0.5) as f32;
                    painter.text(
                        bx.right_bottom() - Vec2::splat(18.0 * s),
                        Align2::RIGHT_BOTTOM,
                        "▼",
                        FontId::proportional(20.0 * s),
                        Color32::from_white_alpha((80.0 + 175.0 * blink) as u8),
                    );
                }
            }
        }

        // --- Choix ---
        if rt.waiting == Waiting::Choice {
            if let Some(Node::Choice { prompt, options }) = rt.current_node(project) {
                let options: Vec<String> =
                    options.iter().map(|o| interpolate(&o.text, rt)).collect();
                let prompt = interpolate(prompt, rt);
                painter.rect_filled(rect, 0.0, Color32::from_black_alpha(90));
                let bh = 56.0 * s;
                let gap = 16.0 * s;
                let total_h = options.len() as f32 * (bh + gap);
                let mut y = rect.center().y - total_h / 2.0;
                if !prompt.is_empty() {
                    painter.text(
                        Pos2::new(rect.center().x, y - 30.0 * s),
                        Align2::CENTER_BOTTOM,
                        prompt,
                        FontId::proportional(30.0 * s),
                        Color32::WHITE,
                    );
                }
                let mut chosen = None;
                for (i, text) in options.iter().enumerate() {
                    let r = Rect::from_center_size(
                        Pos2::new(rect.center().x, y + bh / 2.0),
                        Vec2::new(rect.width() * 0.55, bh),
                    );
                    let resp = ui.interact(r, ui.id().with(("choice", i)), Sense::click());
                    let hovered = resp.hovered();
                    painter.rect_filled(
                        r,
                        12.0 * s,
                        if hovered {
                            Color32::from_rgb(90, 80, 160)
                        } else {
                            Color32::from_black_alpha(210)
                        },
                    );
                    painter.rect_stroke(
                        r,
                        12.0 * s,
                        Stroke::new(
                            2.0 * s,
                            Color32::from_white_alpha(if hovered { 200 } else { 70 }),
                        ),
                        StrokeKind::Inside,
                    );
                    painter.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        text,
                        FontId::proportional(24.0 * s),
                        Color32::WHITE,
                    );
                    if resp.clicked() {
                        chosen = Some(i);
                    }
                    y += bh + gap;
                }
                if let Some(i) = chosen {
                    rt.choose(project, i);
                }
                advance_clicked = false;
            }
        }

        // --- Fin ---
        if rt.waiting == Waiting::Ended {
            painter.rect_filled(rect, 0.0, Color32::from_black_alpha(170));
            painter.text(
                rect.center() - Vec2::new(0.0, 40.0 * s),
                Align2::CENTER_CENTER,
                "FIN",
                FontId::proportional(72.0 * s),
                Color32::WHITE,
            );
            let r = Rect::from_center_size(
                rect.center() + Vec2::new(0.0, 50.0 * s),
                Vec2::new(220.0 * s, 54.0 * s),
            );
            let resp = ui.interact(r, ui.id().with("replay"), Sense::click());
            painter.rect_filled(
                r,
                12.0 * s,
                if resp.hovered() {
                    Color32::from_rgb(90, 80, 160)
                } else {
                    Color32::from_gray(40)
                },
            );
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                "↻ Rejouer",
                FontId::proportional(24.0 * s),
                Color32::WHITE,
            );
            if resp.clicked() {
                self.audio.stop_all();
                rt.restart(project);
            }
            advance_clicked = false;
        }

        // --- Entrées : clic, toucher, espace, entrée, flèche droite ---
        let key_next = !ctx.egui_wants_keyboard_input()
            && ui.input(|i| {
                i.key_pressed(Key::Space)
                    || i.key_pressed(Key::Enter)
                    || i.key_pressed(Key::ArrowRight)
            });
        if (advance_clicked || key_next) && rt.waiting == Waiting::Dialogue {
            if fully_shown {
                rt.advance(project);
            } else {
                self.line_start = f64::MIN / 4.0; // affiche tout le texte d'un coup
            }
        }

        // --- Flash & notifications ---
        if let Some((col, left, total)) = &mut self.flash {
            let a = (*left / *total).clamp(0.0, 1.0);
            painter.rect_filled(
                rect,
                0.0,
                Color32::from_rgba_unmultiplied(col[0], col[1], col[2], (a * 255.0) as u8),
            );
            *left -= dt;
            if *left <= 0.0 {
                self.flash = None;
            }
        }
        self.toasts.retain(|(_, until)| *until > now);
        let mut y = rect.top() + 14.0 * s;
        for (text, until) in &self.toasts {
            let alpha = ((until - now).min(0.5) * 2.0) as f32;
            let g = painter.layout_no_wrap(
                text.clone(),
                FontId::proportional(18.0 * s),
                Color32::WHITE.gamma_multiply(alpha),
            );
            let r = Rect::from_min_size(
                Pos2::new(rect.right() - g.size().x - 40.0 * s, y),
                g.size() + Vec2::new(24.0 * s, 12.0 * s),
            );
            painter.rect_filled(r, 8.0 * s, Color32::from_black_alpha((200.0 * alpha) as u8));
            painter.galley(r.min + Vec2::new(12.0 * s, 6.0 * s), g, Color32::WHITE);
            y += r.height() + 8.0 * s;
        }

        ctx.request_repaint();
    }
}

fn full_uv() -> Rect {
    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
}

/// Rectangle qui couvre `rect` en gardant le ratio de l'image (comme `object-fit: cover`).
fn cover(rect: Rect, img: Vec2) -> Rect {
    let scale = (rect.width() / img.x.max(1.0)).max(rect.height() / img.y.max(1.0));
    Rect::from_center_size(rect.center(), img * scale)
}

/// Remplace `{variable}` par sa valeur dans un texte.
fn interpolate(text: &str, rt: &Runtime) -> String {
    if !text.contains('{') {
        return text.to_string();
    }
    let vars = rt.script.vars();
    let mut out = text.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{k}}}"), v.trim_matches('"'));
    }
    out
}
