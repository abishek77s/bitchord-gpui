//! BitChord's now-playing page as its desktop window shows it: the landscape
//! player (`LandscapePlayerLayout`) over the full-cover blur.
//!
//! Every number here is BitChord's, named after the constant or modifier it
//! comes from in `LandscapePlayer.kt`, `NowPlayingScreen.kt`,
//! `PlayerControls.kt`, `ThinSlider.kt` and `ArtworkMeshBackdrop.kt`.
//!
//! The page is laid out by hand rather than with flexbox, the way those
//! Compose layouts resolve, so each rectangle can be checked against its
//! source; the drawing is one canvas, with the blurred cover on a layer of its
//! own because it fades.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    App, BoxShadow, Bounds, ContentMask, Context, Corners, DispatchPhase, ExternalPaths,
    FocusHandle, Hsla, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, RenderImage, TransformationMatrix, WeakEntity, Window, WindowControlArea, canvas, div,
    fill, hsla, linear_color_stop, linear_gradient, point, prelude::*, px, size,
};

use crate::anim::{
    Bezier, DAMPING_MEDIUM_BOUNCY, DAMPING_NO_BOUNCY, FAST_OUT_SLOW_IN, STIFFNESS_MEDIUM,
    STIFFNESS_MEDIUM_LOW, Spring, Tween,
};
use crate::artwork::{self, Artwork};
use crate::audio::Audio;
use crate::text::{self, Text};
use crate::track::{Quality, Track};

// ---- LandscapePlayer.kt -----------------------------------------------------

const LANDSCAPE_PLAYER_MAX_WIDTH: f32 = 1100.0;
const LANDSCAPE_COMPACT_HEIGHT: f32 = 440.0;
const LANDSCAPE_GUTTER_COMPACT: f32 = 20.0;
const LANDSCAPE_HANDLE_STRIP: f32 = 24.0;
const LANDSCAPE_ART_CORNER: f32 = 10.0;
/// `NowPlayingScreen.PLAYER_GUTTER` / `PLAYER_MAX_WIDTH`.
const PLAYER_GUTTER: f32 = 30.0;
const PLAYER_MAX_WIDTH: f32 = 560.0;

// ---- NowPlayingScreen.kt ----------------------------------------------------

const ARTWORK_EXPANDED_SCALE: f32 = 1.0;
const ARTWORK_PAUSE_SHRINK_SCALE: f32 = 0.88;
const ARTWORK_DRAG_SHRINK_SCALE: f32 = 0.94;
const ARTWORK_SCALE_EASING: Bezier = Bezier(0.215, 0.61, 0.355, 1.0);
const ARTWORK_SCALE_DURATION_MS: u64 = 500;
const SEEK_SETTLE_TOLERANCE_MS: u64 = 1_500;
const SEEK_SETTLE_TIMEOUT_MS: u64 = 4_000;
const BACK_RESTARTS_AFTER_MS: u64 = 10_000;

// ---- PlayerControls.kt ------------------------------------------------------

const PLAYER_SKIP_ICON_SIZE: f32 = 53.0;
const PLAYER_SKIP_TOUCH_SIZE: f32 = 53.0;
const BOTTOM_ACTION_SIZE: f32 = 44.0;
const PILL_SEGMENT_WIDTH: f32 = 64.0;
const PILL_HEADPHONES_SIZE: f32 = 23.0;
const PILL_PARTY_SIZE: f32 = 22.0;
const VOLUME_ROW_HEIGHT: f32 = 32.0;
const MARQUEE_DP_PER_SEC: f32 = 26.0;
const MARQUEE_GAP: f32 = 48.0;
const MARQUEE_REST_MS: u64 = 5_000;
const MARQUEE_ARTIST_STAGGER_MS: u64 = 3_000;

// ---- PlayerLyrics.kt --------------------------------------------------------

const LYRICS_UNAVAILABLE_HOLD_MS: u64 = 5_000;
const LYRICS_UNAVAILABLE_FADE_MS: u64 = 900;

// ---- ArtworkMeshBackdrop.kt -------------------------------------------------

const FALLBACK_BACKDROP: u32 = 0x121212;

// ---- Icons ------------------------------------------------------------------

macro_rules! icon {
    ($name:literal) => {
        (concat!("icons/", $name, ".svg"), include_bytes!(concat!("../assets/icons/", $name, ".svg")) as &[u8])
    };
}
type Icon = (&'static str, &'static [u8]);
const ICON_PLAY: Icon = icon!("play");
const ICON_PAUSE: Icon = icon!("pause");
const ICON_NEXT: Icon = icon!("next");
const ICON_PREVIOUS: Icon = icon!("previous");
const ICON_MORE: Icon = icon!("more_horiz");
const ICON_HEADPHONES: Icon = icon!("headphones");
const ICON_PERSON: Icon = icon!("person");
const ICON_VOLUME_DOWN: Icon = icon!("volume_down");
const ICON_VOLUME_UP: Icon = icon!("volume_up");
const ICON_QUEUE: Icon = icon!("queue");
const ICON_LYRICS: Icon = icon!("lyrics_quote");
const ICON_MUSIC_NOTE: Icon = icon!("music_note");

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "m4a", "aac", "ogg", "oga", "aif", "aiff"];

fn white(alpha: f32) -> Hsla {
    hsla(0.0, 0.0, 1.0, alpha)
}

