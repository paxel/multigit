//! The bunnies and cats of the Kawaii theme, as plain numbers: who sits on
//! which card, what each is doing, and how it holds itself at a moment of
//! the theme's clock. They sleep while the board is calm and ask for no
//! frame; drawing them is the UI's part.

use std::f32::consts::{PI, TAU};
use std::path::{Path, PathBuf};

/// Cards shown per critter.
pub const CARDS_PER_CRITTER: usize = 4;
pub const FEWEST: usize = 2;
pub const MOST: usize = 8;
/// How long a hop from card to card takes, in seconds.
pub const HOP_SECS: f64 = 0.7;
/// How long a critter shows what it feels about a piece of news.
pub const MOOD_SECS: f64 = 3.0;
/// After this many seconds without anything happening the critters sleep.
pub const CALM_SECS: f64 = 20.0;
/// Pastel colors, as red, green and blue.
pub const PASTELS: [(u8, u8, u8); 8] = [
    (255, 209, 220),
    (255, 236, 179),
    (200, 230, 201),
    (187, 222, 251),
    (225, 190, 231),
    (255, 224, 178),
    (178, 235, 242),
    (240, 220, 200),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Species {
    Bunny,
    Cat,
}

impl Species {
    /// As a screen reader names it.
    pub fn label(self) -> &'static str {
        match self {
            Species::Bunny => "bunny",
            Species::Cat => "cat",
        }
    }
}

/// What a critter is doing besides sitting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Plain,
    /// A commit appeared on the remote: it looks up.
    LookUp,
    /// A commit was made here: a small dance.
    Dance,
    /// The fetch failed: the ears hang.
    Droop,
    /// It was clicked: a jump with a heart.
    Pet,
}

/// What happened on a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum News {
    RemoteCommit,
    LocalCommit,
    FetchError,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Critter {
    pub species: Species,
    /// Which of the [`PASTELS`] it has.
    pub color: usize,
    /// The card it sits on, or hops to.
    pub home: PathBuf,
    /// The card its last hop started on, and when.
    pub from: PathBuf,
    pub hop_at: f64,
    pub mood: Mood,
    pub mood_at: f64,
}

/// How a critter holds itself at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// How far along its hop it is: 0 on the card it left, 1 at home.
    pub along: f32,
    /// How high in the air, 0 to 1.
    pub lift: f32,
    /// Its height against its normal one: under 1 when it squashes on
    /// take-off and landing, or lies curled up.
    pub squash: f32,
    /// -1 hanging, 0 upright, up to 1 wiggling outwards.
    pub ears: f32,
    pub eyes: Eyes,
    /// Asleep, curled up.
    pub curled: bool,
    /// Sideways lean of a dance, -1 to 1.
    pub sway: f32,
    /// How far the heart of a petted critter has risen, 0 to 1.
    pub heart: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eyes {
    Open,
    Closed,
    /// Looking up.
    Up,
}

/// All critters of the board.
#[derive(Debug, Clone, PartialEq)]
pub struct Troupe {
    pub critters: Vec<Critter>,
    random: u64,
    /// When the next critter hops off by itself.
    next_hop: f64,
    /// When something last happened.
    stirred_at: f64,
}

impl Default for Troupe {
    fn default() -> Self {
        Self {
            critters: Vec::new(),
            random: 0x2545_F491_4F6C_DD1D,
            next_hop: 0.0,
            stirred_at: 0.0,
        }
    }
}

/// How many critters a board of `cards` shown cards has.
pub fn count_for(cards: usize) -> usize {
    if cards == 0 {
        0
    } else {
        (cards / CARDS_PER_CRITTER).clamp(FEWEST, MOST)
    }
}

impl Troupe {
    fn draw(&mut self, below: usize) -> usize {
        let mut x = self.random.max(1);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.random = x;
        ((x >> 16) % below.max(1) as u64) as usize
    }

