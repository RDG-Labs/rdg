use gpui::{App, Hsla, WindowBackgroundAppearance};
use settings::Settings;
use theme::{ActiveTheme, Appearance};
use theme_settings::ThemeSettings;

/// The opacity used for application surfaces when native window transparency is enabled.
pub const TRANSLUCENT_BACKGROUND_OPACITY: f32 = 0.5;

const GLASS_LIGHT_SURFACE_OPACITY: f32 = 0.88;
const GLASS_DARK_SURFACE_OPACITY: f32 = 0.72;
const GLASS_LIGHT_ELEVATED_OPACITY: f32 = 0.94;
const GLASS_DARK_ELEVATED_OPACITY: f32 = 0.84;
const GLASS_LIGHT_BORDER_OPACITY: f32 = 0.85;
const GLASS_DARK_BORDER_OPACITY: f32 = 0.68;

/// Applies the standard application-surface opacity for translucent windows.
pub fn window_background_color(color: Hsla, cx: &App) -> Hsla {
    if theme_is_transparent(cx) {
        color.opacity(TRANSLUCENT_BACKGROUND_OPACITY)
    } else {
        color
    }
}

/// Applies the glass material to application chrome without changing content surfaces.
pub fn glass_surface_color(color: Hsla, cx: &App) -> Hsla {
    glass_color(color, cx, GLASS_LIGHT_SURFACE_OPACITY, GLASS_DARK_SURFACE_OPACITY)
}

/// Applies a stronger glass material to focused or elevated chrome.
pub fn glass_elevated_color(color: Hsla, cx: &App) -> Hsla {
    glass_color(
        color,
        cx,
        GLASS_LIGHT_ELEVATED_OPACITY,
        GLASS_DARK_ELEVATED_OPACITY,
    )
}

/// Applies the glass material to a chrome border or highlight.
pub fn glass_border_color(color: Hsla, cx: &App) -> Hsla {
    if !theme_is_transparent(cx) {
        return color;
    }

    let opacity = if cx.theme().appearance() == Appearance::Light {
        GLASS_LIGHT_BORDER_OPACITY
    } else {
        GLASS_DARK_BORDER_OPACITY
    };
    color.opacity(opacity)
}

fn glass_color(color: Hsla, cx: &App, light_opacity: f32, dark_opacity: f32) -> Hsla {
    if !theme_is_transparent(cx) {
        return color;
    }

    let is_light = cx.theme().appearance() == Appearance::Light;
    let tint_opacity = if is_light { 0.04 } else { 0.025 };
    let tinted = color.blend(cx.theme().colors().text.opacity(tint_opacity));
    tinted.opacity(if is_light { light_opacity } else { dark_opacity })
}

/// Returns the effective native window background appearance.
pub fn effective_window_background_appearance(cx: &App) -> WindowBackgroundAppearance {
    if ThemeSettings::get_global(cx).reduce_transparency {
        WindowBackgroundAppearance::Opaque
    } else {
        window_appearance(cx)
    }
}

/// Returns the [WindowBackgroundAppearance].
fn window_appearance(cx: &App) -> WindowBackgroundAppearance {
    cx.theme().styles.window_background_appearance
}

/// Returns if the window and it's surfaces are expected
/// to be transparent.
///
/// Helps determine if you need to take extra steps to prevent
/// transparent backgrounds.
pub fn theme_is_transparent(cx: &App) -> bool {
    if ThemeSettings::get_global(cx).reduce_transparency {
        return false;
    }

    matches!(
        window_appearance(cx),
        WindowBackgroundAppearance::Transparent
            | WindowBackgroundAppearance::Blurred
            | WindowBackgroundAppearance::MicaBackdrop
            | WindowBackgroundAppearance::MicaAltBackdrop
    )
}
