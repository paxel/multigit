//! The Kawaii theme's music, made from nothing but arithmetic: ten short
//! lofi sequences (slow drums, a four-note theme on one or two instruments)
//! and six breaks that lead from one sequence into the next.

use crate::domain::audio::SAMPLE_RATE;
use std::f32::consts::TAU;

const RATE: f32 = SAMPLE_RATE as f32;

fn samples(seconds: f32) -> usize {
    (seconds * RATE) as usize
}

/// Frequency of a MIDI note number.
fn pitch(note: u8) -> f32 {
    440.0 * 2f32.powf((f32::from(note) - 69.0) / 12.0)
}

/// Noise that is the same every time: -1 to 1.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
    }
}

/// What plays the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instrument {
    /// A plucked string: guitar-like.
    Pluck,
    /// Struck, with overtones that die at different speeds: piano-like.
    Keys,
}

/// A plucked string (Karplus-Strong): a burst of noise going round a delay
/// line as long as one period, losing its edge on every round.
fn pluck(frequency: f32, seconds: f32) -> Vec<f32> {
    let period = ((RATE / frequency) as usize).max(2);
    let mut noise = Noise(frequency.to_bits());
    let mut line: Vec<f32> = (0..period).map(|_| noise.next()).collect();
    let n = samples(seconds);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let at = i % period;
        let next = line.get((at + 1) % period).copied().unwrap_or(0.0);
        let here = line.get(at).copied().unwrap_or(0.0);
        out.push(here);
        if let Some(slot) = line.get_mut(at) {
            *slot = (here + next) * 0.5 * 0.996;
        }
    }
    fade_out(&mut out, 0.05);
    out
}

/// A struck note: four overtones, the higher the sooner gone.
fn keys(frequency: f32, seconds: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let attack = (t / 0.006).min(1.0);
            let mut value = 0.0;
            for (k, gain) in [(1.0_f32, 1.0_f32), (2.0, 0.45), (3.0, 0.22), (4.0, 0.1)] {
                let decay = (-t * (1.8 + k * 1.4)).exp();
                value += (t * frequency * k * TAU).sin() * gain * decay;
            }
            value * attack * 0.5
        })
        .collect();
    fade_out(&mut out, 0.05);
    out
}

fn note(instrument: Instrument, midi: u8, seconds: f32) -> Vec<f32> {
    match instrument {
        Instrument::Pluck => pluck(pitch(midi), seconds),
        Instrument::Keys => keys(pitch(midi), seconds),
    }
}

/// Bass drum: a tone that drops from a thump to a hum.
fn kick() -> Vec<f32> {
    let n = samples(0.28);
    let mut phase = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let frequency = 45.0 + 85.0 * (-t * 28.0).exp();
            phase += frequency / RATE;
            (phase * TAU).sin() * (-t * 11.0).exp() * 0.95
        })
        .collect()
}

/// Snare: noise with a short body tone under it.
fn snare() -> Vec<f32> {
    let n = samples(0.2);
    let mut noise = Noise(7);
    let mut low = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            // The noise minus its low part: a crude high-pass.
            let raw = noise.next();
            low += 0.3 * (raw - low);
            let hiss = (raw - low) * (-t * 22.0).exp();
            let body = (t * 185.0 * TAU).sin() * (-t * 30.0).exp();
            hiss * 0.55 + body * 0.35
        })
        .collect()
}

/// Closed hi-hat: a tick of bright noise.
fn hat() -> Vec<f32> {
    let n = samples(0.05);
    let mut noise = Noise(99);
    let mut previous = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let raw = noise.next();
            let bright = raw - previous;
            previous = raw;
            bright * (-t * 90.0).exp() * 0.22
        })
        .collect()
}

/// Which vowel the synthetic voice shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vowel {
    Ooh,
    Aah,
}

