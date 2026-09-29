//! Permet d'intégrer un jeu compilé directement dans le binaire (Android, Web) :
//! `VN_EMBED_PACK=chemin/vers/game.vnpak cargo build -p vn-player`
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=VN_EMBED_PACK");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("embedded.vnpak");
    match std::env::var("VN_EMBED_PACK") {
        Ok(path) if !path.is_empty() => {
            println!("cargo:rerun-if-changed={path}");
            std::fs::copy(&path, &out).expect("VN_EMBED_PACK : fichier introuvable");
        }
        _ => std::fs::write(&out, []).unwrap(),
    }
}
