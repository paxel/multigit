//! The bottom command line: `git` label, input, Tab completion popup,
//! fish-like history suggestion, filtered Up/Down and Ctrl+R fuzzy search.

use crate::domain::command::shell_line;
use crate::domain::completion::{
    Completion, RepoValues, TabAction, Vocabulary, complete, replace_word, tab_action,
};
use crate::domain::history::{History, HistoryNav};
use egui::text::{CCursor, CCursorRange, CharIndex};
use egui::text_edit::TextEditState;
use egui::{Align2, Id, Key, Modifiers, RichText, TextEdit, TextStyle, Ui};

/// Rows shown in a popup at most.
const MAX_POPUP_ROWS: usize = 200;

/// Id of the input field (also used by tests).
pub fn input_id() -> Id {
    Id::new("multigit-cli-input")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    Completion,
    History,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Popup {
    pub kind: PopupKind,
    pub selected: usize,
    /// One-shot: scroll the selected row into view on the next frame.
    scroll: bool,
}

impl Popup {
    fn new(kind: PopupKind) -> Self {
        Self {
            kind,
            selected: 0,
            scroll: true,
        }
    }
}

/// Text and navigation state of the command line.
#[derive(Debug, Clone, Default)]
pub struct CliState {
    pub text: String,
    pub popup: Option<Popup>,
    nav: HistoryNav,
    /// Text after the last frame, to detect user edits.
    last_text: String,
    /// The input had focus at the end of the last frame.
    had_focus: bool,
}

/// What the CLI needs from the app for one frame.
pub struct CliContext<'a> {
    pub enabled: bool,
    pub history: &'a History,
    pub vocab: &'a Vocabulary,
    pub values: &'a RepoValues,
    pub auto_insert_single: bool,
    /// Name of the shell `!` lines run in, shown instead of `git` for them.
    pub shell: String,
}

/// What the app has to do after the CLI frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    Run(String),
    NeedOptions(String),
}

/// One popup row: what is shown and the line it produces when accepted.
struct Row {
    text: String,
    description: Option<String>,
    tag: Option<String>,
    line: String,
    cursor: usize,
}

fn completion_rows(c: &Completion, line: &str, cursor: usize) -> Vec<Row> {
    c.candidates
        .iter()
        .take(MAX_POPUP_ROWS)
        .map(|cand| {
            let (line, cursor) = replace_word(line, c.word_start, cursor, &cand.insertion());
            Row {
                text: cand.text.clone(),
                description: cand.description.clone(),
                tag: cand.tag.clone(),
                line,
                cursor,
            }
        })
        .collect()
}

fn history_rows(history: &History, query: &str) -> Vec<Row> {
    history
        .fuzzy(query)
        .into_iter()
        .take(MAX_POPUP_ROWS)
        .map(|e| Row {
            text: e.to_owned(),
            description: None,
            tag: None,
            line: e.to_owned(),
            cursor: e.len(),
        })
        .collect()
}

fn byte_cursor(ctx: &egui::Context, text: &str) -> usize {
    let chars = TextEditState::load(ctx, input_id())
        .and_then(|s| s.cursor.char_range())
        .map_or(usize::MAX, |r| r.primary.index.0);
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(byte, _)| byte)
}

fn set_cursor(ctx: &egui::Context, text: &str, byte: usize) {
    let mut state = TextEditState::load(ctx, input_id()).unwrap_or_default();
    let chars = text[..byte.min(text.len())].chars().count();
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(CharIndex(chars)))));
    state.store(ctx, input_id());
}

fn consume(ui: &mut Ui, modifiers: Modifiers, key: Key) -> bool {
    ui.input_mut(|i| i.consume_key(modifiers, key))
}

impl CliState {
    fn set_line(&mut self, ctx: &egui::Context, line: String, cursor: usize) {
        set_cursor(ctx, &line, cursor);
        self.text = line;
        self.last_text = self.text.clone();
    }

    fn rows(&self, cx: &CliContext, cursor: usize, actions: &mut Vec<CliAction>) -> Vec<Row> {
        match self.popup.as_ref().map(|p| p.kind) {
            Some(PopupKind::Completion) => {
                let c = complete(&self.text, cursor, cx.vocab, cx.values);
                if let Some(cmd) = &c.missing_options_for {
                    actions.push(CliAction::NeedOptions(cmd.clone()));
                }
                completion_rows(&c, &self.text, cursor)
            }
            Some(PopupKind::History) => history_rows(cx.history, &self.text),
            None => Vec::new(),
        }
    }

