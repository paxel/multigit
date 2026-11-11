//! Menu entry and icon in the user's XDG data directory, written by the app itself
//! so it shows up in the application menu (and with its icon on Wayland) no matter
//! where the binary was unpacked.

use std::io;
use std::path::{Path, PathBuf};

/// Window app id; the desktop file must carry the same name for Wayland.
pub const APP_ID: &str = "io.github.paxel.multigit";
/// Theme icon name used by `Icon=`.
pub const ICON_NAME: &str = "multigit";
/// The 256 px app icon.
pub const ICON_PNG: &[u8] = include_bytes!("../../assets/icon_256.png");

const DESKTOP_TEMPLATE: &str =
    include_str!("../../packaging/linux/io.github.paxel.multigit.desktop");
/// Entries carrying this line were written by multigit and may be updated.
const MARKER: &str = "X-Multigit-Managed=true";

/// What happened to one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Written,
    Unchanged,
    /// An entry without our marker exists; left alone.
    Foreign,
}

/// `Exec=` value: the path quoted and escaped as the Desktop Entry spec requires
/// (quoting rules for the argument, then the general backslash escaping of strings).
pub fn exec_value(path: &Path) -> String {
    let mut quoted = String::from("\"");
    for c in path.to_string_lossy().chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push_str("\\\\");
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted
}

