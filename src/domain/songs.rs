//! What the Kawaii theme's music plays. The music is organised in styles
//! (the user calls them themes): each has its own instruments, five songs
//! of about a minute and six breaks. A song is written as parts that move
//! its four-note motif through keys, instruments and drum grooves, with
//! parts on a second theme between them; a few
//! things in it are left to chance, so it never plays the same way twice.
//! After a song, one time in four, a change break leads into another style.

use crate::domain::lofi::{
    Drums, Noise, RATE, Voice, Vowel, add, bomb, crackle, distant, far_call, finish, jaw_harp,
    pitch, reverb, samples, shuffle, voice,
};
use std::f32::consts::TAU;

/// Where a style plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    /// Close and dry, as every style but one.
    Dry,
    /// A big empty place with a war far off: the melody instruments have
    /// an echo, a record crackles all through, and now and then a bomb
    /// falls or something calls, far away and very quiet.
    Far,
}

/// A note of a motto: where it starts and how long it is, in quarters of
/// a beat, and how many semitones it lies above the key's first note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MottoNote {
    pub at: u8,
    pub length: u8,
    pub above: i8,
}

const fn m(at: u8, length: u8, above: i8) -> MottoNote {
    MottoNote { at, length, above }
}

/// A motto as notes in the key that starts on `root`: start and length in
/// beats, MIDI number.
pub fn motto_notes(motto: &[MottoNote], root: u8) -> Vec<(f32, f32, u8)> {
    motto
        .iter()
        .map(|n| {
            let midi = (i16::from(root) + i16::from(n.above)).clamp(0, 127) as u8;
            (f32::from(n.at) / 4.0, f32::from(n.length) / 4.0, midi)
        })
        .collect()
}

/// Who plays the motto when it comes in a song.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MottoBy {
    /// The first melody instrument.
    Lead,
    /// The instrument of the long tunes.
    Long,
    /// The bass, an octave down.
    Bass,
}

/// The instruments of a style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kit {
    /// The two that play melodies.
    pub first: Voice,
    pub second: Voice,
    /// Plays the motif as one long tune under the melody.
    pub long: Voice,
    pub pad: Voice,
    pub bass: Voice,
    pub drums: Drums,
    pub space: Space,
    /// The style's motto: one phrase of two bars that returns across its
    /// songs, always the same notes and rhythm, in the key of the song.
    pub motto: &'static [MottoNote],
    /// The long break of the style that always plays the motto.
    pub motto_in: Link,
}

/// Who plays the melody of a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lead {
    First,
    Second,
    /// The two take turns, bar by bar.
    Both,
    /// Nobody: the part breathes.
    Tacet,
}

/// What lies under the melody of a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Under {
    /// The long instrument plays the motif as one tune, each note eight
    /// times as long as in the melody; `backwards` from its last note.
    Long {
        backwards: bool,
    },
    /// A pad holds the motif's chord.
    Pad,
    None,
}

/// What the drums do in a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Groove {
    None,
    /// Half as busy: bass drum on one, snare on three.
    Half,
    /// Bass drum on one and just before four, snare on two and four.
    Full,
    /// As `Full` with one more bass drum and a quiet snare at the end.
    Busy,
    /// Double bass drum: sixteenths through the second half of the bar.
    Double,
}

/// A stretch of a song that stays with one key, one lead and one groove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub bars: u8,
    /// Semitones the motif is moved by.
    pub shift: i8,
    pub lead: Lead,
    pub under: Under,
    pub groove: Groove,
    /// It plays the song's second theme instead of its first: melody, long
    /// tune, chord and bass.
    pub b: bool,
}

const fn part(bars: u8, shift: i8, lead: Lead, under: Under, groove: Groove) -> Part {
    Part {
        bars,
        shift,
        lead,
        under,
        groove,
        b: false,
    }
}

/// A part that plays the song's second theme.
const fn part_b(bars: u8, shift: i8, lead: Lead, under: Under, groove: Groove) -> Part {
    Part {
        b: true,
        ..part(bars, shift, lead, under, groove)
    }
}

/// A written song of about a minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Song {
    /// Beats per minute.
    pub bpm: u16,
    /// Its four notes, as MIDI numbers. The melody is made from the motif;
    /// it is not the motif over and over.
    pub motif: [u8; 4],
    /// Its second theme: four notes of the motif's scale on another root,
    /// and not rising all the way as the motif does. Some parts play it
    /// instead of the motif, so a song is not one idea for a minute.
    pub second: [u8; 4],
    pub parts: &'static [Part],
}

/// How a bar's end is filled before the next part starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    None,
    /// A snare roll that grows.
    Roll,
    /// Two bass drums.
    Kicks,
}

/// What chance decides each time a song plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variation {
    /// The two melody instruments change places.
    pub swap: bool,
    /// Where the turn of the melody's shapes starts.
    pub turn: usize,
    pub fill: Fill,
    /// The bar whose drums are left out.
    pub skip: usize,
    /// The long tunes run the other way round.
    pub mirror: bool,
    /// The style's motto comes once in the song, played by this one.
    pub motto: Option<MottoBy>,
    /// Which of the places a song has for the motto it takes.
    pub spot: usize,
}

