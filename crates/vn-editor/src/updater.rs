//! Mise à jour automatique : regarde la dernière « release » du dépôt GitHub,
//! télécharge le binaire correspondant à la plateforme et remplace l'exécutable.
//!
//! Les binaires sont publiés par `.github/workflows/release.yml` à chaque tag `vX.Y.Z`,
//! nommés `vn-editor-<cible>` et `vn-player-<cible>` (+ `.exe` sous Windows).

use std::sync::{Arc, Mutex};

pub const REPO: &str = "liolu/visual-novel-conceptor";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Chemin de l'exécutable mémorisé au démarrage (après un remplacement, Linux
/// renverrait « … (deleted) »).
static EXE: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();

pub fn exe_path() -> Option<std::path::PathBuf> {
    EXE.get_or_init(|| std::env::current_exe().ok()).clone()
}

/// Identifiant de plateforme utilisé dans le nom des fichiers publiés.
pub fn target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("android", _) => "android",
        _ => "unknown",
    }
}

fn asset_name(bin: &str) -> String {
    format!("{bin}-{}{}", target(), std::env::consts::EXE_SUFFIX)
}

#[derive(Clone, Debug)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        notes: String,
        editor_url: Option<String>,
        player_url: Option<String>,
        page: String,
    },
    Downloading(String),
    Installed(String),
    Error(String),
}

#[derive(Clone)]
pub struct Updater {
    pub state: Arc<Mutex<State>>,
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::Idle)),
        }
    }
}

impl Updater {
    pub fn get(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    fn set(&self, s: State) {
        *self.state.lock().unwrap() = s;
    }

    pub fn check(&self, ctx: eframe::egui::Context) {
        self.set(State::Checking);
        let me = self.clone();
        std::thread::spawn(move || {
            let res = fetch_latest();
            me.set(match res {
                Ok(s) => s,
                Err(e) => State::Error(e),
            });
            ctx.request_repaint();
        });
    }

    pub fn install(&self, ctx: eframe::egui::Context) {
        let State::Available {
            version,
            editor_url,
            player_url,
            ..
        } = self.get()
        else {
            return;
        };
        self.set(State::Downloading(version.clone()));
        let me = self.clone();
        std::thread::spawn(move || {
            let res = install(editor_url, player_url);
            me.set(match res {
                Ok(()) => State::Installed(version),
                Err(e) => State::Error(e),
            });
            ctx.request_repaint();
        });
    }
}

fn get(url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    let mut req = ureq::get(url)
        .header("User-Agent", "vn-conceptor-updater")
        .header("Accept", "application/vnd.github+json");
    // Dépôt privé : un jeton peut être fourni via la variable d'environnement.
    if let Ok(token) = std::env::var("VN_GITHUB_TOKEN") {
        req = req.header("Authorization", &format!("Bearer {token}"));
    }
    req.call().map_err(|e| match e {
        ureq::Error::StatusCode(404) => {
            "aucune version publiée sur GitHub pour l'instant".to_string()
        }
        e => format!("GitHub injoignable : {e}"),
    })
}

fn fetch_latest() -> Result<State, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let text = get(&url)?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let tag = json["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v')
        .to_string();
    let latest =
        semver::Version::parse(&tag).map_err(|_| format!("tag de version invalide : {tag}"))?;
    let current = semver::Version::parse(CURRENT_VERSION).unwrap();
    if latest <= current {
        return Ok(State::UpToDate);
    }
    let find = |name: String| {
        json["assets"].as_array().and_then(|assets| {
            assets
                .iter()
                .find(|a| a["name"].as_str() == Some(&name))
                .and_then(|a| a["browser_download_url"].as_str().map(String::from))
        })
    };
    Ok(State::Available {
        version: tag,
        notes: json["body"].as_str().unwrap_or("").to_string(),
        editor_url: find(asset_name("vn-editor")),
        player_url: find(asset_name("vn-player")),
        page: json["html_url"].as_str().unwrap_or("").to_string(),
    })
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    get(url)?
        .body_mut()
        .with_config()
        .limit(500 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn install(editor_url: Option<String>, player_url: Option<String>) -> Result<(), String> {
    let editor_url = editor_url.ok_or_else(|| {
        format!(
            "pas de binaire « {} » dans la release",
            asset_name("vn-editor")
        )
    })?;
    let exe = exe_path().ok_or("exécutable introuvable")?;
    let dir = exe
        .parent()
        .ok_or("dossier de l'exécutable introuvable")?
        .to_path_buf();

    // Le lecteur (utilisé pour compiler les jeux) est mis à jour aussi.
    if let Some(url) = player_url {
        let bytes = download(&url)?;
        let path = dir.join(format!("vn-player{}", std::env::consts::EXE_SUFFIX));
        write_exe(&path, &bytes)?;
    }
    let bytes = download(&editor_url)?;
    let tmp = dir.join(format!(".vn-editor-update{}", std::env::consts::EXE_SUFFIX));
    write_exe(&tmp, &bytes)?;
    // Remplace l'exécutable en cours d'exécution (fonctionne aussi sous Windows).
    let res =
        self_replace::self_replace(&tmp).map_err(|e| format!("remplacement impossible : {e}"));
    let _ = std::fs::remove_file(&tmp);
    res
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn install(_editor_url: Option<String>, _player_url: Option<String>) -> Result<(), String> {
    let _ = download;
    Err("sur mobile, installez la nouvelle version depuis la page de la release (APK)".into())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn write_exe(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{} : {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Relance le logiciel (après une mise à jour).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn restart() {
    if let Some(exe) = exe_path() {
        let _ = std::process::Command::new(exe)
            .args(std::env::args().skip(1))
            .spawn();
        std::process::exit(0);
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn restart() {}
