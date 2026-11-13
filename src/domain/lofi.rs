//! The sounds of the Kawaii theme's music, made from nothing but
//! arithmetic: instruments that play one note, drums, a hummed voice, and
//! the lofi finish over a mix. What is played with them is in `songs`.

use crate::domain::audio::SAMPLE_RATE;
use std::f32::consts::TAU;

pub const RATE: f32 = SAMPLE_RATE as f32;

pub fn samples(seconds: f32) -> usize {
    (seconds * RATE) as usize
}

/// Frequency of a MIDI note number.
pub fn pitch(note: u8) -> f32 {
    440.0 * 2f32.powf((f32::from(note) - 69.0) / 12.0)
}

/// Noise that is the same every time: -1 to 1.
pub struct Noise(pub u32);

impl Noise {
    pub fn sample(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
    }
}

/// A plucked string (Karplus-Strong): a burst of noise going round a delay
/// line as long as one period, losing its edge on every round; then the
/// brightness is taken off so it sits behind the drums.
fn pluck(frequency: f32, seconds: f32) -> Vec<f32> {
    let period = ((RATE / frequency) as usize).max(2);
    let mut noise = Noise(frequency.to_bits());
    let mut line: Vec<f32> = (0..period).map(|_| noise.sample()).collect();
    let n = samples(seconds);
    let mut out = Vec::with_capacity(n);
    let mut soft = 0.0_f32;
    for i in 0..n {
        let at = i % period;
        let next = line.get((at + 1) % period).copied().unwrap_or(0.0);
        let here = line.get(at).copied().unwrap_or(0.0);
        soft += 0.25 * (here - soft);
        out.push(soft * (i as f32 / 90.0).min(1.0));
        if let Some(slot) = line.get_mut(at) {
            *slot = (here + next) * 0.5 * 0.997;
        }
    }
    fade_out(&mut out, 0.08);
    out
}

/// A soft struck tone: nearly a pure wave with a short bright start, slow to
/// die, with a slow waver in its loudness.
fn keys(frequency: f32, seconds: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let attack = (t / 0.015).min(1.0);
            let body = (t * frequency * TAU).sin() * (-t * 1.3).exp();
            let bell = (t * frequency * 2.0 * TAU).sin() * 0.25 * (-t * 6.0).exp();
            let waver = 1.0 + 0.12 * (t * 4.0 * TAU).sin();
            (body + bell) * attack * waver * 0.6
        })
        .collect();
    fade_out(&mut out, 0.1);
    out
}

/// A flute-like tone for long notes: it swells in, wavers in pitch once it
/// stands, carries a little breath, and lets go slowly.
fn flute(frequency: f32, seconds: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut noise = Noise(31);
    let mut breath = 0.0_f32;
    let mut phase = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let vibrato = 1.0 + 0.005 * ((t - 0.4).max(0.0) * 3.0).min(1.0) * (t * 4.6 * TAU).sin();
            phase = (phase + frequency * vibrato / RATE).fract();
            let p = phase * TAU;
            let tone = p.sin() + 0.2 * (2.0 * p).sin() + 0.05 * (3.0 * p).sin();
            breath += 0.1 * (noise.sample() - breath);
            let attack = (t / 0.25).min(1.0);
            let release = ((seconds - t) / 0.4).clamp(0.0, 1.0);
            (tone * 0.7 + breath * 0.25) * attack * release
        })
        .collect()
}

/// A pad: three voices a hair apart in pitch, swelling in and out very
/// slowly. For chords held under everything else.
fn pad(frequency: f32, seconds: f32) -> Vec<f32> {
    let n = samples(seconds);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let voices: f32 = [0.996_f32, 1.0, 1.004]
                .iter()
                .map(|detune| (t * frequency * detune * TAU).sin())
                .sum();
            let attack = (t / 0.7).min(1.0);
            let release = ((seconds - t) / 0.9).clamp(0.0, 1.0);
            voices / 3.0 * attack * release
        })
        .collect()
}

/// A low, round bass note.
fn bass(frequency: f32, seconds: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            (t * frequency * TAU).sin() * (t / 0.02).min(1.0) * (-t * 1.5).exp()
        })
        .collect();
    fade_out(&mut out, 0.1);
    out
}

/// A pulse wave, the tone of old game consoles: `duty` is the part of each
/// period it is up. It starts hard and settles a little lower.
fn pulse(frequency: f32, seconds: f32, duty: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut phase = 0.0_f32;
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            phase = (phase + frequency / RATE).fract();
            let wave = if phase < duty { 1.0 } else { -1.0 };
            let level = 0.6 + 0.4 * (-t * 5.0).exp();
            wave * level * (t / 0.004).min(1.0) * 0.32
        })
        .collect();
    fade_out(&mut out, 0.06);
    out
}