fn black(alpha: f32) -> Hsla {
    hsla(0.0, 0.0, 0.0, alpha)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    fn contains(&self, p: (f32, f32)) -> bool {
        p.0 >= self.x && p.0 < self.x + self.w && p.1 >= self.y && p.1 < self.y + self.h
    }
    fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    /// A [side]-square centred in this rectangle.
    fn centered_square(&self, side: f32) -> Rect {
        let (cx, cy) = self.center();
        Rect::new(cx - side / 2.0, cy - side / 2.0, side, side)
    }
    fn scaled(&self, scale: f32) -> Rect {
        let (cx, cy) = self.center();
        Rect::new(cx - self.w * scale / 2.0, cy - self.h * scale / 2.0, self.w * scale, self.h * scale)
    }
    fn bounds(&self) -> Bounds<Pixels> {
        Bounds::new(point(px(self.x), px(self.y)), size(px(self.w), px(self.h)))
    }
}

/// Every rectangle of the landscape player for one window size.
#[derive(Clone, Copy, Debug)]
struct Layout {
    window: Rect,
    /// The square the sleeve is laid out in, before its play/pause scale.
    sleeve: Rect,
    lyrics_glyph: Rect,
    pill: Rect,
    queue_glyph: Rect,
    output_caption: Rect,
    caption: Rect,
    credits: Rect,
    lyric_strip: Rect,
    seek: Rect,
    times: Rect,
    previous: Rect,
    play: Rect,
    next: Rect,
    play_icon: f32,
    skip_icon: f32,
    volume_down: Rect,
    volume: Rect,
    volume_up: Rect,
    handle: Rect,
}

impl Layout {
    fn new(width: f32, height: f32) -> Self {
        // LandscapePlayerLayout: the two columns, capped and centred, under
        // the handle strip.
        let compact = height < LANDSCAPE_COMPACT_HEIGHT;
        let gutter = if compact { LANDSCAPE_GUTTER_COMPACT } else { PLAYER_GUTTER };
        let row_w = width.min(LANDSCAPE_PLAYER_MAX_WIDTH);
        let row_x = (width - row_w) / 2.0;
        let top = LANDSCAPE_HANDLE_STRIP;
        let bottom = height - if compact { 8.0 } else { 20.0 };
        let col_w = row_w / 2.0;

        // Left column: the sleeve takes whichever axis runs out first once the
        // actions row under it has had its height.
        let left_x = row_x + gutter;
        let left_w = (col_w - gutter * 2.0).max(0.0);
        let actions_h = BOTTOM_ACTION_SIZE + 18.0 + 20.0;
        let spacer = if compact { 12.0 } else { 24.0 };
        let art_box_h = (bottom - top - actions_h - spacer).max(0.0);
        let art_box = Rect::new(left_x, top, left_w, art_box_h);
        let sleeve = art_box.centered_square(left_w.min(art_box_h));

        // PlayerActionRow: lyrics, the output/party capsule, queue — inset as
        // if the wider three-up capsule were there.
        let actions_y = top + art_box_h + spacer;
        let pill_w = PILL_SEGMENT_WIDTH * 2.0 + 1.0;
        let widest_row = BOTTOM_ACTION_SIZE * 2.0 + (PILL_SEGMENT_WIDTH * 3.0 + 2.0);
        let edge_inset = ((left_w - widest_row) / 4.0).max(0.0);
        let inner_x = left_x + edge_inset;
        let inner_w = left_w - edge_inset * 2.0;
        let lyrics_glyph = Rect::new(inner_x, actions_y, BOTTOM_ACTION_SIZE, BOTTOM_ACTION_SIZE);
        let queue_glyph = Rect::new(
            inner_x + inner_w - BOTTOM_ACTION_SIZE,
            actions_y,
            BOTTOM_ACTION_SIZE,
            BOTTOM_ACTION_SIZE,
        );
        let pill = Rect::new(inner_x + (inner_w - pill_w) / 2.0, actions_y, pill_w, BOTTOM_ACTION_SIZE);
        let caption_w = left_w * 0.65;
        let output_caption = Rect::new(
            left_x + (left_w - caption_w) / 2.0,
            actions_y + BOTTOM_ACTION_SIZE + 18.0,
            caption_w,
            text::LABEL_SMALL.line_height,
        );

        // Right column: LandscapeMainPane, centred in what the column leaves.
        let right_x = row_x + col_w + gutter;
        let right_w = (col_w - gutter * 2.0).max(0.0);
        let pane_w = right_w.min(PLAYER_MAX_WIDTH);
        let play_touch = if compact { 76.0 } else { 92.0 };
        let caption_h = text::LABEL_SMALL.line_height + 4.0;
        let credits_h = text::TITLE_LARGE.line_height + 2.0 + text::TITLE_MEDIUM.line_height;
        let strip_h = text::TITLE_MEDIUM.line_height + 8.0;
        let seek_h = 12.0 + 22.0;
        let scrubber_h = seek_h + text::LABEL_MEDIUM.line_height;
        let gaps = if compact { [4.0, 2.0, 0.0, 0.0] } else { [8.0, 10.0, 8.0, 12.0] };
        let pane_h = caption_h + gaps[0] + credits_h + gaps[1] + strip_h + scrubber_h + gaps[2]
            + play_touch + gaps[3] + VOLUME_ROW_HEIGHT;
        let pane_x = right_x + (right_w - pane_w) / 2.0;
        let pane_y = top + ((bottom - top) - pane_h) / 2.0;
        let mut y = pane_y;
        let caption = Rect::new(pane_x, y, pane_w, caption_h);
        y += caption_h + gaps[0];
        let credits = Rect::new(pane_x, y, pane_w, credits_h);
        y += credits_h + gaps[1];
        let lyric_strip = Rect::new(pane_x, y, pane_w, strip_h);
        y += strip_h;
        let seek = Rect::new(pane_x, y, pane_w, seek_h);
        // The timestamps are pulled 9dp back up under the bar.
        let times = Rect::new(pane_x, y + seek_h - 9.0, pane_w, text::LABEL_MEDIUM.line_height);
        y += scrubber_h + gaps[2];

        // TransportRow: SpaceAround — each button gets half a gap either side.
        let skip_touch = PLAYER_SKIP_TOUCH_SIZE;
        let free = (pane_w - skip_touch * 2.0 - play_touch).max(0.0) / 3.0;
        let mut x = pane_x + free / 2.0;
        let previous = Rect::new(x, y + (play_touch - skip_touch) / 2.0, skip_touch, skip_touch);
        x += skip_touch + free;
        let play = Rect::new(x, y, play_touch, play_touch);
        x += play_touch + free;
        let next = Rect::new(x, y + (play_touch - skip_touch) / 2.0, skip_touch, skip_touch);
        y += play_touch + gaps[3];

        // VolumeRow: speaker, 10dp, bar, 10dp, speaker.
        let volume_down = Rect::new(pane_x, y + (VOLUME_ROW_HEIGHT - 20.0) / 2.0, 20.0, 20.0);
        let volume_up = Rect::new(pane_x + pane_w - 20.0, y + (VOLUME_ROW_HEIGHT - 20.0) / 2.0, 20.0, 20.0);
        let volume = Rect::new(pane_x + 30.0, y, (pane_w - 60.0).max(0.0), VOLUME_ROW_HEIGHT);

        Layout {
            window: Rect::new(0.0, 0.0, width, height),
            sleeve,
            lyrics_glyph,
            pill,
            queue_glyph,
            output_caption,
            caption,
            credits,
            lyric_strip,
            seek,
            times,
            previous,
            play,
            next,
            play_icon: if compact { 58.0 } else { 74.0 },
            skip_icon: if compact { 44.0 } else { PLAYER_SKIP_ICON_SIZE },
            volume_down,
            volume,
            volume_up,
            handle: Rect::new(width / 2.0 - 19.0, 8.0, 38.0, 5.0),
        }
    }
}