    /// Fit the troupe to the cards the board shows: as many critters as
    /// that many cards have, each on a card that is shown. Bunnies and
    /// cats come in turn, each with the next pastel color.
    pub fn sync(&mut self, shown: &[PathBuf], clock: f64) {
        let wanted = count_for(shown.len());
        self.critters.truncate(wanted);
        for critter in &mut self.critters {
            if !shown.contains(&critter.home) {
                // Its card left the board: it is on another one at once.
                let at = (critter.color * 3 + 1) % shown.len().max(1);
                critter.home = shown.get(at).cloned().unwrap_or_default();
                critter.from = critter.home.clone();
            }
            if !shown.contains(&critter.from) {
                critter.from = critter.home.clone();
            }
        }
        while self.critters.len() < wanted {
            let nth = self.critters.len();
            // Spread over the board, not all on the first cards.
            let at = (nth * shown.len() / wanted + self.draw(2)) % shown.len();
            let home = shown.get(at).cloned().unwrap_or_default();
            self.critters.push(Critter {
                species: if nth.is_multiple_of(2) {
                    Species::Bunny
                } else {
                    Species::Cat
                },
                color: nth % PASTELS.len(),
                from: home.clone(),
                home,
                hop_at: f64::MIN,
                mood: Mood::Plain,
                mood_at: f64::MIN,
            });
            self.stirred_at = clock;
        }
    }

    /// Something happened: the critters wake up.
    pub fn stir(&mut self, clock: f64) {
        if self.asleep(clock) {
            // Nobody hops off the moment it wakes.
            self.next_hop = clock + 2.0;
        }
        self.stirred_at = clock;
    }

    /// Whether the board was calm long enough for the critters to sleep.
    pub fn asleep(&self, clock: f64) -> bool {
        clock - self.stirred_at > CALM_SECS
    }

    fn hop(critter: &mut Critter, to: &Path, clock: f64) {
        if critter.home != to {
            critter.from = std::mem::replace(&mut critter.home, to.to_path_buf());
            critter.hop_at = clock;
        }
    }

    /// One frame: feelings pass, and now and then an awake critter hops to
    /// another card.
    pub fn tick(&mut self, shown: &[PathBuf], clock: f64) {
        for critter in &mut self.critters {
            if critter.mood != Mood::Plain && clock - critter.mood_at > MOOD_SECS {
                critter.mood = Mood::Plain;
            }
        }
        if self.asleep(clock) || self.critters.is_empty() || shown.len() < 2 {
            return;
        }
        if clock >= self.next_hop {
            let who = self.draw(self.critters.len());
            let to = self.draw(shown.len());
            let wait = 4.0 + self.draw(50) as f64 / 10.0;
            self.next_hop = clock + wait;
            if let (Some(critter), Some(card)) = (self.critters.get_mut(who), shown.get(to))
                && critter.mood == Mood::Plain
            {
                Self::hop(critter, card, clock);
            }
        }
    }

    /// News of the card `card`: the critters wake, and one of them shows
    /// it. For a remote commit the nearest hops onto the card and looks
    /// up; a local commit and a failed fetch are shown by one that already
    /// sits there.
    pub fn news(&mut self, news: News, card: &Path, shown: &[PathBuf], clock: f64) {
        self.stir(clock);
        let place = |path: &Path| shown.iter().position(|p| p == path);
        let Some(there) = place(card) else {
            return;
        };
        let who = match news {
            News::RemoteCommit => (0..self.critters.len()).min_by_key(|i| {
                let home = self.critters.get(*i).and_then(|c| place(&c.home));
                home.map_or(usize::MAX, |at| at.abs_diff(there))
            }),
            News::LocalCommit | News::FetchError => {
                self.critters.iter().position(|c| c.home == card)
            }
        };
        let Some(critter) = who.and_then(|i| self.critters.get_mut(i)) else {
            return;
        };
        Self::hop(critter, card, clock);
        critter.mood = match news {
            News::RemoteCommit => Mood::LookUp,
            News::LocalCommit => Mood::Dance,
            News::FetchError => Mood::Droop,
        };
        critter.mood_at = clock;
    }

