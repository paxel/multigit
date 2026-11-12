//! Reads a sound file the user picked (WAV, OGG, MP3, FLAC) into samples.
//! The app turns them into a plain WAV itself, so the player program never
//! has to know more than WAV.

use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// A notification sound is short: longer files are cut here.
pub const MAX_SECONDS: usize = 10;

/// A decoded sound: one channel, -1 to 1.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    pub samples: Vec<f32>,
    /// Samples per second.
    pub rate: u32,
}

/// Decode the first [`MAX_SECONDS`] of `path`, mixed down to one channel.
pub fn decode(path: &Path) -> Result<Decoded, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }
    let unsupported = |e: Error| format!("not a sound file multigit can read: {e}");
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(unsupported)?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| "no audio in the file".to_owned())?;
    let track_id = track.id;
    let rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| "the file does not say its sample rate".to_owned())?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(unsupported)?;

    let limit = rate as usize * MAX_SECONDS;
    let mut samples = Vec::new();
    // The end of the file ends the loop: as an error of its own kind.
    while samples.len() < limit {
        let Ok(packet) = format.next_packet() else {
            break;
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let channels = spec.channels.count().max(1);
                let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                buffer.copy_interleaved_ref(decoded);
                for frame in buffer.samples().chunks(channels) {
                    samples.push(frame.iter().sum::<f32>() / channels as f32);
                }
            }
            // A damaged packet is skipped.
            Err(Error::DecodeError(_)) => {}
            Err(e) => return Err(unsupported(e)),
        }
    }
    samples.truncate(limit);
    if samples.is_empty() {
        return Err("no audio in the file".to_owned());
    }
    Ok(Decoded { samples, rate })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::audio::{Sound, wav_bytes};

    #[test]
    fn a_wav_file_comes_back_as_the_samples_that_went_in() {
        let dir = tempfile::tempdir().unwrap();
        let original = Sound::Beep.default_samples();
        let path = dir.path().join("beep.wav");
        std::fs::write(&path, wav_bytes(&original, 22_050, 100)).unwrap();
        let decoded = decode(&path).unwrap();
        assert_eq!(decoded.rate, 22_050);
        assert_eq!(decoded.samples.len(), original.len());
        let worst = original
            .iter()
            .zip(&decoded.samples)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(worst < 0.001, "{worst}");
    }

    #[test]
    fn long_files_are_cut_and_other_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let long = dir.path().join("long.wav");
        let rate = 8000;
        std::fs::write(
            &long,
            wav_bytes(&vec![0.5; rate * (MAX_SECONDS + 2)], rate as u32, 100),
        )
        .unwrap();
        assert_eq!(decode(&long).unwrap().samples.len(), rate * MAX_SECONDS);

        let text = dir.path().join("notes.mp3");
        std::fs::write(&text, "not audio at all").unwrap();
        assert!(decode(&text).is_err());
        let empty = dir.path().join("empty.wav");
        std::fs::write(&empty, wav_bytes(&[], 8000, 100)).unwrap();
        assert!(decode(&empty).is_err());
        assert!(decode(&dir.path().join("missing.ogg")).is_err());
    }
}
