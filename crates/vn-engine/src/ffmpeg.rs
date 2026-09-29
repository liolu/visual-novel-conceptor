//! Accès à ffmpeg (vidéo de tous formats + conversion des formats image/son exotiques).
//!
//! ffmpeg est cherché à côté de l'exécutable (pour le livrer avec le jeu) puis dans le PATH.
//! Sur mobile/web il n'est pas disponible : les vidéos affichent un message.

use std::path::PathBuf;
use std::sync::OnceLock;

#[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
pub fn path() -> Option<PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| {
        let name = if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        let mut candidates = Vec::new();
        if let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.to_path_buf()))
        {
            candidates.push(dir.join(name));
            candidates.push(dir.join("ffmpeg").join(name));
        }
        candidates.push(PathBuf::from(name));
        candidates.into_iter().find(|c| {
            command(c)
                .arg("-version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
    })
    .clone()
}

#[cfg(any(target_arch = "wasm32", target_os = "android", target_os = "ios"))]
pub fn path() -> Option<PathBuf> {
    let _ = OnceLock::<()>::new;
    None
}

pub fn available() -> bool {
    path().is_some()
}

/// Commande ffmpeg sans fenêtre console (Windows) et silencieuse.
#[cfg(not(target_arch = "wasm32"))]
pub fn command(exe: &std::path::Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// Convertit n'importe quel fichier (image ou son) vers un format lisible en Rust pur.
/// `out_format` : `"png"` (image) ou `"wav"` (son).
#[cfg(not(target_arch = "wasm32"))]
pub fn convert(input: &[u8], out_format: &str) -> Option<Vec<u8>> {
    use std::io::Write;
    use std::process::Stdio;
    let exe = path()?;
    let mut cmd = command(&exe);
    cmd.args(["-i", "pipe:0"]);
    match out_format {
        "png" => cmd.args([
            "-frames:v",
            "1",
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "pipe:1",
        ]),
        _ => cmd.args(["-vn", "-f", "wav", "pipe:1"]),
    };
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let data = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&data);
    });
    let out = child.wait_with_output().ok()?;
    let _ = writer.join();
    (out.status.success() && !out.stdout.is_empty()).then_some(out.stdout)
}

#[cfg(target_arch = "wasm32")]
pub fn convert(_input: &[u8], _out_format: &str) -> Option<Vec<u8>> {
    None
}
