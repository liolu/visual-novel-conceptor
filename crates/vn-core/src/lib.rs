//! Cœur du moteur de Visual Novel : modèle de données, exécution, scripts et format compilé.
//! Ne dépend d'aucune interface graphique : fonctionne sur PC, mobile et web.

pub mod extension;
pub mod model;
pub mod pack;
pub mod runtime;
pub mod script;

pub use rhai;
