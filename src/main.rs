//! multigit: the window, or the terminal front end with `--tui`. On Windows
//! this is a window program, so it opens no console; `multigit-tui` is the
//! console program there.

#![cfg_attr(windows, windows_subsystem = "windows")]

use multigit::start::{Front, main as start};
use std::process::ExitCode;

fn main() -> ExitCode {
    let front = Front::from_args(std::env::args());
    // A window program has no console on Windows: nothing to draw the
    // terminal board in, and nowhere to say so.
    if cfg!(windows) && front == Front::Terminal {
        return ExitCode::FAILURE;
    }
    start(front)
}
