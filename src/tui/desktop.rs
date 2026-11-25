//! The desktop around the app in a terminal: there is none. What would open
//! in a window of its own runs in this terminal instead, while the board
//! steps aside; nothing opens a browser or plays a sound.

use crate::application::desktop::Desktop;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

/// A program that takes over the terminal until it ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Foreground {
    /// A text file in the pager.
    Pager(PathBuf),
}

impl Foreground {
    /// The program and its arguments. `pager` is `$PAGER`.
    pub fn command(&self, pager: Option<OsString>) -> Vec<OsString> {
        match self {
            Self::Pager(file) => {
                let pager = pager
                    .and_then(|p| p.into_string().ok())
                    .and_then(|p| shell_words::split(&p).ok())
                    .filter(|words| !words.is_empty())
                    .unwrap_or_else(|| vec!["less".to_owned()]);
                let mut command: Vec<OsString> = pager.into_iter().map(OsString::from).collect();
                command.push(file.clone().into_os_string());
                command
            }
        }
    }

    /// Where it runs.
    pub fn dir(&self) -> Option<&Path> {
        match self {
            Self::Pager(_) => None,
        }
    }

    /// What it is, for a message when it cannot start.
    pub fn what(&self) -> &'static str {
        match self {
            Self::Pager(_) => "the pager",
        }
    }
}

/// The terminal's stand-in for the desktop. What the app asks it to open
/// waits here until the board can step aside for it.
#[derive(Debug, Default)]
pub struct TerminalDesktop {
    waiting: Mutex<Vec<Foreground>>,
}

impl TerminalDesktop {
    /// What waits to take over the terminal, oldest first; the list is
    /// empty afterwards.
    pub fn take(&self) -> Vec<Foreground> {
        self.waiting
            .lock()
            .map(|mut waiting| std::mem::take(&mut *waiting))
            .unwrap_or_default()
    }

    fn push(&self, program: Foreground) -> io::Result<()> {
        self.waiting
            .lock()
            .map_err(|_| io::Error::other("the terminal is busy"))?
            .push(program);
        Ok(())
    }
}

fn none(what: &str) -> io::Error {
    io::Error::other(format!("{what} cannot be opened in the terminal"))
}

impl Desktop for TerminalDesktop {
    fn open_file(&self, file: &Path) -> io::Result<()> {
        self.push(Foreground::Pager(file.to_path_buf()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pager_is_pager_with_its_words_or_less() {
        let file = Foreground::Pager(PathBuf::from("/tmp/out.txt"));
        assert_eq!(
            file.command(Some("less -R".into())),
            vec![
                OsString::from("less"),
                OsString::from("-R"),
                OsString::from("/tmp/out.txt")
            ]
        );
        assert_eq!(file.command(None)[0], OsString::from("less"));
        assert_eq!(file.command(Some("".into()))[0], OsString::from("less"));
    }

    #[test]
    fn what_the_app_opens_waits_until_it_is_taken() {
        let desktop = TerminalDesktop::default();
        desktop.open_file(Path::new("/tmp/a.txt")).unwrap();
        assert_eq!(
            desktop.take(),
            vec![Foreground::Pager(PathBuf::from("/tmp/a.txt"))]
        );
        assert!(desktop.take().is_empty());
        assert!(desktop.open_url("https://example.org").is_err());
    }
}
