//! Page « Extensions » : installer en un clic, importer, créer et modifier des extensions,
//! y compris pendant que le jeu tourne dans l'aperçu.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, RichText};
use vn_core::extension::{catalog, CommandDef, Extension, ParamDef, SCRIPT_API_DOC};
use vn_core::script::ScriptHost;

use crate::app::EditorApp;

/// Résultat d'un téléchargement en arrière-plan.
type DownloadSlot = Arc<Mutex<Option<Result<String, String>>>>;

#[derive(Default)]
pub struct ExtUi {
    pub selected: Option<String>,
    pub url: String,
    pub download: Option<DownloadSlot>,
    pub message: String,
    /// Utilisé uniquement pour vérifier la syntaxe des scripts.
    checker: Option<ScriptHost>,
}

impl EditorApp {
    fn add_extension(&mut self, mut ext: Extension) {
        if ext.id.trim().is_empty() {
            ext.id = vn_core::model::slug(&ext.name);
        }
        match self.project.extensions.iter_mut().find(|e| e.id == ext.id) {
            Some(existing) => {
                *existing = ext.clone();
                self.ext.message = format!("🔄 « {} » mise à jour", ext.name);
            }
            None => {
                self.ext.message = format!("✅ « {} » installée", ext.name);
                self.project.extensions.push(ext.clone());
            }
        }
        self.ext.selected = Some(ext.id);
        self.mark_changed();
    }

