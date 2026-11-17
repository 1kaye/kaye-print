mod app;
mod canvas;
mod editor;
mod inspector;
mod printer;
mod shared;
mod theme;

rust_i18n::i18n!("locales", fallback = "id");

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

use app::{OpenPrinterSettings, StudioApp};
use gpui_kit::{
    AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions,
    assets::AllAssets,
    component as gpui_component, px, size,
};

fn main() {
    gpui_kit::application().with_assets(AllAssets).run(|cx| {
        rust_i18n::extend!(gpui_component);
        gpui_kit::init(cx);
        gpui_component::set_locale(shared::i18n::DEFAULT_LANGUAGE.locale_code());
        cx.bind_keys([
            gpui_kit::KeyBinding::new("ctrl-p", OpenPrinterSettings, None),
            #[cfg(target_os = "macos")]
            gpui_kit::KeyBinding::new("cmd-p", OpenPrinterSettings, None),
        ]);

        theme::init_theme(cx);

        let bounds = Bounds::centered(None, size(px(1320.0), px(840.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("KayePrint".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| {
                let app = StudioApp::new(cx);
                app.canvas_focus.focus(window, cx);
                app
            })
        })
        .expect("Failed to open KayePrint window");
    });
}
