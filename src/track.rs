//! What the player page shows about a file: its credits, its length, its
//! cover, and the quality badge it earns.

use std::path::Path;

use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::tag::Accessor;

/// BitChord's line for "High quality": lossy, at 256kbps or better.
const HI_QUALITY_KBPS: u32 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    /// The badge's shimmering "Lossless" / "Hi-Res Lossless".
    Lossless { hi_res: bool },
    /// The plain "High quality" label.
    High,
    /// Nothing between the timestamps.
    None,
}

pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_ms: u64,
    pub cover: Option<Vec<u8>>,
    pub quality: Quality,
}

impl Track {
    pub fn read(path: &Path) -> Track {
        let (title, artist) = parse_file_name(
            &path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        let mut track = Track {
            title,
            artist,
            album: None,
            duration_ms: 0,
            cover: None,
            quality: Quality::None,
        };

        let Ok(tagged) = lofty::read_from_path(path) else {
            return track;
        };
        let properties = tagged.properties();
        track.duration_ms = properties.duration().as_millis() as u64;

        // BitChord's NerdStats: FLAC, ALAC and raw PCM are lossless; past 16-bit
        // or past 48kHz is Hi-Res; a lossy stream at 256kbps+ is High quality.
        let lossless = match tagged.file_type() {
            FileType::Flac | FileType::Wav | FileType::Aiff => true,
            // ALAC reports a bit depth; AAC in the same container does not.
            FileType::Mp4 => properties.bit_depth().is_some(),
            _ => false,
        };
        track.quality = if lossless {
            Quality::Lossless {
                hi_res: properties.bit_depth().unwrap_or(0) > 16
                    || properties.sample_rate().unwrap_or(0) > 48_000,
            }
        } else if properties.audio_bitrate().unwrap_or(0) >= HI_QUALITY_KBPS {
            Quality::High
        } else {
            Quality::None
        };

        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(title) = tag.title().filter(|t| !t.trim().is_empty()) {
                track.title = title.trim().to_string();
            }
            if let Some(artist) = tag.artist().filter(|a| !a.trim().is_empty()) {
                track.artist = artist.trim().to_string();
            }
            track.album = tag
                .album()
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty());
            let pictures = tag.pictures();
            track.cover = pictures
                .iter()
                .find(|p| p.pic_type() == PictureType::CoverFront)
                .or_else(|| pictures.first())
                .map(|p| p.data().to_vec());
        }
        track
    }
}

/// BitChord's local-file credit: "Artist - Title", or the whole name "On This Computer".
fn parse_file_name(name: &str) -> (String, String) {
    match name.split_once(" - ") {
        Some((artist, title)) if !artist.trim().is_empty() && !title.trim().is_empty() => {
            (title.trim().to_string(), artist.trim().to_string())
        }
        _ => (name.to_string(), "On This Computer".to_string()),
    }
}
