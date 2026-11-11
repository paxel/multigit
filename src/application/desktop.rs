//! What the app asks of the desktop around it: other programs' windows.

use std::io;
use std::path::Path;

/// Opens things outside the app. Nothing here waits for the program it
/// starts.
pub trait Desktop: Send + Sync {
    /// Open `file` with the system's default program for it.
    fn open_file(&self, file: &Path) -> io::Result<()>;
}
