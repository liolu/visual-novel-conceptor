//! Extensions : petits modules scriptés (Rhai) qui ajoutent des commandes au scénario.
//!
//! Une extension est stockée DANS le projet : elle est donc compilée avec le jeu
//! et fonctionne sur toutes les plateformes (Rhai est 100 % Rust).
//! Format de fichier d'échange : `.vnext` (JSON d'une [`Extension`]).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Extension {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub enabled: bool,
    /// Code Rhai. Chaque commande correspond à une fonction `fn <commande>(args)`
    /// où `args` est une map `#{ param: "valeur" }`.
    pub script: String,
    pub commands: Vec<CommandDef>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandDef {
    pub name: String,
    pub label: String,
    pub params: Vec<ParamDef>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ParamDef {
    pub name: String,
    pub default: String,
}

impl Extension {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn blank(id: &str) -> Self {
        Self {
            id: id.into(),
            name: "Mon extension".into(),
            version: "0.1.0".into(),
            description: "Décris ce que fait ton extension.".into(),
            enabled: true,
            script: "// Chaque commande = une fonction qui reçoit une map d'arguments.\n\
                     fn hello(args) {\n    notify(\"Bonjour \" + args.nom + \" !\");\n}\n"
                .into(),
            commands: vec![cmd("hello", "Dire bonjour", &[("nom", "joueur")])],
            ..Default::default()
        }
    }
}

fn cmd(name: &str, label: &str, params: &[(&str, &str)]) -> CommandDef {
    CommandDef {
        name: name.into(),
        label: label.into(),
        params: params
            .iter()
            .map(|(n, d)| ParamDef {
                name: (*n).into(),
                default: (*d).into(),
            })
            .collect(),
    }
}

/// Documentation de l'API disponible dans les scripts (affichée dans l'éditeur d'extensions).
pub const SCRIPT_API_DOC: &str = "\
get_var(nom)                 → lit une variable du jeu
set_var(nom, valeur)         → écrit une variable du jeu
shake(intensité, secondes)   → fait trembler l'écran
flash(r, g, b, secondes)     → flash de couleur plein écran
notify(texte)                → petite notification en haut de l'écran
play_sound(ressource)        → joue un son du projet
jump(scène)                  → saute vers une scène
log(texte)                   → écrit dans la console de l'éditeur
random(min, max)             → entier aléatoire entre min et max (inclus)";

/// Catalogue d'extensions prêtes à l'emploi, installables en un clic.
pub fn catalog() -> Vec<Extension> {
    vec![
        Extension {
            id: "screen_fx".into(),
            name: "Effets d'écran".into(),
            version: "1.0.0".into(),
            author: "VN Conceptor".into(),
            description: "Tremblement et flash de couleur pour les moments dramatiques.".into(),
            enabled: true,
            script: r#"
fn shake(args) {
    shake(parse_float(args.intensite), parse_float(args.duree));
}

fn flash(args) {
    flash(parse_int(args.r), parse_int(args.g), parse_int(args.b), parse_float(args.duree));
}
"#
            .into(),
            commands: vec![
                cmd(
                    "shake",
                    "Tremblement",
                    &[("intensite", "12"), ("duree", "0.5")],
                ),
                cmd(
                    "flash",
                    "Flash",
                    &[("r", "255"), ("g", "255"), ("b", "255"), ("duree", "0.4")],
                ),
            ],
        },
        Extension {
            id: "affection".into(),
            name: "Jauge d'affection".into(),
            version: "1.0.0".into(),
            author: "VN Conceptor".into(),
            description:
                "Ajoute/retire des points d'affection par personnage et branche selon le score."
                    .into(),
            enabled: true,
            script: r#"
fn add(args) {
    let key = "affection_" + args.personnage;
    let v = get_var(key);
    if type_of(v) != "i64" { v = 0; }
    v += parse_int(args.points);
    set_var(key, v);
    notify(args.personnage + " ♥ " + v);
}

fn branch(args) {
    let v = get_var("affection_" + args.personnage);
    if type_of(v) == "i64" && v >= parse_int(args.seuil) {
        jump(args.scene_si_ok);
    } else if args.scene_sinon != "" {
        jump(args.scene_sinon);
    }
}
"#
            .into(),
            commands: vec![
                cmd(
                    "add",
                    "Ajouter affection",
                    &[("personnage", "heroine"), ("points", "1")],
                ),
                cmd(
                    "branch",
                    "Brancher selon affection",
                    &[
                        ("personnage", "heroine"),
                        ("seuil", "3"),
                        ("scene_si_ok", ""),
                        ("scene_sinon", ""),
                    ],
                ),
            ],
        },
        Extension {
            id: "achievements".into(),
            name: "Succès".into(),
            version: "1.0.0".into(),
            author: "VN Conceptor".into(),
            description: "Débloque des succès avec une notification.".into(),
            enabled: true,
            script: r#"
fn unlock(args) {
    let key = "succes_" + args.id;
    if get_var(key) != true {
        set_var(key, true);
        notify("🏆 Succès débloqué : " + args.titre);
    }
}
"#
            .into(),
            commands: vec![cmd(
                "unlock",
                "Débloquer un succès",
                &[("id", "premier"), ("titre", "Premier pas")],
            )],
        },
        Extension {
            id: "dice".into(),
            name: "Lancer de dés".into(),
            version: "1.0.0".into(),
            author: "VN Conceptor".into(),
            description: "Lance un dé et range le résultat dans une variable (style JDR).".into(),
            enabled: true,
            script: r#"
fn roll(args) {
    let r = random(1, parse_int(args.faces));
    set_var(args.variable, r);
    notify("🎲 " + r);
}
"#
            .into(),
            commands: vec![cmd(
                "roll",
                "Lancer un dé",
                &[("faces", "6"), ("variable", "de")],
            )],
        },
        Extension {
            id: "sound_fx".into(),
            name: "Bruitages rapides".into(),
            version: "1.0.0".into(),
            author: "VN Conceptor".into(),
            description: "Joue un son et fait trembler l'écran en même temps (impact, porte...)."
                .into(),
            enabled: true,
            script: r#"
fn impact(args) {
    if args.son != "" { play_sound(args.son); }
    shake(parse_float(args.force), 0.3);
}
"#
            .into(),
            commands: vec![cmd("impact", "Impact", &[("son", ""), ("force", "20")])],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_vnext_file_is_valid() {
        let text = include_str!("../../../extensions/porte_monnaie.vnext");
        let ext = Extension::from_json(text).unwrap();
        let mut host = crate::script::ScriptHost::new();
        host.check(&ext).unwrap();
        host.call_command(&ext, "gagner", &Default::default())
            .unwrap();
        assert_eq!(host.vars()["argent"], "10");
    }
}
