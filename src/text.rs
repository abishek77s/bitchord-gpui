//! BitChord's type: SF Pro Display at the Material 3 sizes the desktop app
//! overrides, drawn glyph by glyph so letter spacing and Compose's line-box
//! placement come out as they do there.

use gpui::{
    Font, FontFeatures, FontStyle, FontWeight, Hsla, Pixels, Point, ShapedLine, SharedString,
    TextRun, Window, point, px,
};

pub const FAMILY: &str = "SF Pro Display";

#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub size: f32,
    pub weight: f32,
    pub line_height: f32,
    pub letter_spacing: f32,
}

impl Style {
    pub const fn weight(self, weight: f32) -> Self {
        Self { weight, ..self }
    }
    pub const fn size(self, size: f32) -> Self {
        Self { size, ..self }
    }
}

// `desktopTypography()`. Line heights and the label letter spacing are
// Material 3's defaults, which the desktop app leaves in place.
pub const TITLE_LARGE: Style = Style { size: 20.0, weight: 700.0, line_height: 28.0, letter_spacing: -0.3 };
pub const TITLE_MEDIUM: Style = Style { size: 16.0, weight: 600.0, line_height: 24.0, letter_spacing: -0.2 };
pub const LABEL_MEDIUM: Style = Style { size: 12.0, weight: 600.0, line_height: 16.0, letter_spacing: 0.5 };
pub const LABEL_SMALL: Style = Style { size: 11.0, weight: 600.0, line_height: 16.0, letter_spacing: 0.5 };

#[derive(Clone)]
pub struct Text {
    line: ShapedLine,
    style: Style,
    /// Character ordinal of every byte offset that starts a character.
    ordinals: Vec<usize>,
    chars: usize,
}

pub fn shape(window: &Window, text: impl Into<SharedString>, style: Style) -> Text {
    let text: SharedString = text.into();
    let run = TextRun {
        len: text.len(),
        font: Font {
            family: FAMILY.into(),
            features: FontFeatures::default(),
            fallbacks: None,
            weight: FontWeight(style.weight),
            style: FontStyle::Normal,
        },
        color: gpui::white(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut ordinals = vec![0; text.len() + 1];
    let mut chars = 0;
    for (ordinal, (offset, ch)) in text.char_indices().enumerate() {
        for slot in &mut ordinals[offset..offset + ch.len_utf8()] {
            *slot = ordinal;
        }
        chars = ordinal + 1;
    }
    ordinals[text.len()] = chars;
    let line = window
        .text_system()
        .shape_line(text, px(style.size), &[run], None);
    Text { line, style, ordinals, chars }
}

/// [text] cut to [max_width] with an ellipsis, as `TextOverflow.Ellipsis`.
pub fn shape_ellipsized(window: &Window, text: &str, style: Style, max_width: f32) -> Text {
    let full = shape(window, text.to_string(), style);
    if full.width() <= max_width {
        return full;
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>().trim_end().to_string() + "…";
        if shape(window, candidate, style).width() <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let cut: String = chars[..lo].iter().collect::<String>().trim_end().to_string() + "…";
    shape(window, cut, style)
}

impl Text {
    pub fn width(&self) -> f32 {
        f32::from(self.line.width) + self.style.letter_spacing * self.chars as f32
    }

    pub fn line_height(&self) -> f32 {
        self.style.line_height
    }

    /// Paints the line in its line box, whose top-left corner is [origin].
    pub fn paint(&self, window: &mut Window, origin: Point<Pixels>, color: Hsla) {
        self.paint_with(window, origin, |_| color);
    }

    /// As [paint], with each glyph's colour asked for by its x within the line —
    /// the shimmer's moving highlight.
    pub fn paint_with(&self, window: &mut Window, origin: Point<Pixels>, color: impl Fn(f32) -> Hsla) {
        let ascent = f32::from(self.line.ascent);
        let descent = f32::from(self.line.descent);
        // Compose's default `LineHeightStyle`: the line box's extra height is
        // shared above and below in proportion to ascent and descent.
        let extra = self.style.line_height - (ascent + descent);
        let top = if ascent + descent > 0.0 { extra * ascent / (ascent + descent) } else { 0.0 };
        let baseline = f32::from(origin.y) + top + ascent;
        let font_size = px(self.style.size);
        for run in self.line.runs.iter() {
            for glyph in &run.glyphs {
                let ordinal = self.ordinals.get(glyph.index).copied().unwrap_or(0);
                let x = f32::from(glyph.position.x) + self.style.letter_spacing * ordinal as f32;
                let at = point(origin.x + px(x), px(baseline) + glyph.position.y);
                if glyph.is_emoji {
                    let _ = window.paint_emoji(at, run.font_id, glyph.id, font_size);
                } else {
                    let _ = window.paint_glyph(at, run.font_id, glyph.id, font_size, color(x));
                }
            }
        }
    }
}
