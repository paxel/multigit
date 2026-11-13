//! egui front end.

pub mod app;
pub mod cli;
pub mod critter;
pub mod repo_view;
pub mod settings;
pub mod style;
pub mod view;

#[cfg(test)]
mod tests;

/// eframe wrapper around [`app::App`].
pub struct MultigitApp {
    pub app: app::App,
}

impl eframe::App for MultigitApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.show(ui);
    }
}
