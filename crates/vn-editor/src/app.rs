//! Fenêtre principale de l'éditeur.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use eframe::egui::{self, Color32, RichText};
use vn_core::model::{Node, Project};
use vn_core::runtime::{Runtime, Waiting};
use vn_engine::assets::DirAssets;
use vn_engine::images::ImageCache;
use vn_engine::GameView;

use crate::extensions::ExtUi;
use crate::nodes::{self, Structural};
use crate::project_io;
use crate::updater::{self, Updater};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Scenario,
    Extensions,
    Settings,
}

/// Le jeu en cours de test dans l'éditeur.
pub struct Preview {
    pub rt: Runtime,
    pub view: GameView,
    pub detached: bool,
}

pub struct EditorApp {
    pub project: Project,
    pub dir: PathBuf,
    pub dirty: bool,
    /// Modifié pendant cette image : l'aperçu doit se resynchroniser.
    pub changed: bool,
    pub last_change: f64,
    pub autosave: bool,
    pub page: Page,
    pub scene: String,
    pub node: Option<usize>,
    pub character: Option<String>,
    pub preview: Option<Preview>,
    pub thumbs: ImageCache,
    pub ext: ExtUi,
    pub updater: Updater,
    pub auto_check_updates: bool,
    pub status: String,
    pub log: Vec<String>,
    pub path_input: String,
    pub export_report: Option<String>,
    last_scan: f64,
    mtimes: HashMap<String, SystemTime>,
}

