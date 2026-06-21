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
        Some("relate") => run_relate(args.collect()),
        Some(other) => {
            eprintln!("dovetail: unknown command {other:?} (try: survey, relate)");
            ExitCode::from(2)
        }
        None => {
            eprintln!("usage: dovetail <survey|relate> ...");
            ExitCode::from(2)
        }
    }
}

/// `dovetail relate <duckdb-path>` — read a loaded DuckDB, discover and verify
/// candidate foreign keys, report accepted + to-review edges (rejected noise is
/// suppressed), and print constraint DDL for the auto-accepted edges.
fn run_relate(args: Vec<String>) -> ExitCode {
    use dovetail_core::relate::{constraint_ddl, run_path, EdgeStatus};
    // dovetail relate <duckdb-path> [--out <datapackage.json>]
    let (db, out) = match args.as_slice() {
        [d] => (d.as_str(), None),
        [d, flag, path] if flag == "--out" => (d.as_str(), Some(path.as_str())),
        _ => {
            eprintln!("usage: dovetail relate <duckdb-path> [--out <datapackage.json>]");
            return ExitCode::from(2);
        }
    };
    let run = match run_path(db) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("dovetail relate: {db}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let edges = run.edges;

    // Write/update the Data Package descriptor with the discovered foreignKeys.
    if let Some(out_path) = out {
        let json = run.descriptor.to_string();
        if let Err(e) = std::fs::write(out_path, json) {
            eprintln!("dovetail relate: writing {out_path}: {e}");
            return ExitCode::FAILURE;
        }
        eprintln!("dovetail relate: wrote descriptor → {out_path}");
    }

    let (mut accepted_n, mut review_n) = (0u32, 0u32);
    for e in &edges {
        let mark = match e.status {
            EdgeStatus::Accepted => {
                accepted_n += 1;
                "ACCEPT"
            }
            EdgeStatus::Suggested => {
                review_n += 1;
                "REVIEW"
            }
            EdgeStatus::Rejected => continue,
        };
        println!("{mark}  {} -> {}  ({})", e.child.qualified(), e.parent.qualified(), e.reason);
    }
    eprintln!("\ndovetail relate: {accepted_n} accepted, {review_n} to review");
    if accepted_n > 0 {
        println!("\n{}", constraint_ddl(&edges).trim_end());
    }
    ExitCode::SUCCESS
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
