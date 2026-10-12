# BitChord player on gpui-fast

BitChord's now-playing page, rebuilt in Rust on [gpui-fast](https://github.com/longbridge/gpui-fast).
It does one thing: open a local audio file and play it, on the page
[BitChord](https://github.com/kushagrasinghx/BitChord)'s desktop app shows for the playing song.

Windows is the target.

## Run

Needs Rust (the MSVC toolchain on Windows).

```sh
cargo run --release
cargo run --release -- "C:\Music\song.flac"   # start with a file
```

Click the artwork (or the ⋯ button, or press Ctrl+O) to pick a file, or drop one on the window.
Space plays and pauses. Drag along the top edge to move the window.

Supported formats are what rodio's symphonia decoders read: MP3, FLAC, WAV, AIFF, Ogg Vorbis and AAC/M4A.

## What is copied from BitChord

The page is BitChord's landscape player (`LandscapePlayerLayout`, which its desktop window uses),
with every size, colour, alpha and timing taken from its source. `src/player.rs` names the
BitChord constant behind each number.

- **Layout**: the two 1100dp-capped columns, 30dp gutters, the square sleeve, the lyrics /
  output / queue row with the output caption, and the right column's caption, credits,
  lyric strip, scrubber, transport and volume row, centred.
- **Blur**: the full-cover backdrop exactly as BitChord computes it. The cover is shrunk to
  128px, box-blurred three times on the CPU at a twelfth of its short side, stretched across
  the window with bilinear filtering, and darkened by the 34% → 48% → 64% scrim.
- **Animation**: the sleeve's 1.0 / 0.94 / 0.88 play, scrub and pause poses (500ms, cubic
  0.215, 0.61, 0.355, 1), the scrubber's bouncy thickening spring, the faded skip buttons, the
  300ms credits crossfade, the title and artist marquee (26dp/s, 48dp gap, 5s rest, 3s artist
  stagger), the 320ms backdrop fade-in, "Lyrics not available" held 5s then faded, and the
  "Lossless" shimmer.
- **Icons**: BitChord's play, pause and skip vectors (skip squashed to 85% height), its queue,
  lyrics and music-note icons, and the Material Rounded icons it uses, converted to SVG.
- **Type**: SF Pro Display from BitChord's desktop resources, at the desktop app's Material 3
  sizes, weights and letter spacing.
- **Quality badge**: BitChord's rules. FLAC, WAV, AIFF and ALAC are "Lossless", "Hi-Res
  Lossless" past 16-bit or 48kHz, and lossy files at 256kbps or more are "High quality".

The lyrics, output, party and queue buttons are drawn but do nothing, since those features are
out of scope. Previous restarts the song once it is past 10 seconds, as in BitChord; next stays
faded because there is no queue.

## Approximations

- gpui has no text shadows, so the "Playing from" caption's shadow is a few offset copies of the text.
- The sleeve's 14dp Compose elevation shadow is approximated with two box shadows.
- The shimmer colours each glyph by its position rather than each pixel.

## Licence

GPL-3.0, as BitChord is. The SF Pro fonts are Apple's and are subject to Apple's font licence.
