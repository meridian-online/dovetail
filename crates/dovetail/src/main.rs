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
use dovetail_core::transform::{run_jaq, JAQ_CORE_VERSION};
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
        Some("jaq") => run_jaq_cmd(args.collect()),
        Some(other) => {
            eprintln!("dovetail: unknown command {other:?} (try: survey, jaq)");
            ExitCode::from(2)
        }
        None => {
            eprintln!("usage: dovetail <survey|jaq> ...");
            ExitCode::from(2)
        }
    }
}

/// `dovetail jaq <program> <file>` — run a jq program through embedded jaq,
/// emitting NDJSON. `dovetail jaq --version` stamps the embedded jaq version.
fn run_jaq_cmd(args: Vec<String>) -> ExitCode {
    if args.first().map(|s| s.as_str()) == Some("--version") {
        println!("dovetail jaq: embedded jaq-core {JAQ_CORE_VERSION}");
        return ExitCode::SUCCESS;
    }
    let [program, file] = match args.as_slice() {
        [p, f] => [p, f],
        _ => {
            eprintln!("usage: dovetail jaq <program> <file>   (or: dovetail jaq --version)");
            return ExitCode::from(2);
        }
    };
    let input = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("dovetail jaq: {file}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match run_jaq(program, &input) {
        Ok(out) => {
            print!("{}", out.to_ndjson());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
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
