//! The right-click menu on text: `Copy` for what is marked and `Copy all`
//! for the whole text of the pane.
//!
//! egui lets go of a marked text at any click it sees, a right click and a
//! click on a menu entry included. So the clicks that open the menu and
//! the clicks on its entries are taken out of the input before egui reads
//! it, and `Copy` is handed to egui as the copy key press it knows.

use std::path::PathBuf;

use egui::{Event, Key, PointerButton, Pos2, RawInput, Rect, Ui};

/// A text of the app that the menu copies whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextOf {
    /// The message of the picked commit.
    Message,
    /// What the right pane of the repository view shows under its header:
    /// a diff, a file, or the files of a commit with the unfolded diffs.
    Pane,
    /// What a command printed in one repository: the run and the folder.
    Output(u64, PathBuf),
}

/// A place on screen where a right click opens the menu.
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub layer: egui::LayerId,
    pub rect: Rect,
    pub text: TextOf,
}

/// The areas drawn in a frame, kept in egui's memory until the next input
/// is read.
#[derive(Debug, Clone, Default)]
struct Areas(Vec<Area>);

/// Tell that `rect` of this `ui` shows `text`: a right click there opens
/// the menu.
pub fn area(ui: &Ui, rect: Rect, text: TextOf) {
    let area = Area {
        layer: ui.layer_id(),
        rect: rect.intersect(ui.clip_rect()),
        text,
    };
    ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Areas>(egui::Id::NULL)
            .0
            .push(area);
    });
}

/// The areas of the last frame; they are forgotten with this call.
pub fn take_areas(ctx: &egui::Context) -> Vec<Area> {
    ctx.data_mut(|data| data.remove_temp::<Areas>(egui::Id::NULL))
        .map(|areas| areas.0)
        .unwrap_or_default()
}

/// The open menu.
#[derive(Debug, Clone, PartialEq)]
pub struct TextMenu {
    /// Where it was opened.
    pub at: Pos2,
    pub text: TextOf,
    /// Where it was drawn: the whole menu, `Copy` and `Copy all`. `None`
    /// until it was drawn once.
    pub drawn: Option<[Rect; 3]>,
    /// Whether a text was marked when it was last drawn: `Copy` needs one.
    pub marked: bool,
}

/// What a click on the menu asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Picked {
    /// Copy the marked text: egui does it, the copy event is in the input.
    Copy(Pos2),
    /// Copy this whole text.
    CopyAll(TextOf, Pos2),
}

/// Take the clicks that belong to the menu out of `input` before egui
/// reads it. `area_at` names the text under a point, if a menu opens
/// there. Returns what was picked in the menu.
pub fn filter(
    menu: &mut Option<TextMenu>,
    input: &mut RawInput,
    area_at: impl Fn(Pos2) -> Option<TextOf>,
) -> Option<Picked> {
    let mut picked = None;
    input.events.retain(|event| match event {
        Event::PointerButton {
            pos,
            button,
            pressed,
            ..
        } => {
            if let Some(open) = menu.as_ref()
                && let Some([whole, copy, all]) = open.drawn
                && whole.contains(*pos)
            {
                // A click on the menu never reaches what lies under it.
                if *pressed && *button == PointerButton::Primary {
                    if copy.contains(*pos) && open.marked {
                        picked = Some(Picked::Copy(*pos));
                    } else if all.contains(*pos) {
                        picked = Some(Picked::CopyAll(open.text.clone(), *pos));
                    }
                    if picked.is_some() {
                        *menu = None;
                    }
                }
                return false;
            }
            if *button == PointerButton::Secondary
                && let Some(text) = area_at(*pos)
            {
                if *pressed {
                    *menu = Some(TextMenu {
                        at: *pos,
                        text,
                        drawn: None,
                        marked: false,
                    });
                }
                return false;
            }
            // A click anywhere else closes the menu and goes its way.
            if *pressed {
                *menu = None;
            }
            true
        }
        // Esc closes the menu and nothing else.
        Event::Key {
            key: Key::Escape,
            pressed: true,
            ..
        } if menu.is_some() => {
            *menu = None;
            false
        }
        _ => true,
    });
    if matches!(picked, Some(Picked::Copy(_))) {
        input.events.push(Event::Copy);
    }
    picked
}

