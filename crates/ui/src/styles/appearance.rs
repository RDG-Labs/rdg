use crate::prelude::*;
use gpui::{App, Hsla, WindowBackgroundAppearance};

/// The opacity used for application surfaces when native window transparency is enabled.
pub const TRANSLUCENT_BACKGROUND_OPACITY: f32 = 0.5;

/// The opacity used for translucent chrome surfaces. Content remains opaque.
pub const GLASS_SURFACE_OPACITY: f32 = 0.72;
/// The opacity used for focused or elevated glass surfaces.
pub const GLASS_ELEVATED_OPACITY: f32 = 0.84;
/// The opacity used for glass surface borders.
pub const GLASS_BORDER_OPACITY: f32 = 0.68;

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
    if theme_is_transparent(cx) {
        color.opacity(GLASS_SURFACE_OPACITY)
    } else {
        color
    }
}

/// Applies a stronger glass material to focused or elevated chrome.
pub fn glass_elevated_color(color: Hsla, cx: &App) -> Hsla {
    if theme_is_transparent(cx) {
        color.opacity(GLASS_ELEVATED_OPACITY)
    } else {
        color
    }
}

/// Applies the glass material to a chrome border or highlight.
pub fn glass_border_color(color: Hsla, cx: &App) -> Hsla {
    if theme_is_transparent(cx) {
        color.opacity(GLASS_BORDER_OPACITY)
    } else {
        color
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
    matches!(
        window_appearance(cx),
        WindowBackgroundAppearance::Transparent | WindowBackgroundAppearance::Blurred
    )
}
