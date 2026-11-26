//! Terminal front end (`--tui`): the board as a table in a terminal, for a
//! machine without a display, reached over SSH first. It drives the same
//! [`crate::app::App`] as the window; only drawing and keys live here.

pub mod actions;
pub mod board;
pub mod card_menu;
pub mod cli;
pub mod clipboard;
pub mod commits;
pub mod desktop;
pub mod favorites;
pub mod folders;
pub mod graph;
pub mod kawaii;
pub mod keys;
pub mod output;
pub mod repo;
pub mod settings;
pub mod state;
pub mod widgets;

#[cfg(test)]
mod tests;

use desktop::{Foreground, TerminalDesktop};
use ratatui::backend::Backend;
use ratatui::crossterm::event::{
    self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture, Event,
};
use ratatui::crossterm::execute;
use ratatui::{DefaultTerminal, Terminal};
use std::io::{self, Write};
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
    let mut next_event = |wait: Duration| -> io::Result<Option<Event>> {
        if event::poll(wait)? {
            event::read().map(Some)
        } else {
            Ok(None)
        }
    };
    // The board steps aside while the program runs, and comes back.
    let mut foreground = |terminal: &mut DefaultTerminal, program: &Foreground| {
        leave();
        let command = program.command(std::env::var_os("PAGER"), std::env::var_os("SHELL"));
        let ran = run_foreground(&command, program.dir());
        *terminal = enter()?;
        terminal.clear()?;
        Ok(ran)
    };
    let result = run_loop(
        &mut terminal,
        &mut app,
        &desktop,
        &mut io::stdout(),
        &mut next_event,
        &mut foreground,
    );
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

/// What waits for the next event of the terminal, at most this long.
pub type NextEvent<'a> = dyn FnMut(Duration) -> io::Result<Option<Event>> + 'a;

/// What runs a program in the terminal: `Ok` with how the program went.
pub type RunForeground<'a, T> = dyn FnMut(&mut T, &Foreground) -> io::Result<io::Result<()>> + 'a;

/// Draw, wait for an event, handle it, until the user quits. Copies go to
/// `out`; what `desktop` was asked to open goes to `foreground`.
pub fn run_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    desktop: &TerminalDesktop,
    out: &mut impl Write,
    next_event: &mut NextEvent,
    foreground: &mut RunForeground<Terminal<B>>,
) -> io::Result<()>
where
    B::Error: Send + Sync + 'static,
{
    let mut last_frame = Instant::now();
    while !app.gui.quit {
        app.handle_events();
        let now = Instant::now();
        let next = app.tick(now);
        // The Kawaii theme moves while the terminal has focus, and a few
        // seconds after a repository changed.
        let dt = now.duration_since(last_frame).as_secs_f32();
        last_frame = now;
        let moving = app.advance_kawaii(dt, app.gui.focused);
        app.tend_critters();
        let frame = if moving {
            Duration::from_secs_f32(1.0 / crate::domain::kawaii::FPS)
        } else {
            FRAME
        };
        terminal
            .draw(|frame| board::draw(frame, app))
            .map_err(io::Error::other)?;
        send_copies(app, out)?;
        if let Some(event) = next_event(next.min(frame))? {
            on_event(app, event);
        }
        for program in desktop.take() {
            if let Err(e) = foreground(terminal, &program)? {
                app.toast(format!("Cannot start {}: {e}", program.what()));
            }
        }
    }
    Ok(())
}

/// Hand what was copied to the terminal's clipboard.
pub fn send_copies(app: &mut App, out: &mut impl Write) -> io::Result<()> {
    let copies = std::mem::take(&mut app.gui.copies);
    if copies.is_empty() {
        return Ok(());
    }
    for text in copies {
        out.write_all(clipboard::osc52(&text).as_bytes())?;
    }
    out.flush()
}

/// Run `command` in `dir` and wait for it.
pub fn run_foreground(
    command: &[std::ffi::OsString],
    dir: Option<&std::path::Path>,
) -> io::Result<()> {
    let Some((name, args)) = command.split_first() else {
        return Err(io::Error::other("no program"));
    };
    let mut process = std::process::Command::new(name);
    process.args(args);
    if let Some(dir) = dir {
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
