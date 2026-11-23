//! dovetail — thin CLI over dovetail-core; the command line is `dovetail::cli_main`.

use std::process::ExitCode;

fn main() -> ExitCode {
    restore_default_sigpipe();
    dovetail::cli_main()
}

/// Rust starts every program with SIGPIPE ignored, so a write to a stdout pipe
/// whose reader has gone (`dovetail relate … | head`) fails with EPIPE and
/// `println!` panics. Restoring the default action ends the process quietly
/// instead, as `cat` and other filters do. It is set here rather than in the
/// library, so code that links dovetail keeps its own signal handling.
fn restore_default_sigpipe() {
    #[cfg(unix)]
    // SAFETY: runs first in `main`, before any other thread exists, and
    // installs the default disposition rather than a handler.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}