/// A triangle wave: the soft, hollow tone of the same consoles.
fn triangle(frequency: f32, seconds: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut phase = 0.0_f32;
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            phase = (phase + frequency / RATE).fract();
            (4.0 * (phase - 0.5).abs() - 1.0) * (t / 0.01).min(1.0) * 0.5
        })
        .collect();
    fade_out(&mut out, 0.12);
    out
}

/// Two pulse waves a hair apart that swell in: a console's held chord.
fn chip_pad(frequency: f32, seconds: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut phases = [0.0_f32; 2];
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let mut sum = 0.0;
            for (phase, detune) in phases.iter_mut().zip([1.0, 1.006]) {
                *phase = (*phase + frequency * detune / RATE).fract();
                sum += if *phase < 0.5 { 1.0 } else { -1.0 };
            }
            let attack = (t / 0.4).min(1.0);
            let release = ((seconds - t) / 0.6).clamp(0.0, 1.0);
            sum * 0.2 * attack * release
        })
        .collect()
}

/// Saw waves at the given multiples of `frequency`, pushed into `drive`
/// until they clip softly, with the edge taken off after: an overdriven
/// amplifier. `ring` is how slowly the note dies.
fn overdriven(frequency: f32, seconds: f32, partials: &[f32], drive: f32, ring: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut phases = vec![0.0_f32; partials.len()];
    let mut soft = 0.0_f32;
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let vibrato = 1.0 + 0.004 * ((t - 0.3).max(0.0) * 4.0).min(1.0) * (t * 5.5 * TAU).sin();
            let mut sum = 0.0;
            for (phase, partial) in phases.iter_mut().zip(partials) {
                *phase = (*phase + frequency * partial * vibrato / RATE).fract();
                sum += *phase * 2.0 - 1.0;
            }
            let pushed = (sum * drive / partials.len().max(1) as f32).tanh();
            soft += 0.3 * (pushed - soft);
            soft * (t / 0.008).min(1.0) * (-t / ring).exp() * 0.45
        })
        .collect();
    fade_out(&mut out, 0.08);
    out
}

/// An instrument that plays one note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice {
    /// A plucked string with its edge taken off: guitar-like.
    Pluck,
    /// A soft struck tone that rings and wavers a little: an electric
    /// piano, far away.
    Keys,
    /// A flute-like tone for long notes.
    Flute,
    /// Three voices a hair apart, swelling slowly: for held chords.
    Pad,
    /// A low, round tone.
    Bass,
    /// A square wave, as old game consoles made them.
    Square,
    /// A thinner pulse wave.
    Pulse,
    /// A triangle wave: the consoles' soft voice and their bass.
    Triangle,
    /// Two pulse waves swelling in: a console's held chord.
    ChipPad,
    /// An overdriven guitar playing one string.
    Guitar,
    /// An overdriven guitar playing a power chord: root, fifth, octave.
    Chords,
    /// An overdriven bass.
    Growl,
    /// A high voice singing "aah".
    Idol,
    /// The same voice singing "ooh".
    IdolOoh,
}

impl Voice {
    /// The MIDI note `midi` for `seconds`.
    pub fn play(self, midi: u8, seconds: f32) -> Vec<f32> {
        let frequency = pitch(midi);
        match self {
            Voice::Pluck => pluck(frequency, seconds),
            Voice::Keys => keys(frequency, seconds),
            Voice::Flute => flute(frequency, seconds),
            Voice::Pad => pad(frequency, seconds),
            Voice::Bass => bass(frequency, seconds),
            Voice::Square => pulse(frequency, seconds, 0.5),
            Voice::Pulse => pulse(frequency, seconds, 0.25),
            Voice::Triangle => triangle(frequency, seconds),
            Voice::ChipPad => chip_pad(frequency, seconds),
            Voice::Guitar => overdriven(frequency, seconds, &[1.0], 5.0, 1.2),
            Voice::Chords => overdriven(frequency, seconds, &[1.0, 1.5, 2.0], 7.0, 2.5),
            Voice::Growl => overdriven(frequency, seconds, &[1.0, 2.0], 3.0, 0.7),
            Voice::Idol => quieter(voice(frequency, seconds, Vowel::Aah)),
            Voice::IdolOoh => quieter(voice(frequency, seconds, Vowel::Ooh)),
        }
    }
}

/// The hummed voice at the level of the other instruments.
fn quieter(mut sound: Vec<f32>) -> Vec<f32> {
    for value in &mut sound {
        *value *= 0.6;
    }
    sound
}