impl Variation {
    /// The variation `seed` stands for, in a song of `bars` bars.
    pub fn of(seed: u64, bars: usize) -> Self {
        // Seeds that lie close together must not give variations that do.
        let a = shuffle(shuffle(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
        let b = shuffle(a);
        let c = shuffle(b);
        let d = shuffle(c);
        let inner = bars.saturating_sub(2).max(1) as u64;
        Self {
            swap: a % 2 == 1,
            turn: (a >> 8) as usize % SHAPES.len(),
            fill: match b % 3 {
                0 => Fill::None,
                1 => Fill::Roll,
                _ => Fill::Kicks,
            },
            skip: 1 + ((b >> 8) % inner) as usize,
            mirror: c % 2 == 1,
            // In about one song of two.
            motto: match (d >> 4) % 6 {
                0 => Some(MottoBy::Lead),
                1 => Some(MottoBy::Long),
                2 => Some(MottoBy::Bass),
                _ => None,
            },
            spot: (d >> 16) as usize % 8,
        }
    }
}

/// What happens far off during a song in the `Far` space: when the bombs
/// fall and when the call comes, in seconds from the song's start.
#[derive(Debug, Clone, PartialEq)]
pub struct FarEvents {
    pub bombs: Vec<f32>,
    pub call: Option<f32>,
}

/// What `seed` lets happen far off in a song of `seconds` with beats of
/// `beat` seconds: no to three bombs, and in one song of three the call.
/// Nothing falls on a beat, and nothing in the first beats or the last
/// nine seconds, so every echo has died when the song ends.
pub fn far_events(seed: u64, seconds: f32, beat: f32) -> FarEvents {
    // Seeds that lie close together must not give events that do.
    let mut random = shuffle(shuffle(
        (seed ^ 0xB0B5_FA11).wrapping_mul(0x9E37_79B9_7F4A_7C15),
    ));
    let beats = (((seconds - 9.0) / beat) as u64).max(3) - 2;
    let place = |random: &mut u64| {
        *random = shuffle(*random);
        let on = 2 + (*random >> 8) % beats;
        *random = shuffle(*random);
        let off = 0.3 + 0.4 * ((*random >> 8) % 1000) as f32 / 1000.0;
        (on as f32 + off) * beat
    };
    let count = (random >> 8) % 4;
    let bombs = (0..count).map(|_| place(&mut random)).collect();
    random = shuffle(random);
    let call = (random >> 8).is_multiple_of(3).then(|| place(&mut random));
    FarEvents { bombs, call }
}

/// A note: where it starts, counted in beats from the start of the song,
/// how many beats it lasts, its MIDI number, and whether the second melody
/// instrument plays it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Note {
    pub at: f32,
    pub beats: f32,
    pub midi: u8,
    pub second: bool,
}

/// What one bar does with the motif. The first bar of a melody states it;
/// the others answer, shorten, stretch or decorate it, so the motif stays
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
/// plain statement is not among them: the motif is said outright only once.
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
    fn notes(self, motif: [u8; 4]) -> Vec<(f32, f32, u8)> {
        let [a, b, c, d] = motif;
        // A note of the scale between the second and third: the fourth over
        // a minor motif, the second over a major one.
        let step = if b.saturating_sub(a) == 3 {
            a.saturating_add(5)
        } else {
            a.saturating_add(2)
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

/// `motif` moved by `shift` semitones.
fn moved(motif: [u8; 4], shift: i8) -> [u8; 4] {
    motif.map(|midi| (i16::from(midi) + i16::from(shift)).clamp(0, 127) as u8)
}

/// A long tune sings above the melody, but not shrill.
fn sung(midi: u8) -> u8 {
    if midi < 64 { midi + 12 } else { midi }
}

/// A bar of a song: its number, its part, and its number within the part.
struct Bar<'a> {
    number: usize,
    part: &'a Part,
    within: usize,
    /// The last bar of its part.
    closes: bool,
}

impl Song {
    fn beat(&self) -> f32 {
        60.0 / f32::from(self.bpm)
    }

    pub fn bars(&self) -> usize {
        self.parts.iter().map(|p| usize::from(p.bars)).sum()
    }

    /// Its length in seconds.
    pub fn seconds(&self) -> f32 {
        self.bars() as f32 * 4.0 * self.beat()
    }

    /// The four notes `part` plays, in its key: the motif or the second
    /// theme.
    fn theme(&self, part: &Part) -> [u8; 4] {
        moved(if part.b { self.second } else { self.motif }, part.shift)
    }

    fn each_bar(&self) -> Vec<Bar<'_>> {
        let mut bars = Vec::with_capacity(self.bars());
        for part in self.parts {
            for within in 0..usize::from(part.bars) {
                bars.push(Bar {
                    number: bars.len(),
                    part,
                    within,
                    closes: within + 1 == usize::from(part.bars),
                });
            }
        }
        bars
    }

    /// The melody: the theme of each part (the motif or the second theme),
    /// the motif stated in the first bar that has a melody, a
    /// rest in the song's last bar, and between them the other shapes in a
    /// turn that starts where the variation says.
    pub fn melody(&self, variation: &Variation) -> Vec<Note> {
        let last = self.bars().saturating_sub(1);
        let mut notes = Vec::new();
        let mut sung_bars = 0;
        let mut second_stated = false;
        for bar in self.each_bar() {
            let second = match bar.part.lead {
                Lead::Tacet => continue,
                Lead::First => false,
                Lead::Second => true,
                Lead::Both => bar.within % 2 == 1,
            };
            let shape = if sung_bars == 0 {
                Shape::Statement
            } else if bar.number == last {
                Shape::Rest
            } else if bar.part.b && !second_stated {
                // The second theme is said outright too, where it first comes.
                second_stated = true;
                Shape::Statement
            } else {
                let at = (usize::from(self.motif[0]) + variation.turn + sung_bars) % SHAPES.len();
                SHAPES.get(at).copied().unwrap_or(Shape::Answer)
            };
            sung_bars += 1;
            for (at, beats, midi) in shape.notes(self.theme(bar.part)) {
                notes.push(Note {
                    at: bar.number as f32 * 4.0 + at,
                    beats,
                    midi,
                    second: second != variation.swap,
                });
            }
        }
        notes
    }

    /// The long tunes: in every part that has one, the motif with each note
    /// eight beats long, as many notes as the part has room for.
    pub fn long_tune(&self, variation: &Variation) -> Vec<Note> {
        let mut notes = Vec::new();
        let mut start = 0;
        for part in self.parts {
            if let Under::Long { backwards } = part.under {
                let mut motif = self.theme(part);
                if backwards != variation.mirror {
                    motif.reverse();
                }
                for (i, midi) in motif
                    .into_iter()
                    .take(usize::from(part.bars / 2))
                    .enumerate()
                {
                    notes.push(Note {
                        at: (start * 4 + i * 8) as f32,
                        beats: 8.0,
                        midi: sung(midi),
                        second: false,
                    });
                }
            }
            start += usize::from(part.bars);
        }
        notes
    }

    /// Where the motto comes in this playing of the song, and in which
    /// key: the first bar of one of the parts whose lead rests, or, in a
    /// song without such a part, its last two bars. `None` when the motto
    /// does not come.
    pub fn motto_place(&self, variation: &Variation) -> Option<(usize, u8)> {
        variation.motto?;
        let mut places = Vec::new();
        let mut start = 0;
        for part in self.parts {
            if part.lead == Lead::Tacet && part.bars >= 2 {
                places.push((start, moved(self.motif, part.shift)[0]));
            }
            start += usize::from(part.bars);
        }
        if places.is_empty() {
            let shift = self.parts.last().map_or(0, |p| p.shift);
            places.push((self.bars().saturating_sub(2), moved(self.motif, shift)[0]));
        }
        places.get(variation.spot % places.len()).copied()
    }

    /// The motto as the one who plays it in this playing of the song.
    fn motto(&self, kit: &Kit, variation: &Variation, mix: &mut [f32]) {
        let (Some(by), Some((bar, root))) = (variation.motto, self.motto_place(variation)) else {
            return;
        };
        let beat = self.beat();
        let (voice, gain) = match by {
            MottoBy::Lead => (kit.first, 0.26),
            MottoBy::Long => (kit.long, 0.2),
            MottoBy::Bass => (kit.bass, 0.3),
        };
        let mut before = None;
        for (at, beats, midi) in motto_notes(kit.motto, root) {
            let midi = match by {
                MottoBy::Lead => midi,
                MottoBy::Long => sung(midi),
                MottoBy::Bass => midi.saturating_sub(12),
            };
            let sound = voice.play_after(before, midi, (beats + 0.3) * beat);
            add(mix, (bar as f32 * 4.0 + at) * beat, &sound, gain);
            before = Some(midi);
        }
    }

    /// Melody, long tune, pad and bass: everything that is not drums.
    fn instruments(&self, kit: &Kit, variation: &Variation) -> Vec<f32> {
        let beat = self.beat();
        let mut mix = vec![0.0; samples(self.seconds())];
        // The note each melody instrument played last, and where it ended.
        let mut first: Option<(u8, f32)> = None;
        let mut second: Option<(u8, f32)> = None;
        // Where the motto comes the melody is silent: it has the two bars.
        let motto_from = self
            .motto_place(variation)
            .map_or(f32::INFINITY, |(bar, _)| bar as f32 * 4.0);
        for n in self.melody(variation) {
            if (motto_from..motto_from + 8.0).contains(&n.at) {
                continue;
            }
            let (lead, last) = if n.second {
                (kit.second, &mut second)
            } else {
                (kit.first, &mut first)
            };
            // An instrument that slides comes from its last note when that
            // one has only just ended.
            let before = last
                .filter(|(_, end)| n.at - end < 1.0)
                .map(|(midi, _)| midi);
            *last = Some((n.midi, n.at + n.beats));
            // Notes ring a little past their length.
            let sound = lead.play_after(before, n.midi, (n.beats + 0.6) * beat);
            add(&mut mix, n.at * beat, &sound, 0.26);
        }
        for n in self.long_tune(variation) {
            add(
                &mut mix,
                n.at * beat,
                &kit.long.play(n.midi, n.beats * beat),
                0.16,
            );
        }
        self.motto(kit, variation, &mut mix);
        // In the far space the bass stays out of the echo.
        let mut low = (kit.space == Space::Far).then(|| vec![0.0; mix.len()]);
        for bar in self.each_bar() {
            let at = bar.number as f32 * 4.0 * beat;
            let [root, third, fifth, _] = self.theme(bar.part);
            if bar.part.under == Under::Pad && bar.within % 2 == 0 {
                let bars = if bar.closes { 1.0 } else { 2.0 };
                for midi in [root.saturating_sub(12), fifth.saturating_sub(12), third] {
                    let sound = kit.pad.play(midi, bars * 4.0 * beat);
                    add(&mut mix, at, &sound, 0.07);
                }
            }
            if bar.part.groove != Groove::None {
                let note = kit.bass.play(root.saturating_sub(24), 2.5 * beat);
                add(low.as_deref_mut().unwrap_or(&mut mix), at, &note, 0.3);
            }
        }
        if let Some(low) = low {
            mix = reverb(&mix, 2.4, 0.35);
            for (slot, value) in mix.iter_mut().zip(low) {
                *slot += value;
            }
        }
        mix
    }

    /// What the far space adds to a song: the crackle of the record, and
    /// the bombs and the call that `seed` lets happen, far away.
    fn far_off(&self, seed: u64) -> Vec<f32> {
        let seconds = self.seconds();
        let mut mix = vec![0.0; samples(seconds)];
        add(&mut mix, 0.0, &crackle(seconds, seed), CRACKLE);
        let events = far_events(seed, seconds, self.beat());
        for (i, at) in events.bombs.iter().enumerate() {
            let far = distant(&bomb(seed.wrapping_add(i as u64)));
            add(&mut mix, *at, &far, FAR_BOMB);
        }
        if let Some(at) = events.call {
            add(&mut mix, at, &distant(&far_call(seed)), FAR_CALL);
        }
        mix
    }

    /// The drums. One bar is left out, a part may end on a fill, and the
    /// last half bar has no hi-hat, so whatever follows starts from quiet.
    fn drums(&self, kit: &Kit, variation: &Variation) -> Vec<f32> {
        let beat = self.beat();
        let eighth = beat / 2.0;
        let mut mix = vec![0.0; samples(self.seconds())];
        let (kick, snare, hat) = (kit.drums.kick(), kit.drums.snare(), kit.drums.hat());
        let last = self.bars().saturating_sub(1);
        for bar in self.each_bar() {
            if bar.number == variation.skip {
                continue;
            }
            let start = bar.number as f32 * 4.0 * beat;
            let (kicks, snares, hats): (&[f32], &[f32], u8) = match bar.part.groove {
                Groove::None => continue,
                Groove::Half => (&[0.0], &[4.0], 2),
                Groove::Full => (&[0.0, 5.0], &[2.0, 6.0], 1),
                Groove::Busy => (&[0.0, 3.0, 5.0], &[2.0, 6.0], 1),
                Groove::Double => (
                    &[0.0, 1.0, 4.0, 4.5, 5.0, 5.5, 6.5, 7.0, 7.5],
                    &[2.0, 6.0],
                    1,
                ),
            };
            for place in kicks {
                add(&mut mix, start + place * eighth, &kick, 0.9);
            }
            for place in snares {
                add(&mut mix, start + place * eighth, &snare, 0.65);
            }
            for step in (0..8u8).step_by(usize::from(hats)) {
                if bar.number == last && step >= 4 {
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
            if bar.part.groove == Groove::Busy {
                add(&mut mix, start + 7.0 * eighth, &snare, 0.2);
            }
            if bar.closes && bar.number != last {
                match variation.fill {
                    Fill::None => {}
                    Fill::Roll => roll(&mut mix, start + beat * 3.0, beat, &snare),
                    Fill::Kicks => {
                        add(&mut mix, start + 6.5 * eighth, &kick, 0.6);
                        add(&mut mix, start + 7.25 * eighth, &kick, 0.75);
                    }
                }
            }
        }
        mix
    }

    /// The song as samples, played the way `seed` makes it; the end fades.
    pub fn render(&self, kit: &Kit, seed: u64) -> Vec<f32> {
        let variation = Variation::of(seed, self.bars());
        let mut mix = self.instruments(kit, &variation);
        for (slot, drum) in mix.iter_mut().zip(self.drums(kit, &variation)) {
            *slot += drum;
        }
        if kit.space == Space::Far {
            for (slot, far) in mix.iter_mut().zip(self.far_off(seed)) {
                *slot += far;
            }
        }
        finish(&mut mix);
        mix
    }
}

/// How loud the far space's sounds are in the mix: all of them far below
/// the music.
const CRACKLE: f32 = 0.05;
const FAR_BOMB: f32 = 0.1;
const FAR_CALL: f32 = 0.1;

/// A snare roll over one beat that grows into its last hit.
fn roll(mix: &mut [f32], at: f32, beat: f32, snare: &[f32]) {
    for i in 0..6u8 {
        let hit = at + beat * f32::from(i) / 6.0;
        add(mix, hit, snare, 0.15 + f32::from(i) * 0.06);
    }
}

/// A break between two songs of one style. The first four of a style are
/// short (one bar), the last two long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// Silence, then three of the next song's four notes; `true` on the
    /// second melody instrument.
    Notes(bool),
    /// Drums alone, ending in a roll.
    Roll,
    /// The synthetic voice hums the next song's first note.
    Hum(Vowel),
    /// The long instrument lets two notes fall.
    Fall,
    /// Long: the long instrument plays the motif freely over a pad.
    Solo,
    /// Long: the drums come in piece by piece over a walking bass.
    Build,
    /// The motif's chord run up fast over two octaves, like a prize jingle.
    Arp,
    /// Two quick high notes: a coin.
    Coin,
    /// Long: a run of the motif that starts a step higher in every bar.
    Climb,
    /// The chord struck once, a silence, then twice.
    Stab,
    /// A bar of fast bass drums under snares.
    Blast,
    /// Long: the motif sung a note a beat over struck chords.
    Chant,
    /// A roll: the first melody instrument picks the chord's notes in
    /// sixteenths, as a banjo does.
    Rolls,
    /// A jaw harp twangs three times on the next song's first note.
    Twang,
    /// The first melody instrument comes down from the next song's last
    /// note to its first in one slide.
    Glide,
    /// The needle in the groove: crackle, two rim shots and one low note.
    Needle,
    /// Long: the far battlefield. A held chord with its echo, two bombs
    /// and the call of something that walks out there.
    Battlefield,
    /// Long: a porch. A foot stomps, the jaw harp twangs, the bass starts
    /// to walk and the first melody instrument picks its way in.
    Porch,
}

impl Link {
    fn bars(self) -> f32 {
        match self {
            Link::Solo | Link::Chant | Link::Porch | Link::Battlefield => 4.0,
            Link::Build | Link::Climb => 3.0,
            _ => 1.0,
        }
    }

    /// Its length in seconds before `next`, whose tempo it has.
    pub fn seconds(self, next: &Song) -> f32 {
        self.bars() * 4.0 * next.beat()
    }

    /// The break as samples, with the instruments of `kit` and the tempo
    /// and notes of `next`, the song it leads into. It starts and ends
    /// with a rest.
    pub fn render(self, kit: &Kit, next: &Song) -> Vec<f32> {
        let beat = next.beat();
        let [a, b, c, d] = next.motif;
        let mut mix = vec![0.0; samples(self.seconds(next))];
        let (kick, snare, hat) = (kit.drums.kick(), kit.drums.snare(), kit.drums.hat());
        // The motto, from `from` beats on, when this is the style's break
        // for it.
        let has_motto = self == kit.motto_in;
        let motto = |mix: &mut [f32], voice: Voice, from: f32, gain: f32| {
            let mut before = None;
            for (at, beats, midi) in motto_notes(kit.motto, a) {
                let midi = if voice == kit.long { sung(midi) } else { midi };
                let sound = voice.play_after(before, midi, (beats + 0.3) * beat);
                add(mix, (from + at) * beat, &sound, gain);
                before = Some(midi);
            }
        };
        match self {
            Link::Notes(second) => {
                let lead = if second { kit.second } else { kit.first };
                for (i, midi) in [a, b, c].into_iter().enumerate() {
                    let at = beat * (1.0 + i as f32 * 0.75);
                    add(&mut mix, at, &lead.play(midi, beat * 1.6), 0.3);
                }
            }
            Link::Roll => {
                add(&mut mix, beat * 0.5, &kick, 0.9);
                add(&mut mix, beat * 1.5, &snare, 0.65);
                add(&mut mix, beat * 2.25, &kick, 0.9);
                roll(&mut mix, beat * 2.5, beat * 0.75, &snare);
            }
            Link::Hum(vowel) => {
                let hum = voice(pitch(a), beat * 1.8, vowel);
                add(&mut mix, beat * 0.6, &hum, 0.3);
                if vowel == Vowel::Aah {
                    let up = voice(pitch(b), beat, vowel);
                    add(&mut mix, beat * 2.5, &up, 0.25);
                }
            }
            Link::Fall => {
                add(
                    &mut mix,
                    beat * 0.5,
                    &kit.long.play(sung(d), beat * 1.5),
                    0.2,
                );
                add(
                    &mut mix,
                    beat * 1.9,
                    &kit.long.play(sung(a), beat * 1.8),
                    0.2,
                );
            }
            Link::Solo => {
                for midi in [a.saturating_sub(12), c.saturating_sub(12), b] {
                    add(&mut mix, beat * 0.5, &kit.pad.play(midi, beat * 15.0), 0.07);
                }
                let tune = [
                    (1.0, 2.0, a),
                    (3.0, 2.0, b),
                    (5.0, 4.0, c),
                    (9.0, 2.5, d),
                    (11.5, 3.5, c),
                ];
                // The motto takes the place of the tune's first two bars.
                if has_motto {
                    motto(&mut mix, kit.long, 1.0, 0.2);
                }
                let skip = if has_motto { 3 } else { 0 };
                for (at, beats, midi) in tune.into_iter().skip(skip) {
                    let sound = kit.long.play(sung(midi), beat * beats);
                    add(&mut mix, beat * at, &sound, 0.2);
                }
            }
            Link::Build => {
                for place in [0.5, 2.5, 4.0, 6.5, 8.0, 10.5] {
                    add(&mut mix, beat * place, &kick, 0.9);
                }
                for place in [5.0, 7.0, 9.0] {
                    add(&mut mix, beat * place, &snare, 0.65);
                }
                for (i, midi) in [a, b, c, d, c, b].into_iter().enumerate() {
                    let low = kit.bass.play(midi.saturating_sub(24), beat * 1.2);
                    add(&mut mix, beat * (4.0 + i as f32), &low, 0.3);
                }
                for step in 0..5u8 {
                    add(&mut mix, beat * (8.0 + f32::from(step) * 0.5), &hat, 0.7);
                }
                roll(&mut mix, beat * 10.5, beat * 0.9, &snare);
                if has_motto {
                    motto(&mut mix, kit.first, 2.0, 0.26);
                }
            }
            Link::Arp => {
                let run = [a, b, c, d, a + 12, b + 12, c + 12, d + 12];
                for (i, midi) in run.into_iter().enumerate() {
                    let sound = kit.first.play(midi, beat * 0.3);
                    add(&mut mix, beat * (0.75 + i as f32 * 0.25), &sound, 0.26);
                }
                add(&mut mix, beat * 2.75, &kit.first.play(a + 24, beat), 0.26);
            }
            Link::Coin => {
                add(
                    &mut mix,
                    beat * 1.5,
                    &kit.second.play(d + 12, beat * 0.2),
                    0.3,
                );
                add(
                    &mut mix,
                    beat * 1.7,
                    &kit.second.play(d + 17, beat * 1.2),
                    0.3,
                );
            }
            Link::Climb => {
                for bar in 0..3u8 {
                    let start = beat * (f32::from(bar) * 4.0 + 0.5);
                    add(&mut mix, start, &kick, 0.9);
                    add(&mut mix, start + beat * 2.0, &snare, 0.65);
                    let low = kit.bass.play(a.saturating_sub(24) + bar, beat * 2.0);
                    add(&mut mix, start, &low, 0.3);
                    for (i, midi) in [a, b, c, d, c, b].into_iter().enumerate() {
                        let sound = kit.second.play(midi + bar, beat * 0.45);
                        add(&mut mix, start + beat * i as f32 * 0.5, &sound, 0.26);
                    }
                }
            }
            Link::Stab => {
                for place in [0.5, 2.5, 3.0] {
                    add(
                        &mut mix,
                        beat * place,
                        &kit.pad.play(a.saturating_sub(12), beat * 0.45),
                        0.3,
                    );
                    add(&mut mix, beat * place, &kick, 0.9);
                }
            }
            Link::Blast => {
                for sixteenth in 2..14u8 {
                    add(&mut mix, beat * f32::from(sixteenth) * 0.25, &kick, 0.8);
                    if sixteenth % 2 == 0 {
                        add(&mut mix, beat * f32::from(sixteenth) * 0.25, &snare, 0.5);
                    }
                }
            }
            Link::Glide => {
                let slid = kit.first.play_after(Some(d), a, beat * 2.4);
                add(&mut mix, beat * 0.5, &slid, 0.3);
            }
            Link::Needle => {
                add(&mut mix, 0.0, &crackle(self.seconds(next), 78), 0.12);
                add(&mut mix, beat, &snare, 0.65);
                let note = kit.bass.play(a.saturating_sub(24), beat * 1.5);
                add(&mut mix, beat * 1.5, &note, 0.3);
                add(&mut mix, beat * 2.75, &snare, 0.4);
            }
            Link::Battlefield => {
                let mut chord = vec![0.0; mix.len()];
                for midi in [a.saturating_sub(12), c.saturating_sub(12), b] {
                    add(&mut chord, beat * 0.5, &kit.pad.play(midi, beat * 9.0), 0.1);
                }
                if has_motto {
                    motto(&mut chord, kit.first, 4.0, 0.22);
                }
                for (slot, value) in mix.iter_mut().zip(reverb(&chord, 3.0, 0.5)) {
                    *slot += value;
                }
                let note = kit.bass.play(a.saturating_sub(24), beat * 6.0);
                add(&mut mix, beat * 0.5, &note, 0.3);
                // Early, so the echoes have died when the break ends.
                add(&mut mix, beat * 1.37, &distant(&bomb(u64::from(a))), 0.14);
                add(
                    &mut mix,
                    beat * 3.3,
                    &distant(&far_call(u64::from(b))),
                    0.14,
                );
                add(&mut mix, beat * 6.6, &distant(&bomb(u64::from(d))), 0.12);
            }
            Link::Rolls => {
                let picked = [a, c, d, a + 12, c, d, a, c, d, a + 12];
                for (i, midi) in picked.into_iter().enumerate() {
                    let sound = kit.first.play(midi, beat * 0.6);
                    add(&mut mix, beat * (0.5 + i as f32 * 0.25), &sound, 0.26);
                }
                add(&mut mix, beat * 3.0, &kit.first.play(a, beat * 0.9), 0.26);
            }
            Link::Twang => {
                let low = pitch(a.saturating_sub(24));
                for (at, beats) in [(0.5, 0.9), (1.5, 0.7), (2.25, 1.5)] {
                    let twang = jaw_harp(low, beat * beats, 2.0 / beat);
                    add(&mut mix, beat * at, &twang, 0.4);
                }
            }
            Link::Porch => {
                let low = pitch(a.saturating_sub(24));
                for bar in 0..4u8 {
                    let start = beat * f32::from(bar) * 4.0;
                    add(&mut mix, start + beat * 0.5, &kick, 0.9);
                    add(&mut mix, start + beat * 2.5, &kick, 0.9);
                    let twang = jaw_harp(low, beat * 3.0, 2.0 / beat);
                    add(&mut mix, start + beat * 0.5, &twang, 0.3);
                    if bar >= 1 {
                        add(&mut mix, start + beat * 1.5, &snare, 0.65);
                        add(&mut mix, start + beat * 3.5, &snare, 0.65);
                        for (i, midi) in [a, c].into_iter().enumerate() {
                            let walked = kit.bass.play(midi.saturating_sub(24), beat * 1.6);
                            add(
                                &mut mix,
                                start + beat * (0.5 + 2.0 * i as f32),
                                &walked,
                                0.3,
                            );
                        }
                    }
                }
                let picked = [a, b, c, d, c, b, a, b, c, d, c, b];
                for (i, midi) in picked.into_iter().enumerate() {
                    let sound = kit.first.play(midi, beat * 0.7);
                    add(&mut mix, beat * (8.5 + i as f32 * 0.5), &sound, 0.26);
                }
                add(&mut mix, beat * 14.5, &kit.first.play(a + 12, beat), 0.26);
            }
            Link::Chant => {
                let sung_notes = [a, b, c, d, c, b, a, a, b, c, d, d];
                for (i, midi) in sung_notes.into_iter().enumerate() {
                    let sound = kit.second.play(sung(midi), beat * 0.9);
                    add(&mut mix, beat * (1.0 + i as f32), &sound, 0.26);
                }
                for bar in 0..4u8 {
                    let start = beat * f32::from(bar) * 4.0;
                    let chord = kit.pad.play(a.saturating_sub(12), beat * 1.5);
                    add(&mut mix, start + beat, &chord, 0.2);
                    add(&mut mix, start + beat * 3.0, &chord, 0.2);
                    if bar < 3 {
                        add(&mut mix, start + beat, &kick, 0.9);
                        add(&mut mix, start + beat * 3.0, &snare, 0.65);
                    }
                }
            }
        }
        if kit.space == Space::Far {
            // The record crackles through every break of the far space,
            // and a long one has room for a bomb and its echo.
            let seed = u64::from(a) + u64::from(d);
            add(&mut mix, 0.0, &crackle(self.seconds(next), seed), CRACKLE);
            if self.bars() >= 3.0 && self != Link::Battlefield {
                add(&mut mix, beat * 1.37, &distant(&bomb(seed)), 0.14);
            }
        }
        finish(&mut mix);
        mix
    }
}

/// A break that leads from one style into another. Shared by all styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// A radio being tuned: whistles and hiss, and through them more and
    /// more of the next song's notes.
    Radio,
    /// The tape stops: the last song's chord sinks away; after a silence
    /// the next song is counted in.
    TapeStop,
    /// A modem on an acoustic coupler dials into the next style: dial
    /// tone, dialling, the answer, the handshake's warbling and screech.
    Modem,
    /// A record scratch: the last song's chord on a record that a hand
    /// pulls back and pushes forward; then the needle is dropped on the
    /// next song, which is counted in.
    Scratch,
}

/// The change breaks.
pub const CHANGES: [Change; 4] = [
    Change::Radio,
    Change::TapeStop,
    Change::Modem,
    Change::Scratch,
];

/// How fast the scratched record turns `t` seconds into the scratch, as a
/// multiple of its speed; below zero it turns backwards. It plays for a
/// moment, is scratched slowly, held, scratched fast, and stopped.
fn scratch_speed(t: f32) -> f32 {
    // A stroke back and a stroke forward in every `cell` seconds.
    let strokes = |since: f32, cell: f32| -2.6 * ((since / cell).fract() * TAU).sin();
    match t {
        t if t < 1.6 => 1.0,
        t if t < 3.1 => strokes(t - 1.6, 0.5),
        t if t < 3.6 => 0.0,
        t if t < 5.1 => strokes(t - 3.6, 0.25),
        t if t < 5.4 => 1.0 - (t - 5.1) / 0.3,
        _ => 0.0,
    }
}

/// The two tones of each key the modem dials, in Hz.
const DIALLED: [(f32, f32); 8] = [
    (941.0, 1336.0),
    (697.0, 1209.0),
    (852.0, 1336.0),
    (697.0, 1477.0),
    (770.0, 1209.0),
    (770.0, 1336.0),
    (852.0, 1477.0),
    (697.0, 1336.0),
];

/// What the modem's line carries at `t` seconds: the frequencies of up to
/// two tones, and how much noise. `bit` is the data bit sent just now.
fn modem_line(t: f32, bit: bool) -> (f32, f32, f32) {
    match t {
        // Dial tone.
        t if t < 1.6 => (350.0, 440.0, 0.0),
        t if t < 1.8 => (0.0, 0.0, 0.0),
        // Eight keys, each a tenth of a second, a tenth apart.
        t if t < 3.4 => {
            let key = ((t - 1.8) / 0.2) as usize;
            let (low, high) = DIALLED.get(key).copied().unwrap_or((0.0, 0.0));
            if ((t - 1.8) / 0.2).fract() < 0.5 {
                (low, high, 0.0)
            } else {
                (0.0, 0.0, 0.0)
            }
        }
        t if t < 3.9 => (0.0, 0.0, 0.0),
        // The other side answers.
        t if t < 4.9 => (2100.0, 0.0, 0.0),
        t if t < 5.0 => (0.0, 0.0, 0.0),
        // One side warbles between two tones, then both sides do.
        t if t < 6.4 => (if bit { 1180.0 } else { 980.0 }, 0.0, 0.0),
        t if t < 7.6 => (
            if bit { 1180.0 } else { 980.0 },
            if bit { 1650.0 } else { 1850.0 },
            0.0,
        ),
        // Two falling chirps while the line is measured.
        t if t < 8.4 => (2600.0 - 2300.0 * ((t - 7.6) / 0.4).fract(), 0.0, 0.1),
        // The screech of the data itself.
        t if t < 12.3 => (1800.0, if bit { 1200.0 } else { 2400.0 }, 1.0),
        _ => (0.0, 0.0, 0.0),
    }
}

impl Change {
    /// Its length in seconds before `next`.
    pub fn seconds(self, next: &Song) -> f32 {
        match self {
            Change::Radio => 12.0,
            Change::TapeStop => 7.5 + 4.0 * next.beat(),
            Change::Modem => 13.0,
            Change::Scratch => 8.0 + 4.0 * next.beat(),
        }
    }

