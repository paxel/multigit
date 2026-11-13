//! Plays the Kawaii theme's music: a thread that plays one passage after
//! the other, each made while the one before it plays. It asks for no
//! frame and needs none, so the music goes on in a minimised window.

use crate::application::desktop::Desktop;
use crate::domain::audio::{SAMPLE_RATE, wav_bytes};
use crate::domain::songs::Program;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

/// Writes a passage's WAV file and says where it is.
pub type WriteWav = Arc<dyn Fn(&[u8]) -> io::Result<PathBuf> + Send + Sync>;

/// The running music. Dropping it stops the player.
pub struct Music {
    stop: Arc<AtomicBool>,
    /// Ends the player that runs now: set to stop for good and to go on
    /// at another volume.
    halt: Arc<AtomicBool>,
    volume: Arc<AtomicU8>,
    error: Arc<Mutex<Option<String>>>,
    thread: Option<JoinHandle<()>>,
}

/// A rest of a passage shorter than this many samples is not worth a
/// player of its own.
const MIN_REST: usize = SAMPLE_RATE as usize / 20;

/// The samples that play in `seconds`.
fn samples_in(seconds: f64) -> usize {
    (seconds * f64::from(SAMPLE_RATE)) as usize
}

impl Music {
    /// Start playing what the program drawn from `seed` says: songs with
    /// a break between every two.
    pub fn start(desktop: Arc<dyn Desktop>, write: WriteWav, volume: u8, seed: u64) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let halt = Arc::new(AtomicBool::new(false));
        let volume = Arc::new(AtomicU8::new(volume));
        let error = Arc::new(Mutex::new(None));
        let thread = {
            let (stop, volume, error) = (stop.clone(), volume.clone(), error.clone());
            let halt = halt.clone();
            std::thread::spawn(move || {
                // A passage plays to its end, or to the stop. A change of
                // the volume ends the player; the rest of the passage is
                // then written at the new volume and played from where the
                // old player was, by the time it ran.
                let play = |samples: Vec<f32>| -> io::Result<()> {
                    let mut from = 0;
                    while !stop.load(Ordering::SeqCst) {
                        let Some(rest) = samples.get(from..).filter(|r| r.len() > MIN_REST) else {
                            break;
                        };
                        halt.store(false, Ordering::SeqCst);
                        // The stop may have come between the two lines
                        // above; its halt is gone then, so look again.
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        let wav = wav_bytes(rest, SAMPLE_RATE, volume.load(Ordering::SeqCst));
                        let file = write(&wav)?;
                        let started = Instant::now();
                        desktop.play_wait(&file, &halt)?;
                        if !halt.load(Ordering::SeqCst) {
                            break;
                        }
                        from += samples_in(started.elapsed().as_secs_f64());
                    }
                    Ok(())
                };
                let mut program = Program::new(seed);
                let styles = program.styles();
                let mut ready = program.next_passage().render(styles);
                while !stop.load(Ordering::SeqCst) {
                    // The next passage is made while this one plays, so no
                    // silence the length of the making falls between them.
                    let upcoming = program.next_passage();
                    let making = std::thread::spawn(move || upcoming.render(styles));
                    let played = play(ready);
                    ready = making.join().unwrap_or_default();
                    if let Err(e) = played {
                        *error.lock().unwrap_or_else(|p| p.into_inner()) = Some(e.to_string());
                        break;
                    }
                }
            })
        };
        Self {
            stop,
            halt,
            volume,
            error,
            thread: Some(thread),
        }
    }

    /// The volume in percent. A new one is heard at once: the passage that
    /// plays goes on from where it is.
    pub fn set_volume(&self, volume: u8) {
        if self.volume.swap(volume, Ordering::SeqCst) != volume {
            self.halt.store(true, Ordering::SeqCst);
        }
    }

    /// The volume in percent the music plays at.
    pub fn volume(&self) -> u8 {
        self.volume.load(Ordering::SeqCst)
    }

    /// Why the music stopped by itself, told once.
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap_or_else(|p| p.into_inner()).take()
    }
}

impl Drop for Music {
    /// Stop the player and wait until it is gone.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.halt.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{Duration, Instant};

