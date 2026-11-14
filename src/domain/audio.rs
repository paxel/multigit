//! Sound as plain numbers: the two notification sounds, the WAV file they
//! are handed to a player as, and when a change on a card makes one.

use crate::domain::repo::Snapshot;
use crate::domain::status::Upstream;
use std::f32::consts::TAU;

/// Samples per second of everything the app makes itself.
pub const SAMPLE_RATE: u32 = 22_050;

/// The two notification sounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sound {
    /// A remote commit appeared: the unpulled count went up.
    Beep,
    /// A local commit appeared: the unpushed count went up.
    Boop,
}

impl Sound {
    pub const ALL: [Self; 2] = [Self::Beep, Self::Boop];

    /// As Settings names it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Beep => "Beep (new remote commit)",
            Self::Boop => "Boop (new local commit)",
        }
    }

    /// Its file in the scratch folder.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Beep => "beep.wav",
            Self::Boop => "boop.wav",
        }
    }

    /// The sound the app makes when the user picked no file: two short
    /// notes, rising for the beep and falling for the boop, so the ear
    /// tells them apart.
    pub fn default_samples(self) -> Vec<f32> {
        let notes: [(f32, f32); 2] = match self {
            Self::Beep => [(880.0, 0.09), (1174.7, 0.13)],
            Self::Boop => [(392.0, 0.10), (293.7, 0.17)],
        };
        let mut samples = Vec::new();
        for (frequency, seconds) in notes {
            samples.extend(tone(frequency, seconds));
            samples.extend(silence(0.03));
        }
        samples
    }
}

fn count(seconds: f32) -> usize {
    (seconds * SAMPLE_RATE as f32) as usize
}

pub fn silence(seconds: f32) -> Vec<f32> {
    vec![0.0; count(seconds)]
}

/// A sine note that fades in over a few milliseconds and out over its
/// length, so it neither clicks nor drones.
pub fn tone(frequency: f32, seconds: f32) -> Vec<f32> {
    let n = count(seconds);
    (0..n)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let attack = (t / 0.005).min(1.0);
            let release = 1.0 - i as f32 / n.max(1) as f32;
            (t * frequency * TAU).sin() * attack * release * 0.6
        })
        .collect()
}

/// `samples` (-1 to 1, one channel, `rate` per second) as a 16-bit WAV file,
/// at `volume` percent.
pub fn wav_bytes(samples: &[f32], rate: u32, volume: u8) -> Vec<u8> {
    let gain = f32::from(volume.min(100)) / 100.0;
    let data_len = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + samples.len() * 2);
    wav.extend(b"RIFF");
    wav.extend((36 + data_len).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes()); // PCM
    wav.extend(1u16.to_le_bytes()); // one channel
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 2).to_le_bytes()); // bytes per second
    wav.extend(2u16.to_le_bytes()); // bytes per sample
    wav.extend(16u16.to_le_bytes()); // bits per sample
    wav.extend(b"data");
    wav.extend(data_len.to_le_bytes());
    for sample in samples {
        let value = (sample * gain).clamp(-1.0, 1.0) * f32::from(i16::MAX);
        wav.extend((value as i16).to_le_bytes());
    }
    wav
}

/// The command lines to try, in order, to play the WAV file `file`: the
/// first whose program exists plays it.
pub fn player_commands(macos: bool, file: &str) -> Vec<Vec<String>> {
    let commands: &[&[&str]] = if macos {
        &[&["afplay"]]
    } else {
        &[
            &["pw-play"],
            &["paplay"],
            &["aplay", "-q"],
            &["ffplay", "-nodisp", "-autoexit", "-loglevel", "quiet"],
        ]
    };
    commands
        .iter()
        .map(|c| {
            let mut command: Vec<String> = c.iter().map(|s| (*s).to_owned()).collect();
            command.push(file.to_owned());
            command
        })
        .collect()
}

fn ahead_behind(snapshot: &Snapshot) -> (u32, u32) {
    match snapshot.status.upstream {
        Upstream::Tracking { ahead, behind } => (ahead, behind),
        Upstream::None | Upstream::Gone => (0, 0),
    }
}

/// The sounds a card's change from `before` to `after` asks for: a beep
/// when its unpulled count went up, a boop when its unpushed count did.
pub fn news(before: &Snapshot, after: &Snapshot) -> Vec<Sound> {
    let (ahead_before, behind_before) = ahead_behind(before);
    let (ahead_after, behind_after) = ahead_behind(after);
    let mut sounds = Vec::new();
    if behind_after > behind_before {
        sounds.push(Sound::Beep);
    }
    if ahead_after > ahead_before {
        sounds.push(Sound::Boop);
    }
    sounds
}

