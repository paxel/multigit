//! The commit dialog over the repository view: one box whose first line is
//! the subject.

use crate::domain::commit_message::{sends, subject_parts};
use crate::ui::app::App;
use egui::{Key, Modifiers, Ui};

/// What the hint in the box says.
pub const SEND_HINT: &str = "Ctrl+Enter commits";

/// The message as the box shows it: the subject in the strong color, the
/// part of it past the limit in the warning color, the body as text.
fn message_job(ui: &Ui, text: &str, wrap: f32) -> egui::text::LayoutJob {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let part = |color| egui::TextFormat {
        font_id: font.clone(),
        color,
        ..Default::default()
    };
    let (subject, rest) = text
        .split_once('\n')
        .map_or((text, None), |(s, r)| (s, Some(r)));
    let (shown, past) = subject_parts(subject);
    let mut job = egui::text::LayoutJob::default();
    job.append(shown, 0.0, part(ui.visuals().strong_text_color()));
    job.append(past, 0.0, part(ui.visuals().warn_fg_color));
    if let Some(rest) = rest {
        job.append("\n", 0.0, part(ui.visuals().text_color()));
        job.append(rest, 0.0, part(ui.visuals().text_color()));
    }
    job.wrap.max_width = wrap;
    job.keep_trailing_whitespace = true;
    job
}

impl App {
    /// The commit dialog, when it is open.
    pub(super) fn commit_dialog_window(&mut self, ui: &Ui) {
        let Some(dialog) = &mut self.commit_dialog else {
            return;
        };
        let (mut send, mut cancel) = (false, false);
        egui::Modal::new(egui::Id::new("commit-dialog")).show(ui.ctx(), |ui| {
            ui.set_width(600.0);
            ui.heading("Commit");
            // Taken before the box sees the Enter, which would be a line.
            if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter)) {
                send = true;
            }
            let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                let job = message_job(ui, text.as_str(), wrap);
                ui.fonts_mut(|f| f.layout_job(job))
            };
            let output = egui::TextEdit::multiline(&mut dialog.text)
                .id(egui::Id::new("commit-text"))
                .desired_rows(10)
                .desired_width(f32::INFINITY)
                .layouter(&mut layouter)
                .show(ui);
            if dialog.running.is_none() && !output.response.has_focus() && !send {
                output.response.request_focus();
                // The cursor after what was written in, the ticket key.
                let id = egui::Id::new("commit-text");
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                    let end = egui::text::CCursor::new(dialog.text.chars().count());
                    let range = egui::text::CCursorRange::one(end);
                    state.cursor.set_char_range(Some(range));
                    state.store(ui.ctx(), id);
                }
            }
            let hint_at = output.response.rect.right_bottom() - egui::vec2(6.0, 4.0);
            ui.painter().text(
                hint_at,
                egui::Align2::RIGHT_BOTTOM,
                SEND_HINT,
                egui::TextStyle::Small.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            );
            if output.response.changed() {
                let chars = output.cursor_range.map(|c| c.primary.index.0);
                let cursor = chars.map_or(dialog.text.len(), |at| {
                    dialog
                        .text
                        .char_indices()
                        .nth(at)
                        .map_or(dialog.text.len(), |(byte, _)| byte)
                });
                send |= sends(&dialog.text, cursor);
            }
            if let Some(error) = &dialog.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.horizontal(|ui| {
                let idle = dialog.running.is_none();
                if ui.add_enabled(idle, egui::Button::new("Commit")).clicked() {
                    send = true;
                }
                if ui.add_enabled(idle, egui::Button::new("Cancel")).clicked() {
                    cancel = true;
                }
                if !idle {
                    ui.spinner();
                }
            });
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                cancel = true;
            }
        });
        if send {
            self.send_commit();
        } else if cancel {
            self.cancel_commit();
        }
    }
}
