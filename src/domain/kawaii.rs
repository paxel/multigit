//! The motion of the Kawaii theme, as plain numbers. Everything is a function
//! of the theme's clock, which only runs while the window has focus, and for
//! a few seconds after a repository changed: a frozen clock is a frozen
//! picture.

use std::f32::consts::TAU;

/// Frames per second while the theme animates.
pub const FPS: f32 = 30.0;
/// How far a card bobs up and down, in points.
pub const BOB: f32 = 2.0;
/// How far a card swings to each side when it is marked, in points.
pub const WOBBLE: f32 = 4.0;
/// How long that swing lasts, in seconds.
pub const WOBBLE_SECS: f64 = 0.5;
/// Hearts and stars floating over the board.
pub const PARTICLES: usize = 14;

/// How long a change in a repository keeps a window without focus moving,
/// in seconds.
pub const WAKE_SECS: f32 = 5.0;

/// The part of a frame's `dt` seconds the clock takes: a long pause (a
/// hidden window, a still picture) does not make it jump.
fn step(dt: f32) -> f32 {
    dt.clamp(0.0, 0.1)
}

/// The clock after a frame that took `dt` seconds. It stands still unless
/// it is `running`.
pub fn advance(clock: f64, dt: f32, running: bool) -> f64 {
    if running {
        clock + f64::from(step(dt))
    } else {
        clock
    }
}

/// What is left of the seconds a change keeps the picture moving, after a
/// frame of `dt` seconds. A window with focus moves anyway and uses none up.
pub fn awake_after(awake: f32, dt: f32, focused: bool) -> f32 {
    if focused {
        0.0
    } else {
        (awake - step(dt)).max(0.0)
    }
}

/// Vertical offset of the card at `index`: a slow wave, each card a little
/// out of step with its neighbour.
pub fn bob(clock: f64, index: usize) -> f32 {
    let phase = clock as f32 * 1.6 + index as f32 * 0.9;
    phase.sin() * BOB
}

/// Horizontal offset of a card marked at clock time `marked_at`: a swing that
/// dies away; `None` when it is over.
pub fn wobble(clock: f64, marked_at: f64) -> Option<f32> {
    let t = ((clock - marked_at) / WOBBLE_SECS) as f32;
    (0.0..1.0)
        .contains(&t)
        .then(|| (t * TAU * 3.0).sin() * WOBBLE * (1.0 - t))
}

/// Hue (0 to 1) of the rainbow at the horizontal position `x` (0 to 1): the
/// colors drift to the right over time and wrap around.
pub fn rainbow_hue(clock: f64, x: f32) -> f32 {
    (x * 0.8 - clock as f32 * 0.02).rem_euclid(1.0)
}

/// A heart or star floating up the board.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    /// Position on the board, 0 to 1 from the left and from the top.
    pub x: f32,
    pub y: f32,
    pub heart: bool,
    /// Font size in points.
    pub size: f32,
    /// 0 (invisible) to 1; fades in at the bottom and out at the top.
    pub alpha: f32,
}

/// A fraction between 0 and 1 that looks random and is the same for the
/// same `n` every time.
fn scatter(n: usize) -> f32 {
    (n as f32 * 0.618_034).fract()
}

/// The particle `index` at this time. Each rises at its own speed, sways a
/// little, and starts again at the bottom.
pub fn particle(clock: f64, index: usize) -> Particle {
    let speed = 0.03 + 0.03 * scatter(index * 7 + 3);
    let rise = (clock as f32 * speed + scatter(index * 13 + 5)).fract();
    let sway = (clock as f32 * 0.7 + index as f32).sin() * 0.015;
    Particle {
        x: (scatter(index + 1) + sway).clamp(0.0, 1.0),
        y: 1.0 - rise,
        heart: index.is_multiple_of(2),
        size: 14.0 + 12.0 * scatter(index * 5 + 2),
        alpha: (rise * (1.0 - rise) * 4.0).clamp(0.0, 1.0),
    }
}

/// A spark of the copy animation: offset from the click after the fraction
/// `t` (0 to 1) of the animation. `count` sparks fly apart in a circle.
pub fn spark(t: f32, index: usize, count: usize) -> (f32, f32) {
    let angle = index as f32 / count.max(1) as f32 * TAU;
    let radius = 8.0 + 26.0 * t;
    (angle.cos() * radius, angle.sin() * radius)
}

/// Hearts and stars in a burst of confetti, and how long it lasts.
pub const CONFETTI: usize = 14;
pub const CONFETTI_SECS: f64 = 1.1;

/// A heart or star of a burst of confetti.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Confetto {
    /// Offset from where the burst started: right and down, in points.
    pub dx: f32,
    pub dy: f32,
    pub heart: bool,
    /// Font size in points.
    pub size: f32,
    /// 1 at the start, 0 at the end.
    pub alpha: f32,
}

/// How far a burst that started at clock time `started` has come, 0 to 1;
/// `None` when it is over.
pub fn burst(clock: f64, started: f64) -> Option<f32> {
    let t = ((clock - started) / CONFETTI_SECS) as f32;
    (0.0..1.0).contains(&t).then_some(t)
}

