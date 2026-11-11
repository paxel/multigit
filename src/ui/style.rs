//! App-wide look: roomier buttons than egui's defaults, the theme from the
//! settings, and the Kawaii theme's own style and font.

use crate::domain::config::Theme;
use egui::epaint::text::VariationCoords;
use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontTweak, Stroke,
    Style, ThemePreference, Visuals, vec2,
};
use std::sync::Arc;

/// Space between a button's text and its border.
pub const BUTTON_PADDING: egui::Vec2 = egui::Vec2::new(10.0, 5.0);

/// The rounded font of the Kawaii theme (SIL Open Font License, see
/// `assets/fonts/Fredoka-OFL.txt`).
const FREDOKA: &[u8] = include_bytes!("../../assets/fonts/Fredoka.ttf");
const FREDOKA_NAME: &str = "Fredoka";

fn adjust(style: &mut Style) {
    style.spacing.button_padding = BUTTON_PADDING;
    style.spacing.interact_size = vec2(style.spacing.interact_size.x, 28.0);
    style.spacing.item_spacing = vec2(8.0, 6.0);
}

/// Apply to both the light and the dark style.
pub fn apply(ctx: &Context) {
    ctx.all_styles_mut(adjust);
}

/// The light style as [`apply`] leaves it.
fn light_style() -> Style {
    let mut style = Style {
        visuals: Visuals::light(),
        ..Style::default()
    };
    adjust(&mut style);
    style
}

/// Text of the Kawaii theme.
pub const KAWAII_TEXT: Color32 = Color32::from_rgb(0x5a, 0x3e, 0x5c);
/// Its pink for borders, the hover ring and hearts.
pub const KAWAII_PINK: Color32 = Color32::from_rgb(0xf0, 0x6f, 0xa8);
/// Its yellow for stars and sparks.
pub const KAWAII_YELLOW: Color32 = Color32::from_rgb(0xff, 0xc9, 0x3c);

/// Pastel colors, big corner radii, pill buttons that grow under the pointer
/// and pop on a click.
fn kawaii_style() -> Style {
    let mut style = light_style();
    style.spacing.button_padding = vec2(14.0, 6.0);
    style.spacing.item_spacing = vec2(10.0, 8.0);
    let round = CornerRadius::same(14);
    let visuals = &mut style.visuals;
    visuals.panel_fill = Color32::from_rgb(0xff, 0xf0, 0xf7);
    visuals.window_fill = Color32::from_rgb(0xff, 0xfa, 0xfd);
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.faint_bg_color = Color32::from_rgb(0xff, 0xf5, 0xfa);
    visuals.window_corner_radius = CornerRadius::same(18);
    visuals.menu_corner_radius = round;
    visuals.window_stroke = Stroke::new(1.5, Color32::from_rgb(0xf4, 0xb6, 0xd6));
    visuals.selection.bg_fill = Color32::from_rgb(0x9f, 0xd8, 0xff);
    visuals.selection.stroke = Stroke::new(1.5, Color32::from_rgb(0x2f, 0x7f, 0xb8));
    visuals.hyperlink_color = Color32::from_rgb(0xc2, 0x3b, 0x8a);
    // Red, yellow and green of the status texts stay apart on pastel.
    visuals.error_fg_color = Color32::from_rgb(0xd0, 0x1f, 0x4f);
    visuals.warn_fg_color = Color32::from_rgb(0xa8, 0x5f, 0x00);
    let text = Stroke::new(1.0, KAWAII_TEXT);
    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_stroke = Stroke::new(1.5, Color32::from_rgb(0xf4, 0xb6, 0xd6));
    widgets.noninteractive.fg_stroke = text;
    widgets.noninteractive.corner_radius = round;
    let states = [
        (&mut widgets.inactive, 0xff_d6_e8_u32, 0.0, 0.0),
        (&mut widgets.hovered, 0xff_c2_dd, 1.5, 2.0),
        (&mut widgets.active, 0xff_a8_d0, 2.0, 4.0),
        (&mut widgets.open, 0xff_c2_dd, 1.5, 0.0),
    ];
    for (state, rgb, border, expansion) in states {
        let [_, r, g, b] = rgb.to_be_bytes();
        let fill = Color32::from_rgb(r, g, b);
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(border, KAWAII_PINK);
        state.fg_stroke = text;
        state.corner_radius = round;
        state.expansion = expansion;
    }
    style
}

/// egui's fonts with Fredoka in front for all proportional text; glyphs it
/// lacks and the monospace font stay as they are.
fn kawaii_fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    // The file is a variable font whose default weight is its lightest.
    let tweak = FontTweak {
        coords: VariationCoords::new([(b"wght", 500.0)]),
        ..FontTweak::default()
    };
    let data = FontData::from_static(FREDOKA).tweak(tweak);
    fonts
        .font_data
        .insert(FREDOKA_NAME.to_owned(), Arc::new(data));
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, FREDOKA_NAME.to_owned());
    fonts
}

/// Make `ctx` use the theme from the settings; cheap when it already does.
pub fn apply_theme(ctx: &Context, theme: Theme) {
    let applied_id = egui::Id::new("applied-theme");
    let applied: Option<Theme> = ctx.data(|d| d.get_temp(applied_id));
    if applied == Some(theme) {
        return;
    }
    if theme == Theme::Kawaii {
        ctx.set_style_of(egui::Theme::Light, kawaii_style());
        ctx.set_fonts(kawaii_fonts());
    } else if applied == Some(Theme::Kawaii) {
        ctx.set_style_of(egui::Theme::Light, light_style());
        ctx.set_fonts(FontDefinitions::default());
    }
    ctx.set_theme(match theme {
        Theme::System => ThemePreference::System,
        Theme::Light | Theme::Kawaii => ThemePreference::Light,
        Theme::Dark => ThemePreference::Dark,
    });
    ctx.data_mut(|d| d.insert_temp(applied_id, theme));
}