    pub fn install_extension_file(&mut self, path: &std::path::Path) {
        match std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|t| Extension::from_json(&t).map_err(|e| e.to_string()))
        {
            Ok(ext) => {
                self.add_extension(ext);
                self.page = crate::app::Page::Extensions;
            }
            Err(e) => self.ext.message = format!("⚠ {} : {e}", path.display()),
        }
    }

    fn start_url_download(&mut self, ctx: egui::Context) {
        let mut url = self.ext.url.trim().to_string();
        // Lien GitHub « blob » -> fichier brut.
        if url.contains("github.com/") && url.contains("/blob/") {
            url = url
                .replace("github.com/", "raw.githubusercontent.com/")
                .replace("/blob/", "/");
        }
        let slot = Arc::new(Mutex::new(None));
        self.ext.download = Some(slot.clone());
        std::thread::spawn(move || {
            let res = ureq::get(&url)
                .header("User-Agent", "vn-conceptor")
                .call()
                .map_err(|e| e.to_string())
                .and_then(|mut r| r.body_mut().read_to_string().map_err(|e| e.to_string()));
            *slot.lock().unwrap() = Some(res);
            ctx.request_repaint();
        });
    }

    pub fn extensions_page(&mut self, ui: &mut egui::Ui) {
        // Résultat d'un téléchargement en cours ?
        if let Some(slot) = &self.ext.download {
            let res = slot.lock().unwrap().take();
            if let Some(res) = res {
                self.ext.download = None;
                match res.and_then(|t| {
                    Extension::from_json(&t).map_err(|e| format!("fichier .vnext invalide : {e}"))
                }) {
                    Ok(ext) => self.add_extension(ext),
                    Err(e) => self.ext.message = format!("⚠ {e}"),
                }
            }
        }

        ui.horizontal(|ui| {
            ui.heading("🧩 Extensions");
            ui.label(
                RichText::new("Elles sont enregistrées dans le projet et compilées avec le jeu.")
                    .weak(),
            );
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("➕ Nouvelle extension").clicked() {
                let id = self
                    .project
                    .unique_id("mon_extension", |p, id| p.extension(id).is_some());
                self.add_extension(Extension::blank(&id));
            }
            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            if ui.button("📂 Importer un fichier .vnext").clicked() {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("Extension VN", &["vnext", "json"])
                    .pick_files()
                {
                    for f in files {
                        self.install_extension_file(&f);
                    }
                }
            }
            ui.separator();
            ui.label("🌐 URL :");
            ui.add(
                egui::TextEdit::singleline(&mut self.ext.url)
                    .hint_text("https://github.com/…/extension.vnext")
                    .desired_width(300.0),
            );
            let busy = self.ext.download.is_some();
            if ui
                .add_enabled(
                    !busy && !self.ext.url.trim().is_empty(),
                    egui::Button::new("Installer"),
                )
                .clicked()
            {
                self.start_url_download(ui.ctx().clone());
            }
            if busy {
                ui.spinner();
            }
        });
        if !self.ext.message.is_empty() {
            ui.label(&self.ext.message);
        }
        if self.preview.is_some() {
            ui.label(RichText::new("🎮 Le jeu tourne : les changements de script s'appliquent au prochain appel de la commande.").color(Color32::from_rgb(90, 200, 120)));
        }
        ui.separator();

        egui::Panel::left("ext_list")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Installées").strong());
                        if self.project.extensions.is_empty() {
                            ui.label(
                                RichText::new("aucune — installe-en une ci-dessous 👇").weak(),
                            );
                        }
                        let mut remove = None;
                        let mut toggled = false;
                        for e in &mut self.project.extensions {
                            ui.horizontal(|ui| {
                                toggled |= ui
                                    .checkbox(&mut e.enabled, "")
                                    .on_hover_text("Activer / désactiver")
                                    .changed();
                                let sel = self.ext.selected.as_deref() == Some(e.id.as_str());
                                if ui
                                    .selectable_label(sel, format!("{} v{}", e.name, e.version))
                                    .clicked()
                                {
                                    self.ext.selected = Some(e.id.clone());
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button("🗑")
                                            .on_hover_text("Désinstaller")
                                            .clicked()
                                        {
                                            remove = Some(e.id.clone());
                                        }
                                    },
                                );
                            });
                        }
                        if toggled {
                            self.mark_changed();
                        }
                        if let Some(id) = remove {
                            self.project.extensions.retain(|e| e.id != id);
                            self.mark_changed();
                        }

                        ui.add_space(10.0);
                        ui.label(RichText::new("Catalogue — installation en un clic").strong());
                        for ext in catalog() {
                            let installed = self.project.extension(&ext.id).is_some();
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&ext.name).strong());
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if installed {
                                                ui.label(
                                                    RichText::new("✔ installée")
                                                        .color(Color32::from_rgb(90, 200, 120)),
                                                );
                                            } else if ui.button("⬇ Installer").clicked() {
                                                self.add_extension(ext.clone());
                                            }
                                        },
                                    );
                                });
                                ui.label(RichText::new(&ext.description).weak());
                                let cmds: Vec<_> =
                                    ext.commands.iter().map(|c| c.label.clone()).collect();
                                ui.label(
                                    RichText::new(format!("Commandes : {}", cmds.join(", ")))
                                        .small(),
                                );
                            });
                        }
                    });
            });

        egui::CentralPanel::default().show(ui, |ui| self.extension_editor(ui));
    }

    fn extension_editor(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.ext.selected.clone() else {
            ui.centered_and_justified(|ui| ui.label("Sélectionne une extension pour la modifier."));
            return;
        };
        let checker = self.ext.checker.get_or_insert_with(ScriptHost::new);
        let Some(ext) = self.project.extensions.iter_mut().find(|e| e.id == id) else {
            self.ext.selected = None;
            return;
        };
        let mut ch = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("ext_meta").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Identifiant");
                ui.label(RichText::new(&ext.id).monospace());
                ui.end_row();
                ui.label("Nom");
                ch |= ui.text_edit_singleline(&mut ext.name).changed();
                ui.end_row();
                ui.label("Version");
                ch |= ui.text_edit_singleline(&mut ext.version).changed();
                ui.end_row();
                ui.label("Auteur");
                ch |= ui.text_edit_singleline(&mut ext.author).changed();
                ui.end_row();
                ui.label("Description");
                ch |= ui.text_edit_multiline(&mut ext.description).changed();
                ui.end_row();
            });

            ui.add_space(6.0);
            ui.label(RichText::new("Commandes (utilisables comme nœuds dans le scénario)").strong());
            let mut remove_cmd = None;
            for (ci, cmd) in ext.commands.iter_mut().enumerate() {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("fonction");
                        ch |= ui.add(egui::TextEdit::singleline(&mut cmd.name).code_editor().desired_width(120.0)).changed();
                        ui.label("libellé");
                        ch |= ui.add(egui::TextEdit::singleline(&mut cmd.label).desired_width(160.0)).changed();
                        if ui.small_button("🗑").clicked() {
                            remove_cmd = Some(ci);
                        }
                    });
                    let mut remove_param = None;
                    for (pi, p) in cmd.params.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label("  paramètre");
                            ch |= ui.add(egui::TextEdit::singleline(&mut p.name).code_editor().desired_width(100.0)).changed();
                            ui.label("défaut");
                            ch |= ui.add(egui::TextEdit::singleline(&mut p.default).desired_width(100.0)).changed();
                            if ui.small_button("✖").clicked() {
                                remove_param = Some(pi);
                            }
                        });
                    }
                    if let Some(pi) = remove_param {
                        cmd.params.remove(pi);
                        ch = true;
                    }
                    if ui.small_button("➕ paramètre").clicked() {
                        cmd.params.push(ParamDef { name: format!("p{}", cmd.params.len() + 1), default: String::new() });
                        ch = true;
                    }
                });
            }
            if let Some(ci) = remove_cmd {
                ext.commands.remove(ci);
                ch = true;
            }
            if ui.button("➕ Commande").clicked() {
                ext.commands.push(CommandDef { name: "nouvelle_commande".into(), label: "Nouvelle commande".into(), params: vec![] });
                ch = true;
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Script (Rhai)").strong());
                match checker.check(ext) {
                    Ok(()) => ui.label(RichText::new("✔ compile").color(Color32::from_rgb(90, 200, 120))),
                    Err(e) => ui.label(RichText::new(format!("✖ {e}")).color(Color32::LIGHT_RED)),
                };
            });
            ch |= ui
                .add(egui::TextEdit::multiline(&mut ext.script).code_editor().desired_rows(16).desired_width(f32::INFINITY))
                .changed();
            egui::CollapsingHeader::new("📖 Fonctions disponibles dans les scripts").show(ui, |ui| {
                ui.label(RichText::new(SCRIPT_API_DOC).monospace());
                ui.label("Chaque commande est une fonction `fn nom(args)` ; les paramètres sont dans `args.nom_du_parametre` (texte).");
                ui.hyperlink_to("Documentation du langage Rhai", "https://rhai.rs/book/");
            });

            ui.add_space(6.0);
            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            if ui.button("💾 Exporter en .vnext (pour la partager)").clicked() {
                if let Some(path) = rfd::FileDialog::new().set_file_name(format!("{}.vnext", ext.id)).save_file() {
                    let res = ext.to_json().map_err(|e| e.to_string()).and_then(|j| std::fs::write(&path, j).map_err(|e| e.to_string()));
                    self.ext.message = match res {
                        Ok(()) => format!("💾 Exportée : {}", path.display()),
                        Err(e) => format!("⚠ {e}"),
                    };
                }
            }
        });
        if ch {
            self.mark_changed();
        }
    }
}
