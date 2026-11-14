//! Draws the bunnies and cats of the Kawaii theme from simple shapes: a
//! round body, ears, dot eyes and a tail. How each holds itself comes from
//! `domain::critters`.

use crate::domain::critters::{Critter, Eyes, PASTELS, Pose, Species};
use crate::ui::style::KAWAII_PINK;
use crate::ui::view::HEART;
use egui::{Color32, Painter, Pos2, Shape, Stroke, Vec2, pos2, vec2};

/// How high a critter is when it sits, and half its width: the room it
/// needs above a card, and what a pointer has to hit.
pub const HEIGHT: f32 = 26.0;
pub const HALF_WIDTH: f32 = 13.0;
/// How high a hop goes, in points.
pub const HOP_HEIGHT: f32 = 34.0;

/// The critter with its feet at `feet`. `look` is where the pointer is from
/// its face, when it is near enough to be looked at.
pub fn paint(painter: &Painter, feet: Pos2, critter: &Critter, pose: &Pose, look: Option<Vec2>) {
    let (r, g, b) = PASTELS
        .get(critter.color % PASTELS.len())
        .copied()
        .unwrap_or((255, 255, 255));
    let fur = Color32::from_rgb(r, g, b);
    let line = Color32::from_rgb(r / 2, g / 2, b / 2);
    let dark = Color32::from_rgb(60, 50, 60);
    let outline = Stroke::new(1.0, line);

    let radius = if pose.curled {
        vec2(13.0, 7.0)
    } else {
        vec2(11.0, 9.5 * pose.squash)
    };
    let center = feet - vec2(-pose.sway * 2.0, radius.y);
    let top = center.y - radius.y;

    // The tail, behind the body.
    match critter.species {
        Species::Bunny => {
            let tail = pos2(center.x + radius.x - 1.0, feet.y - 4.0);
            painter.circle(tail, 3.0, Color32::WHITE, outline);
        }
        Species::Cat => {
            let from = pos2(center.x + radius.x - 2.0, feet.y - 3.0);
            let bend = from + vec2(6.0, -3.0);
            let tip = from + vec2(5.0, if pose.curled { -4.0 } else { -11.0 });
            painter.line(vec![from, bend, tip], Stroke::new(3.0, fur));
            painter.line(vec![from, bend, tip], Stroke::new(1.0, line));
        }
    }

    // The ears: up, leaning out when they wiggle, hanging when they droop.
    let hang = (-pose.ears).clamp(0.0, 1.0);
    let lean = pose.ears.clamp(0.0, 1.0);
    for side in [-1.0_f32, 1.0] {
        match critter.species {
            Species::Bunny => {
                let length = 7.0 - 2.0 * hang;
                let at = pos2(
                    center.x + side * (4.0 + lean * 2.5 + hang * 6.0),
                    top - length + 3.0 + hang * (length + 6.0),
                );
                let ear = vec2(2.6, length);
                painter.add(Shape::ellipse_filled(at, ear, fur));
                painter.add(Shape::ellipse_stroke(at, ear, outline));
                painter.add(Shape::ellipse_filled(at, ear * 0.45, KAWAII_PINK));
            }
            Species::Cat => {
                let height = 7.0 - 5.0 * hang;
                let points = vec![
                    pos2(center.x + side * 9.0, top + 4.0),
                    pos2(center.x + side * 2.5, top + 1.5),
                    pos2(
                        center.x + side * (7.0 + lean * 2.0 + hang * 3.0),
                        top - height + 2.0,
                    ),
                ];
                painter.add(Shape::convex_polygon(points, fur, outline));
            }
        }
    }

    painter.add(Shape::ellipse_filled(center, radius, fur));
    painter.add(Shape::ellipse_stroke(center, radius, outline));

    // The face: eyes, cheeks, nose.
    let glance = look.map_or(Vec2::ZERO, |to| to.normalized() * 1.2);
    for side in [-1.0_f32, 1.0] {
        let eye = center + vec2(side * 4.0, -1.0);
        match pose.eyes {
            Eyes::Open => {
                painter.circle_filled(eye + glance, 1.6, dark);
            }
            Eyes::Up => {
                painter.circle_filled(eye + vec2(0.0, -1.8), 1.6, dark);
            }
            Eyes::Closed => {
                let half = vec2(1.8, 0.0);
                painter.line_segment([eye - half, eye + half], Stroke::new(1.2, dark));
            }
        }
        let cheek = center + vec2(side * 7.0, 2.5);
        painter.circle_filled(cheek, 1.8, KAWAII_PINK.gamma_multiply(0.6));
    }
    painter.circle_filled(center + vec2(0.0, 1.8), 0.9, KAWAII_PINK);

    if pose.curled {
        let font = egui::FontId::proportional(10.0);
        let at = center + vec2(12.0, -12.0);
        painter.text(at, egui::Align2::CENTER_CENTER, "z", font, line);
    }
    if let Some(risen) = pose.heart {
        let at = feet - vec2(0.0, HEIGHT + 6.0 + risen * 20.0);
        let color = KAWAII_PINK.gamma_multiply(1.0 - risen);
        let font = egui::FontId::proportional(14.0);
        painter.text(at, egui::Align2::CENTER_CENTER, HEART, font, color);
    }
}
