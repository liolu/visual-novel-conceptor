#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use vn_editor::EditorApp;

fn main() -> eframe::Result {
    // Un dossier de projet peut être passé en argument.
    let dir = std::env::args().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(format!("VN Conceptor v{}", env!("CARGO_PKG_VERSION")))
            .with_inner_size([1500.0, 900.0])
            .with_min_inner_size([800.0, 500.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "VN Conceptor",
        options,
        Box::new(|cc| Ok(Box::new(EditorApp::new(cc, dir)))),
    )
}
