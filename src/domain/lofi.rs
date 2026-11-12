//! The Kawaii theme's music, made from nothing but arithmetic: ten short
//! lofi sequences and six breaks that lead from one into the next. A
//! sequence is slow drums, a soft bass, a melody made from a four-note theme
//! (stated once, then answered, shortened, stretched and decorated), and
//! under it a flute that plays the theme eight times slower, a pad, or
//! nothing. The instruments stay behind the drums.

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

/// What plays the melody.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instrument {
    /// A plucked string with its edge taken off: guitar-like.
    Pluck,
    /// A soft struck tone that rings and wavers a little: an electric
    /// piano, far away.
    Keys,
}

/// A plucked string (Karplus-Strong): a burst of noise going round a delay
/// line as long as one period, losing its edge on every round; then the
/// brightness is taken off so it sits behind the drums.
fn pluck(frequency: f32, seconds: f32) -> Vec<f32> {
    let period = ((RATE / frequency) as usize).max(2);
    let mut noise = Noise(frequency.to_bits());
    let mut line: Vec<f32> = (0..period).map(|_| noise.next()).collect();
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
            breath += 0.1 * (noise.next() - breath);
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

/// What plays under the melody.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backing {
    /// A flute plays the theme as one long tune, each note eight times as
    /// long as in the melody; `backwards` plays it from its last note.
    Flute { backwards: bool },
    /// A pad holds the theme's chord.
    Pad,
    /// Melody, bass and drums alone.
    None,
}

/// One of the ten sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sequence {
    /// Beats per minute.
    pub bpm: u16,
    /// Bars of four beats.
    pub bars: u16,
    /// The four notes of its theme, as MIDI numbers. The melody is made
    /// from the theme; it is not the theme over and over.
    pub theme: [u8; 4],
    pub lead: Instrument,
    pub backing: Backing,
}

use Instrument::{Keys, Pluck};

const fn sequence(
    bpm: u16,
    bars: u16,
    theme: [u8; 4],
    lead: Instrument,
    backing: Backing,
) -> Sequence {
    Sequence {
        bpm,
        bars,
        theme,
        lead,
        backing,
    }
}

/// The ten sequences, each 10 to 30 seconds long.
pub const SEQUENCES: [Sequence; 10] = [
    sequence(
        72,
        6,
        [57, 60, 64, 67],
        Keys,
        Backing::Flute { backwards: false },
    ),
    sequence(76, 6, [62, 65, 69, 72], Pluck, Backing::Pad),
    sequence(70, 4, [60, 64, 67, 71], Keys, Backing::None),
    sequence(
        80,
        8,
        [55, 58, 62, 65],
        Pluck,
        Backing::Flute { backwards: true },
    ),
    sequence(74, 6, [53, 57, 60, 64], Keys, Backing::Pad),
    sequence(
        68,
        4,
        [64, 67, 71, 74],
        Pluck,
        Backing::Flute { backwards: false },
    ),
    sequence(
        78,
        8,
        [58, 62, 65, 69],
        Keys,
        Backing::Flute { backwards: true },
    ),
    sequence(72, 6, [59, 62, 66, 69], Pluck, Backing::None),
    sequence(82, 8, [56, 60, 63, 67], Keys, Backing::Pad),
    sequence(
        66,
        4,
        [61, 64, 68, 71],
        Keys,
        Backing::Flute { backwards: false },
    ),
];

/// A note of a melody: where it starts, counted in beats from the start of
/// the sequence, how many beats it lasts, and its MIDI number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Note {
    pub at: f32,
    pub beats: f32,
    pub midi: u8,
}

/// What one bar does with the theme. The first bar of a sequence states it;
/// the others answer, shorten, stretch or decorate it, so the theme stays
/// recognisable without being repeated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// The four notes, one a beat.
    Statement,
    /// The four notes, long-short-long-short.
    Swung,
    /// The first two notes; the second is held.
    Opening,
    /// The four notes backwards.
    Answer,
    /// The last three notes, the last one held: an answer that rises.
    Rise,
    /// The four notes with a step between the second and third.
    Stepping,
    /// The last two notes, then again an octave up.
    Echo,
    /// Third note, then the first, each for half the bar: coming to rest.
    Rest,
}

/// The shapes of the bars between the first and the last, in turn. The
/// plain statement is not among them: the theme is said outright only once.
const SHAPES: [Shape; 6] = [
    Shape::Opening,
    Shape::Stepping,
    Shape::Answer,
    Shape::Swung,
    Shape::Rise,
    Shape::Echo,
];