/// Draw the open menu and remember where its entries are.
pub fn show(ctx: &egui::Context, menu: &mut TextMenu) {
    let marked = ctx
        .plugin::<egui::text_selection::LabelSelectionState>()
        .lock()
        .has_selection();
    let area = egui::Area::new(egui::Id::new("text-menu"))
        .order(egui::Order::Tooltip)
        .fixed_pos(menu.at)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style())
                .show(ui, |ui| {
                    ui.set_min_width(120.0);
                    let copy = ui.add_enabled(marked, egui::Button::new("Copy").frame(false));
                    let all = ui.add(egui::Button::new("Copy all").frame(false));
                    (copy.rect, all.rect)
                })
                .inner
        });
    let (copy, all) = area.inner;
    menu.drawn = Some([area.response.rect, copy, all]);
    menu.marked = marked;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn click(pos: Pos2, which: PointerButton) -> Vec<Event> {
        vec![button(pos, which, true), button(pos, which, false)]
    }

    fn input(events: Vec<Event>) -> RawInput {
        RawInput {
            events,
            ..Default::default()
        }
    }

    /// Text lies left of x = 100.
    fn text_at(pos: Pos2) -> Option<TextOf> {
        (pos.x < 100.0).then_some(TextOf::Message)
    }

    fn drawn(menu: &mut Option<TextMenu>, marked: bool) {
        let open = menu.as_mut().unwrap();
        let rect = |top: f32| Rect::from_min_size(Pos2::new(50.0, top), egui::vec2(80.0, 20.0));
        open.drawn = Some([
            Rect::from_min_size(Pos2::new(50.0, 50.0), egui::vec2(80.0, 40.0)),
            rect(50.0),
            rect(70.0),
        ]);
        open.marked = marked;
    }

    #[test]
    fn a_right_click_on_text_opens_the_menu_and_egui_never_sees_it() {
        let mut menu = None;
        let at = Pos2::new(50.0, 50.0);
        let mut raw = input(click(at, PointerButton::Secondary));
        raw.events.insert(0, Event::PointerMoved(at));
        assert_eq!(filter(&mut menu, &mut raw, text_at), None);
        assert_eq!(raw.events, [Event::PointerMoved(at)]);
        let open = menu.clone().unwrap();
        assert_eq!((open.at, open.text), (at, TextOf::Message));

        // Elsewhere a right click is a right click.
        let mut none = None;
        let elsewhere = click(Pos2::new(300.0, 50.0), PointerButton::Secondary);
        let mut raw = input(elsewhere.clone());
        filter(&mut none, &mut raw, text_at);
        assert_eq!(raw.events, elsewhere);
        assert_eq!(none, None);
        // So is a left click on text: it marks.
        let left = click(at, PointerButton::Primary);
        let mut raw = input(left.clone());
        filter(&mut none, &mut raw, text_at);
        assert_eq!(raw.events, left);
        assert_eq!(none, None);
    }

    #[test]
    fn copy_becomes_the_copy_event_and_the_click_is_swallowed() {
        let mut menu = None;
        let at = Pos2::new(50.0, 50.0);
        filter(
            &mut menu,
            &mut input(click(at, PointerButton::Secondary)),
            text_at,
        );
        drawn(&mut menu, true);
        let on_copy = Pos2::new(60.0, 60.0);
        let mut raw = input(click(on_copy, PointerButton::Primary));
        let picked = filter(&mut menu, &mut raw, text_at);
        assert_eq!(picked, Some(Picked::Copy(on_copy)));
        assert_eq!(
            raw.events,
            [button(on_copy, PointerButton::Primary, false), Event::Copy]
        );
        assert_eq!(menu, None, "the menu closed");
        // The release that follows is on text again, and is a plain event.
    }

    #[test]
    fn copy_needs_a_marked_text_and_copy_all_does_not() {
        let mut menu = None;
        let at = Pos2::new(50.0, 50.0);
        filter(
            &mut menu,
            &mut input(click(at, PointerButton::Secondary)),
            text_at,
        );
        drawn(&mut menu, false);
        let on_copy = Pos2::new(60.0, 60.0);
        let mut raw = input(click(on_copy, PointerButton::Primary));
        assert_eq!(filter(&mut menu, &mut raw, text_at), None);
        assert!(raw.events.is_empty(), "swallowed all the same");
        assert!(menu.is_some(), "and the menu stays");

        let on_all = Pos2::new(60.0, 80.0);
        let mut raw = input(vec![button(on_all, PointerButton::Primary, true)]);
        assert_eq!(
            filter(&mut menu, &mut raw, text_at),
            Some(Picked::CopyAll(TextOf::Message, on_all))
        );
        assert!(raw.events.is_empty());
        assert_eq!(menu, None);
    }

    #[test]
    fn a_click_elsewhere_and_esc_close_the_menu() {
        let at = Pos2::new(50.0, 50.0);
        let open = |marked| {
            let mut menu = None;
            filter(
                &mut menu,
                &mut input(click(at, PointerButton::Secondary)),
                text_at,
            );
            drawn(&mut menu, marked);
            menu
        };
        let mut menu = open(true);
        let away = click(Pos2::new(300.0, 300.0), PointerButton::Primary);
        let mut raw = input(away.clone());
        assert_eq!(filter(&mut menu, &mut raw, text_at), None);
        assert_eq!(raw.events, away, "the click goes its way");
        assert_eq!(menu, None);

        let mut menu = open(true);
        let esc = Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut raw = input(vec![esc.clone()]);
        filter(&mut menu, &mut raw, text_at);
        assert!(raw.events.is_empty(), "Esc closes the menu only");
        assert_eq!(menu, None);
        // Without a menu Esc is Esc.
        let mut raw = input(vec![esc.clone()]);
        filter(&mut menu, &mut raw, text_at);
        assert_eq!(raw.events, [esc]);

        // A right click on other text moves the menu there.
        let mut menu = open(true);
        let other = Pos2::new(10.0, 200.0);
        filter(
            &mut menu,
            &mut input(click(other, PointerButton::Secondary)),
            text_at,
        );
        assert_eq!(menu.unwrap().at, other);
    }
}