/// What a press landed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Sleeve,
    More,
    Previous,
    PlayPause,
    Next,
    Seek,
    Volume,
}

struct Credits {
    title: String,
    artist: String,
    /// When these credits arrived: their marquees count from here.
    since: Instant,
}

pub struct PlayerView {
    focus: FocusHandle,
    audio: Option<Audio>,
    track: Option<Track>,
    /// Bumped on every load, so a cover decoded for an earlier file is dropped.
    track_serial: u64,
    artwork: Option<Artwork>,
    playing: bool,
    position_ms: u64,

    credits: Credits,
    /// The credits being crossfaded out, `Crossfade(tween(300))`.
    old_credits: Option<Credits>,
    credits_fade: Tween,

    // PlayerScrub
    scrubbing: bool,
    scrub_value: f32,
    pending_seek: Option<(f32, Instant)>,
    // PlayerVolume
    volume: f32,
    volume_dragging: bool,

    pressed: Option<Target>,
    layout: Layout,

    art_scale: Tween,
    blur_alpha: Tween,
    lyric_alpha: Tween,
    seek_height: Spring,
    volume_height: Spring,
    previous_alpha: Spring,
    next_alpha: Spring,
    shimmer_since: Instant,
}

impl PlayerView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let audio = match Audio::new() {
            Ok(audio) => Some(audio),
            Err(error) => {
                eprintln!("bitchord: {error:#}");
                None
            }
        };
        let now = Instant::now();
        let size = window.viewport_size();
        Self {
            focus,
            audio,
            track: None,
            track_serial: 0,
            artwork: None,
            playing: false,
            position_ms: 0,
            credits: Credits {
                title: "Not Playing".into(),
                artist: "Click the artwork to open a file".into(),
                since: now,
            },
            old_credits: None,
            credits_fade: Tween::new(1.0),
            scrubbing: false,
            scrub_value: 0.0,
            pending_seek: None,
            volume: 1.0,
            volume_dragging: false,
            pressed: None,
            layout: Layout::new(f32::from(size.width), f32::from(size.height)),
            art_scale: Tween::new(ARTWORK_PAUSE_SHRINK_SCALE),
            blur_alpha: Tween::new(0.0),
            lyric_alpha: Tween::new(0.0),
            seek_height: Spring::new(7.0, DAMPING_MEDIUM_BOUNCY, STIFFNESS_MEDIUM_LOW, 0.1),
            volume_height: Spring::new(6.0, DAMPING_MEDIUM_BOUNCY, STIFFNESS_MEDIUM_LOW, 0.1),
            previous_alpha: Spring::new(0.3, DAMPING_NO_BOUNCY, STIFFNESS_MEDIUM, 0.01),
            next_alpha: Spring::new(0.3, DAMPING_NO_BOUNCY, STIFFNESS_MEDIUM, 0.01),
            shimmer_since: now,
        }
    }

    // ---- Files ----------------------------------------------------------------

    pub fn open_dialog(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Open a music file")
                .add_filter("Audio", AUDIO_EXTENSIONS)
                .pick_file()
                .await;
            if let Some(file) = picked {
                let path = file.path().to_path_buf();
                let _ = this.update(cx, |this, cx| this.load(path, cx));
            }
        })
        .detach();
    }

    pub fn load(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let track = Track::read(&path);
        if let Some(audio) = self.audio.as_mut() {
            if let Err(error) = audio.load(&path) {
                eprintln!("bitchord: {error:#}");
                return;
            }
        }
        self.track_serial += 1;
        let serial = self.track_serial;

        let now = Instant::now();
        let previous = std::mem::replace(
            &mut self.credits,
            Credits { title: track.title.clone(), artist: track.artist.clone(), since: now },
        );
        self.old_credits = Some(previous);
        self.credits_fade.snap_to(0.0);
        self.credits_fade.animate_to(1.0, 300, FAST_OUT_SLOW_IN);

        // "Lyrics not available" is held, then faded — but keeps its line.
        self.lyric_alpha.snap_to(0.55);
        self.lyric_alpha
            .animate_to_after(0.0, LYRICS_UNAVAILABLE_FADE_MS, LYRICS_UNAVAILABLE_HOLD_MS, FAST_OUT_SLOW_IN);

        // The blur keyed on the cover: gone with the old song, faded in when
        // the new one is ready.
        self.artwork = None;
        self.blur_alpha.snap_to(0.0);
        self.shimmer_since = now;
        self.pending_seek = None;
        self.position_ms = 0;
        self.playing = self.audio.is_some();

        if let Some(bytes) = track.cover.clone() {
            let task = cx.background_spawn(async move { artwork::decode(&bytes) });
            cx.spawn(async move |this: WeakEntity<Self>, cx| {
                let decoded = task.await;
                let _ = this.update(cx, |this, cx| {
                    if this.track_serial == serial {
                        if let Some(art) = decoded {
                            this.artwork = Some(art);
                            this.blur_alpha.animate_to(1.0, 320, FAST_OUT_SLOW_IN);
                        }
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        self.track = Some(track);
        cx.notify();
    }

    // ---- Transport -------------------------------------------------------------

    fn duration_ms(&self) -> u64 {
        self.track.as_ref().map(|t| t.duration_ms).unwrap_or(0)
    }

    pub fn toggle_play(&mut self, cx: &mut Context<Self>) {
        if self.track.is_none() {
            self.open_dialog(cx);
            return;
        }
        let Some(audio) = self.audio.as_mut() else { return };
        if self.playing {
            audio.pause();
            self.playing = false;
        } else {
            audio.play();
            self.playing = true;
        }
        cx.notify();
    }

    fn previous(&mut self, cx: &mut Context<Self>) {
        // One file, no queue: back only ever restarts, and only past ten seconds.
        if self.position_ms > BACK_RESTARTS_AFTER_MS {
            self.seek_to_fraction(0.0);
            cx.notify();
        }
    }

    fn seek_to_fraction(&mut self, fraction: f32) {
        let duration = self.duration_ms();
        if let Some(audio) = self.audio.as_mut() {
            audio.seek(Duration::from_millis((fraction * duration as f32) as u64));
            if self.playing {
                audio.play();
            }
        }
    }

    /// `PlayerScrub.shown`.
    fn shown(&self) -> f32 {
        let duration = self.duration_ms();
        if self.scrubbing {
            self.scrub_value
        } else if let Some((pending, _)) = self.pending_seek {
            pending
        } else if duration > 0 {
            (self.position_ms as f32 / duration as f32).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    // ---- Pointer --------------------------------------------------------------

    fn target_at(&self, p: (f32, f32)) -> Option<Target> {
        let l = &self.layout;
        // ThinSlider's touch box spans the bar's full width.
        let seek_box = Rect::new(l.seek.x, l.seek.y, l.seek.w, l.seek.h);
        let volume_box = l.volume;
        if seek_box.contains(p) {
            Some(Target::Seek)
        } else if volume_box.contains(p) {
            Some(Target::Volume)
        } else if l.play.contains(p) {
            Some(Target::PlayPause)
        } else if l.previous.contains(p) {
            Some(Target::Previous)
        } else if l.next.contains(p) {
            Some(Target::Next)
        } else if self.more_glyph().contains(p) {
            Some(Target::More)
        } else if l.sleeve.contains(p) {
            Some(Target::Sleeve)
        } else {
            None
        }
    }

    fn more_glyph(&self) -> Rect {
        let c = self.layout.credits;
        Rect::new(c.x + c.w - 34.0, c.y + (c.h - 34.0) / 2.0, 34.0, 34.0)
    }

    fn mouse_down(&mut self, p: (f32, f32), cx: &mut Context<Self>) {
        let target = self.target_at(p);
        self.pressed = target;
        match target {
            Some(Target::Seek) => {
                self.scrubbing = true;
                self.scrub_value = ((p.0 - self.layout.seek.x) / self.layout.seek.w).clamp(0.0, 1.0);
            }
            Some(Target::Volume) => {
                self.volume_dragging = true;
                self.set_volume(((p.0 - self.layout.volume.x) / self.layout.volume.w).clamp(0.0, 1.0));
            }
            _ => {}
        }
        cx.notify();
    }

    fn mouse_move(&mut self, p: (f32, f32), cx: &mut Context<Self>) {
        if self.scrubbing {
            self.scrub_value = ((p.0 - self.layout.seek.x) / self.layout.seek.w).clamp(0.0, 1.0);
            cx.notify();
        } else if self.volume_dragging {
            self.set_volume(((p.0 - self.layout.volume.x) / self.layout.volume.w).clamp(0.0, 1.0));
            cx.notify();
        }
    }

    fn mouse_up(&mut self, p: (f32, f32), cx: &mut Context<Self>) {
        let pressed = self.pressed.take();
        if self.scrubbing {
            // PlayerScrub.release: hold the drop point until the player agrees.
            self.scrubbing = false;
            if self.track.is_some() {
                self.pending_seek = Some((self.scrub_value, Instant::now()));
                self.seek_to_fraction(self.scrub_value);
            }
        } else if self.volume_dragging {
            self.volume_dragging = false;
        } else if pressed.is_some() && pressed == self.target_at(p) {
            match pressed {
                Some(Target::PlayPause) => self.toggle_play(cx),
                Some(Target::Previous) => self.previous(cx),
                Some(Target::Sleeve) | Some(Target::More) => self.open_dialog(cx),
                _ => {}
            }
        }
        cx.notify();
    }

    fn set_volume(&mut self, level: f32) {
        self.volume = level;
        if let Some(audio) = self.audio.as_mut() {
            audio.set_volume(level);
        }
    }

    // ---- Frame ------------------------------------------------------------------

    /// Brings the playhead and every animation to now; true while anything is
    /// still moving and the next frame is wanted.
    fn tick(&mut self) -> bool {
        if let Some(audio) = self.audio.as_ref() {
            if self.track.is_some() {
                self.position_ms = audio.position().as_millis() as u64;
                if self.playing && audio.finished() {
                    self.playing = false;
                    self.position_ms = self.duration_ms();
                }
            }
        }
        if let Some((target, since)) = self.pending_seek {
            let target_ms = (target * self.duration_ms() as f32) as i64;
            if (self.position_ms as i64 - target_ms).unsigned_abs() < SEEK_SETTLE_TOLERANCE_MS
                || since.elapsed() >= Duration::from_millis(SEEK_SETTLE_TIMEOUT_MS)
            {
                self.pending_seek = None;
            }
        }

        // The artwork's three poses: paused wins outright over a scrub.
        let art_target = if !self.playing {
            ARTWORK_PAUSE_SHRINK_SCALE
        } else if self.scrubbing {
            ARTWORK_DRAG_SHRINK_SCALE
        } else {
            ARTWORK_EXPANDED_SCALE
        };
        self.art_scale.animate_to(art_target, ARTWORK_SCALE_DURATION_MS, ARTWORK_SCALE_EASING);
        self.seek_height.set_target(if self.scrubbing { 12.0 } else { 7.0 });
        self.volume_height.set_target(if self.volume_dragging { 10.0 } else { 6.0 });
        let previous_enabled = self.position_ms > BACK_RESTARTS_AFTER_MS;
        self.previous_alpha.set_target(if previous_enabled { 1.0 } else { 0.3 });
        // A single file has nothing after it.
        self.next_alpha.set_target(0.3);
        self.seek_height.step();
        self.volume_height.step();
        self.previous_alpha.step();
        self.next_alpha.step();
        if !self.credits_fade.running() {
            self.old_credits = None;
        }

        self.playing
            || self.pending_seek.is_some()
            || self.art_scale.running()
            || self.blur_alpha.running()
            || self.lyric_alpha.running()
            || self.credits_fade.running()
            || self.seek_height.running()
            || self.volume_height.running()
            || self.previous_alpha.running()
            || self.next_alpha.running()
    }
}

/// Where a marquee line has crawled to, `elapsed` after it started.
fn marquee_offset(text_w: f32, box_w: f32, elapsed: Duration, delay_ms: u64) -> (f32, bool) {
    if text_w <= box_w {
        return (0.0, false);
    }
    let travel = text_w + MARQUEE_GAP;
    let scroll_ms = ((travel / MARQUEE_DP_PER_SEC) * 1000.0).round().max(400.0) as u64;
    let ms = elapsed.as_millis() as u64;
    if ms < delay_ms {
        return (0.0, true);
    }
    let cycle = (ms - delay_ms) % (scroll_ms + MARQUEE_REST_MS);
    if cycle < scroll_ms {
        (-travel * cycle as f32 / scroll_ms as f32, true)
    } else {
        (0.0, true)
    }
}

fn format_time(ms: i64) -> String {
    if ms <= 0 {
        return "0:00".into();
    }
    let seconds = ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Everything one frame draws, measured in `render` where the window's text
/// system is at hand, then handed to the canvas.
struct Frame {
    layout: Layout,
    sleeve_image: Option<Arc<RenderImage>>,
    art_scale: f32,
    title: Vec<(Text, f32, f32)>,
    artist: Vec<(Text, f32, f32)>,
    caption: Option<Text>,
    lyric_strip: Option<(Text, f32)>,
    elapsed: Text,
    remaining: Text,
    quality: Option<(Text, bool, f32)>,
    output: Text,
    shown: f32,
    seek_height: f32,
    volume: f32,
    volume_height: f32,
    playing: bool,
    previous_alpha: f32,
    next_alpha: f32,
}

impl Render for PlayerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        self.layout = Layout::new(f32::from(viewport.width), f32::from(viewport.height));
        let animating = self.tick();
        let l = self.layout;

        // Credits: title titleLarge, artist titleMedium W500 at 55%, each a
        // marquee when it runs past the room the like and menu glyphs leave.
        let credits_w = (l.credits.w - 10.0 - 34.0).max(0.0);
        let fade = self.credits_fade.value();
        let mut marquee_running = false;
        let mut credit_lines = |credits: &Credits, alpha: f32| {
            let title = text::shape(window, credits.title.clone(), text::TITLE_LARGE);
            let artist = text::shape(window, credits.artist.clone(), text::TITLE_MEDIUM.weight(500.0));
            let elapsed = credits.since.elapsed();
            let (title_x, title_moves) = marquee_offset(title.width(), credits_w, elapsed, 0);
            let delay = if title_moves { MARQUEE_ARTIST_STAGGER_MS } else { 0 };
            let (artist_x, artist_moves) = marquee_offset(artist.width(), credits_w, elapsed, delay);
            marquee_running |= title_moves || artist_moves;
            ((title, title_x, alpha), (artist, artist_x, alpha))
        };
        let mut title = Vec::new();
        let mut artist = Vec::new();
        if let Some(old) = &self.old_credits {
            let (t, a) = credit_lines(old, 1.0 - fade);
            title.push(t);
            artist.push(a);
        }
        let (t, a) = credit_lines(&self.credits, if self.old_credits.is_some() { fade } else { 1.0 });
        title.push(t);
        artist.push(a);

        let caption = self.track.as_ref().map(|track| {
            let source = track.album.clone().unwrap_or_else(|| "Queue".into());
            text::shape_ellipsized(window, &format!("Playing from {source}"), text::LABEL_SMALL, l.caption.w)
        });
        let lyric_alpha = self.lyric_alpha.value();
        let lyric_strip = (self.track.is_some() && lyric_alpha > 0.0).then(|| {
            (
                text::shape_ellipsized(window, "Lyrics not available", text::TITLE_MEDIUM, l.lyric_strip.w),
                lyric_alpha,
            )
        });

        let shown = self.shown();
        let duration = self.duration_ms() as i64;
        let at = (shown * duration as f32) as i64;
        let elapsed = text::shape(window, format_time(at), text::LABEL_MEDIUM);
        let remaining = text::shape(window, format!("-{}", format_time(duration - at)), text::LABEL_MEDIUM);

        // The badge between the timestamps.
        let quality_style = text::LABEL_MEDIUM.size(13.0);
        let quality = self.track.as_ref().and_then(|track| match track.quality {
            Quality::Lossless { hi_res } => Some((
                text::shape(window, if hi_res { "Hi-Res Lossless" } else { "Lossless" }, quality_style),
                true,
                self.shimmer_since.elapsed().as_secs_f32(),
            )),
            Quality::High => Some((text::shape(window, "High quality", quality_style), false, 0.0)),
            Quality::None => None,
        });
        let shimmering = quality.as_ref().is_some_and(|q| q.1);

        let output = text::shape_ellipsized(
            window,
            "System default",
            text::LABEL_SMALL.size(11.0),
            l.output_caption.w,
        );

        if animating || marquee_running || shimmering {
            window.request_animation_frame();
        }

        let frame = Frame {
            layout: l,
            sleeve_image: self.artwork.as_ref().map(|a| a.sleeve.clone()),
            art_scale: self.art_scale.value(),
            title,
            artist,
            caption,
            lyric_strip,
            elapsed,
            remaining,
            quality,
            output,
            shown,
            seek_height: self.seek_height.step(),
            volume: self.volume,
            volume_height: self.volume_height.step(),
            playing: self.playing,
            previous_alpha: self.previous_alpha.step(),
            next_alpha: self.next_alpha.step(),
        };

        let entity = cx.entity().downgrade();
        let blur = self.artwork.as_ref().map(|a| a.blur.clone());
        let blur_alpha = self.blur_alpha.value();

        div()
            .id("player")
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                let keystroke = &event.keystroke;
                if keystroke.key == "space" && !keystroke.modifiers.modified() {
                    this.toggle_play(cx);
                } else if keystroke.key == "o" && (keystroke.modifiers.control || keystroke.modifiers.platform) {
                    this.open_dialog(cx);
                }
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _window, cx| {
                if let Some(path) = paths.paths().first() {
                    this.load(path.clone(), cx);
                }
            }))
            // FullArtworkBlurBackdrop: the fallback ground, the cover blurred
            // across the whole window, and the legibility scrim over both.
            .child(
                canvas(|_, _, _| {}, |bounds, _, window, _| {
                    window.paint_quad(fill(bounds, gpui::rgb(FALLBACK_BACKDROP)));
                })
                .absolute()
                .size_full(),
            )
            .children(blur.map(|image| {
                div().absolute().size_full().opacity(blur_alpha).child(
                    canvas(|_, _, _| {}, move |bounds, _, window, _| {
                        let image_bounds = cover(bounds, &image);
                        let _ = window.paint_image(bounds, image_bounds, Corners::default(), image, 0, false);
                    })
                    .size_full(),
                )
            }))
            .child(
                canvas(|_, _, _| {}, move |_, _, window, cx| paint_player(&frame, entity, window, cx))
                    .absolute()
                    .size_full(),
            )
            // The handle strip moves the window: there is no title bar.
            .child(
                div()
                    .id("drag-strip")
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(px(LANDSCAPE_HANDLE_STRIP))
                    .window_control_area(WindowControlArea::Drag),
            )
    }
}

/// `ContentScale.Crop`: [image] scaled to cover [bounds], centred.
fn cover(bounds: Bounds<Pixels>, image: &RenderImage) -> Bounds<Pixels> {
    let image_size = image.size(0);
    let (iw, ih) = (i32::from(image_size.width) as f32, i32::from(image_size.height) as f32);
    let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    if iw <= 0.0 || ih <= 0.0 {
        return bounds;
    }
    let scale = (bw / iw).max(bh / ih);
    let (w, h) = (iw * scale, ih * scale);
    Bounds::new(
        point(bounds.origin.x + px((bw - w) / 2.0), bounds.origin.y + px((bh - h) / 2.0)),
        size(px(w), px(h)),
    )
}

fn paint_icon(window: &mut Window, cx: &App, icon: Icon, rect: Rect, color: Hsla) {
    let _ = window.paint_svg(rect.bounds(), icon.0.into(), Some(icon.1), TransformationMatrix::unit(), color, cx);
}

fn paint_player(frame: &Frame, entity: WeakEntity<PlayerView>, window: &mut Window, cx: &mut App) {
    let l = &frame.layout;

    // The scrim: black at 34%, 48% by 55% of the way down, 64% at the foot.
    let split = l.window.h * 0.55;
    window.paint_quad(fill(
        Rect::new(0.0, 0.0, l.window.w, split).bounds(),
        linear_gradient(180.0, linear_color_stop(black(0.34), 0.0), linear_color_stop(black(0.48), 1.0)),
    ));
    window.paint_quad(fill(
        Rect::new(0.0, split, l.window.w, l.window.h - split).bounds(),
        linear_gradient(180.0, linear_color_stop(black(0.48), 0.0), linear_color_stop(black(0.64), 1.0)),
    ));

    paint_sleeve(frame, window, cx);
    paint_actions(frame, window, cx);
    paint_main_pane(frame, window, cx);

    // The sheet's handle, at the top centre.
    let handle = l.handle.bounds();
    let radius = Corners::all(px(2.5));
    window.paint_drop_shadows(
        handle,
        radius,
        &[BoxShadow {
            color: black(0.24),
            offset: point(px(0.0), px(1.0)),
            blur_radius: px(2.0),
            spread_radius: px(0.0),
            inset: false,
        }],
    );
    window.paint_quad(fill(handle, white(0.70)).corner_radii(radius));

    // Pointer: presses, drags and releases, wherever the pointer goes once
    // a drag has begun.
    let down = entity.clone();
    window.on_mouse_event(move |event: &MouseDownEvent, phase, _window, cx| {
        if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
            let p = (f32::from(event.position.x), f32::from(event.position.y));
            let _ = down.update(cx, |this, cx| this.mouse_down(p, cx));
        }
    });
    let moved = entity.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
        if phase == DispatchPhase::Bubble {
            let p = (f32::from(event.position.x), f32::from(event.position.y));
            let _ = moved.update(cx, |this, cx| this.mouse_move(p, cx));
        }
    });
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
        if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
            let p = (f32::from(event.position.x), f32::from(event.position.y));
            let _ = entity.update(cx, |this, cx| this.mouse_up(p, cx));
        }
    });
}

