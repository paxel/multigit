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

/// A banjo: a plucked string like `pluck`, but bright, quick to die, and
/// with the knock of the skin it is strung over.
fn banjo(frequency: f32, seconds: f32) -> Vec<f32> {
    let period = ((RATE / frequency) as usize).max(2);
    let mut noise = Noise(frequency.to_bits() ^ 0x000B_A210);
    let mut line: Vec<f32> = (0..period).map(|_| noise.sample()).collect();
    let n = samples(seconds);
    let mut out = Vec::with_capacity(n);
    let mut soft = 0.0_f32;
    for i in 0..n {
        let t = i as f32 / RATE;
        let at = i % period;
        let next = line.get((at + 1) % period).copied().unwrap_or(0.0);
        let here = line.get(at).copied().unwrap_or(0.0);
        soft += 0.7 * (here - soft);
        let knock = (t * 190.0 * TAU).sin() * (-t * 70.0).exp() * 0.5;
        out.push((soft * 0.7 + knock) * (i as f32 / 20.0).min(1.0));
        if let Some(slot) = line.get_mut(at) {
            *slot = (here + next) * 0.5 * 0.988;
        }
    }
    fade_out(&mut out, 0.05);
    out
}

/// A fiddle: a bowed string, rich in overtones, that comes in with the
/// bow, wavers in pitch once it stands and carries a little of the bow's
/// scrape.
fn fiddle(frequency: f32, seconds: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut noise = Noise(2718);
    let mut scrape = 0.0_f32;
    let mut phase = 0.0_f32;
    // Overtones above the half of the sample rate would fold back.
    let overtones = ((RATE * 0.45 / frequency) as usize).clamp(1, 8);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let vibrato = 1.0 + 0.007 * ((t - 0.2).max(0.0) * 4.0).min(1.0) * (t * 5.6 * TAU).sin();
            phase = (phase + frequency * vibrato / RATE).fract();
            let p = phase * TAU;
            let mut tone = 0.0;
            for k in 1..=overtones {
                let k = k as f32;
                tone += (k * p).sin() / k;
            }
            let raw = noise.sample();
            scrape += 0.5 * (raw - scrape);
            let attack = (t / 0.06).min(1.0);
            let release = ((seconds - t) / 0.12).clamp(0.0, 1.0);
            (tone * 0.36 + (raw - scrape) * 0.03) * attack * release
        })
        .collect()
}

/// A washtub bass: one string on a broomstick over a tub. The note comes
/// down to its pitch with a boing and dies in the tub.
fn washtub(frequency: f32, seconds: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut phase = 0.0_f32;
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let bent = frequency * (1.0 + 0.35 * (-t * 18.0).exp());
            phase = (phase + bent / RATE).fract();
            let p = phase * TAU;
            let tone = p.sin() + 0.35 * (2.0 * p).sin() + 0.1 * (3.0 * p).sin();
            tone * 0.62 * (t / 0.008).min(1.0) * (-t * 3.2).exp()
        })
        .collect();
    fade_out(&mut out, 0.1);
    out
}

/// A jaw harp: one low buzzing tongue, and a mouth that opens and closes
/// around it `wahs` times a second, which is all the tune it has.
pub fn jaw_harp(frequency: f32, seconds: f32, wahs: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut phase = 0.0_f32;
    let (mut low, mut band) = (0.0_f32, 0.0_f32);
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            phase = (phase + frequency / RATE).fract();
            let buzz = if phase < 0.12 { 1.0 } else { -0.14 };
            // The mouth: a resonance that moves between 500 and 1900 Hz.
            let mouth = 500.0 + 1400.0 * (t * wahs * TAU / 2.0).sin().abs();
            let f = 2.0 * (std::f32::consts::PI * mouth / RATE).sin();
            low += f * band;
            let high = buzz - low - 0.12 * band;
            band += f * high;
            let twang = band * 0.22 + buzz * 0.06;
            twang / (1.0 + twang.abs()) * (t / 0.004).min(1.0) * (-t * 2.4).exp()
        })
        .collect();
    fade_out(&mut out, 0.08);
    out
}