/// After a sound, the same sound stays quiet this long: the results of one
/// refresh round come in over some seconds, and twelve cards with news are
/// one beep.
pub const QUIET_SECS: f64 = 5.0;

/// Whether a sound that last played `since` seconds ago (`None`: never) may
/// play again.
pub fn may_play(since: Option<f64>) -> bool {
    since.is_none_or(|since| since >= QUIET_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::status::{Counts, Head, Status};

    fn snap(upstream: Upstream) -> Snapshot {
        Snapshot::of(Status {
            head: Head::Branch("main".into()),
            upstream,
            tracking: None,
            counts: Counts::default(),
            changed_paths: vec![],
        })
    }

    fn tracking(ahead: u32, behind: u32) -> Snapshot {
        snap(Upstream::Tracking { ahead, behind })
    }

    #[test]
    fn default_sounds_are_short_differ_and_stay_in_range() {
        let (beep, boop) = (Sound::Beep.default_samples(), Sound::Boop.default_samples());
        assert_ne!(beep, boop);
        for samples in [&beep, &boop] {
            let seconds = samples.len() as f32 / SAMPLE_RATE as f32;
            assert!((0.2..0.5).contains(&seconds), "{seconds}");
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            assert!(samples.iter().any(|s| s.abs() > 0.3), "audible");
            assert_eq!(samples.first(), Some(&0.0), "no click at the start");
        }
        assert_eq!(Sound::ALL.map(Sound::file_name), ["beep.wav", "boop.wav"]);
        assert!(Sound::Beep.label().starts_with("Beep") && Sound::Boop.label().starts_with("Boop"));
        assert!(silence(0.1).iter().all(|s| *s == 0.0));
    }

    #[test]
    fn wav_file_has_a_header_and_scaled_samples() {
        let wav = wav_bytes(&[0.0, 1.0, -1.0, 2.0], 8000, 50);
        assert_eq!(wav.len(), 44 + 8);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 8000);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 8);
        let sample = |i: usize| i16::from_le_bytes(wav[44 + 2 * i..46 + 2 * i].try_into().unwrap());
        assert_eq!(sample(0), 0);
        assert_eq!(sample(1), i16::MAX / 2, "half volume");
        assert_eq!(sample(2), -(i16::MAX / 2));
        assert_eq!(sample(3), i16::MAX, "too loud is clipped, not wrapped");
        let silent = wav_bytes(&[1.0], 8000, 0);
        assert_eq!(&silent[44..46], &[0, 0]);
        assert_eq!(wav_bytes(&[1.0], 8000, 250), wav_bytes(&[1.0], 8000, 100));
    }

    #[test]
    fn players_get_the_file_as_last_argument() {
        assert_eq!(player_commands(true, "/t/a.wav"), [["afplay", "/t/a.wav"]]);
        let linux = player_commands(false, "/t/a.wav");
        assert_eq!(linux[0], ["pw-play", "/t/a.wav"]);
        assert_eq!(linux[1], ["paplay", "/t/a.wav"]);
        assert_eq!(linux[2], ["aplay", "-q", "/t/a.wav"]);
        assert!(linux.iter().all(|c| c.last().unwrap() == "/t/a.wav"));
    }

    #[test]
    fn a_count_going_up_is_news_and_nothing_else_is() {
        assert_eq!(news(&tracking(0, 0), &tracking(0, 2)), [Sound::Beep]);
        assert_eq!(news(&tracking(1, 0), &tracking(2, 0)), [Sound::Boop]);
        assert_eq!(
            news(&tracking(0, 1), &tracking(1, 3)),
            [Sound::Beep, Sound::Boop]
        );
        // Going down (a pull, a push) and standing still are silent.
        assert!(news(&tracking(2, 2), &tracking(0, 0)).is_empty());
        assert!(news(&tracking(1, 1), &tracking(1, 1)).is_empty());
        // A branch that gets its upstream counts from zero.
        assert_eq!(news(&snap(Upstream::None), &tracking(0, 1)), [Sound::Beep]);
        assert!(news(&tracking(3, 3), &snap(Upstream::Gone)).is_empty());
    }

    #[test]
    fn the_same_sound_waits_out_its_quiet_time() {
        assert!(may_play(None));
        assert!(!may_play(Some(QUIET_SECS - 0.1)));
        assert!(may_play(Some(QUIET_SECS)));
    }
}
