// Pas de console sous Windows pour le jeu final.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use vn_player::{load_pack, window_title, PlayerApp};
    let pack = load_pack();
    let title = window_title(&pack);
    let size = pack
        .as_ref()
        .map(|p| p.project.resolution)
        .unwrap_or([1280, 720]);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([size[0] as f32, size[1] as f32])
            .with_min_inner_size([320.0, 180.0]),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(|_| Ok(Box::new(PlayerApp::new(pack)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    vn_player::web_main();
}
