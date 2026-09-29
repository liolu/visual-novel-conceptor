//! VN Conceptor : éditeur de Visual Novel (egui).

pub mod app;
pub mod extensions;
pub mod nodes;
pub mod project_io;
pub mod sidebar;
pub mod updater;
pub mod widgets;

pub use app::EditorApp;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let dir = app
        .internal_data_path()
        .map(|p| p.join("projets").join("mon_visual_novel"));
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "VN Conceptor",
        options,
        Box::new(move |cc| Ok(Box::new(EditorApp::new(cc, dir)))),
    );
}
