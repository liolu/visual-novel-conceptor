//! Moteur de script (Rhai) : variables du jeu, conditions et extensions.

use rhai::{Dynamic, Engine, Map, Scope, AST};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::extension::Extension;

/// Effets déclenchés par le scénario ou par les extensions, consommés par l'affichage.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Shake { intensity: f32, seconds: f32 },
    Flash { color: [u8; 3], seconds: f32 },
    Notify(String),
    PlaySound { asset: String, volume: f32 },
    Log(String),
}

#[derive(Default)]
struct Shared {
    vars: BTreeMap<String, Dynamic>,
    effects: Vec<Effect>,
    jump: Option<String>,
    rng: u64,
}

pub struct ScriptHost {
    engine: Engine,
    shared: Rc<RefCell<Shared>>,
    /// Cache des scripts compilés : id extension -> (texte source, AST).
    compiled: HashMap<String, (String, Result<AST, String>)>,
}

impl Default for ScriptHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptHost {
    pub fn new() -> Self {
        let shared = Rc::new(RefCell::new(Shared {
            rng: seed(),
            ..Default::default()
        }));
        let mut engine = Engine::new();
        engine.set_max_operations(1_000_000); // évite les boucles infinies d'un script
        engine.set_max_call_levels(64);

        let s = shared.clone();
        engine.register_fn("get_var", move |name: &str| -> Dynamic {
            s.borrow().vars.get(name).cloned().unwrap_or(Dynamic::UNIT)
        });
        let s = shared.clone();
        engine.register_fn("set_var", move |name: &str, value: Dynamic| {
            s.borrow_mut().vars.insert(name.to_string(), value);
        });
        let s = shared.clone();
        engine.register_fn("shake", move |intensity: f64, seconds: f64| {
            s.borrow_mut().effects.push(Effect::Shake {
                intensity: intensity as f32,
                seconds: seconds as f32,
            });
        });
        let s = shared.clone();
        engine.register_fn("flash", move |r: i64, g: i64, b: i64, seconds: f64| {
            let c = |v: i64| v.clamp(0, 255) as u8;
            s.borrow_mut().effects.push(Effect::Flash {
                color: [c(r), c(g), c(b)],
                seconds: seconds as f32,
            });
        });
        let s = shared.clone();
        engine.register_fn("notify", move |text: &str| {
            s.borrow_mut()
                .effects
                .push(Effect::Notify(text.to_string()));
        });
        let s = shared.clone();
        engine.register_fn("play_sound", move |asset: &str| {
            s.borrow_mut().effects.push(Effect::PlaySound {
                asset: asset.to_string(),
                volume: 1.0,
            });
        });
        let s = shared.clone();
        engine.register_fn("jump", move |scene: &str| {
            s.borrow_mut().jump = Some(scene.to_string());
        });
        let s = shared.clone();
        engine.register_fn("log", move |text: Dynamic| {
            s.borrow_mut().effects.push(Effect::Log(text.to_string()));
        });
        let s = shared.clone();
        engine.register_fn("random", move |min: i64, max: i64| -> i64 {
            let mut sh = s.borrow_mut();
            // xorshift64
            let mut x = sh.rng;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            sh.rng = x;
            let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
            lo + (x % ((hi - lo + 1) as u64)) as i64
        });
        engine.on_print({
            let s = shared.clone();
            move |text| s.borrow_mut().effects.push(Effect::Log(text.to_string()))
        });

        Self {
            engine,
            shared,
            compiled: HashMap::new(),
        }
    }

    /// Réinitialise les variables avec les valeurs initiales du projet.
    pub fn reset_vars(&mut self, initial: &BTreeMap<String, String>) {
        let mut vars = BTreeMap::new();
        for (name, expr) in initial {
            let v = self
                .engine
                .eval_expression::<Dynamic>(expr)
                .unwrap_or_else(|_| Dynamic::from(expr.clone()));
            vars.insert(name.clone(), v);
        }
        self.shared.borrow_mut().vars = vars;
    }

    pub fn vars(&self) -> BTreeMap<String, String> {
        self.shared
            .borrow()
            .vars
            .iter()
            .map(|(k, v)| (k.clone(), format_dynamic(v)))
            .collect()
    }

