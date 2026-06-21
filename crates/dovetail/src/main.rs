//! dovetail — thin CLI over dovetail-core (choice 0008).
//!
//! `dovetail survey <paths...>` discovers how to load each file: it detects the
//! format and row-level structure, reports which fallback-ladder rung it chose
//! and why (ac-09), and prints the emitted standalone load. Under-confident
//! detections route to suggest-and-confirm rather than emit blind (ac-10).

use std::path::PathBuf;
use std::process::ExitCode;

use dovetail_core::emit::DuplicatePolicy;
use dovetail_core::survey::{survey_file, Outcome};
use dovetail_core::ShapeHeuristicDetector;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("survey") => {
            let paths: Vec<PathBuf> = args.map(PathBuf::from).collect();
            if paths.is_empty() {
                eprintln!("usage: dovetail survey <paths...>");
                return ExitCode::from(2);
            }
            run_survey(&paths)
        }
        Some(other) => {
            eprintln!("dovetail: unknown command {other:?} (try: survey)");
            ExitCode::from(2)
        }
        None => {
            eprintln!("usage: dovetail survey <paths...>");
            ExitCode::from(2)
        }
    }
}

fn run_survey(paths: &[PathBuf]) -> ExitCode {
    // The MVP CLI uses the shape-heuristic structural detector. The canonical
    // detector (ac-04) is finetype-guided, which degrades to exactly this when no
    // model dir is configured — so structure results are identical here.
    let detector = ShapeHeuristicDetector::new();
    let mut had_error = false;

    for path in paths {
        match survey_file(path, &detector, DuplicatePolicy::default(), None) {
            Ok(report) => {
                print!("{}", report.render());
                if let Outcome::Emitted { sql, .. } = &report.outcome {
                    println!("{sql}");
                }
            }
            Err(e) => {
                eprintln!("dovetail survey: {}: {e}", path.display());
                had_error = true;
            }
        }
    }

    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