/// A band-pass filter (two poles): lets a band around `center` through.
struct Formant {
    b0: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Formant {
    fn new(center: f32, q: f32) -> Self {
        let w = TAU * center / RATE;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            a1: -2.0 * w.cos() / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * (x - self.x2) - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// A hummed vowel: a buzzing tone with a little vibrato, shaped by the two
/// resonances that make an "ooh" or an "aah". No words.
fn voice(frequency: f32, seconds: f32, vowel: Vowel) -> Vec<f32> {
    let (first, second) = match vowel {
        Vowel::Ooh => (320.0, 870.0),
        Vowel::Aah => (730.0, 1090.0),
    };
    let mut formants = [Formant::new(first, 6.0), Formant::new(second, 8.0)];
    let n = samples(seconds);
    let mut phase = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let vibrato = 1.0 + 0.006 * (t * 5.0 * TAU).sin();
            phase = (phase + frequency * vibrato / RATE).fract();
            let buzz = phase * 2.0 - 1.0;
            let shaped: f32 = formants.iter_mut().map(|f| f.run(buzz)).sum();
            let attack = (t / 0.18).min(1.0);
            let release = ((seconds - t) / 0.3).clamp(0.0, 1.0);
            shaped * attack * release * 1.6
        })
        .collect()
}

fn fade_out(buffer: &mut [f32], seconds: f32) {
    let n = samples(seconds).min(buffer.len());
    let start = buffer.len() - n;
    for (i, value) in buffer.iter_mut().skip(start).enumerate() {
        *value *= 1.0 - i as f32 / n.max(1) as f32;
    }
}

/// Add `sound` into `mix` from `at` seconds on; what would run past the end
/// is left out.
fn add(mix: &mut [f32], at: f32, sound: &[f32], gain: f32) {
    let start = samples(at.max(0.0));
    for (slot, value) in mix.iter_mut().skip(start).zip(sound) {
        *slot += value * gain;
    }
}

/// The lofi finish: the highs taken off, and loud peaks pressed down softly
/// so nothing leaves -1 to 1.
fn finish(mix: &mut [f32]) {
    let mut low = 0.0_f32;
    for value in mix.iter_mut() {
        low += 0.38 * (*value - low);
        *value = low / (1.0 + low.abs()) * 1.25;
    }
    fade_out(mix, 0.12);
}

/// One of the ten sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sequence {
    /// Beats per minute.
    pub bpm: u16,
    /// Bars of four beats.
    pub bars: u16,
    /// The four notes of its theme, as MIDI numbers.
    pub theme: [u8; 4],
    pub lead: Instrument,
    /// A second instrument for the low notes, or none.
    pub second: Option<Instrument>,
}

use Instrument::{Keys, Pluck};

/// The ten sequences, each 10 to 30 seconds long.
pub const SEQUENCES: [Sequence; 10] = [
    Sequence {
        bpm: 72,
        bars: 6,
        theme: [57, 60, 64, 67],
        lead: Keys,
        second: Some(Pluck),
    },
    Sequence {
        bpm: 76,
        bars: 6,
        theme: [62, 65, 69, 72],
        lead: Pluck,
        second: Some(Keys),
    },
    Sequence {
        bpm: 70,
        bars: 4,
        theme: [60, 64, 67, 71],
        lead: Keys,
        second: None,
    },
    Sequence {
        bpm: 80,
        bars: 8,
        theme: [55, 58, 62, 65],
        lead: Pluck,
        second: Some(Keys),
    },
    Sequence {
        bpm: 74,
        bars: 6,
        theme: [53, 57, 60, 64],
        lead: Keys,
        second: Some(Pluck),
    },
    Sequence {
        bpm: 68,
        bars: 4,
        theme: [64, 67, 71, 74],
        lead: Pluck,
        second: None,
    },
    Sequence {
        bpm: 78,
        bars: 8,
        theme: [58, 62, 65, 69],
        lead: Keys,
        second: Some(Pluck),
    },
    Sequence {
        bpm: 72,
        bars: 6,
        theme: [59, 62, 66, 69],
        lead: Pluck,
        second: Some(Keys),
    },
    Sequence {
        bpm: 82,
        bars: 8,
        theme: [56, 60, 63, 67],
        lead: Keys,
        second: None,
    },
    Sequence {
        bpm: 66,
        bars: 4,
        theme: [61, 64, 68, 71],
        lead: Keys,
        second: Some(Pluck),
    },
];

impl Sequence {
    fn beat(&self) -> f32 {
        60.0 / f32::from(self.bpm)
    }

    /// Its length in seconds.
    pub fn seconds(&self) -> f32 {
        f32::from(self.bars) * 4.0 * self.beat()
    }

