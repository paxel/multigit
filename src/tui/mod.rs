//! Terminal front end (`--tui`): the board as a table in a terminal, for a
//! machine without a display, reached over SSH first. It drives the same
//! [`crate::app::App`] as the window; only drawing and keys live here.

pub mod board;
pub mod desktop;
pub mod keys;
pub mod state;

#[cfg(test)]
mod tests;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture, Event,
};
use ratatui::crossterm::execute;
use std::io;
use std::time::{Duration, Instant};

pub use state::Tui;

/// The board as the terminal drives it.
pub type App = crate::app::App<Tui>;

/// The longest wait for a key: background results (a status, a fetch, a
/// command's output) show at least this often.
const FRAME: Duration = Duration::from_millis(100);

/// Run the terminal front end until the user quits.
pub fn run(mut app: App) -> io::Result<()> {
    app.silent = true;
    // `init` puts the terminal into raw mode and the alternate screen and
    // restores it on a panic too.
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture, EnableFocusChange)?;
    let result = run_loop(&mut terminal, &mut app);
    let _ = execute!(io::stdout(), DisableMouseCapture, DisableFocusChange);
    ratatui::restore();
    result
}

fn run_loop(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.gui.quit {
        app.handle_events();
        let next = app.tick(Instant::now());
        terminal.draw(|frame| board::draw(frame, app))?;
        if event::poll(next.min(FRAME))? {
            on_event(app, event::read()?);
        }
    }
    Ok(())
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
