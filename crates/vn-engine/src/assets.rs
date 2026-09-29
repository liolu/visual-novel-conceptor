//! Accès aux ressources : dossier du projet (éditeur) ou pack compilé (jeu final).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub trait AssetSource {
    /// Contenu brut de la ressource.
    fn bytes(&self, id: &str) -> Option<Arc<[u8]>>;
    /// Chemin sur disque (nécessaire pour la vidéo via ffmpeg). Peut créer un fichier temporaire.
    fn file_path(&self, id: &str) -> Option<PathBuf>;
}

/// Ressources lues dans `<projet>/assets/`. Relues à chaque changement (rechargement à chaud).
pub struct DirAssets {
    pub dir: PathBuf,
}

impl AssetSource for DirAssets {
    fn bytes(&self, id: &str) -> Option<Arc<[u8]>> {
        std::fs::read(self.dir.join(id)).ok().map(Arc::from)
    }

    fn file_path(&self, id: &str) -> Option<PathBuf> {
        let p = self.dir.join(id);
        p.exists().then_some(p)
    }
}

/// Ressources en mémoire (jeu compilé, ou projet sans dossier).
#[derive(Default)]
pub struct MemAssets {
    pub files: HashMap<String, Arc<[u8]>>,
}

impl AssetSource for MemAssets {
    fn bytes(&self, id: &str) -> Option<Arc<[u8]>> {
        self.files.get(id).cloned()
    }

    fn file_path(&self, id: &str) -> Option<PathBuf> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = id;
            None
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let data = self.files.get(id)?;
            let safe: String = id
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '.' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let path = std::env::temp_dir().join(format!("vn_{}_{safe}", std::process::id()));
            if !path.exists() {
                std::fs::write(&path, data).ok()?;
            }
            Some(path)
        }
    }
}
