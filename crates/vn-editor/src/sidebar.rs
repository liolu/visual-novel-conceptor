//! Panneau latéral (scènes, personnages, ressources) et page des paramètres.

use eframe::egui::{self, Color32, RichText};
use vn_core::model::{AssetKind, Character, Node, Scene};

use crate::app::{count_nodes, EditorApp};
use crate::project_io;
use crate::updater::{self, State};
use crate::widgets::{asset_combo, scene_combo};

impl EditorApp {
    pub fn sidebar_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.scenes_ui(ui);
                ui.add_space(6.0);
                self.characters_ui(ui);
                ui.add_space(6.0);
                self.assets_ui(ui);
            });
    }

    fn scenes_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("🎬 Scènes").strong())
            .default_open(true)
            .show(ui, |ui| {
                let mut delete = None;
                for s in &self.project.scenes {
                    ui.horizontal(|ui| {
                        let start = s.id == self.project.start_scene;
                        let label = format!("{}{}", if start { "★ " } else { "" }, s.name);
                        if ui.selectable_label(self.scene == s.id, label).clicked() {
                            self.scene = s.id.clone();
                            self.node = None;
                        }
                        if self.scene == s.id
                            && self.project.scenes.len() > 1
                            && ui
                                .small_button("🗑")
                                .on_hover_text("Supprimer la scène")
                                .clicked()
                        {
                            delete = Some(s.id.clone());
                        }
                    });
                }
                if let Some(id) = delete {
                    self.project.scenes.retain(|s| s.id != id);
                    self.scene = self.project.scenes[0].id.clone();
                    self.mark_changed();
                }
                ui.horizontal(|ui| {
                    if ui.button("➕ Scène").clicked() {
                        let id = self
                            .project
                            .unique_id("scene", |p, id| p.scene(id).is_some());
                        self.project.scenes.push(Scene {
                            id: id.clone(),
                            name: format!("Scène {}", self.project.scenes.len() + 1),
                            nodes: vec![Node::Dialogue {
                                speaker: None,
                                text: String::new(),
                            }],
                        });
                        self.scene = id;
                        self.node = Some(0);
                        self.mark_changed();
                    }
                    if self.project.start_scene != self.scene
                        && ui
                            .button("★ Départ")
                            .on_hover_text("Le jeu commence par cette scène")
                            .clicked()
                    {
                        self.project.start_scene = self.scene.clone();
                        self.mark_changed();
                    }
                });
                if let Some(scene) = self.project.scene_mut(&self.scene) {
                    ui.horizontal(|ui| {
                        ui.label("Nom :");
                        if ui.text_edit_singleline(&mut scene.name).changed() {
                            self.changed = true;
                            self.dirty = true;
                        }
                    });
                }
            });
    }

    fn characters_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("🧍 Personnages").strong())
            .default_open(true)
            .show(ui, |ui| {
                for c in &self.project.characters {
                    let sel = self.character.as_deref() == Some(c.id.as_str());
                    let text = RichText::new(&c.name)
                        .color(Color32::from_rgb(c.color[0], c.color[1], c.color[2]));
                    if ui.selectable_label(sel, text).clicked() {
                        self.character = if sel { None } else { Some(c.id.clone()) };
                    }
                }
                if ui.button("➕ Personnage").clicked() {
                    let id = self
                        .project
                        .unique_id("perso", |p, id| p.character(id).is_some());
                    self.project.characters.push(Character {
                        id: id.clone(),
                        name: "Nouveau personnage".into(),
                        color: [200, 200, 255],
                        sprites: Default::default(),
                    });
                    self.character = Some(id);
                    self.mark_changed();
                }

                let Some(cid) = self.character.clone() else {
                    return;
                };
                let snapshot = self.project.clone();
                let Some(c) = self.project.characters.iter_mut().find(|c| c.id == cid) else {
                    return;
                };
                let mut ch = false;
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(RichText::new(format!("id : {}", c.id)).weak());
                    ui.horizontal(|ui| {
                        ui.label("Nom");
                        ch |= ui.text_edit_singleline(&mut c.name).changed();
                    });
                    ui.horizontal(|ui| {
                        ui.label("Couleur");
                        ch |= ui.color_edit_button_srgb(&mut c.color).changed();
                    });
                    ui.label("Expressions (sprites) :");
                    let mut remove = None;
                    let keys: Vec<String> = c.sprites.keys().cloned().collect();
                    for k in keys {
                        ui.horizontal(|ui| {
                            ui.label(&k);
                            let v = c.sprites.get_mut(&k).unwrap();
                            ch |=
                                asset_combo(ui, ("spr", &cid, &k), v, &snapshot, AssetKind::Image);
                            if ui.small_button("✖").clicked() {
                                remove = Some(k.clone());
                            }
                        });
                    }
                    if let Some(k) = remove {
                        c.sprites.remove(&k);
                        ch = true;
                    }
                    ui.horizontal(|ui| {
                        let id = ui.id().with("new_expr");
                        let mut name = ui
                            .data_mut(|d| d.get_temp::<String>(id))
                            .unwrap_or_default();
                        ui.add(
                            egui::TextEdit::singleline(&mut name)
                                .hint_text("joyeux, triste…")
                                .desired_width(110.0),
                        );
                        if ui.button("➕").clicked() && !name.trim().is_empty() {
                            c.sprites.insert(name.trim().to_string(), String::new());
                            name.clear();
                            ch = true;
                        }
                        ui.data_mut(|d| d.insert_temp(id, name));
                    });
                    if ui
                        .button(
                            RichText::new("🗑 Supprimer le personnage").color(Color32::LIGHT_RED),
                        )
                        .clicked()
                    {
                        self.character = None;
                        ch = true;
                        c.id.clear(); // marqué pour suppression
                    }
                });
                if ch {
                    self.project.characters.retain(|c| !c.id.is_empty());
                    self.mark_changed();
                }
            });
    }

    fn assets_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(
            RichText::new(format!("🗂 Ressources ({})", self.project.assets.len())).strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            if ui.button("📥 Importer…").clicked() {
                self.import_dialog();
            }
            ui.label(
                RichText::new("ou glisse-dépose des fichiers dans la fenêtre")
                    .weak()
                    .small(),
            );
            let assets = self.assets();
            let mut delete = None;
            for kind in [
                AssetKind::Image,
                AssetKind::Audio,
                AssetKind::Video,
                AssetKind::Other,
            ] {
                let list: Vec<_> = self
                    .project
                    .assets
                    .iter()
                    .filter(|a| a.kind == kind)
                    .map(|a| a.id.clone())
                    .collect();
                for id in list {
                    ui.horizontal(|ui| {
                        let r = ui.label(format!("{} {id}", kind.icon()));
                        if kind == AssetKind::Image {
                            r.on_hover_ui(|ui| {
                                let ctx = ui.ctx().clone();
                                match self.thumbs.get(&ctx, &assets, &id) {
                                    Some(img) => {
                                        let t = ui.input(|i| i.time);
                                        let size =
                                            img.size * (200.0 / img.size.max_elem().max(1.0));
                                        ui.image((img.frame_at(t).id(), size));
                                        let anim =
                                            if img.is_animated() { " · animée" } else { "" };
                                        ui.label(format!("{}×{}{anim}", img.size.x, img.size.y));
                                    }
                                    None => {
                                        ui.label(self.thumbs.error(&id).unwrap_or("?"));
                                    }
                                }
                            });
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("🗑").clicked() {
                                delete = Some(id.clone());
                            }
                        });
                    });
                }
            }
            if let Some(id) = delete {
                let _ = std::fs::remove_file(self.dir.join(project_io::ASSETS_DIR).join(&id));
                self.project.assets.retain(|a| a.id != id);
                self.mark_changed();
            }
        });
    }

    pub fn settings_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("⚙ Projet");
            let mut ch = false;
            let snapshot = self.project.clone();
            egui::Grid::new("proj").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Titre du jeu");
                ch |= ui.text_edit_singleline(&mut self.project.name).changed();
                ui.end_row();
                ui.label("Auteur");
                ch |= ui.text_edit_singleline(&mut self.project.author).changed();
                ui.end_row();
                ui.label("Version du jeu");
                ch |= ui.text_edit_singleline(&mut self.project.version).changed();
                ui.end_row();
                ui.label("Résolution");
                ui.horizontal(|ui| {
                    ch |= ui.add(egui::DragValue::new(&mut self.project.resolution[0]).range(320..=7680)).changed();
                    ui.label("×");
                    ch |= ui.add(egui::DragValue::new(&mut self.project.resolution[1]).range(180..=4320)).changed();
                    for (w, h, l) in [(1280, 720, "16:9"), (1920, 1080, "1080p"), (720, 1280, "Mobile portrait")] {
                        if ui.small_button(l).clicked() {
                            self.project.resolution = [w, h];
                            ch = true;
                        }
                    }
                });
                ui.end_row();
                ui.label("Scène de départ");
                ch |= scene_combo(ui, "start", &mut self.project.start_scene, &snapshot, None);
                ui.end_row();
                ui.label("Dossier");
                ui.label(RichText::new(self.dir.display().to_string()).monospace());
                ui.end_row();
            });

            ui.add_space(8.0);
            ui.label(RichText::new("Variables initiales").strong());
            let mut remove = None;
            for (k, v) in self.project.variables.iter_mut() {
                ui.horizontal(|ui| {
                    ui.label(k);
                    ui.label("=");
                    ch |= ui.add(egui::TextEdit::singleline(v).code_editor().desired_width(160.0)).changed();
                    if ui.small_button("✖").clicked() {
                        remove = Some(k.clone());
                    }
                });
            }
            if let Some(k) = remove {
                self.project.variables.remove(&k);
                ch = true;
            }
            ui.horizontal(|ui| {
                let id = ui.id().with("new_var");
                let mut name = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_default();
                ui.add(egui::TextEdit::singleline(&mut name).hint_text("nom").desired_width(120.0));
                if ui.button("➕ Variable").clicked() && !name.trim().is_empty() {
                    self.project.variables.insert(vn_core::model::slug(&name), "0".into());
                    name.clear();
                    ch = true;
                }
                ui.data_mut(|d| d.insert_temp(id, name));
            });
            if ch {
                self.mark_changed();
            }

            ui.add_space(8.0);
            let p = &self.project;
            ui.label(RichText::new(format!(
                "📊 {} scènes · {} dialogues · {} choix · {} personnages · {} ressources · {} extensions",
                p.scenes.len(),
                count_nodes(p, |n| matches!(n, Node::Dialogue { .. })),
                count_nodes(p, |n| matches!(n, Node::Choice { .. })),
                p.characters.len(),
                p.assets.len(),
                p.extensions.len()
            ))
            .weak());

            ui.separator();
            ui.heading("🖥 Logiciel");
            ui.checkbox(&mut self.autosave, "Enregistrement automatique");
            if let Some(p) = &mut self.preview {
                ui.add(egui::Slider::new(&mut p.view.text_speed, 10.0..=200.0).text("vitesse du texte (aperçu)"));
            }
            let ff = if vn_engine::ffmpeg::available() {
                RichText::new("✔ ffmpeg détecté : vidéos et formats exotiques (AVIF, HEIC, Opus, WMA…) pris en charge").color(Color32::from_rgb(90, 200, 120))
            } else {
                RichText::new("✖ ffmpeg non trouvé : les vidéos ne seront pas lues. Place ffmpeg à côté du logiciel.").color(Color32::LIGHT_RED)
            };
            ui.label(ff);

            ui.separator();
            self.update_ui(ui);
        });
    }

    fn update_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("⬆ Mises à jour");
        ui.label(format!(
            "Version installée : v{} ({}) · source : github.com/{}",
            updater::CURRENT_VERSION,
            updater::target(),
            updater::REPO
        ));
        ui.checkbox(&mut self.auto_check_updates, "Vérifier au démarrage");
        let ctx = ui.ctx().clone();
        match self.updater.get() {
            State::Idle => {
                if ui.button("🔍 Vérifier maintenant").clicked() {
                    self.updater.check(ctx);
                }
            }
            State::Checking => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Recherche sur GitHub…");
                });
            }
            State::UpToDate => {
                ui.label(
                    RichText::new("✔ Tu as la dernière version.")
                        .color(Color32::from_rgb(90, 200, 120)),
                );
                if ui.button("🔍 Vérifier à nouveau").clicked() {
                    self.updater.check(ctx);
                }
            }
            State::Available {
                version,
                notes,
                editor_url,
                page,
                ..
            } => {
                ui.label(
                    RichText::new(format!("Nouvelle version disponible : v{version}"))
                        .color(Color32::GOLD)
                        .strong(),
                );
                if !notes.is_empty() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.label(notes);
                    });
                }
                if editor_url.is_some() {
                    if ui.button("⬇ Télécharger et installer").clicked() {
                        self.save();
                        self.updater.install(ctx);
                    }
                } else {
                    ui.label(format!(
                        "Pas de binaire pour {} dans cette version.",
                        updater::target()
                    ));
                }
                if !page.is_empty() {
                    ui.hyperlink_to("Voir la release sur GitHub", page);
                }
            }
            State::Downloading(v) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Téléchargement de v{v}…"));
                });
            }
            State::Installed(v) => {
                ui.label(
                    RichText::new(format!("✔ v{v} installée."))
                        .color(Color32::from_rgb(90, 200, 120)),
                );
                if ui.button("🔄 Redémarrer maintenant").clicked() {
                    self.save();
                    updater::restart();
                }
            }
            State::Error(e) => {
                ui.label(RichText::new(format!("⚠ {e}")).color(Color32::LIGHT_RED));
                if ui.button("🔍 Réessayer").clicked() {
                    self.updater.check(ctx);
                }
            }
        }
    }
}