    /// A player that lets `pass` passages end at once and then holds the
    /// next one until it is stopped.
    struct FakePlayer {
        pass: usize,
        played: Mutex<Vec<usize>>,
        fail: bool,
    }

    impl Desktop for FakePlayer {
        fn open_file(&self, _file: &Path) -> io::Result<()> {
            Ok(())
        }
        fn play(&self, _file: &Path) -> io::Result<()> {
            Ok(())
        }
        fn play_wait(&self, file: &Path, stop: &AtomicBool) -> io::Result<()> {
            if self.fail {
                return Err(io::Error::other("no audio player found"));
            }
            let count = {
                let mut played = self.played.lock().unwrap();
                played.push(std::fs::read(file)?.len());
                played.len()
            };
            while count > self.pass && !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(())
        }
        fn open_url(&self, _url: &str) -> io::Result<()> {
            Ok(())
        }
        fn open_terminal(&self, _dir: &Path, _configured: &str) -> io::Result<()> {
            Ok(())
        }
    }

    fn writer(dir: &Path) -> WriteWav {
        let file = dir.join("music.wav");
        Arc::new(move |wav| std::fs::write(&file, wav).map(|()| file.clone()))
    }

    fn wait(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !done() {
            assert!(Instant::now() < deadline, "timed out: {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn plays_a_song_then_break_and_song_until_it_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let player = Arc::new(FakePlayer {
            pass: 2,
            played: Mutex::default(),
            fail: false,
        });
        let music = Music::start(player.clone(), writer(dir.path()), 50, 7);
        wait("three passages", || {
            player.played.lock().unwrap().len() == 3
        });
        let lengths = player.played.lock().unwrap().clone();
        let seconds = |bytes: usize| (bytes - 44) as f32 / 2.0 / SAMPLE_RATE as f32;
        // A song, then a break, then the next song.
        assert!((54.0..=66.0).contains(&seconds(lengths[0])), "{lengths:?}");
        assert!(seconds(lengths[1]) < 16.0, "{lengths:?}");
        assert!((54.0..=66.0).contains(&seconds(lengths[2])), "{lengths:?}");
        assert_eq!(music.take_error(), None);
        // Dropping stops the passage that is playing and ends the thread.
        drop(music);
        assert_eq!(player.played.lock().unwrap().len(), 3);
    }

    #[test]
    fn a_new_volume_goes_on_with_the_rest_of_the_passage() {
        let dir = tempfile::tempdir().unwrap();
        let player = Arc::new(FakePlayer {
            pass: 0,
            played: Mutex::default(),
            fail: false,
        });
        let music = Music::start(player.clone(), writer(dir.path()), 50, 7);
        let played = || player.played.lock().unwrap().clone();
        wait("the passage", || played().len() == 1);
        // The same volume again is no change.
        music.set_volume(50);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(played().len(), 1);

        music.set_volume(10);
        assert_eq!(music.volume(), 10);
        wait("the rest of the passage", || played().len() == 2);
        let (whole, rest) = (played()[0], played()[1]);
        let lost = (whole - rest) / 2;
        // At least the 300 ms waited above are gone, and not the passage.
        assert!(lost >= samples_in(0.3), "{whole} then {rest}");
        assert!(lost < samples_in(5.0), "{whole} then {rest}");
        // The rest is quieter: the file was written at the new volume.
        let peak = |file: &Path| {
            let wav = std::fs::read(file).unwrap();
            wav[44..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| i16::from_le_bytes(*b).unsigned_abs())
                .max()
                .unwrap()
        };
        let quiet = peak(&dir.path().join("music.wav"));
        assert!(quiet > 0 && quiet < i16::MAX.unsigned_abs() / 9, "{quiet}");
        drop(music);
        assert_eq!(played().len(), 2);
    }

    #[test]
    fn a_player_that_cannot_play_ends_the_music_with_its_error() {
        let dir = tempfile::tempdir().unwrap();
        let player = Arc::new(FakePlayer {
            pass: 0,
            played: Mutex::default(),
            fail: true,
        });
        let music = Music::start(player, writer(dir.path()), 50, 1);
        let mut error = None;
        wait("the error", || {
            error = error.take().or_else(|| music.take_error());
            error.is_some()
        });
        assert_eq!(error.as_deref(), Some("no audio player found"));
        assert_eq!(music.take_error(), None, "told once");
    }
}