    /// Modifie une variable depuis l'éditeur (la valeur est une expression Rhai).
    pub fn set_var_expr(&mut self, name: &str, expr: &str) -> Result<(), String> {
        let v = self.eval(expr)?;
        self.shared.borrow_mut().vars.insert(name.to_string(), v);
        Ok(())
    }

    pub fn set_var(&mut self, name: &str, value: Dynamic) {
        self.shared
            .borrow_mut()
            .vars
            .insert(name.to_string(), value);
    }

    pub fn remove_var(&mut self, name: &str) {
        self.shared.borrow_mut().vars.remove(name);
    }

    fn scope(&self) -> Scope<'static> {
        let mut scope = Scope::new();
        for (k, v) in &self.shared.borrow().vars {
            if is_ident(k) {
                scope.push_dynamic(k.clone(), v.clone());
            }
        }
        scope
    }

    /// Évalue une expression avec les variables du jeu. Les variables modifiées
    /// par l'expression (ex: `score += 1`) sont réécrites.
    pub fn eval(&mut self, expr: &str) -> Result<Dynamic, String> {
        let mut scope = self.scope();
        let r = self
            .engine
            .eval_with_scope::<Dynamic>(&mut scope, expr)
            .map_err(|e| e.to_string());
        let mut sh = self.shared.borrow_mut();
        for (name, _, value) in scope.iter() {
            sh.vars.insert(name.to_string(), value);
        }
        r
    }

    pub fn eval_bool(&mut self, expr: &str) -> Result<bool, String> {
        let v = self.eval(expr)?;
        v.as_bool()
            .map_err(|t| format!("la condition doit être vraie/fausse, reçu : {t}"))
    }

    /// Compile (avec cache) le script d'une extension. Rappelé à chaque appel, donc
    /// une modification du script est prise en compte immédiatement, même en jeu.
    pub fn check(&mut self, ext: &Extension) -> Result<(), String> {
        self.ast(ext).map(|_| ())
    }

    fn ast(&mut self, ext: &Extension) -> Result<AST, String> {
        let entry = self.compiled.get(&ext.id);
        if entry.map(|(src, _)| src != &ext.script).unwrap_or(true) {
            let res = self.engine.compile(&ext.script).map_err(|e| e.to_string());
            self.compiled
                .insert(ext.id.clone(), (ext.script.clone(), res));
        }
        self.compiled[&ext.id].1.clone()
    }

    /// Exécute une commande d'extension. Retourne une éventuelle scène vers laquelle sauter.
    pub fn call_command(
        &mut self,
        ext: &Extension,
        command: &str,
        args: &BTreeMap<String, String>,
    ) -> Result<Option<String>, String> {
        let ast = self.ast(ext)?;
        let mut map = Map::new();
        // Valeurs par défaut des paramètres puis valeurs saisies.
        if let Some(def) = ext.commands.iter().find(|c| c.name == command) {
            for p in &def.params {
                map.insert(p.name.as_str().into(), Dynamic::from(p.default.clone()));
            }
        }
        for (k, v) in args {
            map.insert(k.as_str().into(), Dynamic::from(v.clone()));
        }
        self.shared.borrow_mut().jump = None;
        let mut scope = Scope::new();
        let _ = self
            .engine
            .call_fn::<Dynamic>(&mut scope, &ast, command, (map,))
            .map_err(|e| format!("{}::{command} : {e}", ext.id))?;
        Ok(self.shared.borrow_mut().jump.take())
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.shared.borrow_mut().effects)
    }

    pub fn push_effect(&mut self, e: Effect) {
        self.shared.borrow_mut().effects.push(e);
    }
}

fn format_dynamic(v: &Dynamic) -> String {
    if v.is_string() {
        format!("{:?}", v.to_string())
    } else {
        v.to_string()
    }
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(target_arch = "wasm32")]
fn seed() -> u64 {
    0x9E37_79B9_7F4A_7C15
}

#[cfg(not(target_arch = "wasm32"))]
fn seed() -> u64 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    t | 1
}