    fn on_tab(
        &mut self,
        ctx: &egui::Context,
        cx: &CliContext,
        cursor: usize,
        actions: &mut Vec<CliAction>,
    ) {
        if let Some(popup) = &mut self.popup {
            // Tab in an open popup moves to the next row (wraps in `show`).
            popup.selected += 1;
            popup.scroll = true;
            return;
        }
        let c = complete(&self.text, cursor, cx.vocab, cx.values);
        if let Some(cmd) = &c.missing_options_for {
            actions.push(CliAction::NeedOptions(cmd.clone()));
        }
        match tab_action(&c, cx.auto_insert_single) {
            TabAction::Nothing => {}
            TabAction::Insert(text) => {
                let (line, cursor) = replace_word(&self.text, c.word_start, cursor, &text);
                self.set_line(ctx, line, cursor);
            }
            TabAction::Popup { extend_to } => {
                if let Some(prefix) = extend_to {
                    let (line, cursor) = replace_word(&self.text, c.word_start, cursor, &prefix);
                    self.set_line(ctx, line, cursor);
                }
                self.popup = Some(Popup::new(PopupKind::Completion));
            }
        }
    }

    /// Keyboard handling before the input is drawn, so the keys never reach it.
    fn handle_keys(&mut self, ui: &mut Ui, cx: &CliContext, actions: &mut Vec<CliAction>) {
        let ctx = ui.ctx().clone();
        let cursor = byte_cursor(&ctx, &self.text);
        if consume(ui, Modifiers::CTRL, Key::R) {
            self.popup = Some(Popup::new(PopupKind::History));
            return;
        }
        if consume(ui, Modifiers::NONE, Key::Tab) {
            self.on_tab(&ctx, cx, cursor, actions);
            return;
        }
        if let Some(popup) = &mut self.popup {
            if consume(ui, Modifiers::NONE, Key::Escape) {
                // egui drops focus on Escape before this runs; Escape only closes the popup.
                self.popup = None;
                ctx.memory_mut(|m| m.request_focus(input_id()));
            } else if consume(ui, Modifiers::NONE, Key::ArrowUp) {
                popup.selected = popup.selected.saturating_sub(1);
                popup.scroll = true;
            } else if consume(ui, Modifiers::NONE, Key::ArrowDown) {
                popup.selected += 1;
                popup.scroll = true;
            } else if consume(ui, Modifiers::NONE, Key::Enter) {
                let selected = popup.selected;
                let rows = self.rows(cx, cursor, actions);
                if let Some(row) = rows.get(selected.min(rows.len().saturating_sub(1))) {
                    let (line, cursor) = (row.line.clone(), row.cursor);
                    self.set_line(&ctx, line, cursor);
                }
                self.popup = None;
            }
            return;
        }
        if consume(ui, Modifiers::NONE, Key::ArrowUp) {
            if let Some(line) = self.nav.up(cx.history, &self.text) {
                let len = line.len();
                self.set_line(&ctx, line, len);
            }
        } else if consume(ui, Modifiers::NONE, Key::ArrowDown) {
            if let Some(line) = self.nav.down(cx.history) {
                let len = line.len();
                self.set_line(&ctx, line, len);
            }
        } else if consume(ui, Modifiers::NONE, Key::Enter) {
            let line = std::mem::take(&mut self.text);
            self.nav.reset();
            self.set_line(&ctx, String::new(), 0);
            if !line.trim().is_empty() {
                actions.push(CliAction::Run(line));
            }
        } else if cursor == self.text.len()
            && let Some(suggestion) = cx.history.suggestion(&self.text).map(str::to_owned)
            && consume(ui, Modifiers::NONE, Key::ArrowRight)
        {
            let len = suggestion.len();
            self.set_line(&ctx, suggestion, len);
        }
    }

