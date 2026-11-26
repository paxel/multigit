//! multigit-tui: the terminal front end, as `multigit --tui`. On Windows it
//! is the console program, as `multigit.exe` is a window program there.

use multigit::start::{Front, main as start};
use std::process::ExitCode;

fn main() -> ExitCode {
    start(Front::Terminal)
}