    /// The whole sequence as samples. The last half bar has no hi-hat and
    /// the end fades, so whatever follows starts from quiet.
    pub fn render(&self) -> Vec<f32> {
        let beat = self.beat();
        let eighth = beat / 2.0;
        let mut mix = vec![0.0; samples(self.seconds())];
        let (kick, snare, hat) = (kick(), snare(), hat());
        for bar in 0..self.bars {
            let start = f32::from(bar) * 4.0 * beat;
            let last = bar + 1 == self.bars;
            // The theme: one note a beat; every other bar the second note
            // comes half a beat late.
            let places: [f32; 4] = if bar % 2 == 0 {
                [0.0, 2.0, 4.0, 6.0]
            } else {
                [0.0, 3.0, 4.0, 6.0]
            };
            for (place, midi) in places.into_iter().zip(self.theme) {
                let sound = note(self.lead, midi, beat * 1.6);
                add(&mut mix, start + place * eighth, &sound, 0.5);
            }
            if let Some(second) = self.second {
                let [root, _, fifth, _] = self.theme;
                let low = note(second, root.saturating_sub(12), beat * 2.6);
                add(&mut mix, start, &low, 0.42);
                let mid = note(second, fifth.saturating_sub(12), beat * 1.8);
                add(&mut mix, start + 4.0 * eighth, &mid, 0.3);
            }
            // Slow drums: bass drum on one and just before four, snare on
            // two and four, hi-hats on the eighths with the off ones late.
            for place in [0.0, 5.0] {
                add(&mut mix, start + place * eighth, &kick, 0.8);
            }
            for place in [2.0, 6.0] {
                add(&mut mix, start + place * eighth, &snare, 0.5);
            }
            for step in 0..8u8 {
                if last && step >= 4 {
                    break;
                }
                let swing = if step % 2 == 1 { eighth * 0.3 } else { 0.0 };
                add(
                    &mut mix,
                    start + f32::from(step) * eighth + swing,
                    &hat,
                    0.6,
                );
            }
        }
        finish(&mut mix);
        mix
    }
}

/// A short passage between two sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Break {
    /// Silence, then three of the next sequence's four notes.
    ThreeNotes(Instrument),
    /// Drums alone: `true` is the busier one.
    Drums(bool),
    /// The synthetic voice hums the next sequence's first note.
    Voice(Vowel),
}

/// The six breaks.
pub const BREAKS: [Break; 6] = [
    Break::ThreeNotes(Keys),
    Break::ThreeNotes(Pluck),
    Break::Drums(false),
    Break::Drums(true),
    Break::Voice(Vowel::Ooh),
    Break::Voice(Vowel::Aah),
];

impl Break {
    /// The break as samples, in the tempo and with the notes of `next`, the
    /// sequence it leads into. It starts and ends with a rest.
    pub fn render(&self, next: &Sequence) -> Vec<f32> {
        let beat = next.beat();
        let mut mix = vec![0.0; samples(beat * 4.0)];
        match *self {
            Break::ThreeNotes(instrument) => {
                for (i, midi) in next.theme.into_iter().take(3).enumerate() {
                    let sound = note(instrument, midi, beat * 1.4);
                    add(&mut mix, beat * (1.0 + i as f32 * 0.75), &sound, 0.5);
                }
            }
            Break::Drums(busy) => {
                let (kick, snare) = (kick(), snare());
                add(&mut mix, beat * 0.5, &kick, 0.8);
                add(&mut mix, beat * 1.5, &snare, 0.5);
                add(&mut mix, beat * 2.25, &kick, 0.8);
                if busy {
                    // A roll that grows into the last hit.
                    for i in 0..6u8 {
                        let at = beat * (2.5 + f32::from(i) * 0.125);
                        add(&mut mix, at, &snare, 0.15 + f32::from(i) * 0.06);
                    }
                } else {
                    add(&mut mix, beat * 3.0, &snare, 0.5);
                }
            }
            Break::Voice(vowel) => {
                let [root, third, ..] = next.theme;
                let hum = voice(pitch(root), beat * 1.8, vowel);
                add(&mut mix, beat * 0.6, &hum, 0.5);
                if vowel == Vowel::Aah {
                    let up = voice(pitch(third), beat * 1.0, vowel);
                    add(&mut mix, beat * 2.5, &up, 0.4);
                }
            }
        }
        finish(&mut mix);
        mix
    }
}

/// Which sequence plays after `previous`, picked by `random`: any of the
/// ten but never the same one twice in a row.
pub fn next_sequence(previous: Option<usize>, random: u64) -> usize {
    let count = SEQUENCES.len();
    match previous {
        None => (random % count as u64) as usize,
        Some(previous) => {
            let step = 1 + (random % (count as u64 - 1)) as usize;
            (previous + step) % count
        }
    }
}

/// Which break plays, picked by `random`.
pub fn next_break(random: u64) -> Break {
    let index = (random % BREAKS.len() as u64) as usize;
    BREAKS.get(index).copied().unwrap_or(Break::Drums(false))
}