    /// The break as samples: out of `last`, a song of the style with
    /// `from`, into `next`, a song of the style with `into`.
    pub fn render(self, from: &Kit, last: &Song, into: &Kit, next: &Song) -> Vec<f32> {
        let seconds = self.seconds(next);
        let mut mix = vec![0.0; samples(seconds)];
        match self {
            Change::Radio => {
                let mut noise = Noise(4711);
                let mut hiss = 0.0_f32;
                let mut phase = 0.0_f32;
                for (i, slot) in mix.iter_mut().enumerate() {
                    let t = i as f32 / RATE;
                    // The dial moves in three sweeps; each ends where a
                    // station comes through.
                    let sweep = (t / 3.0).fract();
                    let frequency = 500.0 + 1900.0 * (1.0 - sweep) * (1.0 - sweep);
                    phase = (phase + frequency / RATE).fract();
                    hiss += 0.2 * (noise.sample() - hiss);
                    let station = ((sweep - 0.6) / 0.1).clamp(0.0, 1.0);
                    let off_station = 1.0 - 0.8 * station;
                    let fade = (t / 0.3).min(1.0) * ((seconds - t) / 2.0).clamp(0.0, 1.0);
                    *slot = ((phase * TAU).sin() * 0.05 + hiss * 0.25) * off_station * fade;
                }
                let beat = next.beat();
                for sweep in 0..4u8 {
                    let at = f32::from(sweep) * 3.0 + 1.8;
                    for (i, midi) in next
                        .motif
                        .into_iter()
                        .take(usize::from(sweep) + 1)
                        .enumerate()
                    {
                        let sound = into.first.play(midi, beat * 1.2);
                        add(&mut mix, at + i as f32 * 0.28, &sound, 0.22);
                    }
                }
            }
            Change::TapeStop => {
                let [root, third, fifth, _] = last.motif;
                let mut phases = [0.0_f32; 3];
                let chord = [root.saturating_sub(12), fifth.saturating_sub(12), third];
                for (i, slot) in mix.iter_mut().take(samples(6.0)).enumerate() {
                    let t = i as f32 / RATE;
                    // The tape runs for two seconds, then loses speed.
                    let speed = (1.0 - (t - 2.0).max(0.0) / 4.0).max(0.0);
                    let mut sum = 0.0;
                    for (phase, midi) in phases.iter_mut().zip(chord) {
                        *phase = (*phase + pitch(midi) * speed / RATE).fract();
                        sum += (*phase * TAU).sin();
                    }
                    *slot = sum / 3.0 * 0.3 * (t / 0.4).min(1.0) * speed.sqrt();
                }
                add(&mut mix, 0.0, &from.first.play(root, 2.0), 0.26);
                let beat = next.beat();
                let hat = into.drums.hat();
                for count in 0..4u8 {
                    add(&mut mix, 7.5 + f32::from(count) * beat, &hat, 0.9);
                }
                add(&mut mix, 7.5 + 3.5 * beat, &into.drums.kick(), 0.6);
            }
            Change::Scratch => {
                let [root, third, fifth, _] = last.motif;
                let chord = [root.saturating_sub(12), fifth.saturating_sub(12), third];
                let mut phases = [0.0_f32; 3];
                let mut noise = Noise(1973);
                let mut hiss = 0.0_f32;
                for (i, slot) in mix.iter_mut().take(samples(5.5)).enumerate() {
                    let t = i as f32 / RATE;
                    let speed = scratch_speed(t);
                    let mut sum = 0.0;
                    for (phase, midi) in phases.iter_mut().zip(chord) {
                        *phase = (*phase + pitch(midi) * speed / RATE).rem_euclid(1.0);
                        sum += (*phase * TAU).sin();
                    }
                    // The needle gives out what moves under it: nothing
                    // while the hand holds the record, and its own hiss
                    // the faster the record is pulled.
                    hiss += 0.5 * (noise.sample() - hiss);
                    let moving = speed.abs().min(1.0);
                    *slot =
                        (sum / 3.0 * 0.3 + hiss * 0.1 * speed.abs()) * moving * (t / 0.3).min(1.0);
                }
                add(&mut mix, 0.0, &from.first.play(root, 1.5), 0.26);
                // The needle comes down on the next record.
                add(&mut mix, 7.0, &into.drums.kick(), 0.25);
                add(&mut mix, 7.0, &crackle(seconds - 7.0, 45), 0.2);
                let beat = next.beat();
                let hat = into.drums.hat();
                for count in 0..4u8 {
                    add(&mut mix, 8.0 + f32::from(count) * beat, &hat, 0.9);
                }
                add(&mut mix, 8.0 + 3.5 * beat, &into.drums.kick(), 0.6);
            }
            Change::Modem => {
                let mut noise = Noise(56_000);
                let mut bit = false;
                let mut phases = [0.0_f32; 2];
                let mut level = 0.0_f32;
                for (i, slot) in mix.iter_mut().enumerate() {
                    let t = i as f32 / RATE;
                    // A new data bit 300 times a second.
                    if i % (RATE as usize / 300) == 0 {
                        bit = noise.sample() > 0.0;
                    }
                    let (one, two, hiss) = modem_line(t, bit);
                    let mut sum = hiss * noise.sample() * 0.5;
                    for (phase, frequency) in phases.iter_mut().zip([one, two]) {
                        *phase = (*phase + frequency / RATE).fract();
                        if frequency > 0.0 {
                            sum += (*phase * TAU).sin() * 0.5;
                        }
                    }
                    // Tones come and go without a click.
                    let target = if one > 0.0 || two > 0.0 || hiss > 0.0 {
                        1.0
                    } else {
                        0.0
                    };
                    level += 0.01 * (target - level);
                    *slot = sum * level * 0.5;
                }
            }
        }
        finish(&mut mix);
        mix
    }
}

/// A style: what the user calls a theme of the music.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub name: &'static str,
    /// Its name beside its checkbox in the settings, and in the config.
    pub label: &'static str,
    pub kit: Kit,
    pub songs: [Song; 5],
    /// Four short breaks, then two long ones.
    pub breaks: [Link; 6],
}

