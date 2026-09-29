//! Éditeur des nœuds d'une scène (le scénario).

use eframe::egui::{self, Color32, RichText};
use vn_core::model::{AssetKind, ChoiceOption, Node, Project, StagePosition};

use crate::widgets::{asset_combo, character_combo, scene_combo};

/// Modification de la structure de la scène (pour garder l'aperçu à la bonne position).
pub enum Structural {
    Inserted(usize),
    Removed(usize),
    Moved { from: usize, to: usize },
}

#[derive(Default)]
pub struct Output {
    pub changed: bool,
    pub play_from: Option<usize>,
    pub structural: Option<Structural>,
}

pub fn scene_editor(
    ui: &mut egui::Ui,
    project: &mut Project,
    scene_id: &str,
    selected: &mut Option<usize>,
    playing_at: Option<usize>,
) -> Output {
    let mut out = Output::default();
    let Some(scene) = project.scene(scene_id) else {
        ui.label("Aucune scène sélectionnée.");
        return out;
    };
    let count = scene.nodes.len();

    ui.horizontal(|ui| {
        ui.heading(format!("🎬 {}", scene.name));
        ui.label(RichText::new(format!("({count} nœuds · id : {})", scene.id)).weak());
    });
    ui.label(
        RichText::new("Astuce : lance ▶ Jouer puis modifie le texte, les images ou l'ordre : l'aperçu se met à jour en direct.")
            .weak()
            .italics(),
    );
    ui.separator();

    let mut action: Option<(usize, &str)> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for i in 0..count {
                let node = project.scene(scene_id).unwrap().nodes[i].clone();
                let is_sel = *selected == Some(i);
                let is_playing = playing_at == Some(i);
                let stroke = if is_playing {
                    egui::Stroke::new(2.0, Color32::from_rgb(80, 200, 120))
                } else if is_sel {
                    egui::Stroke::new(1.5, ui.visuals().selection.stroke.color)
                } else {
                    ui.visuals().widgets.noninteractive.bg_stroke
                };
                egui::Frame::new()
                    .fill(ui.visuals().faint_bg_color)
                    .stroke(stroke)
                    .corner_radius(6)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let title = format!("{i:>2}  {}", node.label());
                            if ui
                                .selectable_label(is_sel, RichText::new(title).strong())
                                .clicked()
                            {
                                *selected = if is_sel { None } else { Some(i) };
                            }
                            if is_playing {
                                ui.label(
                                    RichText::new("◀ en cours")
                                        .color(Color32::from_rgb(80, 200, 120)),
                                );
                            }
                            if !is_sel {
                                ui.label(RichText::new(node.summary()).weak());
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("🗑").on_hover_text("Supprimer").clicked()
                                    {
                                        action = Some((i, "delete"));
                                    }
                                    if ui.small_button("⧉").on_hover_text("Dupliquer").clicked() {
                                        action = Some((i, "dup"));
                                    }
                                    if ui
                                        .add_enabled(i + 1 < count, egui::Button::new("⏷").small())
                                        .on_hover_text("Descendre")
                                        .clicked()
                                    {
                                        action = Some((i, "down"));
                                    }
                                    if ui
                                        .add_enabled(i > 0, egui::Button::new("⏶").small())
                                        .on_hover_text("Monter")
                                        .clicked()
                                    {
                                        action = Some((i, "up"));
                                    }
                                    if ui
                                        .small_button("▶")
                                        .on_hover_text("Tester à partir d'ici")
                                        .clicked()
                                    {
                                        out.play_from = Some(i);
                                    }
                                },
                            );
                        });
                        if is_sel {
                            ui.separator();
                            let mut edited = node.clone();
                            if edit_node(ui, &mut edited, project, i) {
                                project.scene_mut(scene_id).unwrap().nodes[i] = edited;
                                out.changed = true;
                            }
                        }
                    });
                ui.add_space(4.0);
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let insert_at = selected.map(|s| s + 1).unwrap_or(count).min(count);
                let mut new_node = None;
                ui.menu_button(RichText::new("➕ Ajouter un nœud").size(16.0), |ui| {
                    for (label, tpl) in Node::templates() {
                        if ui.button(label).clicked() {
                            new_node = Some(tpl);
                            ui.close();
                        }
                    }
                    let exts: Vec<_> = project
                        .extensions
                        .iter()
                        .filter(|e| e.enabled && !e.commands.is_empty())
                        .collect();
                    if !exts.is_empty() {
                        ui.separator();
                        for ext in exts {
                            ui.menu_button(format!("🧩 {}", ext.name), |ui| {
                                for cmd in &ext.commands {
                                    if ui
                                        .button(if cmd.label.is_empty() {
                                            &cmd.name
                                        } else {
                                            &cmd.label
                                        })
                                        .clicked()
                                    {
                                        new_node = Some(Node::Extension {
                                            extension: ext.id.clone(),
                                            command: cmd.name.clone(),
                                            args: cmd
                                                .params
                                                .iter()
                                                .map(|p| (p.name.clone(), p.default.clone()))
                                                .collect(),
                                        });
                                        ui.close();
                                    }
                                }
                            });
                        }
                    }
                });
                if selected.is_some() {
                    ui.label(
                        RichText::new(format!("(sera inséré en position {insert_at})")).weak(),
                    );
                }
                if let Some(n) = new_node {
                    project
                        .scene_mut(scene_id)
                        .unwrap()
                        .nodes
                        .insert(insert_at, n);
                    *selected = Some(insert_at);
                    out.changed = true;
                    out.structural = Some(Structural::Inserted(insert_at));
                }
            });
            ui.add_space(40.0);
        });

    if let Some((i, what)) = action {
        let nodes = &mut project.scene_mut(scene_id).unwrap().nodes;
        match what {
            "delete" => {
                nodes.remove(i);
                *selected = None;
                out.structural = Some(Structural::Removed(i));
            }
            "dup" => {
                let n = nodes[i].clone();
                nodes.insert(i + 1, n);
                *selected = Some(i + 1);
                out.structural = Some(Structural::Inserted(i + 1));
            }
            "up" => {
                nodes.swap(i, i - 1);
                *selected = Some(i - 1);
                out.structural = Some(Structural::Moved { from: i, to: i - 1 });
            }
            "down" => {
                nodes.swap(i, i + 1);
                *selected = Some(i + 1);
                out.structural = Some(Structural::Moved { from: i, to: i + 1 });
            }
            _ => {}
        }
        out.changed = true;
    }
    out
}

