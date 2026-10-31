//! dovetail — thin CLI over dovetail-core; the command line is `dovetail::cli_main`.

use std::process::ExitCode;

fn main() -> ExitCode {
    dovetail::cli_main()
}
