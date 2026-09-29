//! Petits widgets réutilisables (listes déroulantes de scènes, ressources, personnages).

use eframe::egui;
use vn_core::model::{AssetKind, Project};

/// Choix d'une scène. `end_label` : libellé pour « aucune scène » (ex: « Fin du jeu »).
pub fn scene_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut String,
    project: &Project,
    end_label: Option<&str>,
) -> bool {
    let before = value.clone();
    let text = if value.is_empty() {
        end_label.unwrap_or("— choisir —").to_string()
    } else {
        match project.scene(value) {
            Some(s) => s.name.clone(),
            None => format!("⚠ {value}"),
        }
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(text)
        .show_ui(ui, |ui| {
            if let Some(l) = end_label {
                ui.selectable_value(value, String::new(), l);
            }
            for s in &project.scenes {
                ui.selectable_value(value, s.id.clone(), &s.name);
            }
        });
    *value != before
}

pub fn asset_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut String,
    project: &Project,
    kind: AssetKind,
) -> bool {
    let before = value.clone();
    let text = if value.is_empty() {
        "— aucune —".to_string()
    } else if project.asset(value).is_some() {
        value.clone()
    } else {
        format!("⚠ {value}")
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(text)
        .width(200.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, String::new(), "— aucune —");
            for a in project.assets.iter().filter(|a| a.kind == kind) {
                ui.selectable_value(value, a.id.clone(), format!("{} {}", a.kind.icon(), a.id));
            }
        });
    *value != before
}

pub fn character_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut String,
    project: &Project,
    none_label: Option<&str>,
) -> bool {
    let before = value.clone();
    let text = if value.is_empty() {
        none_label.unwrap_or("— choisir —").to_string()
    } else {
        project
            .character(value)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("⚠ {value}"))
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(text)
        .show_ui(ui, |ui| {
            if let Some(l) = none_label {
                ui.selectable_value(value, String::new(), l);
            }
            for c in &project.characters {
                ui.selectable_value(value, c.id.clone(), &c.name);
            }
        });
    *value != before
}