/// Formulaire d'édition d'un nœud. Retourne `true` si quelque chose a changé.
fn edit_node(ui: &mut egui::Ui, node: &mut Node, project: &Project, i: usize) -> bool {
    let mut ch = false;
    let id = ("node", i);
    egui::Grid::new(("grid", i))
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| match node {
            Node::Dialogue { speaker, text } => {
                ui.label("Personnage");
                let mut s = speaker.clone().unwrap_or_default();
                if character_combo(ui, (id, "sp"), &mut s, project, Some("Narrateur")) {
                    *speaker = (!s.is_empty()).then_some(s);
                    ch = true;
                }
                ui.end_row();
                ui.label("Texte");
                ch |= ui
                    .add(
                        egui::TextEdit::multiline(text)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .hint_text("Utilise {variable} pour afficher une variable"),
                    )
                    .changed();
                ui.end_row();
            }
            Node::Background { asset, color } => {
                ui.label("Image");
                let mut a = asset.clone().unwrap_or_default();
                if asset_combo(ui, (id, "bg"), &mut a, project, AssetKind::Image) {
                    *asset = (!a.is_empty()).then_some(a);
                    ch = true;
                }
                ui.end_row();
                ui.label("Couleur de fond");
                ch |= ui.color_edit_button_srgb(color).changed();
                ui.end_row();
            }
            Node::ShowCharacter {
                character,
                expression,
                position,
            } => {
                ui.label("Personnage");
                ch |= character_combo(ui, (id, "c"), character, project, None);
                ui.end_row();
                ui.label("Expression");
                let exprs: Vec<String> = project
                    .character(character)
                    .map(|c| c.sprites.keys().cloned().collect())
                    .unwrap_or_default();
                let before = expression.clone();
                egui::ComboBox::from_id_salt((id, "e"))
                    .selected_text(if expression.is_empty() {
                        "(par défaut)"
                    } else {
                        expression.as_str()
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(expression, String::new(), "(par défaut)");
                        for e in exprs {
                            ui.selectable_value(expression, e.clone(), e);
                        }
                    });
                ch |= *expression != before;
                ui.end_row();
                ui.label("Position");
                ui.horizontal(|ui| {
                    for p in StagePosition::ALL {
                        ch |= ui.radio_value(position, p, p.label()).changed();
                    }
                });
                ui.end_row();
            }
            Node::HideCharacter { character } => {
                ui.label("Personnage");
                ch |= character_combo(ui, (id, "c"), character, project, None);
                ui.end_row();
            }
            Node::PlayMusic {
                asset,
                looped,
                volume,
            } => {
                ui.label("Musique");
                ch |= asset_combo(ui, (id, "m"), asset, project, AssetKind::Audio);
                ui.end_row();
                ui.label("En boucle");
                ch |= ui.checkbox(looped, "").changed();
                ui.end_row();
                ui.label("Volume");
                ch |= ui.add(egui::Slider::new(volume, 0.0..=1.0)).changed();
                ui.end_row();
            }
            Node::PlaySound { asset, volume } => {
                ui.label("Son");
                ch |= asset_combo(ui, (id, "s"), asset, project, AssetKind::Audio);
                ui.end_row();
                ui.label("Volume");
                ch |= ui.add(egui::Slider::new(volume, 0.0..=1.0)).changed();
                ui.end_row();
            }
            Node::PlayVideo { asset, skippable } => {
                ui.label("Vidéo");
                ch |= asset_combo(ui, (id, "v"), asset, project, AssetKind::Video);
                ui.end_row();
                ui.label("Peut être passée");
                ch |= ui.checkbox(skippable, "").changed();
                ui.end_row();
            }
            Node::Choice { prompt, options } => {
                ui.label("Question");
                ch |= ui
                    .add(egui::TextEdit::singleline(prompt).desired_width(f32::INFINITY))
                    .changed();
                ui.end_row();
                let mut remove = None;
                for (k, opt) in options.iter_mut().enumerate() {
                    ui.label(format!("Option {}", k + 1));
                    ui.horizontal(|ui| {
                        ch |= ui
                            .add(
                                egui::TextEdit::singleline(&mut opt.text)
                                    .hint_text("texte du bouton")
                                    .desired_width(220.0),
                            )
                            .changed();
                        ui.label("→");
                        ch |= scene_combo(
                            ui,
                            (id, "o", k),
                            &mut opt.target,
                            project,
                            Some("🏁 Fin du jeu"),
                        );
                        if ui.small_button("✖").clicked() {
                            remove = Some(k);
                        }
                    });
                    ui.end_row();
                }
                if let Some(k) = remove {
                    options.remove(k);
                    ch = true;
                }
                ui.label("");
                if ui.button("➕ Option").clicked() {
                    options.push(ChoiceOption::default());
                    ch = true;
                }
                ui.end_row();
            }
            Node::Jump { scene } => {
                ui.label("Scène");
                ch |= scene_combo(ui, (id, "j"), scene, project, None);
                ui.end_row();
            }
            Node::SetVariable { name, expression } => {
                ui.label("Variable");
                ch |= ui.text_edit_singleline(name).changed();
                ui.end_row();
                ui.label("= Expression");
                ch |= ui
                    .add(
                        egui::TextEdit::singleline(expression)
                            .code_editor()
                            .hint_text("ex: score + 1, \"texte\", true"),
                    )
                    .changed();
                ui.end_row();
            }
            Node::Condition {
                condition,
                then_scene,
                else_scene,
            } => {
                ui.label("Si");
                ch |= ui
                    .add(
                        egui::TextEdit::singleline(condition)
                            .code_editor()
                            .hint_text("ex: score >= 3 && ami == true"),
                    )
                    .changed();
                ui.end_row();
                ui.label("Alors aller à");
                ch |= scene_combo(ui, (id, "t"), then_scene, project, Some("(continuer)"));
                ui.end_row();
                ui.label("Sinon aller à");
                ch |= scene_combo(ui, (id, "f"), else_scene, project, Some("(continuer)"));
                ui.end_row();
            }
            Node::Wait { seconds } => {
                ui.label("Durée");
                ch |= ui
                    .add(
                        egui::DragValue::new(seconds)
                            .range(0.0..=60.0)
                            .speed(0.1)
                            .suffix(" s"),
                    )
                    .changed();
                ui.end_row();
            }
            Node::Extension {
                extension,
                command,
                args,
            } => match project.extension(extension) {
                Some(ext) => {
                    ui.label("Extension");
                    ui.label(format!(
                        "{} ({})",
                        ext.name,
                        if ext.enabled {
                            "active"
                        } else {
                            "désactivée"
                        }
                    ));
                    ui.end_row();
                    if let Some(def) = ext.commands.iter().find(|c| &c.name == command) {
                        for p in &def.params {
                            ui.label(&p.name);
                            let v = args
                                .entry(p.name.clone())
                                .or_insert_with(|| p.default.clone());
                            ch |= ui.text_edit_singleline(v).changed();
                            ui.end_row();
                        }
                    } else {
                        ui.label(
                            RichText::new(format!("⚠ commande « {command} » inconnue"))
                                .color(Color32::LIGHT_RED),
                        );
                        ui.end_row();
                    }
                }
                None => {
                    ui.label(
                        RichText::new(format!("⚠ extension « {extension} » non installée"))
                            .color(Color32::LIGHT_RED),
                    );
                    ui.end_row();
                }
            },
            Node::StopMusic | Node::End => {
                ui.label(RichText::new("Aucun réglage.").weak());
                ui.end_row();
            }
        });
    ch
}
