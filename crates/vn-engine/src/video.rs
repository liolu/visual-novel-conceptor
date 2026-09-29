//! Lecture vidéo tous formats (mp4, webm, mkv, avi, mov...) via un processus ffmpeg
//! qui décode en images RGBA brutes + un second pour la piste audio.

use egui::{self, ColorImage, TextureHandle, TextureOptions};

#[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
mod imp {
    use super::*;
    use std::io::Read;
    use std::process::{Child, Stdio};
    use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
    use std::time::Instant;

    const FPS: f64 = 30.0;

    pub struct VideoPlayer {
        width: usize,
        height: usize,
        frames: Receiver<Vec<u8>>,
        texture: Option<TextureHandle>,
        start: Instant,
        shown: u64,
        pub finished: bool,
        children: Vec<Child>,
        _audio: Option<rodio::Player>,
    }

    impl VideoPlayer {
        pub fn start(
            path: &std::path::Path,
            size: [u32; 2],
            audio: Option<rodio::Player>,
        ) -> Result<Self, String> {
            let exe = crate::ffmpeg::path()
                .ok_or("ffmpeg introuvable : placez ffmpeg à côté du logiciel ou installez-le pour lire les vidéos")?;
            let (w, h) = (size[0] as usize & !1, size[1] as usize & !1);
            let filter = format!(
                "fps={FPS},scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2"
            );
            let mut video = crate::ffmpeg::command(&exe)
                .arg("-i")
                .arg(path)
                .args([
                    "-an", "-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("ffmpeg : {e}"))?;
            let mut out = video.stdout.take().unwrap();
            let (tx, rx) = sync_channel::<Vec<u8>>(4);
            let frame_len = w * h * 4;
            std::thread::spawn(move || loop {
                let mut buf = vec![0u8; frame_len];
                if out.read_exact(&mut buf).is_err() || tx.send(buf).is_err() {
                    break;
                }
            });
            let mut children = vec![video];

            // Piste audio (facultative).
            let mut audio_player = None;
            if let Some(player) = audio {
                if let Ok(mut child) = crate::ffmpeg::command(&exe)
                    .arg("-i")
                    .arg(path)
                    .args(["-vn", "-f", "f32le", "-ac", "2", "-ar", "44100", "pipe:1"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    let mut out = child.stdout.take().unwrap();
                    let (atx, arx) = sync_channel::<Vec<f32>>(64);
                    std::thread::spawn(move || loop {
                        let mut buf = vec![0u8; 4096 * 4];
                        match out.read(&mut buf) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                let n = n - n % 4;
                                let samples = buf[..n]
                                    .chunks_exact(4)
                                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                                    .collect();
                                if atx.send(samples).is_err() {
                                    break;
                                }
                            }
                        }
                    });
                    player.append(crate::audio::ChannelSource {
                        rx: arx,
                        buf: Vec::new(),
                        pos: 0,
                        finished: false,
                    });
                    children.push(child);
                    audio_player = Some(player);
                }
            }

            Ok(Self {
                width: w,
                height: h,
                frames: rx,
                texture: None,
                start: Instant::now(),
                shown: 0,
                finished: false,
                children,
                _audio: audio_player,
            })
        }

        /// Met à jour la texture selon le temps écoulé et la retourne.
        pub fn texture(&mut self, ctx: &egui::Context) -> Option<&TextureHandle> {
            let target = (self.start.elapsed().as_secs_f64() * FPS) as u64;
            let mut latest = None;
            while self.shown <= target && !self.finished {
                match self.frames.try_recv() {
                    Ok(f) => {
                        latest = Some(f);
                        self.shown += 1;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => self.finished = true,
                }
            }
            if let Some(f) = latest {
                let img = ColorImage::from_rgba_unmultiplied([self.width, self.height], &f);
                match &mut self.texture {
                    Some(t) => t.set(img, TextureOptions::LINEAR),
                    None => {
                        self.texture = Some(ctx.load_texture("video", img, TextureOptions::LINEAR))
                    }
                }
            }
            ctx.request_repaint();
            self.texture.as_ref()
        }
    }

    impl Drop for VideoPlayer {
        fn drop(&mut self) {
            for c in &mut self.children {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "android", target_os = "ios"))]
mod imp {
    use super::*;

    pub struct VideoPlayer {
        pub finished: bool,
    }

    impl VideoPlayer {
        pub fn start(
            _path: &std::path::Path,
            _size: [u32; 2],
            _audio: Option<rodio::Player>,
        ) -> Result<Self, String> {
            let _ = (ColorImage::default, TextureOptions::LINEAR);
            Err("vidéo non disponible sur cette plateforme dans ce prototype".into())
        }
        pub fn texture(&mut self, _ctx: &egui::Context) -> Option<&TextureHandle> {
            None
        }
    }
}

pub use imp::VideoPlayer;
