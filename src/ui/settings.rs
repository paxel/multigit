//! Settings window: timers, limits, theme, completion and the card button
//! editor; behind its "About" button the About page and the licence list.

use crate::domain::about;
use crate::domain::audio::Sound;
use crate::domain::config::{ButtonDef, Theme, button_presets};
use crate::domain::naming::title;
use crate::domain::songs::{STYLES, styles_on};
use crate::infrastructure::launcher::ICON_PNG;
use crate::ui::app::{App, SettingsPage};
use crate::ui::view::pick_sound_file;
use egui::{Context, DragValue, RichText, TextEdit, Ui};

/// A click in the sound rows of Settings.
enum SoundAct {
    Choose(Sound),
    Play(Sound),
}

enum RowAction {
    Up(usize),
    Down(usize),
    Remove(usize),
}

/// Draw the settings window when it is open: the settings, or the About page
/// or licence list opened from them.
pub fn show(app: &mut App, ctx: &Context) {
    let mut open = app.settings_open;
    let title = match app.settings_page {
        SettingsPage::Settings => "Settings",
        SettingsPage::About => "About multigit",
        SettingsPage::Licenses => "Open-source licenses",
    };
    egui::Window::new(title)
        .id(egui::Id::new("settings-window"))
        .open(&mut open)
        .default_width(520.0)
        .show(ctx, |ui| match app.settings_page {
            SettingsPage::Settings => settings_page(app, ui),
            SettingsPage::About => about_page(app, ui),
            SettingsPage::Licenses => licenses_page(app, ui),
        });
    if !open {
        // Opened again, the window starts with the settings.
        app.settings_page = SettingsPage::Settings;
    }
    app.settings_open = open;
}

/// The app's icon as a texture, made once.
fn icon_texture(app: &mut App, ctx: &Context) -> Option<egui::TextureHandle> {
    if app.icon.is_none() {
        let icon = eframe::icon_data::from_png_bytes(ICON_PNG).ok()?;
        let size = [icon.width as usize, icon.height as usize];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, &icon.rgba);
        app.icon = Some(ctx.load_texture("app-icon", image, egui::TextureOptions::default()));
    }
    app.icon.clone()
}

/// Icon, name, version and tagline, then the rows that open the project's
/// pages, a mail to the developer and the licence list.
fn about_page(app: &mut App, ui: &mut Ui) {
    ui.vertical_centered(|ui| {
        if let Some(icon) = icon_texture(app, ui.ctx()) {
            ui.add(egui::Image::new(&icon).fit_to_exact_size(egui::Vec2::splat(72.0)));
        }
        ui.label(RichText::new("multigit").strong().size(22.0));
        ui.weak(format!("Version {}", about::VERSION));
        ui.label(about::TAGLINE);
    });
    ui.add_space(8.0);
    for link in about::LINKS {
        if about_row(ui, link.title, link.subtitle) {
            app.open_url(link.url);
        }
    }
    if about_row(ui, "Open-source licenses", "The crates in multigit") {
        app.settings_page = SettingsPage::Licenses;
    }
    ui.add_space(8.0);
    if ui.button("Back").clicked() {
        app.settings_page = SettingsPage::Settings;
    }
}

/// A row of the About page: a button and what it leads to.
fn about_row(ui: &mut Ui, title: &str, subtitle: &str) -> bool {
    let clicked = ui.button(title).clicked();
    ui.weak(subtitle);
    ui.add_space(4.0);
    clicked
}

/// Every crate in the binary with its licence.
fn licenses_page(app: &mut App, ui: &mut Ui) {
    if ui.button("Back").clicked() {
        app.settings_page = SettingsPage::About;
    }
    ui.separator();
    let licenses = about::crate_licenses();
    let row_height = ui.text_style_height(&egui::TextStyle::Body);
    egui::ScrollArea::vertical().max_height(420.0).show_rows(
        ui,
        row_height,
        licenses.len(),
        |ui, rows| {
            for license in licenses.get(rows).unwrap_or_default() {
                ui.horizontal(|ui| {
                    ui.label(license.krate);
                    ui.weak(license.license);
                });
            }
        },
    );
}

