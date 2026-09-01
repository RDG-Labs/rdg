use crate::prelude::*;
use gpui::{App, Hsla, WindowBackgroundAppearance};

/// The opacity used for application surfaces when native window transparency is enabled.
pub const TRANSLUCENT_BACKGROUND_OPACITY: f32 = 0.5;

/// Applies the standard application-surface opacity for translucent windows.
pub fn window_background_color(color: Hsla, cx: &App) -> Hsla {
    if theme_is_transparent(cx) {
        color.opacity(TRANSLUCENT_BACKGROUND_OPACITY)
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
