//! What the app asks of the desktop around it: other programs' windows.

use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// Opens things outside the app. Only `play_wait` waits for the program it
/// starts.
pub trait Desktop: Send + Sync {
    /// Open `file` with the system's default program for it.
    fn open_file(&self, file: &Path) -> io::Result<()>;

    /// Open a web or mail address with the system's default program for it.
    fn open_url(&self, url: &str) -> io::Result<()>;

    /// Play the WAV file `file` with the system's player program.
    fn play(&self, file: &Path) -> io::Result<()>;

    /// Play `file` and return when the player is done, or as soon as `stop`
    /// is set, which ends the player.
    fn play_wait(&self, file: &Path, stop: &AtomicBool) -> io::Result<()>;

    /// Open a terminal window with a shell in `dir`. `configured` is the
    /// terminal command from the settings; blank means find one.
    fn open_terminal(&self, dir: &Path, configured: &str) -> io::Result<()>;
}