    /// Draw the command line; returns the actions for the app.
    pub fn show(&mut self, ui: &mut Ui, cx: &CliContext) -> Vec<CliAction> {
        let mut actions = Vec::new();
        let id = input_id();
        // egui drops focus on Escape before widgets run; with a popup open,
        // Escape belongs to the popup, so treat the input as still focused.
        let escape_for_popup =
            self.had_focus && self.popup.is_some() && ui.input(|i| i.key_pressed(Key::Escape));
        let focused = ui.memory(|m| m.has_focus(id)) || escape_for_popup;
        if !cx.enabled {
            self.popup = None;
        } else if focused {
            self.handle_keys(ui, cx, &mut actions);
        }

        let hint = if cx.enabled {
            "git arguments, a leading git is ignored; ! runs a shell line (Tab completes, Ctrl+R searches history)"
        } else {
            "mark repositories to run commands"
        };
        let label = if shell_line(&self.text).is_some() {
            cx.shell.as_str()
        } else {
            "git"
        };
        let output = ui
            .horizontal(|ui| {
                ui.label(RichText::new(label).monospace().strong());
                ui.add_enabled_ui(cx.enabled, |ui| {
                    TextEdit::singleline(&mut self.text)
                        .id(id)
                        .font(TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .lock_focus(true)
                        .hint_text(hint)
                        .show(ui)
                })
                .inner
            })
            .inner;

        if self.text != self.last_text {
            // The user edited the line.
            self.nav.reset();
            self.last_text = self.text.clone();
            if let Some(popup) = &mut self.popup {
                popup.selected = 0;
            }
        }
        if !cx.enabled {
            return actions;
        }
        let ctx = ui.ctx().clone();
        let cursor = byte_cursor(&ctx, &self.text);
        let has_focus = output.response.has_focus();
        self.had_focus = has_focus;

        // Grey history suggestion after the cursor.
        if has_focus
            && self.popup.is_none()
            && cursor == self.text.len()
            && let Some(suggestion) = cx.history.suggestion(&self.text)
        {
            let suffix = &suggestion[self.text.len()..];
            let pos = output.galley_pos + egui::vec2(output.galley.rect.width(), 0.0);
            let font = TextStyle::Monospace.resolve(ui.style());
            ui.painter().text(
                pos,
                Align2::LEFT_TOP,
                suffix,
                font,
                ui.visuals().weak_text_color(),
            );
        }

        self.show_popup(ui, cx, cursor, output.response.rect, &mut actions);
        actions
    }

    fn show_popup(
        &mut self,
        ui: &mut Ui,
        cx: &CliContext,
        cursor: usize,
        anchor: egui::Rect,
        actions: &mut Vec<CliAction>,
    ) {
        if self.popup.is_none() {
            return;
        }
        // Focus moved elsewhere (and not by a click on the popup itself).
        let clicking = ui.input(|i| i.pointer.any_down() || i.pointer.any_released());
        if !ui.memory(|m| m.has_focus(input_id())) && !clicking {
            self.popup = None;
            return;
        }
        let rows = self.rows(cx, cursor, actions);
        let Some(popup) = &mut self.popup else {
            return;
        };
        if rows.is_empty() {
            self.popup = None;
            return;
        }
        popup.selected %= rows.len();
        let selected = popup.selected;
        let scroll = std::mem::take(&mut popup.scroll);
        let title = match popup.kind {
            PopupKind::Completion => "completions",
            PopupKind::History => "history (fuzzy)",
        };
        let tag_color = ui.visuals().warn_fg_color;
        let mut accepted: Option<(String, usize)> = None;
        egui::Area::new(input_id().with("popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor.left_top())
            .pivot(Align2::LEFT_BOTTOM)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(anchor.width().min(640.0));
                    ui.weak(title);
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            for (i, row) in rows.iter().enumerate() {
                                let resp = ui
                                    .horizontal(|ui| {
                                        let resp = ui.add(egui::Button::selectable(
                                            i == selected,
                                            RichText::new(&row.text).monospace(),
                                        ));
                                        if let Some(tag) = &row.tag {
                                            ui.label(RichText::new(tag).small().color(tag_color));
                                        }
                                        if let Some(d) = &row.description {
                                            ui.weak(d);
                                        }
                                        resp
                                    })
                                    .inner;
                                if i == selected && scroll {
                                    resp.scroll_to_me(None);
                                }
                                if resp.clicked() {
                                    accepted = Some((row.line.clone(), row.cursor));
                                }
                            }
                        });
                });
            });
        if let Some((line, cursor)) = accepted {
            let ctx = ui.ctx().clone();
            self.set_line(&ctx, line, cursor);
            self.popup = None;
            ctx.memory_mut(|m| m.request_focus(input_id()));
        }
    }
}
