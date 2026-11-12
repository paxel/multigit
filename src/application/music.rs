//! Plays the Kawaii theme's music: a thread that makes one passage after
//! the other and waits for the player to finish each. It asks for no frame
//! and needs none, so the music goes on in a minimised window.

use crate::application::desktop::Desktop;
use crate::domain::audio::{SAMPLE_RATE, wav_bytes};
use crate::domain::lofi::{SEQUENCES, next_break, next_sequence, shuffle};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Writes a passage's WAV file and says where it is.
pub type WriteWav = Arc<dyn Fn(&[u8]) -> io::Result<PathBuf> + Send + Sync>;

/// The running music. Dropping it stops the player.
pub struct Music {
    stop: Arc<AtomicBool>,
    volume: Arc<AtomicU8>,
    error: Arc<Mutex<Option<String>>>,
    thread: Option<JoinHandle<()>>,
}

impl Music {
    /// Start playing: sequences in an order drawn from `seed`, a break
    /// between every two.
    pub fn start(desktop: Arc<dyn Desktop>, write: WriteWav, volume: u8, seed: u64) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let volume = Arc::new(AtomicU8::new(volume));
        let error = Arc::new(Mutex::new(None));
        let thread = {
            let (stop, volume, error) = (stop.clone(), volume.clone(), error.clone());
            std::thread::spawn(move || {
                let play = |samples: Vec<f32>| -> io::Result<()> {
                    if stop.load(Ordering::SeqCst) {
                        return Ok(());
                    }
                    let wav = wav_bytes(&samples, SAMPLE_RATE, volume.load(Ordering::SeqCst));
                    desktop.play_wait(&write(&wav)?, &stop)
                };
                let mut random = seed;
                let mut previous = None;
                while !stop.load(Ordering::SeqCst) {
                    random = shuffle(random);
                    let index = next_sequence(previous, random);
                    let Some(sequence) = SEQUENCES.get(index) else {
                        break;
                    };
                    // A break leads into every sequence but the first. Both
                    // are made before either plays, so no silence the length
                    // of the making falls between them.
                    let lead_in = previous.map(|_| next_break(random >> 8).render(sequence));
                    let rendered = sequence.render();
                    let played = lead_in.map_or(Ok(()), play).and_then(|()| play(rendered));
                    if let Err(e) = played {
                        *error.lock().unwrap_or_else(|p| p.into_inner()) = Some(e.to_string());
                        break;
                    }
                    previous = Some(index);
                }
            })
        };
        Self {
            stop,
            volume,
            error,
            thread: Some(thread),
        }
    }

    /// The volume in percent, from the next passage on.
    pub fn set_volume(&self, volume: u8) {
        self.volume.store(volume, Ordering::SeqCst);
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
    fn plays_a_sequence_then_break_and_sequence_until_it_is_dropped() {
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
        // A sequence, then a short break, then the next sequence.
        assert!((10.0..=30.0).contains(&seconds(lengths[0])), "{lengths:?}");
        assert!(seconds(lengths[1]) < 5.0, "{lengths:?}");
        assert!((10.0..=30.0).contains(&seconds(lengths[2])), "{lengths:?}");
        music.set_volume(10);
        assert_eq!(music.take_error(), None);
        // Dropping stops the passage that is playing and ends the thread.
        drop(music);
        assert_eq!(player.played.lock().unwrap().len(), 3);
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
