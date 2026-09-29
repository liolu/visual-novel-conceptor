//! Moteur d'affichage du Visual Novel (egui) partagé par l'éditeur et le jeu compilé.

pub mod assets;
pub mod audio;
pub mod ffmpeg;
pub mod images;
pub mod video;
pub mod view;

pub use egui;
pub use view::GameView;
