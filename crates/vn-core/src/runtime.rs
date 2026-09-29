//! Exécution d'un projet : avance dans les scènes, gère l'état de la scène affichée.
//!
//! Le runtime ne copie PAS le texte des dialogues : il garde seulement une position
//! (scène + index). L'affichage relit le projet à chaque image, donc une modification
//! faite dans l'éditeur pendant que le jeu tourne apparaît immédiatement.

use crate::model::{Node, Project, StagePosition};
use crate::script::{Effect, ScriptHost};

#[derive(Clone, Debug, PartialEq)]
pub struct StageCharacter {
    pub id: String,
    pub expression: String,
    pub position: StagePosition,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Music {
    pub asset: String,
    pub looped: bool,
    pub volume: f32,
}

/// Ce qui est visible / audible à l'écran.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stage {
    pub background: Option<String>,
    pub background_color: [u8; 3],
    pub characters: Vec<StageCharacter>,
    pub music: Option<Music>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Waiting {
    /// Attente d'un clic sur un dialogue.
    Dialogue,
    /// Attente d'un choix du joueur.
    Choice,
    Timer(f32),
    Video {
        asset: String,
        skippable: bool,
    },
    Ended,
}

pub struct Runtime {
    pub scene: String,
    pub index: usize,
    pub stage: Stage,
    pub waiting: Waiting,
    pub script: ScriptHost,
    /// Messages d'erreur / de debug (affichés dans la console de l'éditeur).
    pub log: Vec<String>,
}

impl Runtime {
    pub fn new(project: &Project) -> Self {
        let mut rt = Self {
            scene: project.start_scene.clone(),
            index: 0,
            stage: Stage::default(),
            waiting: Waiting::Ended,
            script: ScriptHost::new(),
            log: Vec::new(),
        };
        rt.restart(project);
        rt
    }

    /// Relance le jeu depuis le début (variables réinitialisées).
    pub fn restart(&mut self, project: &Project) {
        self.script.reset_vars(&project.variables);
        self.stage = Stage::default();
        let start = project.start_scene.clone();
        self.goto(project, &start, 0);
    }

    /// Place le jeu à une position précise (utile pour tester depuis un nœud).
    /// L'état visuel est reconstruit à partir du début de la scène.
    pub fn goto(&mut self, project: &Project, scene: &str, index: usize) {
        self.scene = scene.to_string();
        self.index = index;
        self.resync(project);
        self.run(project);
    }

    /// Recalcule le décor / personnages / musique en rejouant (sans pause) les nœuds
    /// visuels de la scène jusqu'à la position courante. Appelé après une modification
    /// dans l'éditeur pour que le changement soit visible sans relancer le jeu.
    pub fn resync(&mut self, project: &Project) {
        let Some(scene) = project.scene(&self.scene) else {
            return;
        };
        let mut stage = Stage {
            music: self.stage.music.clone(),
            ..Stage::default()
        };
        for node in scene.nodes.iter().take(self.index) {
            apply_visual(&mut stage, node);
        }
        self.stage = stage;
        // Le nœud courant a peut-être changé de type : on vérifie que l'attente correspond.
        let consistent = matches!(
            (scene.nodes.get(self.index), &self.waiting),
            (Some(Node::Dialogue { .. }), Waiting::Dialogue)
                | (Some(Node::Choice { .. }), Waiting::Choice)
                | (Some(Node::Wait { .. }), Waiting::Timer(_))
                | (Some(Node::PlayVideo { .. }), Waiting::Video { .. })
                // Une partie terminée le reste (on ne relance pas le jeu à chaque modification).
                | (_, Waiting::Ended)
        );
        if !consistent {
            self.run(project);
        }
    }

