//! Starts other programs of the desktop: the default program for a file.

use crate::application::desktop::Desktop;
use crate::domain::audio::player_commands;
use crate::domain::command::terminal_commands;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

/// The folder for files this process hands to other programs; removed when
/// the app exits.
pub fn scratch_dir() -> PathBuf {
    std::env::temp_dir().join(format!("multigit-{}", std::process::id()))
}

/// Write `text` to `name` in the scratch folder `dir`, which is made for the
/// user alone: command output can hold secrets.
pub fn write_scratch_file(dir: &Path, name: &str, text: impl AsRef<[u8]>) -> io::Result<PathBuf> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    let file = dir.join(name);
    std::fs::write(&file, text)?;
    Ok(file)
}

/// The system's "open with the default program" command.
pub fn opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// Start `program` with `args` in `dir` and do not wait for it: no input, no
/// output, reaped on a thread of its own when it ends.
pub fn spawn_detached(program: &OsStr, args: &[&OsStr], dir: Option<&Path>) -> io::Result<()> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Start the first of `commands` whose program exists, in `dir`; `none`
/// says what to do when there is none.
pub fn spawn_first(commands: &[Vec<String>], dir: &Path, none: &str) -> io::Result<()> {
    for command in commands {
        let Some((program, args)) = command.split_first() else {
            continue;
        };
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        match spawn_detached(OsStr::new(program), &args, Some(dir)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            result => return result,
        }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, none))
}

/// Run the first of `commands` whose program exists and wait for it; when
/// `stop` gets set meanwhile, end the program and return.
pub fn run_until(commands: &[Vec<String>], stop: &AtomicBool, none: &str) -> io::Result<()> {
    for command in commands {
        let Some((program, args)) = command.split_first() else {
            continue;
        };
        let spawned = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match spawned {
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            other => other?,
        };
        loop {
            if child.try_wait()?.is_some() {
                return Ok(());
            }
            if stop.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, none))
}

const NO_TERMINAL: &str = "no terminal found; set the terminal command in Settings";
const NO_PLAYER: &str = "no audio player found (pw-play, paplay, aplay or ffplay)";

/// The real desktop.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemDesktop;

impl Desktop for SystemDesktop {
    fn open_file(&self, file: &Path) -> io::Result<()> {
        spawn_detached(OsStr::new(opener()), &[file.as_os_str()], None)
    }

    fn play_wait(&self, file: &Path, stop: &AtomicBool) -> io::Result<()> {
        let commands = player_commands(cfg!(target_os = "macos"), &file.to_string_lossy());
        run_until(&commands, stop, NO_PLAYER)
    }

    fn open_url(&self, url: &str) -> io::Result<()> {
        spawn_detached(OsStr::new(opener()), &[OsStr::new(url)], None)
    }

    fn open_terminal(&self, dir: &Path, configured: &str) -> io::Result<()> {
        let env_terminal = std::env::var("TERMINAL").ok();
        let commands = terminal_commands(
            configured,
            env_terminal.as_deref(),
            cfg!(target_os = "macos"),
            &dir.to_string_lossy(),
        )
        .map_err(io::Error::other)?;
        spawn_first(&commands, dir, NO_TERMINAL)
    }

    fn play(&self, file: &Path) -> io::Result<()> {
        let commands = player_commands(cfg!(target_os = "macos"), &file.to_string_lossy());
        let dir = file.parent().unwrap_or(Path::new("/"));
        spawn_first(&commands, dir, NO_PLAYER)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn spawn_detached_runs_the_program_in_the_folder_without_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let script = "sleep 2; pwd > here";
        let started = Instant::now();
        spawn_detached(
            OsStr::new("sh"),
            &[OsStr::new("-c"), OsStr::new(script)],
            Some(dir.path()),
        )
        .unwrap();
        assert!(started.elapsed() < Duration::from_millis(1500), "waited");
        let file = dir.path().join("here");
        let deadline = Instant::now() + Duration::from_secs(20);
        while !file.exists() || std::fs::read_to_string(&file).unwrap().is_empty() {
            assert!(Instant::now() < deadline, "the program never ran");
            std::thread::sleep(Duration::from_millis(10));
        }
        let here = std::fs::read_to_string(&file).unwrap();
        assert_eq!(
            Path::new(here.trim()).canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn scratch_files_land_in_a_private_folder() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("scratch");
        let file = write_scratch_file(&dir, "a.txt", "text").unwrap();
        assert_eq!(file, dir.join("a.txt"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "text");
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        // A second file goes into the same folder.
        write_scratch_file(&dir, "b.txt", "more").unwrap();
        assert!(scratch_dir().starts_with(std::env::temp_dir()));
    }

    #[test]
    fn the_first_existing_program_of_a_list_is_started() {
        let dir = tempfile::tempdir().unwrap();
        let commands = vec![
            vec![],
            vec!["/nonexistent/multigit-test-terminal".to_owned()],
            vec!["sh".to_owned(), "-c".to_owned(), "touch started".to_owned()],
            vec!["sh".to_owned(), "-c".to_owned(), "touch second".to_owned()],
        ];
        spawn_first(&commands, dir.path(), "none").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !dir.path().join("started").exists() {
            assert!(Instant::now() < deadline, "the program never ran");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!dir.path().join("second").exists());

        let none = spawn_first(&commands[..2], dir.path(), NO_TERMINAL).unwrap_err();
        assert_eq!(none.kind(), io::ErrorKind::NotFound);
        assert!(none.to_string().contains("Settings"), "{none}");
        assert!(NO_PLAYER.contains("pw-play"));
        // A configured command that cannot be parsed never starts anything.
        assert!(SystemDesktop.open_terminal(dir.path(), "term 'x").is_err());
    }

    #[test]
    fn a_program_is_waited_for_and_ended_when_stopped() {
        let sh = |script: &str| vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()];
        let missing = vec!["/nonexistent/multigit-test-player".to_owned()];
        let stop = AtomicBool::new(false);
        // It waits for the program to end by itself.
        let started = Instant::now();
        run_until(&[vec![], missing.clone(), sh("sleep 0.3")], &stop, "none").unwrap();
        assert!(
            started.elapsed() >= Duration::from_millis(250),
            "did not wait"
        );

        // Stopped, it ends a program that would run for a minute.
        let started = Instant::now();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(100));
                stop.store(true, Ordering::SeqCst);
            });
            run_until(&[sh("sleep 60")], &stop, "none").unwrap();
        });
        assert!(started.elapsed() < Duration::from_secs(20), "not stopped");

        let none = run_until(&[missing], &stop, NO_PLAYER).unwrap_err();
        assert_eq!(none.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let missing = OsStr::new("/nonexistent/multigit-test-program");
        assert!(spawn_detached(missing, &[], None).is_err());
        assert!(["open", "xdg-open"].contains(&opener()));
    }
}
