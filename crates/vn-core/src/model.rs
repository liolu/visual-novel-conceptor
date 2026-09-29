//! Modèle de données d'un projet de Visual Novel.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::extension::Extension;

/// Un projet complet : scènes, personnages, ressources et extensions.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub name: String,
    pub author: String,
    pub version: String,
    /// Résolution logique du jeu (le rendu s'adapte à l'écran en gardant le ratio).
    pub resolution: [u32; 2],
    pub start_scene: String,
    pub scenes: Vec<Scene>,
    pub characters: Vec<Character>,
    pub assets: Vec<Asset>,
    pub extensions: Vec<Extension>,
    /// Variables initiales (valeurs sous forme d'expressions Rhai, ex: `0`, `"texte"`, `true`).
    pub variables: BTreeMap<String, String>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            name: "Nouveau Visual Novel".into(),
            author: String::new(),
            version: "1.0.0".into(),
            resolution: [1280, 720],
            start_scene: "intro".into(),
            scenes: vec![Scene {
                id: "intro".into(),
                name: "Introduction".into(),
                nodes: vec![
                    Node::Background {
                        asset: None,
                        color: [40, 44, 70],
                    },
                    Node::Dialogue {
                        speaker: None,
                        text: "Il était une fois...".into(),
                    },
                    Node::Dialogue {
                        speaker: Some("heroine".into()),
                        text: "Bonjour ! Modifie-moi pendant que le jeu tourne.".into(),
                    },
                    Node::Choice {
                        prompt: "Que faire ?".into(),
                        options: vec![
                            ChoiceOption {
                                text: "Recommencer".into(),
                                target: "intro".into(),
                            },
                            ChoiceOption {
                                text: "Terminer".into(),
                                target: String::new(),
                            },
                        ],
                    },
                ],
            }],
            characters: vec![Character {
                id: "heroine".into(),
                name: "Héroïne".into(),
                color: [255, 170, 200],
                sprites: BTreeMap::new(),
            }],
            assets: Vec::new(),
            extensions: Vec::new(),
            variables: BTreeMap::new(),
        }
    }
}

