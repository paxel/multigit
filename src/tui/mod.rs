//! Terminal front end (`--tui`): the board as a table in a terminal, for a
//! machine without a display, reached over SSH first. It drives the same
//! [`crate::app::App`] as the window; only drawing and keys live here.

pub mod actions;
pub mod board;
pub mod card_menu;
pub mod cli;
pub mod clipboard;
pub mod desktop;
pub mod favorites;
pub mod folders;
pub mod keys;
pub mod output;
pub mod state;
pub mod widgets;

#[cfg(test)]
mod tests;

use desktop::{Foreground, TerminalDesktop};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture, Event,
};
use ratatui::crossterm::execute;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use state::Tui;

/// The board as the terminal drives it.
pub type App = crate::app::App<Tui>;

/// The longest wait for a key: background results (a status, a fetch, a
/// command's output) show at least this often.
const FRAME: Duration = Duration::from_millis(100);

/// Run the terminal front end until the user quits. `desktop` is the one
/// `app` was made with: what it is asked to open runs in this terminal.
pub fn run(mut app: App, desktop: Arc<TerminalDesktop>) -> io::Result<()> {
    app.silent = true;
    let mut terminal = enter()?;
    let result = run_loop(&mut terminal, &mut app, &desktop);
    leave();
    result
}

/// Take the terminal: raw mode, the alternate screen, mouse and focus
/// reports. `init` restores it on a panic too.
fn enter() -> io::Result<DefaultTerminal> {
    let terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture, EnableFocusChange)?;
    Ok(terminal)
}

/// Give the terminal back as it was.
fn leave() {
    let _ = execute!(io::stdout(), DisableMouseCapture, DisableFocusChange);
    ratatui::restore();
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    desktop: &TerminalDesktop,
) -> io::Result<()> {
    while !app.gui.quit {
        app.handle_events();
        let next = app.tick(Instant::now());
        terminal.draw(|frame| board::draw(frame, app))?;
        send_copies(app)?;
        if event::poll(next.min(FRAME))? {
            on_event(app, event::read()?);
        }
        for program in desktop.take() {
            leave();
            let ran = run_foreground(&program);
            *terminal = enter()?;
            terminal.clear()?;
            if let Err(e) = ran {
                app.toast(format!("Cannot start {}: {e}", program.what()));
            }
        }
    }
    Ok(())
}

/// Hand what was copied to the terminal's clipboard.
fn send_copies(app: &mut App) -> io::Result<()> {
    use std::io::Write;
    let copies = std::mem::take(&mut app.gui.copies);
    if copies.is_empty() {
        return Ok(());
    }
    let mut out = io::stdout();
    for text in copies {
        out.write_all(clipboard::osc52(&text).as_bytes())?;
    }
    out.flush()
}

/// Run `program` in this terminal and wait for it.
fn run_foreground(program: &Foreground) -> io::Result<()> {
    let command = program.command(std::env::var_os("PAGER"), std::env::var_os("SHELL"));
    let Some((name, args)) = command.split_first() else {
        return Err(io::Error::other("no program"));
    };
    let mut process = std::process::Command::new(name);
    process.args(args);
    if let Some(dir) = program.dir() {
        process.current_dir(dir);
    }
    process.status().map(|_| ())
}

/// One event of the terminal.
pub fn on_event(app: &mut App, event: Event) {
    match event {
        Event::Key(key) => keys::on_key(app, key),
        Event::Mouse(mouse) => keys::on_mouse(app, mouse),
        Event::FocusGained => app.gui.focused = true,
        Event::FocusLost => app.gui.focused = false,
        Event::Resize(..) | Event::Paste(_) => {}
    }
}