/// `LandscapeArtwork`: the 10dp-cornered sleeve with its 14dp shadow — cast
/// only once there is a picture — scaled about its centre by the play/pause
/// pose.
fn paint_sleeve(frame: &Frame, window: &mut Window, cx: &App) {
    let rect = frame.layout.sleeve.scaled(frame.art_scale);
    let bounds = rect.bounds();
    let radius = Corners::all(px(LANDSCAPE_ART_CORNER * frame.art_scale));
    if let Some(image) = &frame.sleeve_image {
        // Compose's 14dp elevation, as an ambient and a key shadow.
        let s = frame.art_scale;
        window.paint_drop_shadows(
            bounds,
            radius,
            &[
                BoxShadow {
                    color: black(0.10),
                    offset: point(px(0.0), px(2.0 * s)),
                    blur_radius: px(14.0 * s),
                    spread_radius: px(0.0),
                    inset: false,
                },
                BoxShadow {
                    color: black(0.26),
                    offset: point(px(0.0), px(9.0 * s)),
                    blur_radius: px(26.0 * s),
                    spread_radius: px(0.0),
                    inset: false,
                },
            ],
        );
        window.paint_quad(fill(bounds, black(0.18)).corner_radii(radius));
        let image_bounds = cover(bounds, image);
        let _ = window.paint_image(bounds, image_bounds, radius, image.clone(), 0, false);
    } else {
        window.paint_quad(fill(bounds, black(0.18)).corner_radii(radius));
        paint_icon(window, cx, ICON_MUSIC_NOTE, rect.centered_square(rect.w * 0.36), white(0.35));
    }
}