impl Project {
    pub fn scene(&self, id: &str) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.id == id)
    }

    pub fn scene_mut(&mut self, id: &str) -> Option<&mut Scene> {
        self.scenes.iter_mut().find(|s| s.id == id)
    }

    pub fn character(&self, id: &str) -> Option<&Character> {
        self.characters.iter().find(|c| c.id == id)
    }

    pub fn asset(&self, id: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.id == id)
    }

    pub fn extension(&self, id: &str) -> Option<&Extension> {
        self.extensions.iter().find(|e| e.id == id)
    }

    /// Génère un identifiant libre basé sur `base` (ex: `scene`, `scene_2`, ...).
    pub fn unique_id(&self, base: &str, taken: impl Fn(&Project, &str) -> bool) -> String {
        let base = slug(base);
        if !taken(self, &base) {
            return base;
        }
        (2..)
            .map(|i| format!("{base}_{i}"))
            .find(|id| !taken(self, id))
            .unwrap()
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// Transforme un texte en identifiant simple (`Ma Scène 1` -> `ma_scene_1`).
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let c = match c {
            'à' | 'â' | 'ä' | 'À' | 'Â' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let out = out.trim_end_matches('_').to_string();
    if out.is_empty() {
        "id".into()
    } else {
        out
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub nodes: Vec<Node>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Character {
    pub id: String,
    pub name: String,
    pub color: [u8; 3],
    /// expression -> id de ressource image
    pub sprites: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StagePosition {
    Left,
    #[default]
    Center,
    Right,
}

impl StagePosition {
    pub const ALL: [StagePosition; 3] = [Self::Left, Self::Center, Self::Right];
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Gauche",
            Self::Center => "Centre",
            Self::Right => "Droite",
        }
    }
    /// Position horizontale relative (0..1).
    pub fn x(self) -> f32 {
        match self {
            Self::Left => 0.2,
            Self::Center => 0.5,
            Self::Right => 0.8,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChoiceOption {
    pub text: String,
    /// Scène cible (vide = fin du jeu).
    pub target: String,
}

/// Une instruction du scénario.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Node {
    Dialogue {
        speaker: Option<String>,
        text: String,
    },
    Background {
        asset: Option<String>,
        color: [u8; 3],
    },
    ShowCharacter {
        character: String,
        expression: String,
        position: StagePosition,
    },
    HideCharacter {
        character: String,
    },
    PlayMusic {
        asset: String,
        looped: bool,
        volume: f32,
    },
    StopMusic,
    PlaySound {
        asset: String,
        volume: f32,
    },
    PlayVideo {
        asset: String,
        skippable: bool,
    },
    Choice {
        prompt: String,
        options: Vec<ChoiceOption>,
    },
    Jump {
        scene: String,
    },
    /// `name = expression` (expression Rhai évaluée avec les variables du jeu).
    SetVariable {
        name: String,
        expression: String,
    },
    /// Si la condition Rhai est vraie on saute à `then_scene`, sinon à `else_scene` (ou on continue).
    Condition {
        condition: String,
        then_scene: String,
        else_scene: String,
    },
    Wait {
        seconds: f32,
    },
    /// Commande fournie par une extension.
    Extension {
        extension: String,
        command: String,
        args: BTreeMap<String, String>,
    },
    End,
}

impl Node {
    /// Liste des nœuds proposés dans le menu « Ajouter ».
    pub fn templates() -> Vec<(&'static str, Node)> {
        vec![
            (
                "💬 Dialogue",
                Node::Dialogue {
                    speaker: None,
                    text: String::new(),
                },
            ),
            (
                "🖼 Décor",
                Node::Background {
                    asset: None,
                    color: [0, 0, 0],
                },
            ),
            (
                "🧍 Afficher personnage",
                Node::ShowCharacter {
                    character: String::new(),
                    expression: String::new(),
                    position: StagePosition::Center,
                },
            ),
            (
                "🚪 Cacher personnage",
                Node::HideCharacter {
                    character: String::new(),
                },
            ),
            (
                "🎵 Musique",
                Node::PlayMusic {
                    asset: String::new(),
                    looped: true,
                    volume: 0.8,
                },
            ),
            ("🔇 Stopper musique", Node::StopMusic),
            (
                "🔔 Son",
                Node::PlaySound {
                    asset: String::new(),
                    volume: 1.0,
                },
            ),
            (
                "🎬 Vidéo",
                Node::PlayVideo {
                    asset: String::new(),
                    skippable: true,
                },
            ),
            (
                "🔀 Choix",
                Node::Choice {
                    prompt: String::new(),
                    options: vec![ChoiceOption::default(), ChoiceOption::default()],
                },
            ),
            (
                "➡ Aller à la scène",
                Node::Jump {
                    scene: String::new(),
                },
            ),
            (
                "🔢 Variable",
                Node::SetVariable {
                    name: "score".into(),
                    expression: "score + 1".into(),
                },
            ),
            (
                "❓ Condition",
                Node::Condition {
                    condition: "score > 0".into(),
                    then_scene: String::new(),
                    else_scene: String::new(),
                },
            ),
            ("⏱ Attendre", Node::Wait { seconds: 1.0 }),
            ("🏁 Fin", Node::End),
        ]
    }

    pub fn label(&self) -> String {
        match self {
            Node::Dialogue { .. } => "💬 Dialogue".into(),
            Node::Background { .. } => "🖼 Décor".into(),
            Node::ShowCharacter { .. } => "🧍 Afficher personnage".into(),
            Node::HideCharacter { .. } => "🚪 Cacher personnage".into(),
            Node::PlayMusic { .. } => "🎵 Musique".into(),
            Node::StopMusic => "🔇 Stopper musique".into(),
            Node::PlaySound { .. } => "🔔 Son".into(),
            Node::PlayVideo { .. } => "🎬 Vidéo".into(),
            Node::Choice { .. } => "🔀 Choix".into(),
            Node::Jump { .. } => "➡ Aller à".into(),
            Node::SetVariable { .. } => "🔢 Variable".into(),
            Node::Condition { .. } => "❓ Condition".into(),
            Node::Wait { .. } => "⏱ Attendre".into(),
            Node::Extension {
                extension, command, ..
            } => format!("🧩 {extension}::{command}"),
            Node::End => "🏁 Fin".into(),
        }
    }

    /// Résumé court affiché dans la liste des nœuds.
    pub fn summary(&self) -> String {
        match self {
            Node::Dialogue { speaker, text } => {
                let t: String = text.chars().take(60).collect();
                match speaker {
                    Some(s) if !s.is_empty() => format!("{s} : {t}"),
                    _ => t,
                }
            }
            Node::Background { asset, .. } => asset.clone().unwrap_or_else(|| "couleur".into()),
            Node::ShowCharacter {
                character,
                expression,
                position,
            } => {
                format!("{character} ({expression}) {}", position.label())
            }
            Node::HideCharacter { character } => character.clone(),
            Node::PlayMusic { asset, .. }
            | Node::PlaySound { asset, .. }
            | Node::PlayVideo { asset, .. } => asset.clone(),
            Node::Choice { options, .. } => format!("{} options", options.len()),
            Node::Jump { scene } => scene.clone(),
            Node::SetVariable { name, expression } => format!("{name} = {expression}"),
            Node::Condition { condition, .. } => condition.clone(),
            Node::Wait { seconds } => format!("{seconds} s"),
            Node::Extension { args, .. } => args
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(", "),
            Node::StopMusic | Node::End => String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind {
    Image,
    Audio,
    Video,
    Other,
}

impl AssetKind {
    /// Détecte le type d'une ressource à partir de son extension.
    pub fn from_path(path: &str) -> Self {
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "tga" | "ico"
            | "pnm" | "pbm" | "pgm" | "ppm" | "pam" | "dds" | "hdr" | "exr" | "ff" | "qoi"
            | "avif" => Self::Image,
            "mp3" | "ogg" | "oga" | "opus" | "wav" | "flac" | "aac" | "m4a" | "aiff" | "aif"
            | "caf" | "mka" | "wma" | "alac" | "mp2" | "mp1" => Self::Audio,
            "mp4" | "m4v" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "flv" | "mpg" | "mpeg"
            | "ogv" | "3gp" | "ts" | "m2ts" | "vob" | "asf" => Self::Video,
            _ => Self::Other,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Image => "🖼",
            Self::Audio => "🎵",
            Self::Video => "🎬",
            Self::Other => "📄",
        }
    }
}

/// Une ressource du projet (fichier dans `assets/` ou dans le pack compilé).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asset {
    /// Identifiant = nom du fichier (unique dans le projet).
    pub id: String,
    pub kind: AssetKind,
}