/// A set of drums.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drums {
    /// Slow, soft lofi drums.
    Soft,
    /// A console's drums: a dropping triangle and bursts of coarse noise.
    Chip,
    /// Tight and hard, for fast bass drums.
    Metal,
}

impl Drums {
    pub fn kick(self) -> Vec<f32> {
        match self {
            Drums::Soft => kick(),
            Drums::Chip => chip_kick(),
            Drums::Metal => tight_kick(),
        }
    }

    pub fn snare(self) -> Vec<f32> {
        match self {
            Drums::Soft => snare(),
            Drums::Chip => coarse_noise(0.14, 8, 26.0, 0.6),
            Drums::Metal => hard_snare(),
        }
    }

    pub fn hat(self) -> Vec<f32> {
        match self {
            Drums::Soft => hat(),
            Drums::Chip => coarse_noise(0.03, 2, 110.0, 0.22),
            Drums::Metal => hat(),
        }
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
            let raw = noise.sample();
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
            let raw = noise.sample();
            let bright = raw - previous;
            previous = raw;
            bright * (-t * 90.0).exp() * 0.22
        })
        .collect()
}

/// A console's bass drum: a triangle wave that drops fast.
fn chip_kick() -> Vec<f32> {
    let n = samples(0.16);
    let mut phase = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            phase = (phase + (40.0 + 140.0 * (-t * 30.0).exp()) / RATE).fract();
            (4.0 * (phase - 0.5).abs() - 1.0) * (-t * 14.0).exp() * 0.95
        })
        .collect()
}

/// Noise that holds each value for `hold` samples, as a console makes it,
/// dying at `decay`.
fn coarse_noise(seconds: f32, hold: usize, decay: f32, level: f32) -> Vec<f32> {
    let mut noise = Noise(1983);
    let mut held = 0.0;
    (0..samples(seconds))
        .map(|i| {
            if i % hold.max(1) == 0 {
                held = noise.sample();
            }
            held * (-(i as f32 / RATE) * decay).exp() * level
        })
        .collect()
}

/// A short bass drum with a click in front, so fast ones stay apart.
fn tight_kick() -> Vec<f32> {
    let n = samples(0.18);
    let mut noise = Noise(5);
    let mut phase = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            phase += (48.0 + 130.0 * (-t * 45.0).exp()) / RATE;
            let click = noise.sample() * (-t * 400.0).exp() * 0.3;
            ((phase * TAU).sin() * (-t * 17.0).exp() + click) * 0.75
        })
        .collect()
}