use Groove::{Busy, Double, Full, Half};
use Lead::{Both, First, Second, Tacet};

const FORWARDS: Under = Under::Long { backwards: false };
const BACKWARDS: Under = Under::Long { backwards: true };

/// Japan's motto: a call from the fifth up to the octave that falls back
/// through the open notes to the fifth below. No thirds: it fits every key.
const JAPAN_MOTTO: &[MottoNote] = &[
    m(0, 6, 7),
    m(6, 2, 12),
    m(8, 4, 7),
    m(12, 4, 5),
    m(16, 2, 2),
    m(18, 2, 0),
    m(20, 12, -5),
];

/// Retro's motto: a rising fanfare, like the jingle of a save point.
const RETRO_MOTTO: &[MottoNote] = &[
    m(0, 2, 0),
    m(2, 2, 5),
    m(4, 2, 7),
    m(6, 6, 12),
    m(12, 2, 7),
    m(14, 2, 12),
    m(16, 12, 14),
];

/// Babymetal's motto: a gallop on one note that jumps up, twice, and comes
/// down the minor chord.
const METAL_MOTTO: &[MottoNote] = &[
    m(0, 2, 0),
    m(2, 1, 0),
    m(3, 1, 0),
    m(4, 4, 3),
    m(8, 2, 0),
    m(10, 1, 0),
    m(11, 1, 0),
    m(12, 4, 5),
    m(16, 2, 3),
    m(18, 12, 0),
];

/// Cajun's motto: up the major chord to the sixth, a turn, and a jump to
/// the octave.
const CAJUN_MOTTO: &[MottoNote] = &[
    m(0, 2, 0),
    m(2, 2, 4),
    m(4, 2, 7),
    m(6, 2, 9),
    m(8, 2, 7),
    m(10, 2, 4),
    m(12, 4, 7),
    m(16, 2, 0),
    m(18, 8, 12),
];

/// Trip-hop's motto: eight slow notes that fall from the seventh to the
/// first note of the key, the last step but one a half step.
const TRIPHOP_MOTTO: &[MottoNote] = &[
    m(0, 4, 10),
    m(4, 4, 8),
    m(8, 6, 7),
    m(14, 2, 5),
    m(16, 4, 3),
    m(20, 4, 2),
    m(24, 4, 1),
    m(28, 4, 0),
];

/// Japanese traditional lofi: a plucked string and soft keys, a flute for
/// the long tunes, soft drums.
pub const JAPAN: Style = Style {
    name: "Japanese traditional lofi",
    label: "Japanese",
    kit: Kit {
        first: Voice::Pluck,
        second: Voice::Keys,
        long: Voice::Flute,
        pad: Voice::Pad,
        bass: Voice::Bass,
        drums: Drums::Soft,
        space: Space::Dry,
        motto: JAPAN_MOTTO,
        motto_in: Link::Solo,
    },
    songs: [
        Song {
            bpm: 72,
            motif: [57, 60, 64, 67],
            second: [62, 65, 69, 64],
            parts: &[
                part(2, 0, First, Under::None, Groove::None),
                part(4, 0, First, FORWARDS, Full),
                part_b(4, 5, Second, Under::Pad, Full),
                part(2, -2, Tacet, FORWARDS, Half),
                part_b(4, 0, Both, BACKWARDS, Busy),
                part(2, 0, First, Under::Pad, Groove::None),
            ],
        },
        Song {
            bpm: 76,
            motif: [62, 65, 69, 72],
            second: [58, 62, 65, 60],
            parts: &[
                part(4, 0, Second, Under::Pad, Half),
                part(4, 0, First, Under::Pad, Full),
                part_b(4, -5, Both, FORWARDS, Full),
                part(2, -2, Tacet, Under::Pad, Groove::None),
                part_b(4, 0, First, BACKWARDS, Busy),
                part(2, 0, Second, Under::None, Half),
            ],
        },
        Song {
            bpm: 68,
            motif: [60, 64, 67, 71],
            second: [57, 60, 64, 59],
            parts: &[
                part(2, 0, Tacet, Under::Pad, Groove::None),
                part(4, 0, First, Under::None, Half),
                part_b(4, 5, First, FORWARDS, Full),
                part_b(4, -3, Second, BACKWARDS, Full),
                part(2, 0, Both, Under::Pad, Half),
            ],
        },
        Song {
            bpm: 80,
            motif: [55, 58, 62, 65],
            second: [51, 55, 58, 53],
            parts: &[
                part(4, 0, First, BACKWARDS, Full),
                part(4, 3, Second, Under::None, Busy),
                part_b(2, 0, Tacet, Under::None, Full),
                part(4, -2, Both, Under::Pad, Full),
                part_b(4, 0, First, FORWARDS, Full),
                part(2, 0, Second, Under::Pad, Groove::None),
            ],
        },
        Song {
            bpm: 66,
            motif: [61, 64, 68, 71],
            second: [66, 69, 73, 68],
            parts: &[
                part(4, 0, Second, FORWARDS, Groove::None),
                part(4, 0, First, Under::Pad, Half),
                part_b(4, 5, Both, BACKWARDS, Full),
                part_b(2, 7, Tacet, FORWARDS, Half),
                part(2, 0, First, Under::None, Half),
            ],
        },
    ],
    breaks: [
        Link::Notes(false),
        Link::Roll,
        Link::Hum(Vowel::Ooh),
        Link::Fall,
        Link::Solo,
        Link::Build,
    ],
};

/// Retro gaming lofi: the square, pulse and triangle waves of old game
/// consoles, and their drums made of noise.
pub const RETRO: Style = Style {
    name: "Retro gaming lofi",
    label: "Retro",
    kit: Kit {
        first: Voice::Square,
        second: Voice::Pulse,
        long: Voice::Triangle,
        pad: Voice::ChipPad,
        bass: Voice::Triangle,
        drums: Drums::Chip,
        space: Space::Dry,
        motto: RETRO_MOTTO,
        motto_in: Link::Solo,
    },
    songs: [
        Song {
            bpm: 84,
            motif: [60, 64, 67, 72],
            second: [65, 69, 72, 67],
            parts: &[
                part(2, 0, First, Under::None, Half),
                part(4, 0, First, Under::Pad, Full),
                part_b(4, 5, Second, FORWARDS, Full),
                part(4, 7, Both, Under::Pad, Busy),
                part_b(2, 0, Tacet, BACKWARDS, Half),
                part(4, 0, Both, FORWARDS, Busy),
            ],
        },
        Song {
            bpm: 88,
            motif: [62, 66, 69, 74],
            second: [59, 62, 66, 61],
            parts: &[
                part(4, 0, Second, Under::None, Groove::None),
                part(4, 0, First, FORWARDS, Full),
                part_b(4, -2, Both, Under::Pad, Full),
                part(2, 5, Tacet, Under::Pad, Half),
                part_b(4, 7, First, BACKWARDS, Busy),
                part(4, 0, Second, Under::Pad, Full),
            ],
        },
        Song {
            bpm: 80,
            motif: [57, 60, 64, 69],
            second: [53, 57, 60, 55],
            parts: &[
                part(4, 0, First, BACKWARDS, Half),
                part(4, 3, Second, Under::Pad, Full),
                part_b(2, 0, Tacet, Under::None, Busy),
                part(4, -4, Both, FORWARDS, Full),
                part_b(4, 0, First, Under::Pad, Busy),
                part(2, 0, Second, Under::None, Groove::None),
            ],
        },
        Song {
            bpm: 76,
            motif: [65, 69, 72, 77],
            second: [62, 65, 69, 64],
            parts: &[
                part(2, 0, Tacet, Under::Pad, Groove::None),
                part(4, 0, Second, Under::None, Half),
                part_b(4, -5, First, FORWARDS, Full),
                part(4, -7, Both, BACKWARDS, Full),
                part_b(4, 0, First, Under::Pad, Busy),
                part(2, 0, Second, FORWARDS, Half),
            ],
        },
        Song {
            bpm: 86,
            motif: [55, 59, 62, 67],
            second: [60, 64, 67, 62],
            parts: &[
                part(4, 0, Both, Under::None, Full),
                part(4, 5, First, Under::Pad, Busy),
                part_b(2, 7, Tacet, FORWARDS, Half),
                part(4, 0, Second, BACKWARDS, Full),
                part_b(4, 2, Both, Under::Pad, Busy),
                part(4, 0, First, FORWARDS, Half),
            ],
        },
    ],
    breaks: [
        Link::Arp,
        Link::Coin,
        Link::Notes(true),
        Link::Roll,
        Link::Solo,
        Link::Climb,
    ],
};

/// Babymetal-style lofi: an overdriven guitar and a high voice over struck
/// chords and double bass drums, slowed down.
pub const METAL: Style = Style {
    name: "Babymetal-style lofi",
    label: "Babymetal",
    kit: Kit {
        first: Voice::Guitar,
        second: Voice::Idol,
        long: Voice::IdolOoh,
        pad: Voice::Chords,
        bass: Voice::Growl,
        drums: Drums::Metal,
        space: Space::Dry,
        motto: METAL_MOTTO,
        motto_in: Link::Build,
    },
    songs: [
        Song {
            bpm: 88,
            motif: [52, 55, 59, 64],
            second: [57, 60, 64, 59],
            parts: &[
                part(2, 0, First, Under::None, Groove::None),
                part(4, 0, First, Under::Pad, Double),
                part_b(4, 5, Second, Under::Pad, Full),
                part(4, 3, Both, FORWARDS, Double),
                part_b(2, 0, Tacet, Under::Pad, Half),
                part_b(4, 0, Second, BACKWARDS, Double),
                part(2, 0, First, Under::None, Half),
            ],
        },
        Song {
            bpm: 84,
            motif: [50, 53, 57, 62],
            second: [46, 50, 53, 48],
            parts: &[
                part(4, 0, Second, FORWARDS, Half),
                part(4, 0, First, Under::Pad, Double),
                part_b(4, 7, Both, Under::Pad, Full),
                part(2, 5, Tacet, Under::None, Double),
                part_b(4, 0, First, BACKWARDS, Double),
                part(4, 0, Second, Under::Pad, Full),
            ],
        },
        Song {
            bpm: 80,
            motif: [55, 58, 62, 67],
            second: [60, 63, 67, 62],
            parts: &[
                part(2, 0, Tacet, Under::Pad, Groove::None),
                part(4, 0, First, Under::None, Full),
                part_b(4, -3, Second, FORWARDS, Double),
                part(4, 2, Both, Under::Pad, Double),
                part_b(4, 0, First, BACKWARDS, Full),
                part(2, 0, Second, Under::Pad, Half),
            ],
        },
        Song {
            bpm: 86,
            motif: [59, 62, 66, 71],
            second: [55, 59, 62, 57],
            parts: &[
                part(4, 0, First, Under::Pad, Full),
                part(4, -2, Second, BACKWARDS, Double),
                part_b(2, 0, Tacet, FORWARDS, Half),
                part(4, 5, Both, Under::Pad, Double),
                part_b(4, 0, First, Under::None, Double),
                part(4, 0, Second, FORWARDS, Full),
            ],
        },
        Song {
            bpm: 78,
            motif: [54, 57, 61, 66],
            second: [59, 62, 66, 61],
            parts: &[
                part(4, 0, Second, Under::Pad, Groove::None),
                part(4, 0, First, FORWARDS, Full),
                part_b(4, 3, Both, Under::None, Double),
                part(2, -2, Tacet, Under::Pad, Half),
                part_b(4, 0, First, BACKWARDS, Double),
                part(2, 0, Second, Under::Pad, Half),
            ],
        },
    ],
    breaks: [
        Link::Stab,
        Link::Hum(Vowel::Aah),
        Link::Blast,
        Link::Fall,
        Link::Build,
        Link::Chant,
    ],
};

