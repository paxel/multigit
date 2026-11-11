//! Settings window: timers, limits, completion and the card button editor.

use crate::domain::config::{ButtonDef, button_presets};
use crate::ui::app::App;
use egui::{Context, DragValue, TextEdit};

enum RowAction {
    Up(usize),
    Down(usize),
    Remove(usize),
}

/// Draw the settings window when it is open. Edits apply to `app.config`
/// directly; the app writes the config file when it changed.
pub fn show(app: &mut App, ctx: &Context) {
    let mut open = app.settings_open;
    let config_file = app.config_file().map(|p| p.display().to_string());
    let config = &mut app.config;
    egui::Window::new("Settings")
        .open(&mut open)
        .default_width(520.0)
        .show(ctx, |ui| {
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
                });
            ui.checkbox(
                &mut config.auto_insert_single_match,
                "Tab inserts a single completion directly",
            );

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
            if let Some(file) = &config_file {
                ui.separator();
                ui.weak(format!("Saved to {file}"));
            }
        });
    app.settings_open = open;
}