impl EditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>, start_dir: Option<PathBuf>) -> Self {
        setup_style(&cc.egui_ctx);
        let _ = updater::exe_path();
        let dir = start_dir
            .unwrap_or_else(|| project_io::default_projects_dir().join("mon_visual_novel"));
        let mut app = Self {
            project: Project::default(),
            dir: dir.clone(),
            dirty: false,
            changed: false,
            last_change: 0.0,
            autosave: true,
            page: Page::Scenario,
            scene: String::new(),
            node: None,
            character: None,
            preview: None,
            thumbs: ImageCache::default(),
            ext: ExtUi::default(),
            updater: Updater::default(),
            auto_check_updates: true,
            status: String::new(),
            log: Vec::new(),
            path_input: dir.display().to_string(),
            export_report: None,
            last_scan: 0.0,
            mtimes: HashMap::new(),
        };
        app.open(dir);
        if app.auto_check_updates {
            app.updater.check(cc.egui_ctx.clone());
        }
        app
    }

    pub fn assets(&self) -> DirAssets {
        DirAssets {
            dir: self.dir.join(project_io::ASSETS_DIR),
        }
    }

    /// Ouvre un projet (ou le crée s'il n'existe pas encore).
    pub fn open(&mut self, dir: PathBuf) {
        self.preview = None;
        match project_io::load(&dir) {
            Ok(p) => {
                self.project = p;
                self.status = format!("Projet ouvert : {}", dir.display());
            }
            Err(_) if !dir.join(project_io::PROJECT_FILE).exists() => {
                self.project = Project::default();
                match project_io::save(&dir, &self.project) {
                    Ok(()) => self.status = format!("Nouveau projet créé : {}", dir.display()),
                    Err(e) => self.status = format!("⚠ Impossible de créer le projet : {e}"),
                }
            }
            Err(e) => {
                self.status = format!("⚠ {e}");
                return;
            }
        }
        self.dir = dir;
        self.path_input = self.dir.display().to_string();
        self.scene = self.project.start_scene.clone();
        if self.project.scene(&self.scene).is_none() {
            self.scene = self
                .project
                .scenes
                .first()
                .map(|s| s.id.clone())
                .unwrap_or_default();
        }
        self.node = None;
        self.thumbs.clear();
        self.mtimes.clear();
        self.dirty = false;
    }

    pub fn save(&mut self) {
        match project_io::save(&self.dir, &self.project) {
            Ok(()) => {
                self.dirty = false;
                self.status = "💾 Enregistré".into();
            }
            Err(e) => self.status = format!("⚠ Échec de l'enregistrement : {e}"),
        }
    }

    pub fn mark_changed(&mut self) {
        self.changed = true;
        self.dirty = true;
    }

    pub fn play_from(&mut self, scene: Option<(String, usize)>) {
        let mut rt = Runtime::new(&self.project);
        if let Some((s, i)) = scene {
            rt.goto(&self.project, &s, i);
        }
        let detached = self.preview.as_ref().map(|p| p.detached).unwrap_or(false);
        let view = self.preview.take().map(|p| p.view).unwrap_or_default();
        self.preview = Some(Preview { rt, view, detached });
    }

    /// Garde l'aperçu sur le même nœud quand on insère/supprime/déplace des nœuds.
    fn adjust_preview(&mut self, st: &Structural) {
        let Some(p) = &mut self.preview else { return };
        if p.rt.scene != self.scene {
            return;
        }
        let idx = &mut p.rt.index;
        match *st {
            Structural::Inserted(i) if i <= *idx => *idx += 1,
            Structural::Removed(i) if i < *idx => *idx -= 1,
            Structural::Moved { from, to } if from == *idx => *idx = to,
            Structural::Moved { from, to } if to == *idx => *idx = from,
            _ => {}
        }
    }

    /// Détecte les fichiers de ressources modifiés à l'extérieur (Photoshop, Audacity...)
    /// pour les recharger à chaud dans l'aperçu.
    fn scan_assets(&mut self, now: f64) {
        if now - self.last_scan < 1.0 {
            return;
        }
        self.last_scan = now;
        if project_io::sync_assets(&self.dir, &mut self.project) {
            self.mark_changed();
        }
        let dir = self.dir.join(project_io::ASSETS_DIR);
        for a in &self.project.assets {
            let Ok(m) = std::fs::metadata(dir.join(&a.id)).and_then(|m| m.modified()) else {
                continue;
            };
            if let Some(old) = self.mtimes.insert(a.id.clone(), m) {
                if old != m {
                    self.thumbs.invalidate(&a.id);
                    if let Some(p) = &mut self.preview {
                        p.view.images.invalidate(&a.id);
                    }
                    self.status = format!("♻ {} rechargé", a.id);
                }
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let res = match (&f.path, &f.bytes) {
                (Some(path), _) if path.extension().map(|e| e == "vnext").unwrap_or(false) => {
                    self.install_extension_file(path);
                    continue;
                }
                (Some(path), _) => project_io::import_asset(&self.dir, &mut self.project, path),
                (None, Some(bytes)) => {
                    project_io::import_asset_bytes(&self.dir, &mut self.project, &f.name, bytes)
                }
                _ => continue,
            };
            match res {
                Ok(id) => {
                    self.status = format!("➕ {id} importé");
                    self.thumbs.invalidate(&id);
                    if let Some(p) = &mut self.preview {
                        p.view.images.invalidate(&id);
                    }
                    self.mark_changed();
                }
                Err(e) => self.status = format!("⚠ {e}"),
            }
        }
    }

    pub fn import_dialog(&mut self) {
        #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
        if let Some(files) = rfd::FileDialog::new()
            .set_title("Importer des ressources (images, sons, vidéos)")
            .add_filter(
                "Médias",
                &[
                    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "tga", "ico", "dds",
                    "hdr", "exr", "qoi", "pnm", "avif", "heic", "jxl", "svg", "mp3", "ogg", "opus",
                    "wav", "flac", "aac", "m4a", "aiff", "wma", "mp4", "webm", "mkv", "avi", "mov",
                    "wmv", "flv", "mpg", "mpeg", "ogv", "3gp",
                ],
            )
            .add_filter("Tous les fichiers", &["*"])
            .pick_files()
        {
            for f in files {
                match project_io::import_asset(&self.dir, &mut self.project, &f) {
                    Ok(id) => {
                        self.thumbs.invalidate(&id);
                        self.status = format!("➕ {id} importé");
                    }
                    Err(e) => self.status = format!("⚠ {e}"),
                }
            }
            self.mark_changed();
        }
    }

    fn compile_game(&mut self) {
        self.save();
        let warnings = project_io::validate(&self.project);
        let out_root = self.dir.join("build");
        let mut report = String::new();
        if !warnings.is_empty() {
            report.push_str("⚠ Avertissements :\n");
            for w in &warnings {
                report.push_str(&format!("  • {w}\n"));
            }
            report.push('\n');
        }
        match project_io::export_game(&self.dir, &self.project, &out_root) {
            Ok(r) => {
                report.push_str(&format!("✅ Jeu compilé dans : {}\n", r.out_dir.display()));
                if let Some(exe) = &r.executable {
                    report.push_str(&format!("🎮 Exécutable autonome : {}\n", exe.display()));
                }
                report.push_str(&format!("📦 Pack verrouillé : {}\n", r.pack.display()));
                for n in r.notes {
                    report.push_str(&format!("ℹ {n}\n"));
                }
                self.status = "✅ Jeu compilé".into();
            }
            Err(e) => {
                report.push_str(&format!("❌ Échec : {e}"));
                self.status = "❌ Compilation échouée".into();
            }
        }
        self.export_report = Some(report);
    }

    fn open_folder_dialog(&mut self, create: bool) {
        #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
        {
            let _ = std::fs::create_dir_all(project_io::default_projects_dir());
            let dialog = rfd::FileDialog::new()
                .set_directory(project_io::default_projects_dir())
                .set_title(if create {
                    "Choisir un dossier vide pour le nouveau projet"
                } else {
                    "Ouvrir un dossier de projet"
                });
            if let Some(dir) = dialog.pick_folder() {
                if self.dirty {
                    self.save();
                }
                if create && dir.join(project_io::PROJECT_FILE).exists() {
                    self.status = "⚠ Ce dossier contient déjà un projet".into();
                    return;
                }
                self.open(dir);
            }
        }
        #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
        {
            let _ = create;
            self.status = "Saisis le chemin du projet dans la barre du bas puis « Ouvrir ».".into();
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("📁 Projet", |ui| {
                if ui.button("🆕 Nouveau projet…").clicked() {
                    ui.close();
                    self.open_folder_dialog(true);
                }
                if ui.button("📂 Ouvrir un projet…").clicked() {
                    ui.close();
                    self.open_folder_dialog(false);
                }
                if ui.button("💾 Enregistrer   (Ctrl+S)").clicked() {
                    ui.close();
                    self.save();
                }
                ui.separator();
                if ui.button("📥 Importer des ressources…").clicked() {
                    ui.close();
                    self.import_dialog();
                }
                if ui.button("🏗 Compiler le jeu").clicked() {
                    ui.close();
                    self.compile_game();
                }
            });
            ui.separator();
            ui.selectable_value(&mut self.page, Page::Scenario, "📝 Scénario");
            // Le bouton demandé : ouvre la page des extensions en un clic, à tout moment.
            let ext_btn = egui::Button::new(
                RichText::new("🧩 Extensions")
                    .strong()
                    .color(Color32::WHITE),
            )
            .fill(if self.page == Page::Extensions {
                Color32::from_rgb(120, 90, 200)
            } else {
                Color32::from_rgb(90, 70, 160)
            });
            if ui
                .add(ext_btn)
                .on_hover_text("Ajouter / créer des extensions (même pendant le test du jeu)")
                .clicked()
            {
                self.page = if self.page == Page::Extensions {
                    Page::Scenario
                } else {
                    Page::Extensions
                };
            }
            ui.selectable_value(&mut self.page, Page::Settings, "⚙ Paramètres");
            ui.separator();

            if self.preview.is_none() {
                if ui
                    .button(
                        RichText::new("▶ Jouer")
                            .color(Color32::from_rgb(90, 210, 120))
                            .strong(),
                    )
                    .clicked()
                {
                    self.play_from(None);
                }
            } else if ui
                .button(
                    RichText::new("⏹ Arrêter")
                        .color(Color32::from_rgb(230, 90, 90))
                        .strong(),
                )
                .clicked()
            {
                self.preview = None;
            }
            if ui
                .button("🏗 Compiler")
                .on_hover_text("Crée le jeu final, non modifiable par les joueurs")
                .clicked()
            {
                self.compile_game();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let updater::State::Available { version, .. } = self.updater.get() {
                    if ui
                        .button(
                            RichText::new(format!("⬆ Mise à jour v{version}")).color(Color32::GOLD),
                        )
                        .clicked()
                    {
                        self.page = Page::Settings;
                    }
                }
                let title = format!(
                    "{}{}",
                    self.project.name,
                    if self.dirty { " •" } else { "" }
                );
                ui.label(RichText::new(title).weak());
            });
        });
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui) {
        // On sort l'aperçu de `self` pour pouvoir l'afficher dans une autre fenêtre.
        let Some(mut p) = self.preview.take() else {
            return;
        };
        let assets = self.assets();
        let project = &self.project;
        let mut stop = false;
        let mut restart = false;

        let controls = |ui: &mut egui::Ui, p: &mut Preview, stop: &mut bool, restart: &mut bool| {
            ui.horizontal(|ui| {
                if ui.button("⏮ Recommencer").clicked() {
                    *restart = true;
                }
                if ui.button("⏹").on_hover_text("Arrêter").clicked() {
                    *stop = true;
                }
                let label = if p.detached {
                    "⤓ Intégrer"
                } else {
                    "🗗 Fenêtre séparée"
                };
                if ui.button(label).clicked() {
                    p.detached = !p.detached;
                }
                let state = match &p.rt.waiting {
                    Waiting::Dialogue => "dialogue",
                    Waiting::Choice => "choix",
                    Waiting::Timer(_) => "attente",
                    Waiting::Video { .. } => "vidéo",
                    Waiting::Ended => "terminé",
                };
                ui.label(RichText::new(format!("{} #{} · {state}", p.rt.scene, p.rt.index)).weak());
            });
        };

        let debug = |ui: &mut egui::Ui, p: &mut Preview| {
            egui::CollapsingHeader::new("🔢 Variables (modifiables en direct)")
                .default_open(false)
                .show(ui, |ui| {
                    let vars = p.rt.script.vars();
                    if vars.is_empty() {
                        ui.label(RichText::new("aucune variable").weak());
                    }
                    for (k, v) in vars {
                        ui.horizontal(|ui| {
                            ui.label(&k);
                            let id = ui.id().with(("var", &k));
                            let mut text = ui
                                .data_mut(|d| d.get_temp::<String>(id))
                                .unwrap_or(v.clone());
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut text)
                                    .desired_width(120.0)
                                    .code_editor(),
                            );
                            if r.changed() {
                                ui.data_mut(|d| d.insert_temp(id, text.clone()));
                            }
                            if r.lost_focus() {
                                let _ = p.rt.script.set_var_expr(&k, &text);
                                ui.data_mut(|d| d.remove::<String>(id));
                            }
                        });
                    }
                });
            egui::CollapsingHeader::new(format!("📜 Console ({})", p.rt.log.len())).show(
                ui,
                |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for l in &p.rt.log {
                                ui.label(RichText::new(l).monospace().size(11.0));
                            }
                        });
                },
            );
        };

        if p.detached {
            let ctx = ui.ctx().clone();
            let size = [
                project.resolution[0] as f32 * 0.75,
                project.resolution[1] as f32 * 0.75 + 40.0,
            ];
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("vn_preview"),
                egui::ViewportBuilder::default()
                    .with_title(format!("Aperçu — {}", project.name))
                    .with_inner_size(size),
                |ui, _class| {
                    egui::Panel::top("preview_controls")
                        .show(ui, |ui| controls(ui, &mut p, &mut stop, &mut restart));
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new())
                        .show(ui, |ui| {
                            p.view.show(ui, project, &mut p.rt, &assets);
                        });
                    if ui.input(|i| i.viewport().close_requested()) {
                        p.detached = false;
                    }
                },
            );
            // Rien dans le panneau principal : la vue est dans l'autre fenêtre.
            egui::Panel::right("preview_panel")
                .resizable(true)
                .default_size(260.0)
                .show(ui, |ui| {
                    ui.heading("🎮 Aperçu (fenêtre séparée)");
                    debug(ui, &mut p);
                });
        } else {
            egui::Panel::right("preview_panel")
                .resizable(true)
                .default_size(560.0)
                .min_size(260.0)
                .show(ui, |ui| {
                    controls(ui, &mut p, &mut stop, &mut restart);
                    let h = (ui.available_width()
                        / (project.resolution[0].max(1) as f32
                            / project.resolution[1].max(1) as f32))
                        .min(ui.available_height() - 60.0)
                        .max(120.0);
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), h),
                        egui::Sense::hover(),
                    );
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                    p.view.show(&mut child, project, &mut p.rt, &assets);
                    debug(ui, &mut p);
                });
        }

        if restart {
            p.view.audio.stop_all();
            p.rt.restart(&self.project);
        }
        if !stop {
            self.preview = Some(p);
        }
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Ouvrir").clicked() {
                        let dir = PathBuf::from(self.path_input.trim());
                        if self.dirty {
                            self.save();
                        }
                        self.open(dir);
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut self.path_input)
                            .desired_width(320.0)
                            .hint_text("dossier du projet"),
                    );
                    ui.label("📂");
                });
            });
        });
    }
}