/// The styles the music moves through.
/// Cajun porch lofi: a banjo leads and a fiddle answers and plays the long
/// tunes, over a washtub bass, a stomping foot and a washboard.
pub const CAJUN: Style = Style {
    name: "Cajun porch lofi",
    label: "Cajun",
    kit: Kit {
        first: Voice::Banjo,
        second: Voice::Fiddle,
        long: Voice::Fiddle,
        pad: Voice::Fiddle,
        bass: Voice::Washtub,
        drums: Drums::Porch,
        space: Space::Dry,
        motto: CAJUN_MOTTO,
        motto_in: Link::Solo,
    },
    songs: [
        Song {
            bpm: 104,
            motif: [55, 59, 62, 64],
            second: [60, 64, 67, 62],
            parts: &[
                part(2, 0, First, Under::None, Half),
                part(4, 0, First, Under::Pad, Full),
                part_b(4, 5, Both, FORWARDS, Busy),
                part_b(4, 7, Second, Under::Pad, Full),
                part(2, 0, Tacet, BACKWARDS, Half),
                part(4, 5, First, FORWARDS, Busy),
                part_b(4, 0, Both, Under::Pad, Full),
                part(2, 0, First, Under::None, Half),
            ],
        },
        Song {
            bpm: 112,
            motif: [57, 61, 64, 66],
            second: [54, 57, 61, 56],
            parts: &[
                part(4, 0, First, Under::None, Full),
                part(4, 0, Both, Under::Pad, Busy),
                part_b(4, 5, Second, FORWARDS, Full),
                part_b(2, 7, Tacet, Under::Pad, Half),
                part(4, 0, First, BACKWARDS, Busy),
                part(4, -5, Both, Under::Pad, Full),
                part_b(4, 0, First, FORWARDS, Busy),
                part(2, 0, Second, Under::None, Half),
            ],
        },
        Song {
            bpm: 96,
            motif: [60, 64, 67, 69],
            second: [65, 69, 72, 67],
            parts: &[
                part(2, 0, Tacet, Under::Pad, Groove::None),
                part(4, 0, First, Under::None, Half),
                part_b(4, 5, First, FORWARDS, Full),
                part(4, -2, Second, BACKWARDS, Full),
                part_b(4, 7, Both, Under::Pad, Busy),
                part_b(4, 0, First, FORWARDS, Full),
                part(2, 0, Second, Under::Pad, Half),
            ],
        },
        Song {
            bpm: 108,
            motif: [62, 66, 69, 71],
            second: [59, 62, 66, 61],
            parts: &[
                part(4, 0, Both, FORWARDS, Full),
                part(4, 7, First, Under::Pad, Busy),
                part_b(2, 5, Tacet, Under::None, Half),
                part(4, 0, Second, BACKWARDS, Full),
                part_b(4, 2, First, Under::Pad, Busy),
                part_b(4, 0, Both, FORWARDS, Busy),
                part(4, 0, First, Under::None, Full),
            ],
        },
        Song {
            bpm: 100,
            motif: [52, 56, 59, 61],
            second: [57, 61, 64, 59],
            parts: &[
                part(2, 0, First, Under::None, Groove::None),
                part(4, 0, First, BACKWARDS, Half),
                part_b(4, 5, Second, Under::Pad, Full),
                part_b(4, 0, Both, FORWARDS, Busy),
                part(2, -5, Tacet, Under::Pad, Half),
                part(4, 7, First, Under::Pad, Full),
                part_b(4, 0, Both, BACKWARDS, Busy),
                part(2, 0, Second, Under::None, Half),
            ],
        },
    ],
    breaks: [
        Link::Rolls,
        Link::Twang,
        Link::Notes(true),
        Link::Roll,
        Link::Solo,
        Link::Porch,
    ],
};

/// Trip-hop lofi: a theremin over a slow, heavy beat, a sub bass and dark
/// keys, on a crackling record, with a war far off.
pub const TRIPHOP: Style = Style {
    name: "Trip-hop lofi",
    label: "Trip-hop",
    kit: Kit {
        first: Voice::Theremin,
        second: Voice::DarkKeys,
        long: Voice::Theremin,
        pad: Voice::DarkKeys,
        bass: Voice::Sub,
        drums: Drums::Dusty,
        space: Space::Far,
        motto: TRIPHOP_MOTTO,
        motto_in: Link::Battlefield,
    },
    songs: [
        Song {
            bpm: 80,
            motif: [50, 53, 57, 60],
            second: [55, 58, 62, 57],
            parts: &[
                part(2, 0, Tacet, Under::Pad, Groove::None),
                part(4, 0, First, Under::Pad, Half),
                part_b(4, 0, First, FORWARDS, Full),
                part(4, 5, Second, Under::Pad, Full),
                part_b(2, -2, Tacet, BACKWARDS, Half),
                part(4, 0, Both, FORWARDS, Busy),
            ],
        },
        Song {
            bpm: 76,
            motif: [52, 55, 59, 62],
            second: [48, 52, 55, 50],
            parts: &[
                part(4, 0, First, Under::None, Half),
                part(4, 0, Second, Under::Pad, Full),
                part_b(4, -5, First, FORWARDS, Full),
                part_b(2, 3, Tacet, Under::Pad, Groove::None),
                part(4, 0, Both, BACKWARDS, Full),
            ],
        },
        Song {
            bpm: 84,
            motif: [59, 62, 66, 69],
            second: [64, 67, 71, 66],
            parts: &[
                part(2, 0, First, Under::None, Groove::None),
                part(4, 0, First, Under::Pad, Full),
                part_b(4, -4, Both, BACKWARDS, Busy),
                part(4, 0, Second, FORWARDS, Full),
                part_b(2, 5, Tacet, Under::Pad, Half),
                part_b(4, 0, First, FORWARDS, Busy),
                part(2, 0, Second, Under::None, Half),
            ],
        },
        Song {
            bpm: 78,
            motif: [54, 57, 61, 64],
            second: [50, 54, 57, 52],
            parts: &[
                part(4, 0, Second, Under::Pad, Groove::None),
                part(4, 0, First, FORWARDS, Half),
                part_b(4, 7, First, Under::Pad, Full),
                part(2, 5, Tacet, Under::None, Full),
                part_b(4, 0, Both, BACKWARDS, Full),
                part(2, 0, First, Under::Pad, Half),
            ],
        },
        Song {
            bpm: 82,
            motif: [53, 56, 60, 63],
            second: [58, 61, 65, 60],
            parts: &[
                part(2, 0, Tacet, FORWARDS, Groove::None),
                part(4, 0, First, Under::None, Half),
                part_b(4, 3, Second, Under::Pad, Full),
                part(4, -2, First, BACKWARDS, Full),
                part_b(4, 0, Both, Under::Pad, Busy),
                part(2, 0, First, Under::None, Half),
            ],
        },
    ],
    breaks: [
        Link::Glide,
        Link::Needle,
        Link::Notes(true),
        Link::Stab,
        Link::Solo,
        Link::Battlefield,
    ],
};

pub static STYLES: [Style; 5] = [JAPAN, RETRO, METAL, CAJUN, TRIPHOP];

/// After a song the music leaves its style one time in this many.
pub const CHANGE_ONE_IN: usize = 4;

/// The styles the music may play, as numbers into `styles`: all but the
/// ones whose label is in `off`. All of them when none would be left.
pub fn styles_on(styles: &[Style], off: &[String]) -> Vec<usize> {
    let on: Vec<usize> = (0..styles.len())
        .filter(|i| {
            styles
                .get(*i)
                .is_some_and(|s| !off.iter().any(|o| o == s.label))
        })
        .collect();
    if on.is_empty() {
        (0..styles.len()).collect()
    } else {
        on
    }
}

/// One thing the music plays. The numbers count into the styles the
/// program runs over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passage {
    Song {
        style: usize,
        song: usize,
        /// Decides the song's variation.
        seed: u64,
    },
    /// A break of `style` before its song `into`.
    Break {
        style: usize,
        link: usize,
        into: usize,
    },
    /// A change break from a (style, song) into another.
    Change {
        change: usize,
        from: (usize, usize),
        into: (usize, usize),
    },
}

fn song_of(styles: &[Style], (style, song): (usize, usize)) -> Option<(&Kit, &Song)> {
    let style = styles.get(style)?;
    Some((&style.kit, style.songs.get(song)?))
}

impl Passage {
    /// Whether every style it plays is one of `styles`.
    pub fn within(&self, styles: &[usize]) -> bool {
        match *self {
            Passage::Song { style, .. } | Passage::Break { style, .. } => styles.contains(&style),
            Passage::Change { from, into, .. } => {
                styles.contains(&from.0) && styles.contains(&into.0)
            }
        }
    }

    /// The passage as samples; nothing for numbers that name no song.
    pub fn render(&self, styles: &[Style]) -> Vec<f32> {
        let rendered = match *self {
            Passage::Song { style, song, seed } => {
                song_of(styles, (style, song)).map(|(kit, song)| song.render(kit, seed))
            }
            Passage::Break { style, link, into } => styles.get(style).and_then(|s| {
                let link = s.breaks.get(link)?;
                Some(link.render(&s.kit, s.songs.get(into)?))
            }),
            Passage::Change { change, from, into } => CHANGES.get(change).and_then(|change| {
                let (from, last) = song_of(styles, from)?;
                let (into, next) = song_of(styles, into)?;
                Some(change.render(from, last, into, next))
            }),
        };
        rendered.unwrap_or_default()
    }
}

/// Decides what plays next: song, break, song, break, and now and then,
/// by chance, a change of style instead of a break.
#[derive(Debug, Clone)]
pub struct Program {
    styles: &'static [Style],
    /// The styles that may play, as numbers into `styles`.
    on: Vec<usize>,
    random: u64,
    /// The style and song that play, or played last.
    at: Option<(usize, usize)>,
    /// Songs of this style not played yet since the bag was last filled.
    songs: Vec<usize>,
    /// Breaks of this style not played yet.
    links: Vec<usize>,
    /// The song the last break led into.
    queued: Option<Passage>,
}

impl Program {
    /// The program of the app's styles, in an order drawn from `seed`.
    pub fn new(seed: u64) -> Self {
        Self::over(&STYLES, seed)
    }

    pub fn over(styles: &'static [Style], seed: u64) -> Self {
        Self {
            styles,
            on: (0..styles.len()).collect(),
            random: seed,
            at: None,
            songs: Vec::new(),
            links: Vec::new(),
            queued: None,
        }
    }

