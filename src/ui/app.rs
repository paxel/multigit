//! What the egui window keeps beside the board's [`crate::app::App`].

use crate::ui::cli::CliState;
use crate::ui::text_menu::{self, Picked, TextMenu};

pub use crate::app::{
    Card, FavoriteDialog, MarkBy, PendingRun, RepoView, SettingsPage, TextOf, Toast,
};

/// The board as the egui window drives it.
pub type App = crate::app::App<Gui>;

/// The window's own state: widgets, animations and textures.
#[derive(Default)]
pub struct Gui {
    pub cli: CliState,
    pub copy_flash: Option<CopyFlash>,
    /// The open right-click menu on a text.
    pub text_menu: Option<TextMenu>,
    /// What was picked in it, until the next frame does it.
    pub text_picked: Option<Picked>,
    /// The app icon for the About page, once it was drawn.
    pub icon: Option<egui::TextureHandle>,
}

/// The "Copied" animation after a click on a card text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CopyFlash {
    /// Pointer position of the click.
    pub at: egui::Pos2,
    /// `egui` input time when the copy happened, in seconds.
    pub started: f64,
}

impl App {
    /// Before egui reads the input of a frame: the clicks that belong to
    /// the right-click menu on a text are taken out of it.
    pub fn filter_input(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        let areas = text_menu::take_areas(ctx);
        let picked = text_menu::filter(&mut self.gui.text_menu, input, |pos| {
            // Only what lies on top at that point is meant.
            let layer = ctx.layer_id_at(pos)?;
            let hit = |area: &&text_menu::Area| area.layer == layer && area.rect.contains(pos);
            areas.iter().rev().find(hit).map(|area| area.text.clone())
        });
        if picked.is_some() {
            self.gui.text_picked = picked;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::App;
    use crate::infrastructure::store::Paths;
    use std::sync::Arc;

    pub use crate::app::tests::{FakeBackend, FakeDesktop, snapshot};

    pub fn app_with(backend: Arc<FakeBackend>, paths: Option<Paths>) -> App {
        app_on(backend, Arc::default(), paths)
    }

    pub fn app_on(
        backend: Arc<FakeBackend>,
        desktop: Arc<FakeDesktop>,
        paths: Option<Paths>,
    ) -> App {
        crate::app::tests::app_on_front(backend, desktop, paths)
    }
}