impl Shape {
    /// The notes of one bar: start within the bar in beats, length in
    /// beats, MIDI number.
    fn notes(self, theme: [u8; 4]) -> Vec<(f32, f32, u8)> {
        let [a, b, c, d] = theme;
        // A note of the scale between the second and third: the fourth over
        // a minor theme, the second over a major one.
        let step = if b.saturating_sub(a) == 3 {
            a + 5
        } else {
            a + 2
        };
        match self {
            Shape::Statement => vec![(0.0, 1.0, a), (1.0, 1.0, b), (2.0, 1.0, c), (3.0, 1.0, d)],
            Shape::Swung => vec![(0.0, 1.5, a), (1.5, 0.5, b), (2.0, 1.5, c), (3.5, 0.5, d)],
            Shape::Opening => vec![(0.0, 1.0, a), (1.0, 3.0, b)],
            Shape::Answer => vec![(0.0, 1.0, d), (1.0, 1.0, c), (2.0, 1.0, b), (3.0, 1.0, a)],
            Shape::Rise => vec![(0.5, 0.5, b), (1.0, 1.0, c), (2.0, 2.0, d)],
            Shape::Stepping => vec![
                (0.0, 1.0, a),
                (1.0, 0.5, b),
                (1.5, 0.5, step),
                (2.0, 1.0, c),
                (3.0, 1.0, d),
            ],
            Shape::Echo => vec![
                (0.0, 0.5, c),
                (0.5, 1.0, d),
                (2.0, 0.5, c.saturating_add(12)),
                (2.5, 1.5, d.saturating_add(12)),
            ],
            Shape::Rest => vec![(0.0, 2.0, c), (2.0, 2.0, a)],
        }
    }
}

impl Sequence {
    fn beat(&self) -> f32 {
        60.0 / f32::from(self.bpm)
    }

    /// Its length in seconds.
    pub fn seconds(&self) -> f32 {
        f32::from(self.bars) * 4.0 * self.beat()
    }

    /// The shape of each bar: the theme first, a rest last, and between
    /// them the other shapes in a turn that starts somewhere else for every
    /// theme.
    fn shapes(&self) -> Vec<Shape> {
        let turn = usize::from(self.theme[0]);
        (0..usize::from(self.bars))
            .map(|bar| {
                if bar == 0 {
                    Shape::Statement
                } else if bar + 1 == usize::from(self.bars) {
                    Shape::Rest
                } else {
                    let at = (turn + bar - 1) % SHAPES.len();
                    SHAPES.get(at).copied().unwrap_or(Shape::Answer)
                }
            })
            .collect()
    }

    /// The melody the lead instrument plays.
    pub fn melody(&self) -> Vec<Note> {
        let mut notes = Vec::new();
        for (bar, shape) in self.shapes().into_iter().enumerate() {
            for (at, beats, midi) in shape.notes(self.theme) {
                notes.push(Note {
                    at: bar as f32 * 4.0 + at,
                    beats,
                    midi,
                });
            }
        }
        notes
    }

    /// The flute's tune: the theme with every note eight beats long, as
    /// many notes as the sequence has room for, forwards or backwards.
    pub fn long_tune(&self, backwards: bool) -> Vec<Note> {
        let mut theme = self.theme;
        if backwards {
            theme.reverse();
        }
        theme
            .into_iter()
            .take(usize::from(self.bars / 2))
            .enumerate()
            .map(|(i, midi)| Note {
                at: i as f32 * 8.0,
                beats: 8.0,
                // The flute sings above the melody, but not shrill.
                midi: if midi < 64 { midi + 12 } else { midi },
            })
            .collect()
    }

    /// Melody, backing and bass: everything that is not drums.
    fn instruments(&self) -> Vec<f32> {
        let beat = self.beat();
        let mut mix = vec![0.0; samples(self.seconds())];
        for n in self.melody() {
            // Notes ring a little past their length.
            let sound = note(self.lead, n.midi, (n.beats + 0.6) * beat);
            add(&mut mix, n.at * beat, &sound, 0.26);
        }
        match self.backing {
            Backing::Flute { backwards } => {
                for n in self.long_tune(backwards) {
                    let sound = flute(pitch(n.midi), n.beats * beat);
                    add(&mut mix, n.at * beat, &sound, 0.16);
                }
            }
            Backing::Pad => {
                let [root, third, fifth, _] = self.theme;
                for pair in 0..self.bars / 2 {
                    let at = f32::from(pair) * 8.0 * beat;
                    for midi in [root.saturating_sub(12), fifth.saturating_sub(12), third] {
                        add(&mut mix, at, &pad(pitch(midi), 8.0 * beat), 0.07);
                    }
                }
            }
            Backing::None => {}
        }
        let low = bass(pitch(self.theme[0].saturating_sub(24)), 2.5 * beat);
        for bar in 0..self.bars {
            add(&mut mix, f32::from(bar) * 4.0 * beat, &low, 0.3);
        }
        mix
    }

