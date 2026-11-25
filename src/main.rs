//! multigit binary: loads config and history, starts the window, or the
//! terminal front end with `--tui`.

use multigit::infrastructure::backend::GitBackend;
use multigit::infrastructure::desktop::SystemDesktop;
use multigit::infrastructure::launcher::{APP_ID, ICON_PNG};
use multigit::infrastructure::store::{self, Paths};
use multigit::tui::desktop::TerminalDesktop;
use multigit::ui::{MultigitApp, app::App, style};
use std::process::ExitCode;
use std::sync::Arc;

fn main() -> ExitCode {
    let tui = std::env::args().skip(1).any(|arg| arg == "--tui");
    let paths = Paths::from_env();
    init_tracing(tui, paths.as_ref());
    let config = match &paths {
        Some(p) => store::load_config(&p.config_file).map_err(|e| e.to_string()),
        None => Err("HOME is not set".to_owned()),
    };
    let history = paths
        .as_ref()
        .map(|p| store::load_history(&p.history_file))
        .unwrap_or_default();
    let result = if tui {
        run_terminal(config, paths, history)
    } else {
        run_window(config, paths, history)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("multigit: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Log to stderr in the window. In the terminal stderr is the screen: the
/// log goes to `tui.log` in the state folder, and only when `RUST_LOG` asks
/// for it.
fn init_tracing(tui: bool, paths: Option<&Paths>) {
    let filter = tracing_subscriber::EnvFilter::from_default_env();
    if !tui {
        tracing_subscriber::fmt().with_env_filter(filter).init();
        return;
    }
    if std::env::var_os("RUST_LOG").is_none() {
        return;
    }
    let Some(dir) = paths.and_then(|p| p.history_file.parent()) else {
        return;
    };
    let file = std::fs::create_dir_all(dir).and_then(|()| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("tui.log"))
    });
    if let Ok(file) = file {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(file))
            .init();
    }
}

type Loaded = Result<multigit::domain::config::Config, String>;

fn run_terminal(
    config: Loaded,
    paths: Option<Paths>,
    history: multigit::domain::history::History,
) -> Result<(), String> {
    let backend = Arc::new(GitBackend::default());
    let app = multigit::tui::App::new(
        config,
        paths,
        history,
        backend,
        Arc::new(TerminalDesktop),
        Arc::new(|| {}),
    );
    multigit::tui::run(app).map_err(|e| e.to_string())
}

fn run_window(
    config: Loaded,
    paths: Option<Paths>,
    history: multigit::domain::history::History,
) -> Result<(), String> {
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
                app: App::new(
                    config,
                    paths,
                    history,
                    backend,
                    Arc::new(SystemDesktop),
                    notify,
                ),
            }))
        }),
    )
    .map_err(|e| e.to_string())
}