/// Edits apply to `app.config` directly; the app writes the config file when
/// it changed.
fn settings_page(app: &mut App, ui: &mut Ui) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
        if ui.button("About multigit").clicked() {
            app.settings_page = SettingsPage::About;
        }
    });
    let config_file = app.config_file().map(|p| p.display().to_string());
    let mut sound_act = None;
    // `Some(None)` unhides all, `Some(Some(path))` that one.
    let mut unhide: Option<Option<std::path::PathBuf>> = None;
    let config = &mut app.config;
    egui::Grid::new("settings-grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Background fetch every");
            ui.add(
                DragValue::new(&mut config.fetch_interval_secs)
                    .range(10..=86_400)
                    .suffix(" s"),
            );
            ui.end_row();
            ui.label("Parallel git processes");
            ui.add(DragValue::new(&mut config.concurrency).range(1..=32));
            ui.end_row();
            ui.label("Folder scan depth");
            ui.add(DragValue::new(&mut config.scan_depth).range(1..=10));
            ui.end_row();
            ui.label("Terminal command");
            ui.add(
                TextEdit::singleline(&mut config.terminal)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("found automatically")
                    .desired_width(240.0),
            )
            .on_hover_text("Opens a terminal in a repository; {dir} stands for its folder");
            ui.end_row();
            ui.label("Theme");
            ui.horizontal(|ui| {
                for theme in Theme::ALL {
                    ui.radio_value(&mut config.theme, theme, theme.label());
                }
            });
            ui.end_row();
        });
    ui.checkbox(
        &mut config.auto_insert_single_match,
        "Tab inserts a single completion directly",
    );

    ui.separator();
    ui.horizontal(|ui| {
        ui.checkbox(&mut config.music, "Kawaii music")
            .on_hover_text("Lofi beats, made by multigit itself, while the Kawaii theme is on");
        ui.add(
            egui::Slider::new(&mut config.music_volume, 0..=100)
                .suffix(" %")
                .text("volume"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Music themes:");
        let on = styles_on(&STYLES, &config.music_off);
        for (i, style) in STYLES.iter().enumerate() {
            let mut checked = on.contains(&i);
            // The last theme stays: the switch above turns the music off.
            let last = checked && on.len() == 1;
            let hint = if last {
                "The last theme stays on; the Kawaii music switch turns the music off"
            } else {
                style.name
            };
            if ui
                .add_enabled(!last, egui::Checkbox::new(&mut checked, style.label))
                .on_hover_text(hint)
                .on_disabled_hover_text(hint)
                .changed()
            {
                config.music_off = STYLES
                    .iter()
                    .enumerate()
                    .filter(|(other, _)| {
                        if *other == i {
                            !checked
                        } else {
                            !on.contains(other)
                        }
                    })
                    .map(|(_, style)| style.label.to_owned())
                    .collect();
            }
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut config.sounds, "Sounds")
            .on_hover_text("A beep when a remote commit appears, a boop for a local one");
        ui.add(
            egui::Slider::new(&mut config.sound_volume, 0..=100)
                .suffix(" %")
                .text("volume"),
        );
    });
    egui::Grid::new("sounds-grid")
        .num_columns(5)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            for sound in Sound::ALL {
                ui.label(sound.label());
                let file = config.sound_file(sound).map(|f| title(f));
                ui.weak(file.as_deref().unwrap_or("built-in tone"));
                if ui.button("Choose file\u{2026}").clicked() {
                    sound_act = Some(SoundAct::Choose(sound));
                }
                let has_file = config.sound_file(sound).is_some();
                if ui
                    .add_enabled(has_file, egui::Button::new("Default"))
                    .clicked()
                {
                    config.set_sound_file(sound, None);
                }
                if ui.button("Play").clicked() {
                    sound_act = Some(SoundAct::Play(sound));
                }
                ui.end_row();
            }
        });

    ui.separator();
    ui.strong("Card buttons");
    let mut action = None;
    let count = config.buttons.len();
    egui::Grid::new("buttons-grid")
        .num_columns(5)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            ui.weak("Label");
            ui.weak("git arguments, or ! and a shell line");
            ui.end_row();
            for (i, b) in config.buttons.iter_mut().enumerate() {
                ui.add(TextEdit::singleline(&mut b.label).desired_width(140.0));
                ui.add(
                    TextEdit::singleline(&mut b.args)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(200.0),
                );
                if ui.add_enabled(i > 0, egui::Button::new("Up")).clicked() {
                    action = Some(RowAction::Up(i));
                }
                if ui
                    .add_enabled(i + 1 < count, egui::Button::new("Down"))
                    .clicked()
                {
                    action = Some(RowAction::Down(i));
                }
                if ui.button("Remove").clicked() {
                    action = Some(RowAction::Remove(i));
                }
                ui.end_row();
            }
        });
    match action {
        Some(RowAction::Up(i)) => config.buttons.swap(i, i - 1),
        Some(RowAction::Down(i)) => config.buttons.swap(i, i + 1),
        Some(RowAction::Remove(i)) => {
            config.buttons.remove(i);
        }
        None => {}
    }
    ui.horizontal(|ui| {
        if ui.button("Add button").clicked() {
            config.buttons.push(ButtonDef::new("new", "status"));
        }
        egui::ComboBox::from_id_salt("presets")
            .selected_text("Add preset")
            .show_ui(ui, |ui| {
                for preset in button_presets() {
                    let text = format!("{}  (git {})", preset.label, preset.args);
                    if ui.selectable_label(false, text).clicked() {
                        config.buttons.push(preset);
                    }
                }
            });
    });
    if !config.hidden.is_empty() {
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("Hidden repositories");
            if ui.button("Unhide all").clicked() {
                unhide = Some(None);
            }
        });
        for path in &config.hidden {
            ui.horizontal(|ui| {
                if ui.button("Unhide").clicked() {
                    unhide = Some(Some(path.clone()));
                }
                ui.label(path.display().to_string());
            });
        }
    }
    if let Some(file) = &config_file {
        ui.separator();
        ui.weak(format!("Saved to {file}"));
    }
    match sound_act {
        Some(SoundAct::Choose(sound)) => app.open_sound_picker(sound, pick_sound_file),
        Some(SoundAct::Play(sound)) => app.play_sound(sound),
        None => {}
    }
    match unhide {
        Some(Some(path)) => app.unhide(&path),
        Some(None) => app.unhide_all(),
        None => {}
    }
}