    pub fn current_node<'a>(&self, project: &'a Project) -> Option<&'a Node> {
        project.scene(&self.scene)?.nodes.get(self.index)
    }

    /// Clic / touche « suivant ».
    pub fn advance(&mut self, project: &Project) {
        if self.waiting == Waiting::Dialogue {
            self.index += 1;
            self.run(project);
        }
    }

    pub fn choose(&mut self, project: &Project, option: usize) {
        if self.waiting != Waiting::Choice {
            return;
        }
        if let Some(Node::Choice { options, .. }) = self.current_node(project) {
            if let Some(opt) = options.get(option) {
                let target = opt.target.clone();
                if target.is_empty() {
                    self.waiting = Waiting::Ended;
                } else {
                    self.jump(project, &target);
                }
            }
        }
    }

    pub fn video_finished(&mut self, project: &Project) {
        if matches!(self.waiting, Waiting::Video { .. }) {
            self.index += 1;
            self.run(project);
        }
    }

    /// À appeler à chaque image avec le temps écoulé.
    pub fn update(&mut self, project: &Project, dt: f32) {
        if let Waiting::Timer(t) = &mut self.waiting {
            *t -= dt;
            if *t <= 0.0 {
                self.index += 1;
                self.run(project);
            }
        }
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        let effects = self.script.take_effects();
        for e in &effects {
            if let Effect::Log(msg) = e {
                self.log.push(msg.clone());
            }
        }
        effects
    }

    fn error(&mut self, msg: String) {
        self.log
            .push(format!("⚠ [{}#{}] {msg}", self.scene, self.index));
        self.script.push_effect(Effect::Notify(format!("⚠ {msg}")));
    }

    fn jump(&mut self, project: &Project, scene: &str) {
        if project.scene(scene).is_none() {
            self.error(format!("scène introuvable : « {scene} »"));
            self.waiting = Waiting::Ended;
            return;
        }
        self.scene = scene.to_string();
        self.index = 0;
        self.run(project);
    }

    /// Exécute les nœuds jusqu'au prochain nœud bloquant.
    fn run(&mut self, project: &Project) {
        for _ in 0..10_000 {
            let Some(scene) = project.scene(&self.scene) else {
                let s = self.scene.clone();
                self.error(format!("scène introuvable : « {s} »"));
                self.waiting = Waiting::Ended;
                return;
            };
            let Some(node) = scene.nodes.get(self.index) else {
                self.waiting = Waiting::Ended;
                return;
            };
            if apply_visual(&mut self.stage, node) {
                self.index += 1;
                continue;
            }
            match node {
                Node::Dialogue { .. } => {
                    self.waiting = Waiting::Dialogue;
                    return;
                }
                Node::Choice { .. } => {
                    self.waiting = Waiting::Choice;
                    return;
                }
                Node::Wait { seconds } => {
                    self.waiting = Waiting::Timer(*seconds);
                    return;
                }
                Node::PlayVideo { asset, skippable } => {
                    self.waiting = Waiting::Video {
                        asset: asset.clone(),
                        skippable: *skippable,
                    };
                    return;
                }
                Node::End => {
                    self.waiting = Waiting::Ended;
                    return;
                }
                Node::PlaySound { asset, volume } => {
                    let e = Effect::PlaySound {
                        asset: asset.clone(),
                        volume: *volume,
                    };
                    self.script.push_effect(e);
                    self.index += 1;
                }
                Node::Jump { scene } => {
                    let scene = scene.clone();
                    return self.jump(project, &scene);
                }
                Node::SetVariable { name, expression } => {
                    let (name, expression) = (name.clone(), expression.clone());
                    match self.script.eval(&expression) {
                        Ok(v) if !name.trim().is_empty() => self.script.set_var(name.trim(), v),
                        Ok(_) => {}
                        Err(e) => self.error(format!("variable {name} : {e}")),
                    }
                    self.index += 1;
                }
                Node::Condition {
                    condition,
                    then_scene,
                    else_scene,
                } => {
                    let (condition, then_scene, else_scene) =
                        (condition.clone(), then_scene.clone(), else_scene.clone());
                    match self.script.eval_bool(&condition) {
                        Ok(true) if !then_scene.is_empty() => {
                            return self.jump(project, &then_scene)
                        }
                        Ok(false) if !else_scene.is_empty() => {
                            return self.jump(project, &else_scene)
                        }
                        Ok(_) => self.index += 1,
                        Err(e) => {
                            self.error(format!("condition « {condition} » : {e}"));
                            self.index += 1;
                        }
                    }
                }
                Node::Extension {
                    extension,
                    command,
                    args,
                } => {
                    let jump = match project.extension(extension) {
                        Some(ext) if ext.enabled => {
                            match self.script.call_command(ext, command, args) {
                                Ok(j) => j,
                                Err(e) => {
                                    self.error(e);
                                    None
                                }
                            }
                        }
                        Some(_) => None, // extension désactivée : on ignore
                        None => {
                            self.error(format!("extension « {extension} » non installée"));
                            None
                        }
                    };
                    if let Some(scene) = jump {
                        return self.jump(project, &scene);
                    }
                    self.index += 1;
                }
                // Déjà traités par apply_visual.
                Node::Background { .. }
                | Node::ShowCharacter { .. }
                | Node::HideCharacter { .. }
                | Node::PlayMusic { .. }
                | Node::StopMusic => self.index += 1,
            }
        }
        self.error("boucle infinie détectée (10 000 nœuds sans pause)".into());
        self.waiting = Waiting::Ended;
    }
}

