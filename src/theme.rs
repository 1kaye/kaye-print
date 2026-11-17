use gpui_kit::{
    App,
    component::{Theme, ThemeMode, ThemeRegistry},
};

pub const STUDIO_THEMES_JSON: &str = r##"{
  "name": "KayePrint Studio",
  "themes": [
    {
      "name": "KayePrint Light",
      "mode": "light",
      "font.family": "Adwaita Sans",
      "font.size": 13.0,
      "mono_font.family": "Adwaita Mono",
      "mono_font.size": 12.0,
      "radius": 6,
      "radius.lg": 8,
      "colors": {
        "background": "#FAF7F2",
        "foreground": "#292524",
        "border": "#E7E2DB",
        "input.border": "#D7D0C5",
        "muted.background": "#F2ECE4",
        "muted.foreground": "#78716C",
        "primary.background": "#C86A4B",
        "primary.hover.background": "#D77B5C",
        "primary.active.background": "#B5593C",
        "primary.foreground": "#FFFFFF",
        "secondary.background": "#F0EAE1",
        "secondary.hover.background": "#E6DFD4",
        "secondary.active.background": "#DCD4C7",
        "secondary.foreground": "#292524",
        "accent.background": "#FBEFEA",
        "accent.foreground": "#C86A4B",
        "sidebar.background": "#F4EFEA",
        "sidebar.foreground": "#292524",
        "sidebar.border": "#E7E2DB",
        "sidebar.accent.background": "#FBEFEA",
        "sidebar.accent.foreground": "#C86A4B",
        "sidebar.primary.background": "#C86A4B",
        "sidebar.primary.foreground": "#FFFFFF",
        "group_box.background": "#FFFFFF",
        "group_box.foreground": "#292524",
        "popover.background": "#FFFFFF",
        "popover.foreground": "#292524",
        "title_bar.background": "#F4EFEA",
        "title_bar.border": "#E7E2DB",
        "status_bar.background": "#F4EFEA",
        "status_bar.border": "#E7E2DB",
        "list.background": "#FFFFFF",
        "list.hover.background": "#F6F1EA",
        "list.active.background": "#C86A4B1F",
        "list.active.border": "#C86A4B",
        "slider.background": "#C86A4B",
        "slider.thumb.background": "#FFFFFF",
        "switch.background": "#D7D0C5",
        "switch.thumb.background": "#FFFFFF",
        "ring": "#D77B5C",
        "selection.background": "#C86A4B",
        "danger.background": "#DC2626",
        "danger.foreground": "#FFFFFF",
        "warning.background": "#D97706",
        "warning.foreground": "#FFFFFF",
        "success.background": "#059669",
        "success.foreground": "#FFFFFF",
        "info.background": "#D97757",
        "info.foreground": "#FFFFFF"
      }
    },
    {
      "name": "KayePrint Dark",
      "mode": "dark",
      "font.family": "Adwaita Sans",
      "font.size": 13.0,
      "mono_font.family": "Adwaita Mono",
      "mono_font.size": 12.0,
      "radius": 6,
      "radius.lg": 8,
      "colors": {
        "background": "#161412",
        "foreground": "#F4EFEA",
        "border": "#2E2925",
        "input.border": "#3F3934",
        "muted.background": "#221E1A",
        "muted.foreground": "#9C9388",
        "primary.background": "#D97B5E",
        "primary.hover.background": "#E48D72",
        "primary.active.background": "#C66B4E",
        "primary.foreground": "#FFFFFF",
        "secondary.background": "#24201C",
        "secondary.hover.background": "#322D27",
        "secondary.active.background": "#1B1815",
        "secondary.foreground": "#F4EFEA",
        "accent.background": "#2D2520",
        "accent.foreground": "#F4AB92",
        "sidebar.background": "#1B1815",
        "sidebar.foreground": "#F4EFEA",
        "sidebar.border": "#2E2925",
        "sidebar.accent.background": "#2D2520",
        "sidebar.accent.foreground": "#F4AB92",
        "sidebar.primary.background": "#D97B5E",
        "sidebar.primary.foreground": "#FFFFFF",
        "group_box.background": "#1F1B18",
        "group_box.foreground": "#F4EFEA",
        "popover.background": "#1F1B18",
        "popover.foreground": "#F4EFEA",
        "title_bar.background": "#1B1815",
        "title_bar.border": "#2E2925",
        "status_bar.background": "#1B1815",
        "status_bar.border": "#2E2925",
        "list.background": "#1B1815",
        "list.hover.background": "#24201C",
        "list.active.background": "#D97B5E33",
        "list.active.border": "#E48D72",
        "slider.background": "#D97B5E",
        "slider.thumb.background": "#FFFFFF",
        "switch.background": "#3F3934",
        "switch.thumb.background": "#F4EFEA",
        "ring": "#E48D72",
        "selection.background": "#D97B5E",
        "danger.background": "#EF4444",
        "danger.foreground": "#FFFFFF",
        "warning.background": "#F59E0B",
        "warning.foreground": "#111827",
        "success.background": "#10B981",
        "success.foreground": "#FFFFFF",
        "info.background": "#F4AB92",
        "info.foreground": "#111827"
      }
    }
  ]
}"##;

/// Initializes the KayePrint theme palette into ThemeRegistry and sets Dark as the default mode.
pub fn init_theme(cx: &mut App) {
    ThemeRegistry::global_mut(cx)
        .load_themes_from_str(STUDIO_THEMES_JSON)
        .expect("Failed to load KayePrint themes");

    let light_theme = ThemeRegistry::global(cx)
        .themes()
        .get("KayePrint Light")
        .cloned();
    let dark_theme = ThemeRegistry::global(cx)
        .themes()
        .get("KayePrint Dark")
        .cloned();

    Theme::update(cx, |theme| {
        if let Some(light) = light_theme {
            theme.light_theme = light;
        }
        if let Some(dark) = dark_theme {
            theme.dark_theme = dark;
        }
    });

    Theme::change(ThemeMode::Dark, None, cx);

    // Customize MotionTokens so slide transitions look clear, proportional, and smooth
    Theme::update(cx, |theme| {
        theme.motion.duration_normal = std::time::Duration::from_millis(240);
        theme.motion.distance_short = gpui_kit::rems(1.75);
    });
}
