//! Son : musique de fond (déclarative, suit l'état du jeu) et bruitages.

use rodio::{Decoder, MixerDeviceSink, Player, Source};
use std::io::Cursor;
use std::sync::Arc;

use crate::assets::AssetSource;
use vn_core::runtime::Music;

pub struct Audio {
    sink: Option<MixerDeviceSink>,
    music: Option<(Music, Player)>,
    sounds: Vec<Player>,
    pub last_error: Option<String>,
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

impl Audio {
    pub fn new() -> Self {
        let (sink, last_error) = match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(mut s) => {
                s.log_on_drop(false);
                (Some(s), None)
            }
            Err(e) => (None, Some(format!("pas de sortie audio : {e}"))),
        };
        Self {
            sink,
            music: None,
            sounds: Vec::new(),
            last_error,
        }
    }

    /// Fait correspondre la musique jouée avec celle demandée par le jeu.
    pub fn sync_music(&mut self, wanted: Option<&Music>, assets: &dyn AssetSource) {
        let same_track = match (&self.music, wanted) {
            (Some((cur, _)), Some(w)) => cur.asset == w.asset && cur.looped == w.looped,
            (None, None) => true,
            _ => false,
        };
        if same_track {
            if let (Some((cur, player)), Some(w)) = (&mut self.music, wanted) {
                if cur.volume != w.volume {
                    player.set_volume(w.volume);
                    cur.volume = w.volume;
                }
            }
            return;
        }
        self.music = None; // stoppe l'ancienne musique
        let Some(w) = wanted else { return };
        if w.asset.is_empty() {
            return;
        }
        match self.player_for(&w.asset, w.looped, assets) {
            Ok(player) => {
                player.set_volume(w.volume);
                self.music = Some((w.clone(), player));
            }
            Err(e) => {
                self.last_error = Some(e);
                // on mémorise quand même pour ne pas réessayer à chaque image
                if let Some(sink) = &self.sink {
                    self.music = Some((w.clone(), Player::connect_new(sink.mixer())));
                }
            }
        }
    }

    pub fn play_sound(&mut self, asset: &str, volume: f32, assets: &dyn AssetSource) {
        self.sounds.retain(|p| !p.empty());
        match self.player_for(asset, false, assets) {
            Ok(p) => {
                p.set_volume(volume);
                self.sounds.push(p);
            }
            Err(e) => self.last_error = Some(e),
        }
    }

    pub fn stop_all(&mut self) {
        self.music = None;
        self.sounds.clear();
    }

    /// Accès au mixeur (utilisé par la vidéo pour sa bande son).
    pub fn new_player(&self) -> Option<Player> {
        self.sink.as_ref().map(|s| Player::connect_new(s.mixer()))
    }

    fn player_for(
        &self,
        asset: &str,
        looped: bool,
        assets: &dyn AssetSource,
    ) -> Result<Player, String> {
        let sink = self.sink.as_ref().ok_or("pas de sortie audio")?;
        let bytes = assets
            .bytes(asset)
            .ok_or_else(|| format!("son introuvable : {asset}"))?;
        let player = Player::connect_new(sink.mixer());
        match append(&player, bytes.clone(), looped) {
            Ok(()) => Ok(player),
            // Format non géré nativement (opus, wma...) : conversion en WAV par ffmpeg.
            Err(e) => match crate::ffmpeg::convert(&bytes, "wav") {
                Some(wav) => append(&player, Arc::from(wav), looped).map(|_| player),
                None => Err(format!("{asset} : {e}")),
            },
        }
    }
}

fn append(player: &Player, bytes: Arc<[u8]>, looped: bool) -> Result<(), String> {
    if looped {
        let d = Decoder::new_looped(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        player.append(d);
    } else {
        let d = Decoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        player.append(d);
    }
    Ok(())
}

/// Source audio alimentée par un canal (utilisée pour la bande son des vidéos).
pub struct ChannelSource {
    pub rx: std::sync::mpsc::Receiver<Vec<f32>>,
    pub buf: Vec<f32>,
    pub pos: usize,
    pub finished: bool,
}

impl Iterator for ChannelSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.pos >= self.buf.len() {
            if self.finished {
                return None;
            }
            match self.rx.try_recv() {
                Ok(b) => {
                    self.buf = b;
                    self.pos = 0;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return Some(0.0),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.finished = true;
                    return None;
                }
            }
            if self.buf.is_empty() {
                return Some(0.0);
            }
        }
        let s = self.buf[self.pos];
        self.pos += 1;
        Some(s)
    }
}

impl Source for ChannelSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        rodio::ChannelCount::new(2).unwrap()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        rodio::SampleRate::new(44_100).unwrap()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}
