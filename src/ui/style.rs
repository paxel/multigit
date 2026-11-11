//! App-wide look: roomier buttons than egui's defaults.

use crate::domain::config::Theme;
use egui::{Context, Style, ThemePreference, vec2};

/// Space between a button's text and its border.
pub const BUTTON_PADDING: egui::Vec2 = egui::Vec2::new(10.0, 5.0);

fn adjust(style: &mut Style) {
    style.spacing.button_padding = BUTTON_PADDING;
    style.spacing.interact_size = vec2(style.spacing.interact_size.x, 28.0);
    style.spacing.item_spacing = vec2(8.0, 6.0);
}

/// Apply to both the light and the dark style.
pub fn apply(ctx: &Context) {
    ctx.all_styles_mut(adjust);
}

/// Make `ctx` use the theme from the settings; cheap when it already does.
pub fn apply_theme(ctx: &Context, theme: Theme) {
    let preference = match theme {
        Theme::System => ThemePreference::System,
        Theme::Light => ThemePreference::Light,
        Theme::Dark => ThemePreference::Dark,
    };
    if ctx.options(|o| o.theme_preference) != preference {
        ctx.set_theme(preference);
    }
}
