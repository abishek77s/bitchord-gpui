//! The cover, twice: as the sleeve, and as BitChord's full-cover blur.
//!
//! The blur is BitChord's own `rememberFullArtworkBlurImage`, step for step: a
//! 128px copy of the cover, three box-blur passes on the CPU at a radius of a
//! twelfth of its short side, and then nothing but a bilinear stretch across
//! the window. No live blur effect is ever run over the screen.

use std::sync::Arc;

use gpui::RenderImage;
use image::imageops::FilterType;
use image::{Frame, RgbaImage};

/// The player's sleeve resolution in BitChord (`ART_PX`).
const ART_PX: u32 = 1200;
/// `FULL_BLUR_SOURCE_PX`: a screen cannot recover detail discarded here.
const FULL_BLUR_SOURCE_PX: u32 = 128;
/// `FULL_BLUR_PASSES`.
const FULL_BLUR_PASSES: usize = 3;
/// The blurred thumbnail is enlarged this much before upload, bilinearly — the
/// same filter the GPU would use — so the atlas never samples a neighbour at
/// the stretched image's edges.
const BLUR_UPLOAD_SCALE: u32 = 4;

pub struct Artwork {
    pub sleeve: Arc<RenderImage>,
    pub blur: Arc<RenderImage>,
}

pub fn decode(bytes: &[u8]) -> Option<Artwork> {
    let source = image::load_from_memory(bytes).ok()?.to_rgba8();
    if source.width() < 2 || source.height() < 2 {
        return None;
    }

    let sleeve = if source.width().max(source.height()) > ART_PX {
        let (w, h) = fit(source.width(), source.height(), ART_PX);
        image::imageops::resize(&source, w, h, FilterType::CatmullRom)
    } else {
        source.clone()
    };

    let (w, h) = fit(source.width(), source.height(), FULL_BLUR_SOURCE_PX);
    let mut small = image::imageops::resize(&source, w, h, FilterType::Triangle);
    box_blurred(&mut small, FULL_BLUR_PASSES);
    let blur = image::imageops::resize(
        &small,
        w * BLUR_UPLOAD_SCALE,
        h * BLUR_UPLOAD_SCALE,
        FilterType::Triangle,
    );

    Some(Artwork {
        sleeve: Arc::new(render_image(sleeve)),
        blur: Arc::new(render_image(blur)),
    })
}

/// [width]×[height] scaled to fit inside a [max] square, as Coil's `size(max)` does.
fn fit(width: u32, height: u32, max: u32) -> (u32, u32) {
    let scale = max as f32 / width.max(height) as f32;
    (
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    )
}

/// gpui wants BGRA.
fn render_image(mut image: RgbaImage) -> RenderImage {
    for pixel in image.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    RenderImage::new([Frame::new(image)])
}

/// BitChord's `boxBlurred`: [passes] horizontal-then-vertical box passes,
/// edges clamped, alpha forced opaque.
fn box_blurred(image: &mut RgbaImage, passes: usize) {
    let width = image.width() as usize;
    let height = image.height() as usize;
    let radius = (width.min(height) / 12).max(2);
    let mut source: Vec<[i32; 3]> = image
        .pixels()
        .map(|p| [p[0] as i32, p[1] as i32, p[2] as i32])
        .collect();
    let mut target = vec![[0i32; 3]; source.len()];
    for _ in 0..passes {
        box_blur_pass(&source, &mut target, width, height, radius, true);
        box_blur_pass(&target, &mut source, width, height, radius, false);
    }
    for (pixel, rgb) in image.pixels_mut().zip(source) {
        *pixel = image::Rgba([rgb[0] as u8, rgb[1] as u8, rgb[2] as u8, 255]);
    }
}

fn box_blur_pass(
    source: &[[i32; 3]],
    target: &mut [[i32; 3]],
    width: usize,
    height: usize,
    radius: usize,
    horizontal: bool,
) {
    let (major, minor) = if horizontal { (width, height) } else { (height, width) };
    let window = (radius * 2 + 1) as i32;
    let r = radius as isize;
    for fixed in 0..minor {
        let index = |moving: usize| {
            if horizontal {
                fixed * width + moving
            } else {
                moving * width + fixed
            }
        };
        let pixel = |at: isize| source[index(at.clamp(0, major as isize - 1) as usize)];
        let mut sum = [0i32; 3];
        for offset in -r..=r {
            let c = pixel(offset);
            sum[0] += c[0];
            sum[1] += c[1];
            sum[2] += c[2];
        }
        for moving in 0..major {
            target[index(moving)] = [sum[0] / window, sum[1] / window, sum[2] / window];
            let leaving = pixel(moving as isize - r);
            let entering = pixel(moving as isize + r + 1);
            for channel in 0..3 {
                sum[channel] += entering[channel] - leaving[channel];
            }
        }
    }
}