    /// The drums: bass drum on one and just before four, snare on two and
    /// four, hi-hats on the eighths with the off ones late. The last half
    /// bar has no hi-hat, so whatever follows starts from quiet.
    fn drums(&self) -> Vec<f32> {
        let beat = self.beat();
        let eighth = beat / 2.0;
        let mut mix = vec![0.0; samples(self.seconds())];
        let (kick, snare, hat) = (kick(), snare(), hat());
        for bar in 0..self.bars {
            let start = f32::from(bar) * 4.0 * beat;
            let last = bar + 1 == self.bars;
            for place in [0.0, 5.0] {
                add(&mut mix, start + place * eighth, &kick, 0.9);
            }
            for place in [2.0, 6.0] {
                add(&mut mix, start + place * eighth, &snare, 0.65);
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
                    0.7,
                );
            }
        }
        mix
    }

    /// The whole sequence as samples; the end fades.
    pub fn render(&self) -> Vec<f32> {
        let mut mix = self.instruments();
        for (slot, drum) in mix.iter_mut().zip(self.drums()) {
            *slot += drum;
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
                    let sound = note(instrument, midi, beat * 1.6);
                    add(&mut mix, beat * (1.0 + i as f32 * 0.75), &sound, 0.3);
                }
            }
            Break::Drums(busy) => {
                let (kick, snare) = (kick(), snare());
                add(&mut mix, beat * 0.5, &kick, 0.9);
                add(&mut mix, beat * 1.5, &snare, 0.65);
                add(&mut mix, beat * 2.25, &kick, 0.9);
                if busy {
                    // A roll that grows into the last hit.
                    for i in 0..6u8 {
                        let at = beat * (2.5 + f32::from(i) * 0.125);
                        add(&mut mix, at, &snare, 0.15 + f32::from(i) * 0.06);
                    }
                } else {
                    add(&mut mix, beat * 3.0, &snare, 0.65);
                }
            }
            Break::Voice(vowel) => {
                let [root, third, ..] = next.theme;
                let hum = voice(pitch(root), beat * 1.8, vowel);
                add(&mut mix, beat * 0.6, &hum, 0.3);
                if vowel == Vowel::Aah {
                    let up = voice(pitch(third), beat * 1.0, vowel);
                    add(&mut mix, beat * 2.5, &up, 0.25);
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
        check("flute", &flute(440.0, 3.0));
        check("pad", &pad(220.0, 3.0));
        check("bass", &bass(55.0, 1.0));
        // The dreamy ones come in slowly: quiet at first, full later.
        for slow in [flute(440.0, 3.0), pad(220.0, 3.0)] {
            let early = slow[..1000].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            let later = slow[20_000..30_000]
                .iter()
                .fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!(early < later / 3.0, "{early} {later}");
            assert!(slow.last().unwrap().abs() < 0.01, "lets go at the end");
        }
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
        let with = |backing: fn(&Backing) -> bool| {
            SEQUENCES.iter().filter(|s| backing(&s.backing)).count()
        };
        assert!(with(|b| matches!(b, Backing::Flute { backwards: false })) >= 2);
        assert!(with(|b| matches!(b, Backing::Flute { backwards: true })) >= 2);
        assert!(with(|b| *b == Backing::Pad) >= 2 && with(|b| *b == Backing::None) >= 2);
        assert!(
            SEQUENCES.iter().any(|s| s.lead == Pluck) && SEQUENCES.iter().any(|s| s.lead == Keys)
        );
        for (i, sequence) in SEQUENCES.iter().enumerate() {
            let seconds = sequence.seconds();
            assert!((10.0..=30.0).contains(&seconds), "{i}: {seconds} s");
            assert_eq!(sequence.bars % 2, 0, "{i}: whole phrases");
        }
    }

    /// The notes of each bar of a melody, with their place in the bar.
    fn bars_of(sequence: &Sequence) -> Vec<Vec<(u32, u32, u8)>> {
        let mut bars = vec![Vec::new(); usize::from(sequence.bars)];
        for n in sequence.melody() {
            let bar = (n.at / 4.0) as usize;
            let within = n.at - bar as f32 * 4.0;
            bars[bar].push(((within * 2.0) as u32, (n.beats * 2.0) as u32, n.midi));
        }
        bars
    }

    #[test]
    fn the_melody_states_the_theme_once_and_then_varies_it() {
        for (i, sequence) in SEQUENCES.iter().enumerate() {
            let bars = bars_of(sequence);
            let [a, b, c, d] = sequence.theme;
            // The first bar is the theme, one note a beat.
            assert_eq!(bars[0], [(0, 2, a), (2, 2, b), (4, 2, c), (6, 2, d)], "{i}");
            // It is not the same bar over and over.
            let mut different = bars.clone();
            different.sort();
            different.dedup();
            assert!(
                different.len() >= bars.len().min(4),
                "{i}: {} kinds of bar",
                different.len()
            );
            let repeats = bars.windows(2).filter(|w| w[0] == w[1]).count();
            assert_eq!(repeats, 0, "{i}: a bar repeats the one before");
            // Every bar still comes from the theme: its notes, an octave up,
            // or the one step between them.
            let step = if b - a == 3 { a + 5 } else { a + 2 };
            let allowed = [a, b, c, d, c + 12, d + 12, step];
            for (n, bar) in bars.iter().enumerate() {
                assert!(!bar.is_empty(), "{i}: bar {n} is empty");
                assert!(
                    bar.iter().all(|note| allowed.contains(&note.2)),
                    "{i}: bar {n}"
                );
                let end = bar.iter().map(|note| note.0 + note.1).max().unwrap();
                assert!(end <= 8, "{i}: bar {n} runs over");
            }
            // The last bar comes to rest on the first note of the theme.
            assert_eq!(bars.last().unwrap().last().unwrap().2, a, "{i}");
        }
        // Different themes take the shapes in a different turn.
        let shapes: Vec<_> = SEQUENCES
            .iter()
            .filter(|s| s.bars == 6)
            .map(Sequence::shapes)
            .collect();
        assert!(shapes.windows(2).any(|w| w[0] != w[1]));
    }

    #[test]
    fn the_flute_plays_the_theme_eight_times_slower_forwards_or_backwards() {
        let eight_bars = SEQUENCES[3];
        let forwards = eight_bars.long_tune(false);
        let backwards = eight_bars.long_tune(true);
        assert_eq!(forwards.len(), 4);
        // A melody note is one beat, a flute note eight.
        assert!(forwards.iter().all(|n| n.beats == 8.0));
        let starts: Vec<f32> = forwards.iter().map(|n| n.at).collect();
        assert_eq!(starts, [0.0, 8.0, 16.0, 24.0]);
        let class = |notes: &[Note]| -> Vec<u8> { notes.iter().map(|n| n.midi % 12).collect() };
        let theme: Vec<u8> = eight_bars.theme.iter().map(|m| m % 12).collect();
        assert_eq!(class(&forwards), theme);
        let mut reversed = theme.clone();
        reversed.reverse();
        assert_eq!(class(&backwards), reversed);
        // A short sequence has room for the first notes only.
        assert_eq!(SEQUENCES[5].long_tune(false).len(), 2);
        assert!(
            forwards.iter().all(|n| (64..=86).contains(&n.midi)),
            "sings above"
        );
    }

    #[test]
    fn the_instruments_stay_behind_the_drums() {
        let peak = |samples: &[f32]| samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        let loudness = |samples: &[f32]| {
            (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
        };
        // One with a flute, one with a pad, one with neither.
        for index in [5, 1, 2] {
            let sequence = SEQUENCES[index];
            let (instruments, drums) = (sequence.instruments(), sequence.drums());
            check(&format!("instruments {index}"), &instruments);
            assert!(
                peak(&instruments) < peak(&drums) * 0.75,
                "{index}: instruments peak {} against drums {}",
                peak(&instruments),
                peak(&drums)
            );
            assert!(loudness(&instruments) > 0.02, "{index}: but they are there");
        }
    }

    #[test]
    fn sequences_render_to_their_length_in_range_and_end_quiet() {
        // Two of the ten, one without backing and one with the flute;
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