/// The next of a row of numbers that look random (xorshift); never zero
/// after a seed that is not zero.
pub fn shuffle(state: u64) -> u64 {
    let mut x = state.max(1);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(name: &str, samples: &[f32]) {
        assert!(samples.iter().all(|s| s.is_finite()), "{name}: not finite");
        let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak <= 1.0, "{name}: peak {peak}");
        assert!(peak > 0.1, "{name}: silent, peak {peak}");
    }

    #[test]
    fn instruments_and_drums_sound_and_stay_in_range() {
        check("pluck", &pluck(220.0, 1.0));
        check("keys", &keys(220.0, 1.0));
        check("kick", &kick());
        check("snare", &snare());
        assert!(hat().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        check("ooh", &voice(220.0, 1.5, Vowel::Ooh));
        check("aah", &voice(220.0, 1.5, Vowel::Aah));
        assert_ne!(voice(220.0, 0.5, Vowel::Ooh), voice(220.0, 0.5, Vowel::Aah));
        assert_ne!(pluck(220.0, 0.5), keys(220.0, 0.5));
        assert!((pitch(69) - 440.0).abs() < 0.01 && (pitch(57) - 220.0).abs() < 0.01);
        // A note ends in silence, so notes can follow each other.
        assert!(pluck(110.0, 0.5).last().unwrap().abs() < 0.01);
    }

    #[test]
    fn ten_sequences_of_ten_to_thirty_seconds_each_their_own() {
        assert_eq!(SEQUENCES.len(), 10);
        let mut themes: Vec<_> = SEQUENCES.iter().map(|s| s.theme).collect();
        themes.sort_unstable();
        themes.dedup();
        assert_eq!(themes.len(), 10);
        assert!(SEQUENCES.iter().any(|s| s.second.is_none()));
        assert!(
            SEQUENCES.iter().any(|s| s.lead == Pluck) && SEQUENCES.iter().any(|s| s.lead == Keys)
        );
        for (i, sequence) in SEQUENCES.iter().enumerate() {
            let seconds = sequence.seconds();
            assert!((10.0..=30.0).contains(&seconds), "{i}: {seconds} s");
            assert_eq!(sequence.bars % 2, 0, "{i}: whole phrases");
        }
    }

    #[test]
    fn sequences_render_to_their_length_in_range_and_end_quiet() {
        // Two of the ten, one with and one without a second instrument;
        // rendering all of them takes too long for a unit test.
        for index in [2, 9] {
            let sequence = SEQUENCES[index];
            let rendered = sequence.render();
            check(&format!("sequence {index}"), &rendered);
            let expected = (sequence.seconds() * RATE) as usize;
            assert_eq!(rendered.len(), expected);
            let tail = &rendered[rendered.len() - 200..];
            assert!(tail.iter().all(|s| s.abs() < 0.05), "{index}: loud end");
            assert_eq!(rendered, sequence.render(), "the same every time");
        }
    }

    #[test]
    fn six_breaks_render_short_in_range_and_start_and_end_on_a_rest() {
        assert_eq!(BREAKS.len(), 6);
        let next = SEQUENCES[0];
        let mut rendered: Vec<Vec<f32>> = Vec::new();
        for (i, a_break) in BREAKS.iter().enumerate() {
            let samples = a_break.render(&next);
            check(&format!("break {i}"), &samples);
            let seconds = samples.len() as f32 / RATE;
            assert!((2.0..5.0).contains(&seconds), "{i}: {seconds} s");
            assert!(
                samples[..2000].iter().all(|s| s.abs() < 0.02),
                "{i}: no rest first"
            );
            assert!(
                samples[samples.len() - 200..]
                    .iter()
                    .all(|s| s.abs() < 0.05),
                "{i}"
            );
            rendered.push(samples);
        }
        rendered.dedup();
        assert_eq!(rendered.len(), 6, "all different");
    }

    #[test]
    fn the_order_is_random_but_never_the_same_sequence_twice_in_a_row() {
        let mut random = 12345;
        let mut previous = None;
        let mut seen = std::collections::HashSet::new();
        let mut breaks = std::collections::HashSet::new();
        for _ in 0..500 {
            random = shuffle(random);
            let next = next_sequence(previous, random);
            assert!(next < SEQUENCES.len());
            assert_ne!(Some(next), previous);
            seen.insert(next);
            breaks.insert(format!("{:?}", next_break(random)));
            previous = Some(next);
        }
        assert_eq!(seen.len(), 10, "every sequence comes up");
        assert_eq!(breaks.len(), 6, "every break comes up");
        assert_ne!(shuffle(0), 0);
    }
}
