//! Format du jeu compilé (`.vnpak`).
//!
//! Le pack contient le projet + toutes les ressources. Il est brouillé et protégé par
//! une somme de contrôle : le lecteur refuse un pack modifié, et le lecteur n'a aucune
//! interface d'édition. Le jeu compilé n'est donc plus modifiable par les joueurs.
//!
//! Disposition : `MAGIC | version u32 | checksum u64 | données brouillées`.
//! Un pack peut aussi être collé à la fin de l'exécutable du lecteur
//! (`exe | pack | taille u64 | TRAILER`) pour distribuer un seul fichier.

use std::collections::HashMap;
use std::sync::Arc;

use crate::model::Project;

const MAGIC: &[u8; 8] = b"VNPAK\0\0\x01";
const TRAILER: &[u8; 8] = b"VNPAKEND";
const FORMAT_VERSION: u32 = 1;
const KEY: u64 = 0x5EED_CAFE_F00D_B0BA;

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("ce fichier n'est pas un jeu compilé")]
    NotAPack,
    #[error("version de pack non supportée ({0})")]
    Version(u32),
    #[error("le jeu a été modifié ou est corrompu")]
    Tampered,
    #[error("pack tronqué")]
    Truncated,
    #[error("projet illisible : {0}")]
    Json(#[from] serde_json::Error),
}

pub struct GamePack {
    pub project: Project,
    pub assets: HashMap<String, Arc<[u8]>>,
}

impl GamePack {
    pub fn to_bytes(&self) -> Result<Vec<u8>, PackError> {
        let json = serde_json::to_vec(&self.project)?;
        let mut body = Vec::new();
        put_bytes(&mut body, &json);
        let mut names: Vec<_> = self.assets.keys().collect();
        names.sort();
        body.extend_from_slice(&(names.len() as u32).to_le_bytes());
        for name in names {
            put_bytes(&mut body, name.as_bytes());
            put_bytes(&mut body, &self.assets[name]);
        }
        let checksum = fnv1a(&body);
        scramble(&mut body, checksum);
        let mut out = Vec::with_capacity(body.len() + 20);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(&body);
        Ok(out)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, PackError> {
        if data.len() < 20 || &data[..8] != MAGIC {
            return Err(PackError::NotAPack);
        }
        let version = u32::from_le_bytes(data[8..12].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(PackError::Version(version));
        }
        let checksum = u64::from_le_bytes(data[12..20].try_into().unwrap());
        let mut body = data[20..].to_vec();
        scramble(&mut body, checksum);
        if fnv1a(&body) != checksum {
            return Err(PackError::Tampered);
        }
        let mut r = Reader {
            data: &body,
            pos: 0,
        };
        let project: Project = serde_json::from_slice(r.bytes()?)?;
        let count = r.u32()?;
        let mut assets = HashMap::new();
        for _ in 0..count {
            let name = String::from_utf8_lossy(r.bytes()?).into_owned();
            let bytes: Arc<[u8]> = r.bytes()?.into();
            assets.insert(name, bytes);
        }
        Ok(Self { project, assets })
    }

    /// Ajoute le pack à la fin d'un exécutable (jeu en un seul fichier).
    pub fn append_to_executable(exe: &[u8], pack: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(exe.len() + pack.len() + 16);
        out.extend_from_slice(exe);
        out.extend_from_slice(pack);
        out.extend_from_slice(&(pack.len() as u64).to_le_bytes());
        out.extend_from_slice(TRAILER);
        out
    }

    /// Cherche un pack collé à la fin d'un exécutable.
    pub fn find_in_executable(exe: &[u8]) -> Option<&[u8]> {
        if exe.len() < 16 || &exe[exe.len() - 8..] != TRAILER {
            return None;
        }
        let len = u64::from_le_bytes(exe[exe.len() - 16..exe.len() - 8].try_into().ok()?) as usize;
        let end = exe.len() - 16;
        end.checked_sub(len).map(|start| &exe[start..end])
    }
}

fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    out.extend_from_slice(&(b.len() as u64).to_le_bytes());
    out.extend_from_slice(b);
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], PackError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|e| *e <= self.data.len())
            .ok_or(PackError::Truncated)?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, PackError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn bytes(&mut self) -> Result<&'a [u8], PackError> {
        let n = u64::from_le_bytes(self.take(8)?.try_into().unwrap()) as usize;
        self.take(n)
    }
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Brouillage réversible (XOR avec un flux pseudo-aléatoire).
fn scramble(data: &mut [u8], seed: u64) {
    let mut x = seed ^ KEY | 1;
    for chunk in data.chunks_mut(8) {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        for (b, k) in chunk.iter_mut().zip(x.to_le_bytes()) {
            *b ^= k;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_tamper() {
        let mut assets = HashMap::new();
        assets.insert("a.png".to_string(), Arc::from(&b"hello"[..]));
        let pack = GamePack {
            project: Project::default(),
            assets,
        };
        let bytes = pack.to_bytes().unwrap();
        let back = GamePack::from_bytes(&bytes).unwrap();
        assert_eq!(&*back.assets["a.png"], b"hello");
        let exe = GamePack::append_to_executable(b"EXE", &bytes);
        assert_eq!(GamePack::find_in_executable(&exe).unwrap(), &bytes[..]);
        let mut bad = bytes.clone();
        let n = bad.len() - 1;
        bad[n] ^= 1;
        assert!(matches!(
            GamePack::from_bytes(&bad),
            Err(PackError::Tampered)
        ));
    }
}
