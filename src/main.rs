// No console window behind the player on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod anim;
mod artwork;
mod audio;
mod player;
mod text;
mod track;

use std::borrow::Cow;

use gpui::{
    App, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowOptions, prelude::*, px, size,
};

use player::PlayerView;

/// BitChord's desktop window: 1220×780 to start, never smaller than 900×600.
const WINDOW_WIDTH: f32 = 1220.0;
const WINDOW_HEIGHT: f32 = 780.0;
const MIN_WINDOW_WIDTH: f32 = 900.0;
const MIN_WINDOW_HEIGHT: f32 = 600.0;

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(include_bytes!("../assets/fonts/sf_pro_display_regular.otf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/sf_pro_display_medium.otf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/sf_pro_display_semibold.otf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/sf_pro_display_bold.otf").as_slice()),
            ])
            .expect("bundled fonts load");

        let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    // BitChord draws its own frame on Windows, and the player
                    // covers all of it.
                    titlebar: Some(TitlebarOptions {
                        title: Some("BitChord".into()),
                        appears_transparent: true,
                        traffic_light_position: None,
                    }),
                    window_background: WindowBackgroundAppearance::Opaque,
                    window_min_size: Some(size(px(MIN_WINDOW_WIDTH), px(MIN_WINDOW_HEIGHT))),
                    app_id: Some("bitchord-gpui".into()),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| PlayerView::new(window, cx)),
            )
            .expect("window opens");

        // A file named on the command line (or dropped on the exe) plays at once.
        if let Some(path) = std::env::args_os().nth(1) {
            let _ = window.update(cx, |player, _window, cx| player.load(path.into(), cx));
        }
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
}