/// `PlayerActionRow` and `OutputCaption`, under the sleeve.
fn paint_actions(frame: &Frame, window: &mut Window, cx: &App) {
    let l = &frame.layout;
    paint_icon(window, cx, ICON_LYRICS, l.lyrics_glyph.centered_square(26.0), white(0.75));
    paint_icon(window, cx, ICON_QUEUE, l.queue_glyph.centered_square(26.0), white(0.75));

    // The output/party capsule: one rounded slab, a hairline between halves.
    let pill = l.pill;
    window.paint_quad(fill(pill.bounds(), white(0.12)).corner_radii(px(pill.h / 2.0)));
    let output = Rect::new(pill.x, pill.y, PILL_SEGMENT_WIDTH, pill.h);
    let party = Rect::new(pill.x + PILL_SEGMENT_WIDTH + 1.0, pill.y, PILL_SEGMENT_WIDTH, pill.h);
    window.paint_quad(fill(Rect::new(pill.x + PILL_SEGMENT_WIDTH, pill.y, 1.0, pill.h).bounds(), white(0.20)));
    paint_icon(window, cx, ICON_HEADPHONES, output.centered_square(PILL_HEADPHONES_SIZE), white(0.75));
    paint_icon(window, cx, ICON_PERSON, party.centered_square(PILL_PARTY_SIZE), white(0.75));

    let caption = l.output_caption;
    let x = caption.x + (caption.w - frame.output.width()) / 2.0;
    frame.output.paint(window, point(px(x), px(caption.y)), white(0.55));
}