/// A theremin: nearly a pure tone that slides to its note from `from`,
/// the pitch the hand was at, and wavers widely and slowly once it is there.
fn theremin(from: f32, frequency: f32, seconds: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut phase = 0.0_f32;
    let interval = (from / frequency).max(0.01);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let slide = interval.powf((-t / 0.07).exp());
            let vibrato =
                1.0 + 0.013 * ((t - 0.15).max(0.0) * 3.0).min(1.0) * (t * 5.0 * TAU).sin();
            phase = (phase + frequency * slide * vibrato / RATE).fract();
            let p = phase * TAU;
            let tone = p.sin() + 0.12 * (2.0 * p).sin() + 0.04 * (3.0 * p).sin();
            let attack = (t / 0.09).min(1.0);
            let release = ((seconds - t) / 0.18).clamp(0.0, 1.0);
            tone * 0.55 * attack * release
        })
        .collect()
}

/// Dark keys: an electric piano with its highs taken off and a slow
/// tremolo.
fn dark_keys(frequency: f32, seconds: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut soft = 0.0_f32;
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let body = (t * frequency * TAU).sin() * (-t * 0.9).exp();
            let warm = (t * frequency * 2.0 * TAU).sin() * 0.3 * (-t * 3.0).exp();
            let tremolo = 1.0 + 0.18 * (t * 3.2 * TAU).sin();
            soft += 0.3 * ((body + warm) * tremolo - soft);
            soft * (t / 0.02).min(1.0) * 0.6
        })
        .collect();
    fade_out(&mut out, 0.15);
    out
}

/// A sub bass: deep, long, with just enough above it to be heard on small
/// speakers.
fn sub(frequency: f32, seconds: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let p = t * frequency * TAU;
            let tone = p.sin() + 0.35 * (2.0 * p).sin() + 0.1 * (3.0 * p).sin();
            tone * 0.62 * (t / 0.03).min(1.0) * (-t * 0.9).exp()
        })
        .collect();
    fade_out(&mut out, 0.12);
    out
}

/// A room: `dry` with its echo, which dies away over about `decay`
/// seconds. `wet` from 0 (the sound alone) to 1 (its echo alone, as from
/// far away). As long as `dry`: a tail needs silence at the end of `dry`.
pub fn reverb(dry: &[f32], decay: f32, wet: f32) -> Vec<f32> {
    struct Line {
        held: Vec<f32>,
        feedback: f32,
        dull: f32,
    }
    let line = |seconds: f32, feedback: f32| Line {
        held: vec![0.0; samples(seconds).max(1)],
        feedback,
        dull: 0.0,
    };
    // Four echoes of different lengths side by side, then two short ones
    // that smear them (Schroeder's reverberator).
    let mut combs: Vec<Line> = [0.0297_f32, 0.0371, 0.0411, 0.0437]
        .into_iter()
        .map(|seconds| line(seconds, 10f32.powf(-3.0 * seconds / decay.max(0.05))))
        .collect();
    let mut smears = [line(0.005, 0.7), line(0.0017, 0.7)];
    dry.iter()
        .enumerate()
        .map(|(i, x)| {
            let mut sum = 0.0;
            for comb in &mut combs {
                let at = i % comb.held.len();
                if let Some(slot) = comb.held.get_mut(at) {
                    let out = *slot;
                    // Every round loses a little of its highs, as in air.
                    comb.dull += 0.45 * (out - comb.dull);
                    *slot = x + comb.dull * comb.feedback;
                    sum += out;
                }
            }
            let mut y = sum / 4.0;
            for smear in &mut smears {
                let at = i % smear.held.len();
                if let Some(slot) = smear.held.get_mut(at) {
                    let out = *slot - smear.feedback * y;
                    *slot = y + smear.feedback * out;
                    y = out;
                }
            }
            x * (1.0 - wet) + y * wet
        })
        .collect()
}

/// `sound` as heard from far away: its echo alone, with five seconds for
/// it to die.
pub fn distant(sound: &[f32]) -> Vec<f32> {
    let mut long = sound.to_vec();
    long.resize(sound.len() + samples(5.0), 0.0);
    let mut far = reverb(&long, 5.0, 1.0);
    fade_out(&mut far, 1.0);
    far
}