/// Applique un nœud purement visuel/sonore persistant. Retourne `true` s'il l'était.
fn apply_visual(stage: &mut Stage, node: &Node) -> bool {
    match node {
        Node::Background { asset, color } => {
            stage.background = asset.clone().filter(|a| !a.is_empty());
            stage.background_color = *color;
        }
        Node::ShowCharacter {
            character,
            expression,
            position,
        } => {
            let c = StageCharacter {
                id: character.clone(),
                expression: expression.clone(),
                position: *position,
            };
            match stage.characters.iter_mut().find(|x| x.id == *character) {
                Some(existing) => *existing = c,
                None => stage.characters.push(c),
            }
        }
        Node::HideCharacter { character } => stage.characters.retain(|c| c.id != *character),
        Node::PlayMusic {
            asset,
            looped,
            volume,
        } => {
            stage.music = Some(Music {
                asset: asset.clone(),
                looped: *looped,
                volume: *volume,
            });
        }
        Node::StopMusic => stage.music = None,
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::catalog;
    use crate::model::*;

    #[test]
    fn default_project_runs() {
        let p = Project::default();
        let mut rt = Runtime::new(&p);
        assert_eq!(rt.waiting, Waiting::Dialogue);
        assert_eq!(rt.index, 1);
        rt.advance(&p);
        assert_eq!(rt.index, 2);
        rt.advance(&p);
        assert_eq!(rt.waiting, Waiting::Choice);
        rt.choose(&p, 1);
        assert_eq!(rt.waiting, Waiting::Ended);
    }

    #[test]
    fn variables_and_conditions() {
        let mut p = Project::default();
        p.variables.insert("score".into(), "0".into());
        p.scenes.push(Scene {
            id: "win".into(),
            name: "win".into(),
            nodes: vec![Node::End],
        });
        p.scenes[0].nodes = vec![
            Node::SetVariable {
                name: "score".into(),
                expression: "score + 5".into(),
            },
            Node::Condition {
                condition: "score > 3".into(),
                then_scene: "win".into(),
                else_scene: String::new(),
            },
            Node::Dialogue {
                speaker: None,
                text: "perdu".into(),
            },
        ];
        let rt = Runtime::new(&p);
        assert_eq!(rt.scene, "win");
        assert_eq!(rt.script.vars()["score"], "5");
    }

    #[test]
    fn catalog_extensions_compile_and_run() {
        let mut p = Project {
            extensions: catalog(),
            ..Default::default()
        };
        let mut rt = Runtime::new(&p);
        for ext in &p.extensions {
            rt.script
                .check(ext)
                .unwrap_or_else(|e| panic!("{} : {e}", ext.id));
        }
        p.scenes[0].nodes = vec![
            Node::Extension {
                extension: "affection".into(),
                command: "add".into(),
                args: Default::default(),
            },
            Node::Extension {
                extension: "dice".into(),
                command: "roll".into(),
                args: Default::default(),
            },
            Node::Extension {
                extension: "screen_fx".into(),
                command: "shake".into(),
                args: Default::default(),
            },
            Node::Extension {
                extension: "achievements".into(),
                command: "unlock".into(),
                args: Default::default(),
            },
            Node::Dialogue {
                speaker: None,
                text: "ok".into(),
            },
        ];
        rt.restart(&p);
        assert!(rt.log.is_empty(), "{:?}", rt.log);
        let vars = rt.script.vars();
        assert_eq!(vars["affection_heroine"], "1");
        assert!(vars.contains_key("de"));
        let fx = rt.take_effects();
        assert!(fx.iter().any(|e| matches!(e, Effect::Shake { .. })));
    }

    #[test]
    fn live_edit_resync() {
        let mut p = Project::default();
        let mut rt = Runtime::new(&p);
        rt.advance(&p); // sur le dialogue de l'héroïne
        p.scenes[0].nodes.insert(
            0,
            Node::ShowCharacter {
                character: "heroine".into(),
                expression: String::new(),
                position: StagePosition::Left,
            },
        );
        rt.index += 1; // l'éditeur décale la position quand on insère avant
        rt.resync(&p);
        assert_eq!(rt.stage.characters.len(), 1);
        assert_eq!(rt.waiting, Waiting::Dialogue);
    }
}
