//! multigit binary: loads config and history, starts the window.

use multigit::infrastructure::backend::GitBackend;
use multigit::infrastructure::launcher::{APP_ID, ICON_PNG};
use multigit::infrastructure::store::{self, Paths};
use multigit::ui::{MultigitApp, app::App, style};
use std::sync::Arc;

fn main() -> eframe::Result {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let paths = Paths::from_env();
    let config = match &paths {
        Some(p) => store::load_config(&p.config_file).map_err(|e| e.to_string()),
        None => Err("HOME is not set".to_owned()),
    };
    let history = paths
        .as_ref()
        .map(|p| store::load_history(&p.history_file))
        .unwrap_or_default();

    // Desktop entries are a freedesktop.org convention; macOS uses the .app bundle.
    #[cfg(all(unix, not(target_os = "macos")))]
    multigit::infrastructure::launcher::register_in_background();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("multigit")
        .with_app_id(APP_ID)
        .with_inner_size([1200.0, 800.0]);
    match eframe::icon_data::from_png_bytes(ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => tracing::warn!("cannot load window icon: {e}"),
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "multigit",
        options,
        Box::new(move |cc| {
            style::apply(&cc.egui_ctx);
            let ctx = cc.egui_ctx.clone();
            let notify = Arc::new(move || ctx.request_repaint());
            let backend = Arc::new(GitBackend::default());
            Ok(Box::new(MultigitApp {
                app: App::new(config, paths, history, backend, notify),
            }))
        }),
    )
}