/// The crackle of a record: clicks at no fixed time, about fourteen a
/// second, over a faint hiss. `seed` decides where they fall.
pub fn crackle(seconds: f32, seed: u64) -> Vec<f32> {
    let mut noise = Noise((seed as u32) | 1);
    let (mut click, mut hiss) = (0.0_f32, 0.0_f32);
    let chance = 1.0 - 2.0 * 14.0 / RATE;
    (0..samples(seconds))
        .map(|i| {
            let t = i as f32 / RATE;
            if noise.sample() > chance {
                click = 0.3 + 0.7 * noise.sample();
            }
            click *= 0.7;
            hiss += 0.2 * (noise.sample() - hiss);
            (click + hiss * 0.1) * (t / 0.05).min(1.0)
        })
        .collect()
}

/// A bomb, close by: a deep thump and a rumble that rolls on. `distant`
/// puts it where it belongs. `seed` makes each one its own.
pub fn bomb(seed: u64) -> Vec<f32> {
    let n = samples(2.6);
    let mut noise = Noise((seed as u32) | 1);
    let (mut one, mut two) = (0.0_f32, 0.0_f32);
    let mut phase = 0.0_f32;
    let depth = 30.0 + (seed % 9) as f32;
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            phase += (depth + 40.0 * (-t * 5.0).exp()) / RATE;
            let thump = (phase * TAU).sin() * (-t * 2.0).exp() * (t / 0.02).min(1.0);
            one += 0.03 * (noise.sample() - one);
            two += 0.03 * (one - two);
            let rumble = two * 9.0 * (-t * 1.3).exp();
            let both = thump * 0.8 + rumble;
            both / (1.0 + both.abs()) * 1.3
        })
        .collect();
    fade_out(&mut out, 0.3);
    out
}

/// The call of something big and far off: a low moan like a horn that
/// rises, breaks and sinks below where it began. `distant` puts it where
/// it belongs.
pub fn far_call(seed: u64) -> Vec<f32> {
    let seconds = 3.8;
    let n = samples(seconds);
    let base = 78.0 + (seed % 5) as f32 * 4.0;
    let mut formants = [Formant::new(310.0, 4.0), Formant::new(720.0, 5.0)];
    let mut phases = [0.0_f32; 2];
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let u = t / seconds;
            let rise = (u * std::f32::consts::PI).sin().max(0.0).powf(1.5);
            let frequency = base * (1.0 + 0.55 * rise) * (1.0 - 0.22 * u);
            let mut buzz = 0.0;
            for (phase, (ratio, level)) in phases.iter_mut().zip([(1.0, 1.0), (1.498, 0.4)]) {
                *phase = (*phase + frequency * ratio / RATE).fract();
                buzz += (*phase * 2.0 - 1.0) * level;
            }
            let shaped: f32 = formants.iter_mut().map(|f| f.run(buzz)).sum();
            let flutter = 1.0 + 0.25 * (t * 7.5 * TAU).sin();
            let attack = (t / 0.6).min(1.0);
            let release = ((seconds - t) / 1.2).clamp(0.0, 1.0);
            let moan = shaped * flutter * 1.8;
            moan / (1.0 + moan.abs()) * attack * release
        })
        .collect()
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
    /// A banjo: bright, short, with the knock of its skin.
    Banjo,
    /// A fiddle: a bowed string.
    Fiddle,
    /// A washtub bass: every note a boing.
    Washtub,
    /// A theremin: it slides from note to note.
    Theremin,
    /// A dark electric piano.
    DarkKeys,
    /// A sub bass.
    Sub,
}

impl Voice {
    /// The MIDI note `midi` for `seconds`.
    pub fn play(self, midi: u8, seconds: f32) -> Vec<f32> {
        self.play_after(None, midi, seconds)
    }