/// A snare with more noise and a longer tail.
fn hard_snare() -> Vec<f32> {
    let n = samples(0.26);
    let mut noise = Noise(11);
    let mut low = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let raw = noise.sample();
            low += 0.2 * (raw - low);
            let hiss = (raw - low) * (-t * 15.0).exp();
            let body = (t * 210.0 * TAU).sin() * (-t * 24.0).exp();
            hiss * 0.6 + body * 0.4
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
pub struct Formant {
    b0: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Formant {
    pub fn new(center: f32, q: f32) -> Self {
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

    pub fn run(&mut self, x: f32) -> f32 {
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
pub fn voice(frequency: f32, seconds: f32, vowel: Vowel) -> Vec<f32> {
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

pub fn fade_out(buffer: &mut [f32], seconds: f32) {
    let n = samples(seconds).min(buffer.len());
    let start = buffer.len() - n;
    for (i, value) in buffer.iter_mut().skip(start).enumerate() {
        *value *= 1.0 - i as f32 / n.max(1) as f32;
    }
}

/// Add `sound` into `mix` from `at` seconds on; what would run past the end
/// is left out.
pub fn add(mix: &mut [f32], at: f32, sound: &[f32], gain: f32) {
    let start = samples(at.max(0.0));
    for (slot, value) in mix.iter_mut().skip(start).zip(sound) {
        *slot += value * gain;
    }
}

/// The lofi finish: the highs taken off, and loud peaks pressed down softly
/// so nothing leaves -1 to 1.
pub fn finish(mix: &mut [f32]) {
    let mut low = 0.0_f32;
    for value in mix.iter_mut() {
        low += 0.38 * (*value - low);
        *value = low / (1.0 + low.abs()) * 1.25;
    }
    fade_out(mix, 0.12);
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

    pub fn check(name: &str, samples: &[f32]) {
        assert!(samples.iter().all(|s| s.is_finite()), "{name}: not finite");
        let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak <= 1.0, "{name}: peak {peak}");
        assert!(peak > 0.1, "{name}: silent, peak {peak}");
    }

    #[test]
    fn instruments_and_drums_sound_and_stay_in_range() {
        check("pluck", &Voice::Pluck.play(57, 1.0));
        check("keys", &Voice::Keys.play(57, 1.0));
        check("flute", &Voice::Flute.play(69, 3.0));
        check("pad", &Voice::Pad.play(57, 3.0));
        check("bass", &Voice::Bass.play(33, 1.0));
        // The dreamy ones come in slowly: quiet at first, full later.
        for slow in [Voice::Flute.play(69, 3.0), Voice::Pad.play(57, 3.0)] {
            let early = slow[..1000].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            let later = slow[20_000..30_000]
                .iter()
                .fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!(early < later / 3.0, "{early} {later}");
            assert!(slow.last().unwrap().abs() < 0.01, "lets go at the end");
        }
        for drums in [Drums::Soft, Drums::Chip, Drums::Metal] {
            check(&format!("{drums:?} kick"), &drums.kick());
            check(&format!("{drums:?} snare"), &drums.snare());
            let hat = drums.hat();
            assert!(hat.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            assert!(hat.len() < drums.snare().len());
        }
        assert_ne!(Drums::Soft.kick(), Drums::Chip.kick());
        assert_ne!(Drums::Soft.kick(), Drums::Metal.kick());
        assert_ne!(Drums::Chip.snare(), Drums::Metal.snare());
        check("ooh", &voice(220.0, 1.5, Vowel::Ooh));
        check("aah", &voice(220.0, 1.5, Vowel::Aah));
        assert_ne!(voice(220.0, 0.5, Vowel::Ooh), voice(220.0, 0.5, Vowel::Aah));
        assert_ne!(Voice::Pluck.play(57, 0.5), Voice::Keys.play(57, 0.5));
        assert!((pitch(69) - 440.0).abs() < 0.01 && (pitch(57) - 220.0).abs() < 0.01);
        // A note ends in silence, so notes can follow each other.
        assert!(Voice::Pluck.play(45, 0.5).last().unwrap().abs() < 0.01);
    }

    #[test]
    fn console_and_overdriven_voices_sound_each_their_own() {
        let voices = [
            Voice::Square,
            Voice::Pulse,
            Voice::Triangle,
            Voice::ChipPad,
            Voice::Guitar,
            Voice::Chords,
            Voice::Growl,
            Voice::Idol,
            Voice::IdolOoh,
        ];
        let sounds: Vec<_> = voices.iter().map(|v| v.play(57, 1.5)).collect();
        for (voice, sound) in voices.iter().zip(&sounds) {
            check(&format!("{voice:?}"), sound);
            let peak = sound.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!(peak <= 0.8, "{voice:?}: {peak}, louder than the soft ones");
            assert_eq!(sound.len(), samples(1.5));
            assert!(sound.last().unwrap().abs() < 0.02, "{voice:?} lets go");
        }
        for (i, a) in sounds.iter().enumerate() {
            for b in sounds.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
        // A square wave is up half the time, the thin pulse a quarter.
        let up =
            |sound: &[f32]| sound.iter().filter(|s| **s > 0.0).count() as f32 / sound.len() as f32;
        assert!((up(&sounds[0]) - 0.5).abs() < 0.05, "{}", up(&sounds[0]));
        assert!((up(&sounds[1]) - 0.25).abs() < 0.05, "{}", up(&sounds[1]));
        // Overdrive flattens the top: much of the guitar sits near its peak.
        let guitar = &sounds[4][..samples(0.2)];
        let top = guitar.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        let near = guitar.iter().filter(|s| s.abs() > top * 0.7).count();
        assert!(near > guitar.len() / 5, "{near} of {}", guitar.len());
    }

    #[test]
    fn the_finish_keeps_a_loud_mix_in_range_and_ends_quiet() {
        let mut mix = vec![3.0_f32; 20_000];
        finish(&mut mix);
        assert!(mix.iter().all(|s| s.abs() <= 1.0));
        assert!(mix.last().unwrap().abs() < 0.01);
        let mut placed = vec![0.0_f32; 30_000];
        add(&mut placed, 1.0, &[1.0, 1.0], 0.5);
        assert_eq!(placed[samples(1.0)], 0.5);
        assert_eq!(placed.iter().filter(|s| **s != 0.0).count(), 2);
    }

    #[test]
    fn shuffled_numbers_differ_and_never_stick_at_zero() {
        let mut seen = std::collections::HashSet::new();
        let mut random = 0;
        for _ in 0..1000 {
            random = shuffle(random);
            assert_ne!(random, 0);
            seen.insert(random);
        }
        assert_eq!(seen.len(), 1000);
    }
}