/// Paints [text] with BitChord's caption shadow (black 55%, 1px down, 4px blur)
/// under it, the blur approximated by a few offset copies.
fn paint_shadowed(window: &mut Window, text: &Text, x: f32, y: f32, color: Hsla) {
    for (dx, dy, a) in [(0.0, 1.0, 0.22), (-1.0, 1.5, 0.08), (1.0, 1.5, 0.08), (0.0, 2.5, 0.08), (0.0, 0.0, 0.06)] {
        text.paint(window, point(px(x + dx), px(y + dy)), black(a));
    }
    text.paint(window, point(px(x), px(y)), color);
}

fn paint_main_pane(frame: &Frame, window: &mut Window, cx: &App) {
    let l = &frame.layout;

    // "Playing from …", inside its 2dp of padding.
    if let Some(caption) = &frame.caption {
        paint_shadowed(window, caption, l.caption.x, l.caption.y + 2.0, white(1.0));
    }

    // Credits, clipped to the text column and crawling when too long.
    let credits = l.credits;
    let text_w = (credits.w - 44.0).max(0.0);
    let title_y = credits.y;
    let artist_y = credits.y + text::TITLE_LARGE.line_height + 2.0;
    for (lines, y, color) in [(&frame.title, title_y, 1.0), (&frame.artist, artist_y, 0.55)] {
        for (line, offset, alpha) in lines {
            let clip = Rect::new(credits.x, y, text_w.min(line.width()), line.line_height());
            window.with_content_mask(Some(ContentMask { bounds: clip.bounds() }), |window| {
                line.paint(window, point(px(credits.x + offset), px(y)), white(color * alpha));
                if *offset != 0.0 || line.width() > text_w {
                    let second = credits.x + offset + line.width() + MARQUEE_GAP;
                    line.paint(window, point(px(second), px(y)), white(color * alpha));
                }
            });
        }
    }
    // CircleGlyph: the three-dot menu on its 18% disc.
    let more = Rect::new(credits.x + credits.w - 34.0, credits.y + (credits.h - 34.0) / 2.0, 34.0, 34.0);
    window.paint_quad(fill(more.bounds(), white(0.18)).corner_radii(px(17.0)));
    paint_icon(window, cx, ICON_MORE, more.centered_square(19.0), white(1.0));

    // CurrentLyricStrip, 6dp down into the slider's dead space.
    if let Some((line, alpha)) = &frame.lyric_strip {
        line.paint(window, point(px(l.lyric_strip.x), px(l.lyric_strip.y + 6.0 + 4.0)), white(*alpha));
    }

    // ThinSlider: a capsule, no thumb, thicker while held.
    paint_thin_slider(window, l.seek, frame.seek_height, frame.shown);
    let times = l.times;
    frame.elapsed.paint(window, point(px(times.x), px(times.y)), white(0.55));
    frame
        .remaining
        .paint(window, point(px(times.x + times.w - frame.remaining.width()), px(times.y)), white(0.55));
    if let Some((label, animated, t)) = &frame.quality {
        // LosslessLabel: a 13dp headphones glyph, 4dp, the word.
        let row_w = 13.0 + 4.0 + label.width();
        let x = times.x + (times.w - row_w) / 2.0;
        let icon_tint = if *animated { 0.7 } else { 0.45 };
        let icon = Rect::new(x, times.y + (times.h - 13.0) / 2.0, 13.0, 13.0);
        paint_icon(window, cx, ICON_HEADPHONES, icon, white(icon_tint));
        let text_x = x + 17.0;
        let text_y = times.y + (times.h - label.line_height()) / 2.0;
        if *animated {
            // ShimmerText: a band 60% of the word's width sweeping across it
            // every three seconds, white at its centre over 55%.
            let width = label.width();
            let band = width * 0.6;
            let progress = (t % 3.0) / 3.0;
            let centre = -band + progress * (width + 2.0 * band);
            label.paint_with(window, point(px(text_x), px(text_y)), |gx| {
                let d = ((gx - centre) / band).abs().min(1.0);
                white(1.0 - 0.45 * d)
            });
        } else {
            label.paint(window, point(px(text_x), px(text_y)), white(0.45));
        }
    }

    // TransportRow.
    let skip = l.skip_icon;
    paint_icon(window, cx, ICON_PREVIOUS, l.previous.centered_square(skip), white(frame.previous_alpha));
    let play_icon = if frame.playing { ICON_PAUSE } else { ICON_PLAY };
    paint_icon(window, cx, play_icon, l.play.centered_square(l.play_icon), white(1.0));
    paint_icon(window, cx, ICON_NEXT, l.next.centered_square(skip), white(frame.next_alpha));

    // VolumeRow.
    paint_icon(window, cx, ICON_VOLUME_DOWN, l.volume_down, white(0.5));
    paint_icon(window, cx, ICON_VOLUME_UP, l.volume_up, white(0.5));
    paint_thin_slider(window, l.volume, frame.volume_height, frame.volume);
}

fn paint_thin_slider(window: &mut Window, area: Rect, height: f32, value: f32) {
    let bar = Rect::new(area.x, area.y + (area.h - height) / 2.0, area.w, height);
    let radius = px(height / 2.0);
    window.paint_quad(fill(bar.bounds(), white(0.26)).corner_radii(radius));
    let filled = area.w * value.clamp(0.0, 1.0);
    if filled > 0.0 {
        let w = filled.max(height).min(area.w);
        window.paint_quad(fill(Rect::new(bar.x, bar.y, w, height).bounds(), white(0.92)).corner_radii(radius));
    }
}

