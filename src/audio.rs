//! Playback: one file at a time, through rodio's symphonia decoders.
//!
//! The only things the player page asks of it are BitChord's own transport
//! verbs — load, play, pause, seek, volume — and where the playhead is.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

pub struct Audio {
    // Dropping the stream closes the device, so it lives as long as the player.
    _stream: OutputStream,
    handle: OutputStreamHandle,
    sink: Option<Sink>,
    path: Option<PathBuf>,
    volume: f32,
}

impl Audio {
    pub fn new() -> Result<Self> {
        let (stream, handle) = OutputStream::try_default().context("no audio output device")?;
        Ok(Self {
            _stream: stream,
            handle,
            sink: None,
            path: None,
            volume: 1.0,
        })
    }

    /// Opens [path] and starts it from the top, playing.
    pub fn load(&mut self, path: &Path) -> Result<()> {
        if let Some(old) = self.sink.take() {
            old.stop();
        }
        let sink = self.open(path)?;
        sink.play();
        self.sink = Some(sink);
        self.path = Some(path.to_path_buf());
        Ok(())
    }

    fn open(&self, path: &Path) -> Result<Sink> {
        let file = File::open(path).with_context(|| format!("can't open {}", path.display()))?;
        let decoder = Decoder::new(BufReader::new(file))
            .with_context(|| format!("can't decode {}", path.display()))?;
        let sink = Sink::try_new(&self.handle)?;
        sink.set_volume(self.volume);
        sink.pause();
        sink.append(decoder);
        Ok(sink)
    }

    pub fn play(&mut self) {
        // A track that has run out is opened again, the way pressing play on a
        // finished song starts it over.
        if self.finished() {
            if let Some(path) = self.path.clone() {
                if let Ok(sink) = self.open(&path) {
                    self.sink = Some(sink);
                }
            }
        }
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }

    pub fn pause(&mut self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    pub fn seek(&mut self, to: Duration) {
        // Seeking back into a finished track reopens it, paused where it lands.
        if self.finished() {
            if let Some(path) = self.path.clone() {
                if let Ok(sink) = self.open(&path) {
                    self.sink = Some(sink);
                }
            }
        }
        if let Some(sink) = &self.sink {
            let _ = sink.try_seek(to);
        }
    }

    /// 0..1, linear gain — what BitChord's desktop engine hands its sink.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
        }
    }

    pub fn position(&self) -> Duration {
        self.sink.as_ref().map(Sink::get_pos).unwrap_or_default()
    }

    /// Whether the loaded track has played out to its end.
    pub fn finished(&self) -> bool {
        self.sink.as_ref().is_some_and(Sink::empty)
    }
}