/// The piece `index` of a burst after the fraction `t` of its time: thrown
/// up and outwards in a fan, each at its own speed, then falling.
pub fn confetto(t: f32, index: usize) -> Confetto {
    // A fan over the upper half: from the left over the top to the right.
    let angle = (0.1 + 0.8 * (index as f32 + 0.5) / CONFETTI as f32) * TAU / 2.0;
    let speed = 70.0 + 60.0 * scatter(index * 11 + 4);
    Confetto {
        dx: -angle.cos() * speed * t,
        dy: -angle.sin() * speed * t + 150.0 * t * t,
        heart: index.is_multiple_of(2),
        size: 10.0 + 8.0 * scatter(index * 3 + 1),
        alpha: (1.0 - t * t).clamp(0.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confetti_flies_up_and_apart_then_falls_and_fades() {
        assert_eq!(burst(10.0, 10.0), Some(0.0));
        assert!((burst(10.55, 10.0).unwrap() - 0.5).abs() < 0.01);
        assert_eq!(burst(10.0 + CONFETTI_SECS, 10.0), None);
        assert_eq!(burst(9.0, 10.0), None, "not before it started");
        let pieces = |t: f32| -> Vec<Confetto> { (0..CONFETTI).map(|i| confetto(t, i)).collect() };
        assert!(
            pieces(0.0)
                .iter()
                .all(|p| p.dx == 0.0 && p.dy == 0.0 && p.alpha == 1.0)
        );
        // Early on every piece is above the start; they go left and right.
        let early = pieces(0.2);
        assert!(early.iter().all(|p| p.dy < 0.0), "{early:?}");
        assert!(early.iter().any(|p| p.dx < -5.0) && early.iter().any(|p| p.dx > 5.0));
        // Each falls again: lower at the end than at its highest.
        for index in 0..CONFETTI {
            let highest = (0..=20)
                .map(|i| confetto(i as f32 / 20.0, index).dy)
                .fold(f32::MAX, f32::min);
            assert!(confetto(1.0, index).dy > highest + 5.0, "piece {index}");
        }
        let late = pieces(0.95);
        assert!(late.iter().all(|p| p.alpha < 0.15 && p.alpha >= 0.0));
        assert!(early.iter().any(|p| p.heart) && early.iter().any(|p| !p.heart));
        assert!(early.iter().all(|p| (10.0..=18.0).contains(&p.size)));
    }

    #[test]
    fn clock_runs_only_with_focus_and_never_jumps() {
        assert_eq!(advance(5.0, 0.033, false), 5.0);
        assert!((advance(5.0, 0.033, true) - 5.033).abs() < 1e-6);
        // A frame after a long pause counts as a tenth of a second.
        assert!((advance(5.0, 30.0, true) - 5.1).abs() < 1e-6);
        assert_eq!(advance(5.0, -1.0, true), 5.0);
    }

    #[test]
    fn a_change_keeps_a_window_without_focus_moving_for_five_seconds() {
        let mut awake = WAKE_SECS;
        let mut frames = 0;
        while awake > 0.0 {
            awake = awake_after(awake, 1.0 / FPS, false);
            frames += 1;
        }
        assert!((149..=152).contains(&frames), "{frames}");
        // The first frame after a long still picture counts as a tenth.
        assert!((awake_after(WAKE_SECS, 60.0, false) - 4.9).abs() < 1e-5);
        assert_eq!(awake_after(0.0, 0.033, false), 0.0);
        assert_eq!(awake_after(WAKE_SECS, 0.033, true), 0.0);
    }

    #[test]
    fn cards_bob_within_bounds_and_out_of_step() {
        for step in 0..200 {
            let clock = f64::from(step) * 0.05;
            assert!(bob(clock, 3).abs() <= BOB);
        }
        assert!((bob(1.0, 0) - bob(1.0, 1)).abs() > 0.1);
        assert_eq!(bob(2.0, 4), bob(2.0, 4), "a frozen clock is a frozen card");
    }

    #[test]
    fn wobble_swings_and_ends() {
        assert_eq!(wobble(10.0, 10.0), Some(0.0));
        let early = wobble(10.04, 10.0).unwrap();
        assert!(early.abs() > 1.0 && early.abs() <= WOBBLE, "{early}");
        assert!(wobble(10.45, 10.0).unwrap().abs() < 1.0, "dies away");
        assert_eq!(wobble(10.0 + WOBBLE_SECS, 10.0), None);
        assert_eq!(wobble(9.0, 10.0), None);
    }

    #[test]
    fn rainbow_drifts_and_wraps() {
        for step in 0..100 {
            let hue = rainbow_hue(f64::from(step) * 3.7, step as f32 / 100.0);
            assert!((0.0..1.0).contains(&hue), "{hue}");
        }
        assert_ne!(rainbow_hue(0.0, 0.5), rainbow_hue(10.0, 0.5));
        assert_ne!(rainbow_hue(0.0, 0.2), rainbow_hue(0.0, 0.6));
    }

    #[test]
    fn particles_rise_inside_the_board_and_fade_at_its_edges() {
        let mut hearts = 0;
        for index in 0..PARTICLES {
            for step in 0..300 {
                let p = particle(f64::from(step) * 0.5, index);
                assert!((0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y));
                assert!((0.0..=1.0).contains(&p.alpha) && p.size >= 14.0);
            }
            let (now, later) = (particle(1.0, index), particle(1.5, index));
            assert!(later.y < now.y || later.y > 0.9, "rises or starts again");
            hearts += usize::from(now.heart);
        }
        assert_eq!(hearts, PARTICLES / 2);
        let xs: Vec<f32> = (0..PARTICLES).map(|i| particle(0.0, i).x).collect();
        assert!(
            xs.iter().any(|x| *x < 0.3) && xs.iter().any(|x| *x > 0.7),
            "spread"
        );
    }

    #[test]
    fn sparks_fly_apart_in_a_circle() {
        let (x0, y0) = spark(0.0, 0, 6);
        let (x1, y1) = spark(1.0, 0, 6);
        assert!(x1 > x0 && y0.abs() < 1e-3 && y1.abs() < 1e-3);
        let (x, _) = spark(0.5, 3, 6);
        assert!(x < 0.0, "the opposite spark goes the other way");
        assert!(spark(0.5, 0, 0).0.is_finite());
    }
}
