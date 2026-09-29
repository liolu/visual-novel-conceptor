//! Lecture / écriture d'un projet sur disque et compilation du jeu final.
//!
//! Un projet est un dossier :
//! ```text
//! MonJeu/
//!   project.json   <- scénario, personnages, extensions...
//!   assets/        <- images, sons, vidéos (tous formats)
//!   build/         <- jeu compilé
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vn_core::model::{Asset, AssetKind, Node, Project};
use vn_core::pack::GamePack;

pub const PROJECT_FILE: &str = "project.json";
pub const ASSETS_DIR: &str = "assets";

/// Dossier par défaut où sont rangés les projets.
pub fn default_projects_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    home.join("VN-Conceptor")
}

pub fn load(dir: &Path) -> Result<Project, String> {
    let text = std::fs::read_to_string(dir.join(PROJECT_FILE))
        .map_err(|e| format!("{} : {e}", dir.display()))?;
    let mut project =
        Project::from_json(&text).map_err(|e| format!("project.json invalide : {e}"))?;
    sync_assets(dir, &mut project);
    Ok(project)
}

pub fn save(dir: &Path, project: &Project) -> Result<(), String> {
    std::fs::create_dir_all(dir.join(ASSETS_DIR)).map_err(|e| e.to_string())?;
    let json = project.to_json().map_err(|e| e.to_string())?;
    // Écriture atomique : on écrit à côté puis on renomme.
    let tmp = dir.join("project.json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(PROJECT_FILE)).map_err(|e| e.to_string())
}

/// Ajoute au projet les fichiers présents dans `assets/` (copiés à la main par exemple)
/// et retire ceux qui n'existent plus.
pub fn sync_assets(dir: &Path, project: &mut Project) -> bool {
    let mut on_disk = Vec::new();
    // Dossier illisible (supprimé, clé USB retirée…) : on ne touche à rien.
    let Ok(rd) = std::fs::read_dir(dir.join(ASSETS_DIR)) else {
        return false;
    };
    for e in rd.flatten() {
        if e.path().is_file() {
            on_disk.push(e.file_name().to_string_lossy().into_owned());
        }
    }
    on_disk.sort();
    let before = project.assets.len();
    project.assets.retain(|a| on_disk.contains(&a.id));
    let mut changed = before != project.assets.len();
    for name in on_disk {
        if project.asset(&name).is_none() {
            let kind = AssetKind::from_path(&name);
            project.assets.push(Asset { id: name, kind });
            changed = true;
        }
    }
    changed
}

/// Copie un fichier dans `assets/`. Retourne l'identifiant de la ressource.
pub fn import_asset(dir: &Path, project: &mut Project, file: &Path) -> Result<String, String> {
    let name = file
        .file_name()
        .ok_or("nom de fichier invalide")?
        .to_string_lossy()
        .into_owned();
    let dest_dir = dir.join(ASSETS_DIR);
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(&name);
    if dest != file {
        std::fs::copy(file, &dest).map_err(|e| format!("{name} : {e}"))?;
    }
    if project.asset(&name).is_none() {
        project.assets.push(Asset {
            id: name.clone(),
            kind: AssetKind::from_path(&name),
        });
    }
    Ok(name)
}

pub fn import_asset_bytes(
    dir: &Path,
    project: &mut Project,
    name: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let dest_dir = dir.join(ASSETS_DIR);
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    std::fs::write(dest_dir.join(name), bytes).map_err(|e| e.to_string())?;
    if project.asset(name).is_none() {
        project.assets.push(Asset {
            id: name.to_string(),
            kind: AssetKind::from_path(name),
        });
    }
    Ok(name.to_string())
}

/// Vérifie les références cassées avant compilation.
pub fn validate(project: &Project) -> Vec<String> {
    let mut w = Vec::new();
    let scene_ok = |s: &str| s.is_empty() || project.scene(s).is_some();
    let asset_ok = |a: &str| a.is_empty() || project.asset(a).is_some();
    if project.scene(&project.start_scene).is_none() {
        w.push(format!(
            "scène de départ « {} » introuvable",
            project.start_scene
        ));
    }
    for scene in &project.scenes {
        for (i, node) in scene.nodes.iter().enumerate() {
            let at = format!("{}#{}", scene.id, i);
            match node {
                Node::Jump { scene: s } if s.is_empty() || !scene_ok(s) => {
                    w.push(format!("{at} : scène « {s} » introuvable"))
                }
                Node::Choice { options, .. } => {
                    for o in options.iter().filter(|o| !scene_ok(&o.target)) {
                        w.push(format!("{at} : choix vers « {} » introuvable", o.target));
                    }
                }
                Node::Condition {
                    then_scene,
                    else_scene,
                    ..
                } => {
                    for s in [then_scene, else_scene]
                        .into_iter()
                        .filter(|s| !scene_ok(s))
                    {
                        w.push(format!("{at} : scène « {s} » introuvable"));
                    }
                }
                Node::Background { asset: Some(a), .. } if !asset_ok(a) => {
                    w.push(format!("{at} : image « {a} » manquante"))
                }
                Node::PlayMusic { asset, .. }
                | Node::PlaySound { asset, .. }
                | Node::PlayVideo { asset, .. }
                    if asset.is_empty() || !asset_ok(asset) =>
                {
                    w.push(format!("{at} : ressource « {asset} » manquante"))
                }
                Node::ShowCharacter { character, .. } | Node::HideCharacter { character }
                    if project.character(character).is_none() =>
                {
                    w.push(format!("{at} : personnage « {character} » inconnu"))
                }
                Node::Extension { extension, .. } if project.extension(extension).is_none() => {
                    w.push(format!("{at} : extension « {extension} » non installée"))
                }
                _ => {}
            }
        }
    }
    w
}

pub struct ExportResult {
    pub out_dir: PathBuf,
    pub executable: Option<PathBuf>,
    pub pack: PathBuf,
    pub notes: Vec<String>,
}

/// Compile le jeu : pack verrouillé + exécutable autonome (si le lecteur est disponible).
pub fn export_game(dir: &Path, project: &Project, out_root: &Path) -> Result<ExportResult, String> {
    let mut assets = HashMap::new();
    for a in &project.assets {
        let bytes = std::fs::read(dir.join(ASSETS_DIR).join(&a.id))
            .map_err(|e| format!("{} : {e}", a.id))?;
        assets.insert(a.id.clone(), Arc::<[u8]>::from(bytes));
    }
    let pack = GamePack {
        project: project.clone(),
        assets,
    };
    let bytes = pack.to_bytes().map_err(|e| e.to_string())?;

    let slug = vn_core::model::slug(&project.name);
    let out_dir = out_root.join(&slug);
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let pack_path = out_dir.join("game.vnpak");
    std::fs::write(&pack_path, &bytes).map_err(|e| e.to_string())?;

    let mut notes = Vec::new();
    let mut executable = None;
    let exe_suffix = std::env::consts::EXE_SUFFIX;
    let player = crate::updater::exe_path()
        .and_then(|e| e.parent().map(|p| p.join(format!("vn-player{exe_suffix}"))))
        .filter(|p| p.exists());
    match player {
        Some(player) => {
            let player_bytes = std::fs::read(&player).map_err(|e| e.to_string())?;
            // Le lecteur contient peut-être déjà un pack (on repart de l'exécutable nu).
            let base = match GamePack::find_in_executable(&player_bytes) {
                Some(p) => &player_bytes[..player_bytes.len() - p.len() - 16],
                None => &player_bytes[..],
            };
            let exe_path = out_dir.join(format!("{slug}{exe_suffix}"));
            std::fs::write(&exe_path, GamePack::append_to_executable(base, &bytes))
                .map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&exe_path, std::fs::Permissions::from_mode(0o755));
            }
            executable = Some(exe_path);
        }
        None => notes.push(format!(
            "vn-player{exe_suffix} introuvable à côté de l'éditeur : seul game.vnpak a été créé \
             (placez-le à côté de vn-player pour jouer)."
        )),
    }

    // Vidéos : on livre ffmpeg avec le jeu s'il est fourni à côté de l'éditeur.
    if project.assets.iter().any(|a| a.kind == AssetKind::Video) {
        let ff = format!("ffmpeg{exe_suffix}");
        let local = crate::updater::exe_path()
            .and_then(|e| e.parent().map(|p| p.join(&ff)))
            .filter(|p| p.exists());
        match local {
            Some(src) => {
                let _ = std::fs::copy(src, out_dir.join(&ff));
            }
            None => notes.push(
                "Le jeu contient des vidéos : ajoutez ffmpeg à côté de l'exécutable du jeu.".into(),
            ),
        }
    }
    notes.push(format!(
        "Android / Web : VN_EMBED_PACK=\"{}\" cargo apk build -p vn-player --lib --release  (ou trunk build web/index.html)",
        pack_path.display()
    ));
    Ok(ExportResult {
        out_dir,
        executable,
        pack: pack_path,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_export_roundtrip() {
        let dir = std::env::temp_dir().join(format!("vn_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut project = Project::default();
        save(&dir, &project).unwrap();
        let src = dir.join("fond.png");
        std::fs::write(&src, b"pas vraiment une image").unwrap();
        import_asset(&dir, &mut project, &src).unwrap();
        assert_eq!(project.asset("fond.png").unwrap().kind, AssetKind::Image);
        save(&dir, &project).unwrap();

        let loaded = load(&dir).unwrap();
        assert_eq!(loaded.assets.len(), 1);
        assert!(validate(&loaded).is_empty(), "{:?}", validate(&loaded));

        let r = export_game(&dir, &loaded, &dir.join("build")).unwrap();
        let pack = GamePack::from_bytes(&std::fs::read(&r.pack).unwrap()).unwrap();
        assert_eq!(&*pack.assets["fond.png"], b"pas vraiment une image");
        assert_eq!(pack.project.name, loaded.name);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