/// The desktop entry with `Exec` pointing at `exec`.
pub fn desktop_entry(exec: &Path) -> String {
    DESKTOP_TEMPLATE
        .lines()
        .map(|line| {
            if line.starts_with("Exec=") {
                format!("Exec={}", exec_value(exec))
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// `$XDG_DATA_HOME`, or `~/.local/share`.
pub fn data_home() -> Option<PathBuf> {
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local/share")))
}

fn write_if_changed(path: &Path, content: &[u8]) -> io::Result<Outcome> {
    if std::fs::read(path).is_ok_and(|old| old == content) {
        return Ok(Outcome::Unchanged);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(Outcome::Written)
}

/// Write the desktop entry and icon below `data_home`. An entry that does not
/// carry our marker belongs to someone else and is left alone, icon included.
pub fn register(data_home: &Path, exec: &Path) -> io::Result<(Outcome, Outcome)> {
    let entry_path = data_home.join(format!("applications/{APP_ID}.desktop"));
    let icon_path = data_home.join(format!("icons/hicolor/256x256/apps/{ICON_NAME}.png"));
    if let Ok(existing) = std::fs::read_to_string(&entry_path)
        && !existing.lines().any(|l| l.trim() == MARKER)
    {
        return Ok((Outcome::Foreign, Outcome::Foreign));
    }
    let entry = write_if_changed(&entry_path, desktop_entry(exec).as_bytes())?;
    let icon = write_if_changed(&icon_path, ICON_PNG)?;
    Ok((entry, icon))
}

/// The `<prefix>/bin/<name>` symlink for a binary in a Homebrew Cellar,
/// verified to point back at it; `None` for everything else. The running
/// binary resolves into the versioned Cellar directory, which the next
/// `brew upgrade` deletes; the prefix's `bin` is repointed on every upgrade.
pub fn stable_brew_path(exe: &Path) -> Option<PathBuf> {
    let prefix = exe
        .ancestors()
        .find(|a| a.file_name() == Some("Cellar".as_ref()))?
        .parent()?;
    let candidate = prefix.join("bin").join(exe.file_name()?);
    (candidate.canonicalize().ok()? == exe.canonicalize().ok()?).then_some(candidate)
}

/// Register the running binary in a background thread; failures are only logged.
pub fn register_in_background() {
    std::thread::spawn(|| {
        let exec = std::env::current_exe()
            .and_then(|p| p.canonicalize())
            .map(|exe| stable_brew_path(&exe).unwrap_or(exe));
        match (data_home(), exec) {
            (Some(data_home), Ok(exec)) => match register(&data_home, &exec) {
                Ok(outcome) => tracing::debug!(?outcome, "menu entry registered"),
                Err(e) => tracing::warn!("cannot write menu entry: {e}"),
            },
            (None, _) => tracing::warn!("no XDG data directory; menu entry not written"),
            (_, Err(e)) => tracing::warn!("cannot locate own binary: {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_is_quoted_and_escaped() {
        assert_eq!(exec_value(Path::new("/opt/multigit")), "\"/opt/multigit\"");
        assert_eq!(
            exec_value(Path::new("/home/a b/$x/multigit")),
            "\"/home/a b/\\\\$x/multigit\""
        );
    }

    #[test]
    fn entry_has_app_id_icon_marker_and_exec() {
        let entry = desktop_entry(Path::new("/opt/bin/multigit"));
        assert!(entry.contains("\nExec=\"/opt/bin/multigit\"\n"));
        assert!(entry.contains(&format!("StartupWMClass={APP_ID}")));
        assert!(entry.contains(&format!("Icon={ICON_NAME}\n")));
        assert!(entry.contains(MARKER));
        assert_eq!(entry.matches("Exec=").count(), 1);
    }

    #[test]
    fn icon_is_a_256px_png() {
        assert_eq!(&ICON_PNG[1..4], b"PNG");
        // IHDR width and height, big-endian, at bytes 16..24.
        assert_eq!(&ICON_PNG[16..24], &[0, 0, 1, 0, 0, 0, 1, 0]);
    }

    #[test]
    fn register_writes_then_is_unchanged_then_updates() {
        let dir = tempfile::tempdir().unwrap();
        let exec = Path::new("/a/multigit");
        assert_eq!(
            register(dir.path(), exec).unwrap(),
            (Outcome::Written, Outcome::Written)
        );
        assert_eq!(
            register(dir.path(), exec).unwrap(),
            (Outcome::Unchanged, Outcome::Unchanged)
        );
        assert_eq!(
            register(dir.path(), Path::new("/moved/multigit")).unwrap(),
            (Outcome::Written, Outcome::Unchanged)
        );
        let entry =
            std::fs::read_to_string(dir.path().join(format!("applications/{APP_ID}.desktop")))
                .unwrap();
        assert!(entry.contains("Exec=\"/moved/multigit\""));
        let icon = dir
            .path()
            .join(format!("icons/hicolor/256x256/apps/{ICON_NAME}.png"));
        assert_eq!(std::fs::read(icon).unwrap(), ICON_PNG);
    }

    #[cfg(unix)]
    #[test]
    fn a_cellar_binary_registers_the_stable_bin_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path().canonicalize().unwrap();
        let libexec = prefix.join("Cellar/multigit/0.1.0/libexec");
        std::fs::create_dir_all(&libexec).unwrap();
        let exe = libexec.join("multigit");
        std::fs::write(&exe, "").unwrap();
        std::fs::create_dir_all(prefix.join("bin")).unwrap();
        std::os::unix::fs::symlink(
            "../Cellar/multigit/0.1.0/libexec/multigit",
            prefix.join("bin/multigit"),
        )
        .unwrap();
        assert_eq!(stable_brew_path(&exe), Some(prefix.join("bin/multigit")));

        // A bin symlink naming another binary is not ours.
        std::fs::remove_file(prefix.join("bin/multigit")).unwrap();
        std::fs::write(prefix.join("other"), "").unwrap();
        std::os::unix::fs::symlink(prefix.join("other"), prefix.join("bin/multigit")).unwrap();
        assert_eq!(stable_brew_path(&exe), None);
    }

    #[test]
    fn a_binary_outside_a_cellar_has_no_brew_path() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("multigit");
        std::fs::write(&exe, "").unwrap();
        assert_eq!(stable_brew_path(&exe), None);
        // A Cellar without the bin symlink.
        let cellar = dir.path().join("Cellar/multigit/0.1.0/libexec");
        std::fs::create_dir_all(&cellar).unwrap();
        std::fs::write(cellar.join("multigit"), "").unwrap();
        assert_eq!(stable_brew_path(&cellar.join("multigit")), None);
    }

    #[test]
    fn foreign_entry_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("applications/{APP_ID}.desktop"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "[Desktop Entry]\nExec=/custom\n").unwrap();
        assert_eq!(
            register(dir.path(), Path::new("/a/multigit")).unwrap(),
            (Outcome::Foreign, Outcome::Foreign)
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[Desktop Entry]\nExec=/custom\n"
        );
    }
}
