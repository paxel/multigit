//! Starting multigit: loads config and history, then runs the window or
//! the terminal front end. Both programs, `multigit` and `multigit-tui`,
//! start here.

use crate::infrastructure::backend::GitBackend;
use crate::infrastructure::desktop::SystemDesktop;
use crate::infrastructure::launcher::{APP_ID, ICON_PNG};
use crate::infrastructure::store::{self, Paths};
use crate::tui::desktop::TerminalDesktop;
use crate::ui::{MultigitApp, app::App, style};
use std::process::ExitCode;
use std::sync::Arc;

/// Which front end to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Front {
    Window,
    Terminal,
}

impl Front {
    /// The front end the command line asks for: the terminal with `--tui`.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Self {
        if args.into_iter().skip(1).any(|arg| arg == "--tui") {
            Self::Terminal
        } else {
            Self::Window
        }
    }
}

/// Load config and history, then run `front` until it ends.
pub fn main(front: Front) -> ExitCode {
    let tui = front == Front::Terminal;
    let paths = Paths::from_env();
    init_tracing(tui, paths.as_ref());
    let config = match &paths {
        Some(p) => store::load_config(&p.config_file).map_err(|e| e.to_string()),
        None => Err(Paths::missing().to_owned()),
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

type Loaded = Result<crate::domain::config::Config, String>;

fn run_terminal(
    config: Loaded,
    paths: Option<Paths>,
    history: crate::domain::history::History,
) -> Result<(), String> {
    let backend = Arc::new(GitBackend::default());
    let desktop = Arc::new(TerminalDesktop::default());
    let app = crate::tui::App::new(
        config,
        paths,
        history,
        backend,
        desktop.clone(),
        Arc::new(|| {}),
    );
    crate::tui::run(app, desktop).map_err(|e| e.to_string())
}

fn run_window(
    config: Loaded,
    paths: Option<Paths>,
    history: crate::domain::history::History,
) -> Result<(), String> {
    // Desktop entries are a freedesktop.org convention; macOS uses the .app bundle.
    #[cfg(all(unix, not(target_os = "macos")))]
    crate::infrastructure::launcher::register_in_background();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tui_on_the_command_line_starts_the_terminal() {
        let args = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(Front::from_args(args(&["multigit"])), Front::Window);
        assert_eq!(
            Front::from_args(args(&["multigit", "--tui"])),
            Front::Terminal
        );
        assert_eq!(
            Front::from_args(args(&["--tui"])),
            Front::Window,
            "the program name"
        );
    }
}