    /// The note `midi` for `seconds`, played right after the note
    /// `before`. Only an instrument that slides from note to note plays it
    /// differently then: without a note before, it comes up from two
    /// semitones below.
    pub fn play_after(self, before: Option<u8>, midi: u8, seconds: f32) -> Vec<f32> {
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
            // The melody voice sings an octave above what is written: a
            // high voice. The "ooh" of the long tunes stays where it is; that
            // high it would have no body left.
            Voice::Idol => quieter(voice(frequency * 2.0, seconds, Vowel::Aah)),
            Voice::IdolOoh => quieter(voice(frequency, seconds, Vowel::Ooh)),
            Voice::Banjo => banjo(frequency, seconds),
            Voice::Fiddle => fiddle(frequency, seconds),
            Voice::Washtub => washtub(frequency, seconds),
            Voice::Theremin => {
                let from = before.map_or(frequency * 0.89, pitch);
                theremin(from, frequency, seconds)
            }
            Voice::DarkKeys => dark_keys(frequency, seconds),
            Voice::Sub => sub(frequency, seconds),
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
    /// No drums at all: a foot on the porch and a washboard.
    Porch,
    /// A slow, heavy beat off an old record: a dull bass drum, a rim shot.
    Dusty,
}

impl Drums {
    pub fn kick(self) -> Vec<f32> {
        match self {
            Drums::Soft => kick(),
            Drums::Chip => chip_kick(),
            Drums::Metal => tight_kick(),
            Drums::Porch => stomp(),
            Drums::Dusty => dusty_kick(),
        }
    }

    pub fn snare(self) -> Vec<f32> {
        match self {
            Drums::Soft => snare(),
            Drums::Chip => coarse_noise(0.14, 8, 26.0, 0.6),
            Drums::Metal => hard_snare(),
            Drums::Porch => washboard(0.16, 0.62),
            Drums::Dusty => rim_shot(),
        }
    }

    pub fn hat(self) -> Vec<f32> {
        match self {
            Drums::Soft => hat(),
            Drums::Chip => coarse_noise(0.03, 2, 110.0, 0.22),
            Drums::Metal => hat(),
            Drums::Porch => washboard(0.05, 0.3),
            Drums::Dusty => dull_hat(),
        }
    }
}

/// A foot stomping on wooden boards: a dull thud and the boards' rattle.
fn stomp() -> Vec<f32> {
    let n = samples(0.24);
    let mut noise = Noise(1865);
    let mut rattle = 0.0_f32;
    let mut phase = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let frequency = 52.0 + 40.0 * (-t * 40.0).exp();
            phase += frequency / RATE;
            rattle += 0.12 * (noise.sample() - rattle);
            let thud = (phase * TAU).sin() * (-t * 15.0).exp();
            thud * 0.85 + rattle * (-t * 35.0).exp() * 0.5
        })
        .collect()
}

/// A thimble drawn over a washboard for `seconds`: bright noise that comes
/// in ridges.
fn washboard(seconds: f32, level: f32) -> Vec<f32> {
    let n = samples(seconds);
    let mut noise = Noise(1927);
    let mut low = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let raw = noise.sample();
            low += 0.35 * (raw - low);
            let ridges = 0.35 + 0.65 * (t * 70.0 * TAU).sin().max(0.0);
            (raw - low) * ridges * (-t * 3.0 / seconds).exp() * level
        })
        .collect()
}

/// A bass drum off an old record: deep, long and dull.
fn dusty_kick() -> Vec<f32> {
    let n = samples(0.36);
    let mut noise = Noise(1994);
    let mut dust = 0.0_f32;
    let mut phase = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let frequency = 42.0 + 70.0 * (-t * 22.0).exp();
            phase += frequency / RATE;
            dust += 0.2 * (noise.sample() - dust);
            (phase * TAU).sin() * (-t * 8.0).exp() * 0.92 + dust * (-t * 20.0).exp() * 0.08
        })
        .collect()
}

/// A rim shot: the stick on the drum's edge, a short woody click.
fn rim_shot() -> Vec<f32> {
    let n = samples(0.1);
    let mut noise = Noise(1998);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let wood = (t * 1650.0 * TAU).sin() * 0.5 + (t * 410.0 * TAU).sin() * 0.35;
            wood * (-t * 70.0).exp() + noise.sample() * 0.25 * (-t * 110.0).exp()
        })
        .collect()
}

