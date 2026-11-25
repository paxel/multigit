//! The desktop around the app in a terminal: there is none. Nothing opens a
//! window, a browser or a speaker.

use crate::application::desktop::Desktop;
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// The terminal's stand-in for the desktop.
#[derive(Debug, Default)]
pub struct TerminalDesktop;

fn none(what: &str) -> io::Error {
    io::Error::other(format!("{what} cannot be opened in the terminal"))
}

impl Desktop for TerminalDesktop {
    fn open_file(&self, _file: &Path) -> io::Result<()> {
        Err(none("a file"))
    }

    fn open_url(&self, _url: &str) -> io::Result<()> {
        Err(none("a web page"))
    }

    fn play(&self, _file: &Path) -> io::Result<()> {
        Ok(())
    }

    fn play_wait(&self, _file: &Path, _stop: &AtomicBool) -> io::Result<()> {
        Ok(())
    }

    fn open_terminal(&self, _dir: &Path, _configured: &str) -> io::Result<()> {
        Err(none("a terminal window"))
    }
}