    /// A click on the critter `index`: it jumps, with a heart.
    pub fn pet(&mut self, index: usize, clock: f64) {
        self.stir(clock);
        if let Some(critter) = self.critters.get_mut(index) {
            critter.mood = Mood::Pet;
            critter.mood_at = clock;
        }
    }
}

/// A number between 0 and 1 that is the critter's own.
fn own(critter: &Critter, n: usize) -> f32 {
    ((critter.color * 7 + n * 13 + 1) as f32 * 0.618_034).fract()
}

/// How the critter holds itself at `clock`; `asleep` when the troupe
/// sleeps.
pub fn pose(critter: &Critter, clock: f64, asleep: bool) -> Pose {
    let since = |at: f64| (clock - at) as f32;
    let hop = (since(critter.hop_at) / HOP_SECS as f32).clamp(0.0, 1.0);
    let hopping = hop < 1.0;
    // Faster in the middle of a hop than at its ends.
    let along = hop * hop * (3.0 - 2.0 * hop);
    let mut lift = if hopping { (hop * PI).sin() } else { 0.0 };
    // Low just after take-off and just before landing.
    let mut squash = if hopping {
        1.0 - 0.25 * (1.0 - (hop * PI).sin()) * (hop * TAU).sin().abs()
    } else {
        1.0
    };
    let mood_for = since(critter.mood_at);
    let mood = if hopping { Mood::Plain } else { critter.mood };
    let mut pose = Pose {
        along,
        lift: 0.0,
        squash: 1.0,
        ears: 0.0,
        eyes: Eyes::Open,
        curled: false,
        sway: 0.0,
        heart: None,
    };
    if asleep && !hopping && critter.mood == Mood::Plain {
        return Pose {
            squash: 0.6,
            ears: -0.6,
            eyes: Eyes::Closed,
            curled: true,
            ..pose
        };
    }
    // A blink about every four seconds, each critter at its own moment.
    let blink = (clock as f32 / (3.5 + own(critter, 1)) + own(critter, 2)).fract();
    if blink > 0.96 {
        pose.eyes = Eyes::Closed;
    }
    // The ears wiggle for a moment, about every seven seconds.
    let wiggle = (clock as f32 / (6.0 + 2.0 * own(critter, 3)) + own(critter, 4)).fract();
    if wiggle > 0.9 {
        pose.ears = ((wiggle - 0.9) * 10.0 * TAU * 2.0).sin() * 0.6;
    }
    match mood {
        Mood::Plain => {}
        Mood::LookUp => {
            pose.eyes = Eyes::Up;
            pose.ears = 0.5;
        }
        Mood::Dance => {
            pose.sway = (mood_for * TAU * 2.0).sin();
            lift = (mood_for * TAU * 2.0).sin().abs() * 0.25;
        }
        Mood::Droop => {
            pose.ears = -1.0;
            squash = 0.9;
        }
        Mood::Pet => {
            let jump = (mood_for / 0.5).clamp(0.0, 1.0);
            lift = (jump * PI).sin().max(0.0) * 0.6;
            pose.eyes = Eyes::Closed;
            pose.heart = Some((mood_for / MOOD_SECS as f32).clamp(0.0, 1.0));
        }
    }
    Pose {
        lift,
        squash,
        ..pose
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cards(n: usize) -> Vec<PathBuf> {
        (0..n).map(|i| PathBuf::from(format!("/r/{i}"))).collect()
    }

    fn troupe(n: usize) -> (Troupe, Vec<PathBuf>) {
        let shown = cards(n);
        let mut troupe = Troupe::default();
        troupe.sync(&shown, 0.0);
        (troupe, shown)
    }

    #[test]
    fn one_critter_for_four_cards_at_least_two_at_most_eight_in_turn() {
        let counts: Vec<_> = [0, 1, 4, 8, 11, 12, 20, 32, 33, 400]
            .map(count_for)
            .to_vec();
        assert_eq!(counts, [0, 2, 2, 2, 2, 3, 5, 8, 8, 8]);
        let (troupe, shown) = troupe(32);
        assert_eq!(troupe.critters.len(), 8);
        for (i, critter) in troupe.critters.iter().enumerate() {
            let species = if i % 2 == 0 {
                Species::Bunny
            } else {
                Species::Cat
            };
            assert_eq!(critter.species, species);
            assert_eq!(critter.color, i);
            assert!(shown.contains(&critter.home));
            assert_eq!(critter.home, critter.from);
        }
        // They do not all sit on the first cards.
        let last = troupe
            .critters
            .iter()
            .filter_map(|c| shown.iter().position(|p| *p == c.home))
            .max();
        assert!(last > Some(20), "{last:?}");
        assert_eq!(
            (Species::Bunny.label(), Species::Cat.label()),
            ("bunny", "cat")
        );
        assert!(Troupe::default().critters.is_empty());
        let (none, _) = troupe_of_none();
        assert!(none.critters.is_empty());
    }

    fn troupe_of_none() -> (Troupe, Vec<PathBuf>) {
        troupe(0)
    }

    #[test]
    fn the_troupe_follows_the_cards_that_are_shown() {
        let (mut troupe, _) = troupe(20);
        assert_eq!(troupe.critters.len(), 5);
        // Fewer cards: fewer critters, all on cards that are still there.
        let fewer = cards(3);
        troupe.sync(&fewer, 1.0);
        assert_eq!(troupe.critters.len(), 2);
        assert!(
            troupe
                .critters
                .iter()
                .all(|c| fewer.contains(&c.home) && fewer.contains(&c.from))
        );
        let kept = troupe.critters.clone();
        troupe.sync(&fewer, 2.0);
        assert_eq!(troupe.critters, kept, "nothing moves without a reason");
        troupe.sync(&[], 3.0);
        assert!(troupe.critters.is_empty());
        troupe.tick(&[], 4.0);
        troupe.news(News::RemoteCommit, Path::new("/r/0"), &[], 4.0);
        troupe.pet(3, 4.0);
    }

    #[test]
    fn critters_sleep_when_calm_and_wake_when_something_happens() {
        let (mut troupe, shown) = troupe(8);
        assert!(!troupe.asleep(CALM_SECS - 1.0));
        assert!(troupe.asleep(CALM_SECS + 1.0));
        let asleep = pose(&troupe.critters[0], 30.0, true);
        assert!(asleep.curled && asleep.eyes == Eyes::Closed && asleep.squash < 0.7);
        // Asleep, nobody hops, however long.
        let before = troupe.critters.clone();
        for second in 21..200 {
            troupe.tick(&shown, f64::from(second));
        }
        assert_eq!(troupe.critters, before);
        troupe.stir(200.0);
        assert!(!troupe.asleep(201.0));
        assert!(!pose(&troupe.critters[0], 201.0, false).curled);
        // Awake, somebody hops within a while; the hop is an arc.
        let mut clock = 200.0;
        while troupe.critters == before {
            clock += 0.1;
            troupe.stir(clock);
            troupe.tick(&shown, clock);
            assert!(clock < 260.0, "nobody hopped");
        }
        let hopper = troupe.critters.iter().find(|c| c.home != c.from).unwrap();
        assert_eq!(hopper.hop_at, clock);
        let at = |t: f64| pose(hopper, clock + t, false);
        assert_eq!(at(0.0).along, 0.0);
        assert!(at(HOP_SECS / 2.0).lift > 0.9);
        assert!((at(HOP_SECS / 2.0).along - 0.5).abs() < 0.01);
        assert!(at(HOP_SECS * 0.1).squash < 1.0, "squashes on take-off");
        let landed = at(HOP_SECS + 0.01);
        assert_eq!((landed.along, landed.lift), (1.0, 0.0));
        assert_eq!(at(HOP_SECS + 1.0).squash, 1.0);
    }

    #[test]
    fn news_makes_the_nearest_hop_over_or_the_one_there_show_it() {
        let (mut troupe, shown) = troupe(12);
        let homes = |t: &Troupe| -> Vec<usize> {
            t.critters
                .iter()
                .map(|c| shown.iter().position(|p| *p == c.home).unwrap())
                .collect()
        };
        let before = homes(&troupe);
        // A card nobody sits on.
        let empty = (0..12).find(|i| !before.contains(i)).unwrap();
        let nearest = (0..before.len())
            .min_by_key(|i| before[*i].abs_diff(empty))
            .unwrap();
        troupe.news(News::LocalCommit, &shown[empty], &shown, 50.0);
        troupe.news(News::FetchError, &shown[empty], &shown, 50.0);
        assert!(
            troupe.critters.iter().all(|c| c.mood == Mood::Plain),
            "nobody is there"
        );
        assert!(!troupe.asleep(51.0), "but everybody woke up");

        troupe.news(News::RemoteCommit, &shown[empty], &shown, 60.0);
        let critter = &troupe.critters[nearest];
        assert_eq!(homes(&troupe)[nearest], empty);
        assert_eq!((critter.mood, critter.hop_at), (Mood::LookUp, 60.0));
        // In the air it only hops; landed, it looks up with its ears up.
        assert_ne!(pose(critter, 60.2, false).eyes, Eyes::Up);
        let landed = pose(critter, 61.0, false);
        assert_eq!(landed.eyes, Eyes::Up);
        assert!(landed.ears > 0.0);

        // Now somebody is there: a local commit is a dance, a failed fetch
        // hanging ears.
        troupe.news(News::LocalCommit, &shown[empty], &shown, 70.0);
        let dance = pose(&troupe.critters[nearest], 70.1, false);
        assert!(dance.sway.abs() > 0.5 && dance.lift > 0.0, "{dance:?}");
        troupe.news(News::FetchError, &shown[empty], &shown, 80.0);
        let droop = pose(&troupe.critters[nearest], 80.5, false);
        assert_eq!(droop.ears, -1.0);
        assert!(droop.squash < 1.0);
        // Feelings pass.
        troupe.tick(&shown, 80.0 + MOOD_SECS + 0.1);
        assert_eq!(troupe.critters[nearest].mood, Mood::Plain);
        // News of a card that is not shown wakes, and nothing else.
        let homes_before = homes(&troupe);
        troupe.news(News::RemoteCommit, Path::new("/elsewhere"), &shown, 90.0);
        assert_eq!(homes(&troupe), homes_before);
    }

    #[test]
    fn a_petted_critter_jumps_with_a_heart_and_awake_ones_blink_and_wiggle() {
        let (mut troupe, _) = troupe(8);
        troupe.pet(1, 500.0);
        assert!(!troupe.asleep(501.0));
        let critter = &troupe.critters[1];
        assert_eq!(critter.mood, Mood::Pet);
        let jump = pose(critter, 500.25, false);
        assert!(jump.lift > 0.5 && jump.eyes == Eyes::Closed);
        let (early, late) = (
            jump.heart.unwrap(),
            pose(critter, 502.0, false).heart.unwrap(),
        );
        assert!(early < late && late <= 1.0, "the heart rises");
        assert_eq!(pose(critter, 501.0, false).lift, 0.0, "landed again");
        // Even while the others sleep, the petted one is up.
        assert!(!pose(critter, 500.25, true).curled);
        troupe.pet(99, 500.0);

        // Over a minute an idle critter blinks and wiggles its ears, and
        // sits still in between.
        let idle = &troupe.critters[0];
        let poses: Vec<_> = (0..1800)
            .map(|i| pose(idle, 600.0 + f64::from(i) / 30.0, false))
            .collect();
        let blinks = poses.iter().filter(|p| p.eyes == Eyes::Closed).count();
        assert!((10..200).contains(&blinks), "{blinks} frames of blinking");
        let wiggles = poses.iter().filter(|p| p.ears != 0.0).count();
        assert!((10..400).contains(&wiggles), "{wiggles} frames of wiggling");
        assert!(
            poses
                .iter()
                .all(|p| p.lift == 0.0 && p.squash == 1.0 && !p.curled)
        );
        assert!(
            PASTELS
                .iter()
                .all(|(r, g, b)| u32::from(*r) + u32::from(*g) + u32::from(*b) > 540)
        );
    }
}