    pub fn styles(&self) -> &'static [Style] {
        self.styles
    }

    /// Let only these styles play from the next passage on; all of them
    /// when `on` names none. What was planned stays: a caller that must
    /// not hear a style any more starts over with `restart`.
    pub fn allow(&mut self, on: &[usize]) {
        let mut on: Vec<usize> = on
            .iter()
            .copied()
            .filter(|i| *i < self.styles.len())
            .collect();
        on.sort_unstable();
        on.dedup();
        if on.is_empty() {
            on = (0..self.styles.len()).collect();
        }
        self.on = on;
    }

    /// Forget what played and what was planned: a song comes next.
    pub fn restart(&mut self) {
        self.at = None;
        self.queued = None;
        self.songs.clear();
        self.links.clear();
    }

    /// One of the styles that may play, but not `not`.
    fn draw_style(&mut self, not: Option<usize>) -> Option<usize> {
        let from: Vec<usize> = self
            .on
            .iter()
            .copied()
            .filter(|i| Some(*i) != not)
            .collect();
        let at = self.draw(from.len());
        from.get(at).copied()
    }

    fn draw(&mut self, below: usize) -> usize {
        self.random = shuffle(self.random);
        ((self.random >> 16) % below.max(1) as u64) as usize
    }

    /// Take one number out of `bag`; an empty bag is first filled with
    /// `0..size` without `not`, so nothing comes twice in a row.
    fn take(&mut self, songs: bool, size: usize, not: Option<usize>) -> usize {
        let mut bag = std::mem::take(if songs {
            &mut self.songs
        } else {
            &mut self.links
        });
        if bag.is_empty() {
            bag = (0..size).filter(|i| Some(*i) != not).collect();
        }
        let at = self.draw(bag.len());
        let taken = if at < bag.len() { bag.remove(at) } else { 0 };
        *(if songs {
            &mut self.songs
        } else {
            &mut self.links
        }) = bag;
        taken
    }

    fn song(&mut self, style: usize, song: usize) -> Passage {
        self.at = Some((style, song));
        self.random = shuffle(self.random);
        Passage::Song {
            style,
            song,
            seed: self.random,
        }
    }

    /// What plays next.
    pub fn next_passage(&mut self) -> Passage {
        if let Some(song) = self.queued.take() {
            return song;
        }
        let Some((style, last)) = self.at else {
            let style = self.draw_style(None).unwrap_or(0);
            let song = self.take(true, 5, None);
            return self.song(style, song);
        };
        let others = self.on.iter().any(|i| *i != style);
        if others
            && self.draw(CHANGE_ONE_IN) == 0
            && let Some(next) = self.draw_style(Some(style))
        {
            self.songs.clear();
            self.links.clear();
            let song = self.take(true, 5, None);
            let change = self.draw(CHANGES.len());
            self.queued = Some(self.song(next, song));
            return Passage::Change {
                change,
                from: (style, last),
                into: (next, song),
            };
        }
        let song = self.take(true, 5, Some(last));
        let link = self.take(false, 6, None);
        self.queued = Some(self.song(style, song));
        Passage::Break {
            style,
            link,
            into: song,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
    }

    fn check(name: &str, samples: &[f32]) {
        assert!(samples.iter().all(|s| s.is_finite()), "{name}: not finite");
        assert!(peak(samples) <= 1.0, "{name}: peak {}", peak(samples));
        assert!(peak(samples) > 0.05, "{name}: silent");
    }

    fn plain() -> Variation {
        Variation {
            swap: false,
            turn: 0,
            fill: Fill::None,
            skip: 1,
            mirror: false,
            motto: None,
            spot: 0,
        }
    }

    #[test]
    fn every_style_has_five_songs_of_about_a_minute_that_change_inside() {
        for style in &STYLES {
            let mut motifs: Vec<_> = style.songs.iter().map(|s| s.motif).collect();
            motifs.sort_unstable();
            motifs.dedup();
            assert_eq!(motifs.len(), 5, "{}: each song its own motif", style.name);
            for (i, song) in style.songs.iter().enumerate() {
                let name = format!("{} {i}", style.name);
                let seconds = song.seconds();
                assert!((54.0..=66.0).contains(&seconds), "{name}: {seconds} s");
                assert!(song.parts.len() >= 5, "{name}");
                // Not one passage looped: keys, leads, backings and grooves
                // change from part to part.
                let kinds = |of: fn(&Part) -> String| {
                    let mut all: Vec<_> = song.parts.iter().map(of).collect();
                    all.sort();
                    all.dedup();
                    all.len()
                };
                assert!(kinds(|p| p.shift.to_string()) >= 3, "{name}: keys");
                assert!(kinds(|p| format!("{:?}", p.lead)) >= 3, "{name}: leads");
                assert!(kinds(|p| format!("{:?}", p.under)) >= 3, "{name}: under");
                assert!(kinds(|p| format!("{:?}", p.groove)) >= 2, "{name}: grooves");
                assert!(song.parts.iter().all(|p| p.bars % 2 == 0), "{name}");
            }
        }
    }

    /// The notes of each bar of a melody, with their place in the bar.
    fn bars_of(song: &Song, variation: &Variation) -> Vec<Vec<(u32, u32, u8)>> {
        let mut bars = vec![Vec::new(); song.bars()];
        for n in song.melody(variation) {
            let bar = (n.at / 4.0) as usize;
            let within = n.at - bar as f32 * 4.0;
            bars[bar].push(((within * 2.0) as u32, (n.beats * 2.0) as u32, n.midi));
        }
        bars
    }

    #[test]
    fn the_melody_states_the_motif_once_and_then_varies_it() {
        for song in STYLES.iter().flat_map(|s| &s.songs) {
            let bars = bars_of(song, &plain());
            let stated: Vec<_> = (0..4u32).map(|i| (i * 2, 2, 0)).collect();
            let plain_bars = bars
                .iter()
                .filter(|bar| {
                    bar.len() == 4
                        && bar
                            .iter()
                            .zip(&stated)
                            .all(|(n, s)| (n.0, n.1) == (s.0, s.1))
                })
                .filter(|bar| bar.windows(2).all(|w| w[0].2 < w[1].2))
                .count();
            assert_eq!(plain_bars, 1, "said outright once: {bars:?}");
            let sung: Vec<_> = bars.iter().filter(|b| !b.is_empty()).collect();
            let first: Vec<u8> = sung[0].iter().map(|n| n.2).collect();
            let shift = song
                .parts
                .iter()
                .find(|p| p.lead != Lead::Tacet)
                .unwrap()
                .shift;
            assert_eq!(first, moved(song.motif, shift));
            // No two neighbours among the sung bars are the same.
            assert!(sung.windows(2).all(|w| w[0] != w[1]), "{bars:?}");
            // Tacet parts have no melody.
            let mut bar = 0;
            for part in song.parts {
                for _ in 0..part.bars {
                    assert_eq!(bars[bar].is_empty(), part.lead == Lead::Tacet);
                    bar += 1;
                }
            }
        }
    }

    #[test]
    fn every_song_has_a_second_theme_that_some_parts_play_whole() {
        for style in &STYLES {
            for (i, song) in style.songs.iter().enumerate() {
                let name = format!("{} {i}", style.name);
                let (a, b) = (song.motif, song.second);
                assert_ne!(a[0] % 12, b[0] % 12, "{name}: another root");
                assert!(b[3] < b[2], "{name}: it does not rise all the way");
                // The same scale: with the motif's notes it stays within
                // seven notes, as the notes of one key do.
                let mut notes: Vec<u8> = a.iter().chain(&b).map(|n| n % 12).collect();
                notes.sort_unstable();
                notes.dedup();
                assert!(notes.len() <= 7, "{name}: {notes:?}");
                let second: Vec<bool> = song.parts.iter().map(|p| p.b).collect();
                let count = second.iter().filter(|b| **b).count();
                assert!((2..=3).contains(&count), "{name}: {count} parts");
                assert!(!second[0], "{name}: opens with its own theme");
                assert!(!second[second.len() - 1], "{name}: and comes home");
            }
        }
        // A part on the second theme has its notes in the melody, the long
        // tune, the chord and the bass.
        let song = &JAPAN.songs[0];
        let (at, part) = song
            .parts
            .iter()
            .scan(0, |start, part| {
                let at = *start;
                *start += usize::from(part.bars);
                Some((at, part))
            })
            .find(|(_, part)| part.b && part.lead != Lead::Tacet)
            .unwrap();
        let theme = moved(song.second, part.shift);
        assert_eq!(song.theme(part), theme);
        let bars = bars_of(song, &plain());
        let stated: Vec<u8> = bars[at].iter().map(|n| n.2).collect();
        assert_eq!(stated, theme, "said outright where it first comes");
        let as_first = Song {
            second: song.motif,
            ..*song
        };
        let variation = plain();
        let bar = samples(4.0 * song.beat());
        let (with, without) = (
            song.instruments(&JAPAN.kit, &variation),
            as_first.instruments(&JAPAN.kit, &variation),
        );
        assert_ne!(
            with[at * bar..(at + 1) * bar],
            without[at * bar..(at + 1) * bar]
        );
        assert_eq!(with[..bar], without[..bar], "the first part is the motif");
        // The bass of that part is on the second theme's root.
        let low = pitch(theme[0].saturating_sub(24));
        let there = &with[at * bar..at * bar + samples(1.5 * song.beat())];
        let other = pitch(moved(song.motif, part.shift)[0].saturating_sub(24));
        assert!(tone(there, low) > tone(there, other), "the bass moved");
        // A long tune in a part on the second theme plays its notes.
        for song in STYLES.iter().flat_map(|s| &s.songs) {
            let mut start = 0;
            for part in song.parts {
                if part.b && matches!(part.under, Under::Long { .. }) {
                    let tune = song.long_tune(&plain());
                    let note = tune.iter().find(|n| n.at == (start * 4) as f32).unwrap();
                    let pitches: Vec<u8> = song.theme(part).iter().map(|m| m % 12).collect();
                    assert!(pitches.contains(&(note.midi % 12)));
                }
                start += usize::from(part.bars);
            }
        }
    }

    #[test]
    fn every_style_has_a_motto_of_its_own_that_is_always_the_same_phrase() {
        let mottos: Vec<_> = STYLES.iter().map(|s| s.kit.motto).collect();
        for (style, motto) in STYLES.iter().zip(&mottos) {
            assert!((6..=10).contains(&motto.len()), "{}", style.name);
            // Two bars, the notes one after the other.
            assert!(
                motto.iter().all(|n| n.at + n.length <= 32),
                "{}",
                style.name
            );
            assert!(motto.windows(2).all(|w| w[0].at + w[0].length <= w[1].at));
            // A rhythm of its own: not every note as long as the others.
            assert!(motto.iter().any(|n| n.length != motto[0].length));
            assert!(
                style.breaks[4..].contains(&style.kit.motto_in),
                "a long break"
            );
        }
        for (i, a) in mottos.iter().enumerate() {
            for b in mottos.iter().skip(i + 1) {
                assert_ne!(a, b);
                let shape = |m: &[MottoNote]| -> Vec<i8> {
                    m.windows(2)
                        .map(|w| (w[1].above - w[0].above).signum())
                        .collect()
                };
                assert_ne!(shape(a), shape(b), "each another shape");
            }
        }
        // Retro's is a fanfare that rises, trip-hop's eight notes that fall
        // slowly, with a half step, and end lower than they began.
        let retro = RETRO.kit.motto;
        assert!(retro[retro.len() - 1].above > retro[0].above + 12);
        let dark = TRIPHOP.kit.motto;
        assert_eq!(dark.len(), 8);
        assert!(dark.windows(2).all(|w| w[1].above < w[0].above), "falls");
        assert!(dark.windows(2).any(|w| w[0].above - w[1].above == 1));
        assert!(dark.iter().all(|n| n.length >= 2), "slow");
        // Moved into a key it is the same phrase, higher or lower.
        let (low, high) = (motto_notes(dark, 50), motto_notes(dark, 57));
        assert_eq!(low[0], (0.0, 1.0, 60));
        for (a, b) in low.iter().zip(&high) {
            assert_eq!((a.0, a.1, a.2 + 7), *b);
        }
    }

    #[test]
    fn the_motto_comes_in_about_one_song_of_two_where_the_lead_rests() {
        let song = &TRIPHOP.songs[0];
        let (mut with, mut by) = (0, std::collections::HashSet::new());
        let mut places = std::collections::HashSet::new();
        for seed in 0..400u64 {
            let variation = Variation::of(seed, song.bars());
            assert_eq!(
                song.motto_place(&variation).is_some(),
                variation.motto.is_some()
            );
            if let (Some(who), Some((bar, _))) = (variation.motto, song.motto_place(&variation)) {
                with += 1;
                by.insert(format!("{who:?}"));
                places.insert(bar);
            }
        }
        assert!((140..=260).contains(&with), "{with} of 400");
        assert_eq!(by.len(), 3, "lead, long instrument and bass: {by:?}");
        // Its places are the parts where the lead rests: bars 0 and 14 here.
        let mut places: Vec<_> = places.into_iter().collect();
        places.sort_unstable();
        assert_eq!(places, [0, 14]);

        // Played, it is in those two bars and nowhere else, in the key of
        // its part; each player sounds different.
        let bar = samples(4.0 * song.beat());
        let none = song.instruments(&TRIPHOP.kit, &plain());
        let mut heard = Vec::new();
        for who in [MottoBy::Lead, MottoBy::Long, MottoBy::Bass] {
            let variation = Variation {
                motto: Some(who),
                spot: 1,
                ..plain()
            };
            assert_eq!(
                song.motto_place(&variation),
                Some((14, 48)),
                "shifted by -2"
            );
            let dry = Kit {
                space: Space::Dry,
                ..TRIPHOP.kit
            };
            let (played, without) = (
                song.instruments(&dry, &variation),
                song.instruments(&dry, &plain()),
            );
            assert_eq!(played[..14 * bar], without[..14 * bar]);
            assert_ne!(played[14 * bar..16 * bar], without[14 * bar..16 * bar]);
            let after = 16 * bar + samples(song.beat());
            assert_eq!(played[after..], without[after..]);
            heard.push(played);
        }
        assert_ne!(heard[0], heard[1]);
        assert_ne!(heard[1], heard[2]);
        assert!(peak(&none) > 0.1);

        // A song without a resting part gives the motto its last two bars,
        // and the melody is silent there.
        static NO_REST: [Part; 2] = [
            part(4, 0, First, Under::Pad, Full),
            part(4, 5, Second, Under::None, Full),
        ];
        let busy = Song {
            parts: &NO_REST,
            ..*song
        };
        let variation = Variation {
            motto: Some(MottoBy::Lead),
            spot: 5,
            ..plain()
        };
        assert_eq!(busy.motto_place(&variation), Some((6, 55)));
        let alone = Kit {
            motto: &[],
            space: Space::Dry,
            ..TRIPHOP.kit
        };
        let quiet = busy.instruments(&alone, &variation);
        let sung = busy.instruments(&alone, &plain());
        assert_eq!(quiet[..6 * bar], sung[..6 * bar]);
        assert_ne!(
            quiet[6 * bar..],
            sung[6 * bar..],
            "the melody left its bars"
        );
    }

    #[test]
    fn one_long_break_of_every_style_plays_the_motto() {
        for style in &STYLES {
            let next = &style.songs[0];
            let silent = Kit {
                motto: &[],
                ..style.kit
            };
            for link in style.breaks {
                let (with, without) = (link.render(&style.kit, next), link.render(&silent, next));
                assert_eq!(
                    with != without,
                    link == style.kit.motto_in,
                    "{} {link:?}",
                    style.name
                );
            }
        }
        // The breaks that lead into another style know no motto.
        let (last, next) = (&JAPAN.songs[0], &RETRO.songs[0]);
        let silent = Kit {
            motto: &[],
            ..RETRO.kit
        };
        for change in CHANGES {
            assert_eq!(
                change.render(&JAPAN.kit, last, &RETRO.kit, next),
                change.render(&JAPAN.kit, last, &silent, next)
            );
        }
    }

    #[test]
    fn chance_changes_the_turn_the_leads_the_fills_and_the_long_tunes() {
        let song = &JAPAN.songs[0];
        let mut seen = std::collections::HashSet::new();
        for seed in 1..200u64 {
            let v = Variation::of(seed, song.bars());
            assert!((1..song.bars() - 1).contains(&v.skip), "{v:?}");
            assert!(v.turn < SHAPES.len());
            seen.insert((v.swap, v.turn, format!("{:?}", v.fill), v.mirror));
        }
        assert!(seen.len() > 40, "{}", seen.len());

        let turned = Variation { turn: 3, ..plain() };
        assert_ne!(bars_of(song, &plain()), bars_of(song, &turned));
        // The motif is still stated first, whatever the turn.
        assert_eq!(bars_of(song, &plain())[0], bars_of(song, &turned)[0]);
        let swapped = Variation {
            swap: true,
            ..plain()
        };
        let seconds = |v: &Variation| song.melody(v).iter().filter(|n| n.second).count();
        assert_ne!(seconds(&plain()), seconds(&swapped));

        // The long tune: eight beats a note, the part's motif, and the
        // other way round when mirrored.
        let tune = song.long_tune(&plain());
        assert!(tune.iter().all(|n| n.beats == 8.0));
        let first: Vec<u8> = tune.iter().take(2).map(|n| n.midi).collect();
        assert_eq!(first, [69, 72], "the motif an octave up where it is low");
        assert_eq!(tune[0].at, 8.0, "after the two bars of the first part");
        let mirrored = song.long_tune(&Variation {
            mirror: true,
            ..plain()
        });
        assert_eq!(mirrored.len(), tune.len());
        assert_eq!(mirrored[0].midi, 67);
    }

    #[test]
    fn a_song_renders_to_its_length_differently_for_every_seed() {
        let song = &JAPAN.songs[2];
        let one = song.render(&JAPAN.kit, 1);
        check("song", &one);
        assert_eq!(one.len(), samples(song.seconds()));
        assert!(one.last().unwrap().abs() < 0.01, "ends quiet");
        assert_eq!(one, song.render(&JAPAN.kit, 1), "a seed is a version");
        let mut other = 2;
        while Variation::of(other, song.bars()) == Variation::of(1, song.bars()) {
            other += 1;
        }
        let two = song.render(&JAPAN.kit, other);
        assert_eq!(one.len(), two.len());
        assert_ne!(one, two);
    }

    #[test]
    fn the_instruments_stay_behind_the_drums_and_a_bar_has_no_drums() {
        let song = &JAPAN.songs[0];
        let variation = Variation {
            skip: 3,
            fill: Fill::Roll,
            ..plain()
        };
        let instruments = song.instruments(&JAPAN.kit, &variation);
        let drums = song.drums(&JAPAN.kit, &variation);
        assert!(
            peak(&instruments) < peak(&drums) * 0.75,
            "{} against {}",
            peak(&instruments),
            peak(&drums)
        );
        let bar = samples(4.0 * song.beat());
        assert_eq!(peak(&drums[3 * bar..4 * bar - 6000]), 0.0, "bar 3 left out");
        assert!(peak(&drums[2 * bar..3 * bar]) > 0.3);
        assert_eq!(peak(&drums[..2 * bar - 6000]), 0.0, "no drums in the intro");
        // A fill changes the end of a part.
        let unfilled = song.drums(&JAPAN.kit, &Variation { skip: 3, ..plain() });
        assert_ne!(drums[5 * bar..6 * bar], unfilled[5 * bar..6 * bar]);
        assert_eq!(drums[4 * bar..5 * bar], unfilled[4 * bar..5 * bar]);
        let kicks = song.drums(
            &JAPAN.kit,
            &Variation {
                skip: 3,
                fill: Fill::Kicks,
                ..plain()
            },
        );
        assert_ne!(kicks[5 * bar..6 * bar], drums[5 * bar..6 * bar]);
    }

    #[test]
    fn every_style_has_four_short_and_two_long_breaks_all_different() {
        for style in &STYLES {
            for song in &style.songs {
                for (i, link) in style.breaks.iter().enumerate() {
                    let seconds = link.seconds(song);
                    let range = if i < 4 { 2.0..=4.0 } else { 8.0..=15.0 };
                    assert!(
                        range.contains(&seconds),
                        "{} {link:?}: {seconds}",
                        style.name
                    );
                }
            }
            let next = &style.songs[0];
            let rendered: Vec<_> = style
                .breaks
                .iter()
                .map(|l| l.render(&style.kit, next))
                .collect();
            for (link, sound) in style.breaks.iter().zip(&rendered) {
                check(&format!("{link:?}"), sound);
                assert_eq!(sound.len(), samples(link.seconds(next)));
                assert!(sound[0].abs() < 0.01 && sound.last().unwrap().abs() < 0.01);
            }
            for (i, a) in rendered.iter().enumerate() {
                for b in rendered.iter().skip(i + 1) {
                    assert_ne!(a, b);
                }
            }
            // A break is in the key of the song it leads into.
            let other = style.breaks[0].render(&style.kit, &style.songs[1]);
            assert_ne!(rendered[0], other);
        }
    }

    #[test]
    fn change_breaks_take_ten_to_twenty_seconds_and_differ() {
        let (last, next) = (&JAPAN.songs[0], &JAPAN.songs[3]);
        let mut rendered = Vec::new();
        for change in CHANGES {
            let seconds = change.seconds(next);
            assert!((10.0..=20.0).contains(&seconds), "{change:?}: {seconds}");
            let sound = change.render(&JAPAN.kit, last, &JAPAN.kit, next);
            check(&format!("{change:?}"), &sound);
            assert_eq!(sound.len(), samples(seconds));
            assert!(sound.last().unwrap().abs() < 0.01);
            rendered.push(sound);
        }
        assert_ne!(rendered[0], rendered[1]);
        assert_ne!(rendered[1], rendered[2]);
        // The tape stop is silent between the sinking chord and the count.
        let quiet = &rendered[1][samples(6.3)..samples(7.4)];
        assert!(peak(quiet) < 0.01, "{}", peak(quiet));
    }

    #[test]
    fn the_record_scratch_goes_back_and_forth_stops_and_drops_the_needle() {
        // The hand: the record plays, is pulled back and pushed forward,
        // held, scratched twice as fast, and stopped.
        assert_eq!(scratch_speed(0.5), 1.0);
        let slow: Vec<f32> = (0..150)
            .map(|i| scratch_speed(1.6 + i as f32 * 0.01))
            .collect();
        assert!(slow.iter().any(|s| *s < -2.0) && slow.iter().any(|s| *s > 2.0));
        let turns = |speeds: &[f32]| {
            speeds
                .windows(2)
                .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                .count()
        };
        let fast: Vec<f32> = (0..150)
            .map(|i| scratch_speed(3.6 + i as f32 * 0.01))
            .collect();
        assert!(turns(&fast) >= 2 * turns(&slow) - 2, "faster strokes");
        assert_eq!(scratch_speed(3.3), 0.0, "held");
        assert_eq!(scratch_speed(6.0), 0.0, "stopped");

        assert_eq!(CHANGES[3], Change::Scratch);
        let (last, next) = (&JAPAN.songs[0], &JAPAN.songs[3]);
        let seconds = Change::Scratch.seconds(next);
        assert!((10.0..=20.0).contains(&seconds), "{seconds}");
        for style in &STYLES {
            let other = Change::Scratch.seconds(&style.songs[1]);
            assert!((10.0..=20.0).contains(&other), "{}: {other}", style.name);
        }
        let sound = Change::Scratch.render(&JAPAN.kit, last, &RETRO.kit, next);
        check("scratch", &sound);
        assert_eq!(sound.len(), samples(seconds));
        assert!(sound.last().unwrap().abs() < 0.01);
        // Scratching is heard, the held record is not, nor the stopped one.
        assert!(peak(&sound[samples(1.7)..samples(3.0)]) > 0.05);
        assert!(peak(&sound[samples(3.25)..samples(3.55)]) < 0.02, "held");
        assert!(peak(&sound[samples(5.8)..samples(6.9)]) < 0.01, "stopped");
        // Then the needle and the count into the next song.
        assert!(
            peak(&sound[samples(7.0)..samples(7.9)]) > 0.02,
            "the needle"
        );
        assert!(peak(&sound[samples(8.0)..]) > 0.05, "the count");
        for change in [Change::Radio, Change::TapeStop, Change::Modem] {
            assert_ne!(sound, change.render(&JAPAN.kit, last, &RETRO.kit, next));
        }
        // It leads out of the song it follows: another chord, another sound.
        assert_ne!(
            sound,
            Change::Scratch.render(&JAPAN.kit, &JAPAN.songs[1], &RETRO.kit, next)
        );
    }

    #[test]
    fn in_every_style_the_instruments_stay_behind_the_drums() {
        for style in &STYLES {
            for song in style.songs.iter().take(2) {
                let variation = plain();
                let instruments = song.instruments(&style.kit, &variation);
                let drums = song.drums(&style.kit, &variation);
                assert!(
                    peak(&instruments) < peak(&drums) * 0.75,
                    "{}: {} against {}",
                    style.name,
                    peak(&instruments),
                    peak(&drums)
                );
                assert!(peak(&instruments) > 0.1, "{}", style.name);
            }
        }
        // The styles do not share an instrument for their melodies.
        let mut leads: Vec<_> = STYLES
            .iter()
            .flat_map(|s| [s.kit.first, s.kit.second])
            .map(|v| format!("{v:?}"))
            .collect();
        leads.sort();
        leads.dedup();
        assert_eq!(leads.len(), STYLES.len() * 2);
        let mut all_breaks: Vec<_> = STYLES
            .iter()
            .flat_map(|s| s.breaks)
            .map(|l| format!("{l:?}"))
            .collect();
        all_breaks.sort();
        all_breaks.dedup();
        assert!(all_breaks.len() >= 12, "{all_breaks:?}");
    }

    #[test]
    fn the_cajun_style_is_a_porch_band_with_a_jaw_harp_in_its_breaks() {
        assert_eq!(STYLES[3], CAJUN);
        assert_eq!(CAJUN.label, "Cajun");
        assert_eq!(CAJUN.kit.first, Voice::Banjo, "the banjo leads");
        assert_eq!(
            CAJUN.kit.long,
            Voice::Fiddle,
            "the fiddle has the long tunes"
        );
        assert_eq!(CAJUN.kit.bass, Voice::Washtub);
        assert_eq!(CAJUN.kit.drums, Drums::Porch);
        let next = &CAJUN.songs[0];
        // The twang is the jaw harp alone: no other style's kit changes it.
        let twang = Link::Twang.render(&CAJUN.kit, next);
        assert_eq!(twang, Link::Twang.render(&JAPAN.kit, next));
        check("twang", &twang);
        // The roll is picked on the banjo: sixteenths, so ten notes start
        // within two and a half beats.
        let roll = Link::Rolls.render(&CAJUN.kit, next);
        assert_ne!(roll, Link::Rolls.render(&JAPAN.kit, next));
        // The porch break stomps from its first bar and picks from the third.
        let porch = Link::Porch.render(&CAJUN.kit, next);
        let bar = samples(4.0 * next.beat());
        assert!(peak(&porch[..bar]) > 0.3, "stomps at once");
        assert!((8.0..=15.0).contains(&Link::Porch.seconds(next)));
        // A song of it plays, in range and to its length.
        let song = next.render(&CAJUN.kit, 3);
        check("cajun song", &song);
        assert_eq!(song.len(), samples(next.seconds()));
    }

    #[test]
    fn far_events_are_few_off_the_beat_and_early_enough_for_their_echo() {
        let (seconds, beat) = (60.0, 0.75);
        let mut counts = [0; 4];
        let mut calls = 0;
        let mut places = Vec::new();
        for seed in 0..300 {
            let events = far_events(seed, seconds, beat);
            assert_eq!(events, far_events(seed, seconds, beat));
            counts[events.bombs.len()] += 1;
            calls += usize::from(events.call.is_some());
            for at in events.bombs.iter().chain(&events.call) {
                assert!((1.5..=51.0).contains(at), "seed {seed}: at {at}");
                let within = (at / beat).fract();
                assert!((0.29..=0.71).contains(&within), "seed {seed}: {within}");
                places.push((at * 100.0) as u32);
            }
        }
        // No bomb, one, two and three all happen; the call in one of three.
        assert!(counts.iter().all(|c| *c > 30), "{counts:?}");
        assert!((60..=140).contains(&calls), "{calls} calls");
        // Never at the same places: nearly every place is its own.
        let all = places.len();
        places.sort_unstable();
        places.dedup();
        assert!(places.len() > all * 8 / 10, "{} of {all}", places.len());
    }

    #[test]
    fn the_trip_hop_style_has_a_theremin_an_echo_and_a_war_far_off() {
        assert_eq!(STYLES[4], TRIPHOP);
        assert_eq!(TRIPHOP.label, "Trip-hop");
        assert_eq!(TRIPHOP.kit.first, Voice::Theremin, "the theremin leads");
        assert_eq!(TRIPHOP.kit.space, Space::Far);
        // It alone is in the far space: the others sound as they did.
        let far: Vec<_> = STYLES
            .iter()
            .filter(|s| s.kit.space == Space::Far)
            .map(|s| s.label)
            .collect();
        assert_eq!(far, ["Trip-hop"]);
        for song in &TRIPHOP.songs {
            assert!((74..=86).contains(&song.bpm), "slow: {}", song.bpm);
            // Dark: every motif starts with a minor third.
            assert_eq!(song.motif[1] - song.motif[0], 3);
        }
        let song = &TRIPHOP.songs[0];
        // What is far is very quiet against the music, and is there: the
        // crackle all through, and for some seed a bomb with its echo.
        let seed = (0..50)
            .find(|s| !far_events(*s, song.seconds(), song.beat()).bombs.is_empty())
            .unwrap();
        let far_off = song.far_off(seed);
        assert_eq!(far_off.len(), samples(song.seconds()));
        let bomb_at = far_events(seed, song.seconds(), song.beat()).bombs[0];
        let around = &far_off[samples(bomb_at)..samples(bomb_at + 3.0)];
        assert!(peak(around) > 0.005, "{}", peak(around));
        assert!(peak(&far_off) < 0.15, "{}", peak(&far_off));
        let rendered = song.render(&TRIPHOP.kit, seed);
        check("trip-hop song", &rendered);
        assert_eq!(rendered.len(), samples(song.seconds()));
        assert!(
            peak(&rendered) > 4.0 * peak(&far_off),
            "far below the music"
        );
        assert_ne!(rendered, song.render(&TRIPHOP.kit, seed + 1));
        // The echo: the melody instruments ring on in a bar without notes,
        // which they do not in a dry kit.
        let dry = Kit {
            space: Space::Dry,
            ..TRIPHOP.kit
        };
        let variation = plain();
        assert_ne!(
            song.instruments(&TRIPHOP.kit, &variation),
            song.instruments(&dry, &variation)
        );
        // The theremin slides: a melody is not the same notes played apart.
        let glide = Link::Glide.render(&TRIPHOP.kit, song);
        check("glide", &glide);
        // The battlefield break is long and has more in it than the chord.
        let field = Link::Battlefield.render(&TRIPHOP.kit, song);
        check("battlefield", &field);
        assert!((8.0..=15.0).contains(&Link::Battlefield.seconds(song)));
        assert!(field.last().unwrap().abs() < 0.01);
    }

    #[test]
    fn the_double_groove_has_fast_bass_drums() {
        let song = &METAL.songs[0];
        let bar = samples(4.0 * song.beat());
        let drums = song.drums(
            &METAL.kit,
            &Variation {
                skip: 20,
                ..plain()
            },
        );
        // Bars 2 to 5 are Double; bars 6 to 9 Full: more is hit in the first.
        let energy =
            |range: std::ops::Range<usize>| -> f32 { drums[range].iter().map(|s| s * s).sum() };
        assert!(energy(2 * bar..3 * bar) > energy(6 * bar..7 * bar) * 1.5);
    }

    /// How strongly `sound` holds the tone `frequency` (Goertzel).
    fn tone(sound: &[f32], frequency: f32) -> f32 {
        let k = 2.0 * (TAU * frequency / RATE).cos();
        let (mut a, mut b) = (0.0_f32, 0.0_f32);
        for sample in sound {
            let next = sample + k * a - b;
            b = a;
            a = next;
        }
        (a * a + b * b - k * a * b).max(0.0).sqrt() / sound.len() as f32
    }

    #[test]
    fn the_modem_dials_gets_an_answer_shakes_hands_and_screeches() {
        assert!(CHANGES.contains(&Change::Modem));
        let next = &RETRO.songs[0];
        let sound = Change::Modem.render(&JAPAN.kit, &JAPAN.songs[0], &RETRO.kit, next);
        assert_eq!(sound.len(), samples(13.0));
        check("modem", &sound);
        let part = |from: f32, to: f32| &sound[samples(from)..samples(to)];
        // Dial tone: 350 and 440 Hz together, and no answer tone yet.
        let dial = part(0.2, 1.5);
        assert!(tone(dial, 350.0) > 10.0 * tone(dial, 2100.0), "dial tone");
        assert!(tone(dial, 440.0) > 10.0 * tone(dial, 2100.0), "dial tone");
        // The first key: 941 and 1336 Hz, with a gap after it.
        let key = part(1.81, 1.89);
        assert!(tone(key, 941.0) > 5.0 * tone(key, 440.0), "first key");
        assert!(peak(part(1.95, 1.99)) < peak(key) / 4.0, "gap between keys");
        // The answer tone.
        let answer = part(4.0, 4.8);
        assert!(tone(answer, 2100.0) > 10.0 * tone(answer, 440.0), "answer");
        // The handshake warbles between its two tones.
        let warble = part(5.1, 6.3);
        assert!(tone(warble, 980.0) > 3.0 * tone(warble, 440.0));
        assert!(tone(warble, 1180.0) > 3.0 * tone(warble, 440.0));
        // The screech is noise: no single tone carries it.
        let screech = part(9.0, 12.0);
        assert!(peak(screech) > 0.1);
        assert!(peak(part(12.7, 13.0)) < 0.02, "ends quiet");
        // The line itself, piece by piece.
        assert_eq!(modem_line(0.5, true), (350.0, 440.0, 0.0));
        assert_eq!(modem_line(1.85, true), (941.0, 1336.0, 0.0));
        assert_eq!(modem_line(1.95, true), (0.0, 0.0, 0.0));
        assert_eq!(modem_line(4.0, false), (2100.0, 0.0, 0.0));
        assert_eq!(modem_line(7.0, true), (1180.0, 1650.0, 0.0));
        assert_eq!(modem_line(7.0, false), (980.0, 1850.0, 0.0));
        assert_eq!(modem_line(10.0, true).2, 1.0);
        assert_eq!(modem_line(12.9, true), (0.0, 0.0, 0.0));
    }

    static TWO: [Style; 2] = [JAPAN, JAPAN];

    #[test]
    fn the_program_plays_song_and_break_in_turn_and_changes_style_by_chance() {
        let (mut songs, mut changes, mut stays) = (0, 0, Vec::new());
        for seed in [1, 2, 99, 12345] {
            let mut program = Program::over(&TWO, seed);
            let mut style_now = None;
            // The first songs of the stay.
            let mut bag: Vec<usize> = Vec::new();
            let mut last_song = None;
            let mut stay = 0;
            for step in 0..2000 {
                let passage = program.next_passage();
                match (step % 2, passage) {
                    (0, Passage::Song { style, song, .. }) => {
                        if style_now != Some(style) {
                            style_now = Some(style);
                            bag.clear();
                            stays.push(stay);
                            stay = 0;
                        } else {
                            assert_ne!(last_song, Some(song), "seed {seed}: twice in a row");
                        }
                        // The first five of a stay are the style's five.
                        if bag.len() < 5 {
                            assert!(!bag.contains(&song), "seed {seed}: {bag:?} then {song}");
                            bag.push(song);
                        }
                        last_song = Some(song);
                        stay += 1;
                        songs += 1;
                    }
                    (1, Passage::Break { style, link, into }) => {
                        assert_eq!(Some(style), style_now);
                        assert!(into < 5 && link < 6);
                    }
                    (1, Passage::Change { change, from, into }) => {
                        assert_eq!(Some(from.0), style_now);
                        assert_eq!(Some(from.1), last_song);
                        assert_ne!(into.0, from.0, "another style");
                        assert!(change < CHANGES.len());
                        changes += 1;
                    }
                    other => panic!("seed {seed}, step {step}: {other:?}"),
                }
            }
        }
        // One song in four is followed by a change of style.
        let share = changes as f32 / songs as f32;
        assert!((0.2..0.3).contains(&share), "{changes} of {songs}");
        // Pure chance: stays of a single song and long stays both happen.
        assert!(stays.contains(&1), "{stays:?}");
        assert!(stays.iter().any(|s| *s > 8), "{stays:?}");
    }

    static THREE: [Style; 3] = [JAPAN, RETRO, METAL];

    #[test]
    fn only_the_allowed_styles_play_and_one_alone_never_changes() {
        let mut program = Program::over(&THREE, 11);
        program.allow(&[2, 0, 0, 9]);
        let played: Vec<_> = (0..600).map(|_| program.next_passage()).collect();
        assert!(played.iter().all(|p| p.within(&[0, 2])), "never style 1");
        let changes = played
            .iter()
            .filter(|p| matches!(p, Passage::Change { .. }))
            .count();
        assert!(changes > 20, "{changes} changes between the two");

        // One style: its songs and breaks, never a change break.
        program.allow(&[1]);
        program.restart();
        let alone: Vec<_> = (0..600).map(|_| program.next_passage()).collect();
        assert!(matches!(alone[0], Passage::Song { style: 1, .. }));
        assert!(alone.iter().all(|p| p.within(&[1])));
        assert!(alone.iter().all(|p| !matches!(p, Passage::Change { .. })));
        assert!(
            alone
                .iter()
                .any(|p| matches!(p, Passage::Break { style: 1, .. }))
        );

        // No style at all means all of them.
        program.allow(&[]);
        program.restart();
        let all: Vec<_> = (0..600).map(|_| program.next_passage()).collect();
        for style in 0..3 {
            assert!(all.iter().any(|p| p.within(&[style])), "style {style}");
        }

        let change = Passage::Change {
            change: 0,
            from: (0, 1),
            into: (2, 3),
        };
        assert!(change.within(&[0, 2]) && !change.within(&[0]) && !change.within(&[2]));
    }

    #[test]
    fn the_styles_that_are_switched_off_are_named_by_their_labels() {
        let labels: Vec<_> = STYLES.iter().map(|s| s.label).collect();
        let mut unique = labels.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), labels.len(), "{labels:?}");
        assert_eq!(&labels[..3], ["Japanese", "Retro", "Babymetal"]);
        let all: Vec<usize> = (0..STYLES.len()).collect();
        assert_eq!(styles_on(&STYLES, &[]), all);
        let off = vec!["Retro".to_owned(), "Polka".to_owned()];
        let on = styles_on(&STYLES, &off);
        assert!(!on.contains(&1) && on.len() == STYLES.len() - 1, "{on:?}");
        let every: Vec<String> = labels.iter().map(|l| (*l).to_owned()).collect();
        assert_eq!(styles_on(&STYLES, &every), all, "none left means all");
    }

    #[test]
    fn one_style_alone_never_changes_and_seeds_give_different_programs() {
        let run = |seed| {
            let mut program = Program::new(seed);
            assert_eq!(program.styles().len(), STYLES.len());
            (0..40).map(|_| program.next_passage()).collect::<Vec<_>>()
        };
        assert_ne!(run(1), run(2));
        assert_eq!(run(5), run(5));
        if STYLES.len() == 1 {
            assert!(run(3).iter().all(|p| !matches!(p, Passage::Change { .. })));
        }
        // A song never follows itself, also when the bag is filled again.
        let songs: Vec<_> = run(7)
            .into_iter()
            .filter_map(|p| match p {
                Passage::Song { style, song, .. } => Some((style, song)),
                _ => None,
            })
            .collect();
        assert!(songs.windows(2).all(|w| w[0] != w[1]), "{songs:?}");
    }

    #[test]
    fn passages_render_what_they_name_and_nothing_for_a_wrong_number() {
        let a_break = Passage::Break {
            style: 0,
            link: 1,
            into: 2,
        };
        let expected = JAPAN.breaks[1].render(&JAPAN.kit, &JAPAN.songs[2]);
        assert_eq!(a_break.render(&TWO), expected);
        let change = Passage::Change {
            change: 1,
            from: (0, 0),
            into: (1, 3),
        };
        assert_eq!(
            change.render(&TWO).len(),
            samples(Change::TapeStop.seconds(&JAPAN.songs[3]))
        );
        let song = Passage::Song {
            style: 1,
            song: 4,
            seed: 9,
        };
        assert_eq!(song.render(&TWO).len(), samples(JAPAN.songs[4].seconds()));
        for wrong in [
            Passage::Song {
                style: 7,
                song: 0,
                seed: 1,
            },
            Passage::Song {
                style: 0,
                song: 9,
                seed: 1,
            },
            Passage::Break {
                style: 0,
                link: 9,
                into: 0,
            },
            Passage::Break {
                style: 0,
                link: 0,
                into: 9,
            },
            Passage::Change {
                change: 9,
                from: (0, 0),
                into: (0, 1),
            },
            Passage::Change {
                change: 0,
                from: (0, 0),
                into: (5, 1),
            },
        ] {
            assert!(wrong.render(&TWO).is_empty(), "{wrong:?}");
        }
    }
}