impl eframe::App for EditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        self.changed = false;

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save();
        }
        self.handle_dropped_files(&ctx);
        self.scan_assets(now);

        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        self.bottom_bar(ui);
        self.preview_ui(ui);

        match self.page {
            Page::Scenario => {
                egui::Panel::left("sidebar")
                    .resizable(true)
                    .default_size(270.0)
                    .show(ui, |ui| self.sidebar_ui(ui));
                egui::CentralPanel::default().show(ui, |ui| {
                    let playing_at = self
                        .preview
                        .as_ref()
                        .filter(|p| p.rt.scene == self.scene)
                        .map(|p| p.rt.index);
                    let scene = self.scene.clone();
                    let out = nodes::scene_editor(
                        ui,
                        &mut self.project,
                        &scene,
                        &mut self.node,
                        playing_at,
                    );
                    if let Some(st) = &out.structural {
                        self.adjust_preview(st);
                    }
                    if out.changed {
                        self.mark_changed();
                    }
                    if let Some(i) = out.play_from {
                        self.play_from(Some((scene, i)));
                    }
                });
            }
            Page::Extensions => {
                egui::CentralPanel::default().show(ui, |ui| self.extensions_page(ui));
            }
            Page::Settings => {
                egui::CentralPanel::default().show(ui, |ui| self.settings_page(ui));
            }
        }

        // Modification en direct : l'aperçu reflète tout de suite l'état du projet.
        if self.changed {
            self.last_change = now;
            if let Some(p) = &mut self.preview {
                p.rt.resync(&self.project);
            }
        }
        if self.autosave && self.dirty && now - self.last_change > 2.0 {
            self.save();
        }
        if self.dirty {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        if let Some(report) = self.export_report.clone() {
            let mut open = true;
            egui::Window::new("🏗 Compilation du jeu")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .show(&ctx, |ui| {
                    ui.label(RichText::new(&report).monospace());
                    if ui.button("OK").clicked() {
                        self.export_report = None;
                    }
                });
            if !open {
                self.export_report = None;
            }
        }
    }
}

fn setup_style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
    });
}

/// Compte les nœuds d'un type (utilisé par le tableau de bord).
pub fn count_nodes(project: &Project, f: impl Fn(&Node) -> bool) -> usize {
    project
        .scenes
        .iter()
        .flat_map(|s| &s.nodes)
        .filter(|n| f(n))
        .count()
}
