//! Lecteur de jeu : charge un `.vnpak` et le joue. Aucune interface d'édition.

use eframe::egui;
use vn_core::pack::GamePack;
use vn_core::runtime::Runtime;
use vn_engine::assets::MemAssets;
use vn_engine::GameView;

/// Pack intégré à la compilation (vide si `VN_EMBED_PACK` n'était pas défini).
static EMBEDDED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedded.vnpak"));

/// Nom du fichier cherché à côté de l'exécutable.
pub const PACK_FILE: &str = "game.vnpak";

/// Cherche le jeu : intégré au binaire, collé à l'exécutable, à côté, ou passé en argument.
pub fn load_pack() -> Result<GamePack, String> {
    if !EMBEDDED.is_empty() {
        return GamePack::from_bytes(EMBEDDED).map_err(|e| e.to_string());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        if let Ok(bytes) = std::fs::read(&exe) {
            if let Some(pack) = GamePack::find_in_executable(&bytes) {
                return GamePack::from_bytes(pack).map_err(|e| e.to_string());
            }
        }
        let mut candidates: Vec<std::path::PathBuf> =
            std::env::args().skip(1).map(Into::into).collect();
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(PACK_FILE));
        }
        for c in candidates {
            if let Ok(bytes) = std::fs::read(&c) {
                return GamePack::from_bytes(&bytes).map_err(|e| format!("{} : {e}", c.display()));
            }
        }
    }
    Err(format!("Aucun jeu trouvé ({PACK_FILE})."))
}

pub struct PlayerApp {
    state: Result<(vn_core::model::Project, Runtime, MemAssets, GameView), String>,
}

impl PlayerApp {
    pub fn new(pack: Result<GamePack, String>) -> Self {
        let state = pack.map(|p| {
            let rt = Runtime::new(&p.project);
            (
                p.project,
                rt,
                MemAssets { files: p.assets },
                GameView::new(),
            )
        });
        Self { state }
    }
}

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match &mut self.state {
            Ok((project, rt, assets, view)) => {
                #[cfg(not(target_arch = "wasm32"))]
                if ui.input(|i| i.key_pressed(egui::Key::F11)) {
                    let full = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
                }
                view.show(ui, project, rt, assets);
            }
            Err(e) => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.centered_and_justified(|ui| ui.heading(format!("⚠ {e}")));
                });
            }
        }
    }
}

pub fn window_title(pack: &Result<GamePack, String>) -> String {
    pack.as_ref()
        .map(|p| p.project.name.clone())
        .unwrap_or_else(|_| "Visual Novel".into())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let pack = load_pack();
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    let _ = eframe::run_native(
        &window_title(&pack),
        options,
        Box::new(|_| Ok(Box::new(PlayerApp::new(pack)))),
    );
}

#[cfg(target_arch = "wasm32")]
pub fn web_main() {
    use wasm_bindgen::JsCast;
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document
            .get_element_by_id("vn_canvas")
            .expect("il faut un <canvas id=\"vn_canvas\">")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        let pack = load_pack();
        let _ = eframe::WebRunner::new()
            .start(
                canvas,
                Default::default(),
                Box::new(|_| Ok(Box::new(PlayerApp::new(pack)))),
            )
            .await;
    });
}