/// A hi-hat with a cloth over it.
fn dull_hat() -> Vec<f32> {
    let n = samples(0.035);
    let mut noise = Noise(1991);
    let mut soft = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            soft += 0.5 * (noise.sample() - soft);
            soft * (-t * 90.0).exp() * 0.16
        })
        .collect()
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
        for drums in [
            Drums::Soft,
            Drums::Chip,
            Drums::Metal,
            Drums::Porch,
            Drums::Dusty,
        ] {
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

    /// How much of `sound` is a tone of `frequency`.
    fn tone(sound: &[f32], frequency: f32) -> f32 {
        let (mut sine, mut cosine) = (0.0_f32, 0.0_f32);
        for (i, value) in sound.iter().enumerate() {
            let angle = i as f32 / RATE * frequency * TAU;
            sine += value * angle.sin();
            cosine += value * angle.cos();
        }
        (sine * sine + cosine * cosine).sqrt() / sound.len() as f32
    }

    fn loudest(sound: &[f32]) -> f32 {
        sound.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn the_porch_band_sounds_like_a_banjo_a_fiddle_a_tub_and_a_jaw_harp() {
        let voices = [Voice::Banjo, Voice::Fiddle, Voice::Washtub];
        for voice in voices {
            let sound = voice.play(57, 1.5);
            check(&format!("{voice:?}"), &sound);
            assert!(loudest(&sound) <= 0.9, "{voice:?}: {}", loudest(&sound));
            assert_eq!(sound.len(), samples(1.5));
            assert!(sound.last().unwrap().abs() < 0.02, "{voice:?} lets go");
            // In tune: more of the note than of the note beside it.
            let in_tune = tone(&sound, pitch(57));
            assert!(in_tune > 2.0 * tone(&sound, pitch(58)), "{voice:?}");
        }
        // A banjo is gone long before a plucked guitar-like string is.
        let late = |voice: Voice| loudest(&voice.play(57, 1.5)[samples(0.8)..samples(1.2)]);
        let early = |voice: Voice| loudest(&voice.play(57, 1.5)[..samples(0.1)]);
        assert!(late(Voice::Banjo) < early(Voice::Banjo) * 0.2);
        assert!(
            late(Voice::Banjo) / early(Voice::Banjo) < late(Voice::Pluck) / early(Voice::Pluck)
        );
        // And brighter: more of its third overtone.
        let bright = |voice: Voice| {
            let sound = voice.play(57, 0.3);
            tone(&sound, 3.0 * pitch(57)) / tone(&sound, pitch(57))
        };
        assert!(bright(Voice::Banjo) > bright(Voice::Pluck));
        // The fiddle comes in with the bow and holds its note to the end.
        let bowed = Voice::Fiddle.play(69, 2.0);
        assert!(loudest(&bowed[..200]) < loudest(&bowed[samples(0.5)..samples(1.0)]) / 3.0);
        assert!(loudest(&bowed[samples(1.5)..samples(1.8)]) > 0.2);
        // The tub's note starts above its pitch and comes down.
        let boing = Voice::Washtub.play(33, 1.0);
        let start = &boing[..samples(0.04)];
        assert!(tone(start, pitch(33) * 1.3) > tone(start, pitch(33) * 0.8));
        // The jaw harp twangs: its loudness in the mouth's band goes up
        // and down, while its buzz stays on one note.
        let harp = jaw_harp(98.0, 1.0, 4.0);
        check("jaw harp", &harp);
        assert_eq!(harp.len(), samples(1.0));
        assert!(harp.last().unwrap().abs() < 0.02);
        let open = tone(&harp[samples(0.10)..samples(0.15)], 1764.0);
        let closed = tone(&harp[samples(0.0)..samples(0.04)], 1764.0);
        assert!(open > closed, "{open} {closed}");
        assert_ne!(jaw_harp(98.0, 0.5, 4.0), jaw_harp(98.0, 0.5, 8.0));
        // A stomp is low, a washboard is not.
        let (stomp, board) = (Drums::Porch.kick(), Drums::Porch.snare());
        assert!(tone(&stomp, 60.0) > 5.0 * tone(&board, 60.0));
        assert!(tone(&board, 4000.0) > tone(&stomp, 4000.0));
    }

    #[test]
    fn the_theremin_slides_to_its_note_and_the_band_under_it_is_dark_and_deep() {
        for voice in [Voice::Theremin, Voice::DarkKeys, Voice::Sub] {
            let sound = voice.play(57, 1.5);
            check(&format!("{voice:?}"), &sound);
            assert!(loudest(&sound) <= 0.9, "{voice:?}: {}", loudest(&sound));
            assert_eq!(sound.len(), samples(1.5));
            assert!(sound.last().unwrap().abs() < 0.02, "{voice:?} lets go");
        }
        // After a note a fifth above, the theremin starts up there and
        // comes down; once it stands it is on its own note either way.
        let slid = Voice::Theremin.play_after(Some(64), 57, 1.5);
        let start = &slid[..samples(0.03)];
        assert!(
            tone(start, pitch(64)) > tone(start, pitch(57)),
            "starts above"
        );
        let stands = &slid[samples(0.5)..samples(1.2)];
        assert!(tone(stands, pitch(57)) > 3.0 * tone(stands, pitch(64)));
        // Alone it comes up from below.
        let alone = Voice::Theremin.play(57, 1.5);
        assert_ne!(alone, slid);
        let start = &alone[..samples(0.03)];
        assert!(
            tone(start, pitch(55)) > tone(start, pitch(59)),
            "from below"
        );
        // Nearly a pure tone: little of the second overtone.
        assert!(tone(stands, 2.0 * pitch(57)) < 0.25 * tone(stands, pitch(57)));
        // Nothing else slides.
        assert_eq!(
            Voice::DarkKeys.play_after(Some(64), 57, 0.5),
            Voice::DarkKeys.play(57, 0.5)
        );
        assert_eq!(
            Voice::Pluck.play_after(Some(64), 57, 0.5),
            Voice::Pluck.play(57, 0.5)
        );
        // The rim shot is short and woody, the bass drum long and deep.
        let (kick, rim) = (Drums::Dusty.kick(), Drums::Dusty.snare());
        assert!(kick.len() > 3 * rim.len());
        assert!(tone(&kick, 50.0) > 5.0 * tone(&rim, 50.0));
        assert!(tone(&rim, 1650.0) > 5.0 * tone(&kick, 1650.0));
    }

    #[test]
    fn a_reverb_rings_on_and_far_sounds_are_only_their_echo() {
        // One click, then silence: the room goes on ringing and dies away.
        let mut click = vec![0.0_f32; samples(4.0)];
        click[0] = 1.0;
        let room = reverb(&click, 2.0, 1.0);
        assert_eq!(room.len(), click.len());
        assert!(room.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert_eq!(room[0], 0.0, "the echo alone: the click itself is gone");
        let early = loudest(&room[samples(0.1)..samples(0.5)]);
        let late = loudest(&room[samples(2.5)..samples(3.0)]);
        assert!(early > 0.01, "{early}");
        assert!(late > 0.0 && late < early * 0.1, "{early} then {late}");
        // A longer decay rings longer.
        let hall = reverb(&click, 5.0, 1.0);
        assert!(loudest(&hall[samples(2.5)..samples(3.0)]) > 2.0 * late);
        // Dry is the sound itself; half and half keeps the click.
        assert_eq!(reverb(&click, 2.0, 0.0), click);
        assert_eq!(reverb(&click, 2.0, 0.5)[0], 0.5);

        let near = bomb(1);
        check("bomb", &near);
        assert_ne!(near, bomb(2));
        assert!(tone(&near, 45.0) > 4.0 * tone(&near, 2000.0), "deep");
        let far = distant(&near);
        assert_eq!(far.len(), near.len() + samples(5.0));
        assert!(far.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        // It rings on after the bomb itself is over, and ends in silence.
        assert!(loudest(&far[near.len()..near.len() + samples(1.0)]) > 0.005);
        assert!(far.last().unwrap().abs() < 0.001);

        // The call rises and sinks below where it began.
        let call = far_call(0);
        check("call", &call);
        let (first, middle, last) = (
            &call[samples(0.3)..samples(0.8)],
            &call[samples(1.6)..samples(2.1)],
            &call[samples(3.2)..samples(3.7)],
        );
        assert!(tone(middle, 110.0) > tone(first, 110.0), "it rises");
        assert!(tone(last, 66.0) > tone(middle, 66.0), "and sinks");

        // Crackle: clicks now and then, not a tone, different for a seed.
        let dust = crackle(2.0, 5);
        assert_eq!(dust.len(), samples(2.0));
        assert!(dust.iter().all(|s| s.is_finite() && s.abs() <= 1.2));
        let clicks = dust.windows(2).filter(|w| w[1] - w[0] > 0.25).count();
        assert!((10..=60).contains(&clicks), "{clicks} clicks in 2 s");
        assert!(dust[0].abs() < 0.01, "comes in softly");
        assert_ne!(dust, crackle(2.0, 6));
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
