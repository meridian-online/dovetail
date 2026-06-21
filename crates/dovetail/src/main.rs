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

const HELP: &str = "\
dovetail — the modelling layer of meridian. Discovers how to load unfamiliar
data and how datasets relate, then compiles those decisions into runnable,
auditable artifacts. It never executes a pipeline itself.

USAGE:
    dovetail <command> [args]

COMMANDS:
    survey <paths...>             Detect each file's format and row structure,
                                  then emit a standalone .sql load and a
                                  datapackage.json descriptor.
    jaq <program> <file>          Run a jq program through embedded jaq,
                                  emitting NDJSON (true passthrough).
    relate <db> [--out <path>]    Discover and verify foreign keys across a
                                  loaded DuckDB; auto-accept the provable ones
                                  and write foreignKeys into the descriptor.

OPTIONS:
    -h, --help                    Show this help.

Run `dovetail <command> --help` for command-specific usage.
";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("survey") => run_survey_cmd(args.collect()),
        Some("jaq") => run_jaq_cmd(args.collect()),
        Some("relate") => run_relate(args.collect()),
        Some("--help") | Some("-h") | Some("help") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("dovetail: unknown command {other:?}");
            eprintln!("run `dovetail --help` for usage.");
            ExitCode::from(2)
        }
        None => {
            eprint!("{HELP}");
            ExitCode::from(2)
        }
    }
}

/// True if the args request help for a subcommand.
fn wants_help(args: &[String]) -> bool {
    args.iter().any(|a| a == "--help" || a == "-h")
}

const SURVEY_HELP: &str = "\
dovetail survey <paths...>

Discover how to load each file into DuckDB. Detects the format and row-level
structure, emits a standalone .sql load plus a datapackage.json descriptor, and
reports which fallback-ladder rung it chose and why. Under-confident detections
are surfaced for confirmation rather than emitted blind.

ARGS:
    <paths...>    One or more input files (csv, tsv, parquet, ndjson, json).
";

fn run_survey_cmd(args: Vec<String>) -> ExitCode {
    if wants_help(&args) {
        print!("{SURVEY_HELP}");
        return ExitCode::SUCCESS;
    }
    if args.is_empty() {
        eprintln!("usage: dovetail survey <paths...>   (see: dovetail survey --help)");
        return ExitCode::from(2);
    }
    let paths: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();
    run_survey(&paths)
}

/// `dovetail relate <duckdb-path>` — read a loaded DuckDB, discover and verify
/// candidate foreign keys, report accepted + to-review edges (rejected noise is
/// suppressed), and print constraint DDL for the auto-accepted edges.
const RELATE_HELP: &str = "\
dovetail relate <duckdb-path> [--out <datapackage.json>]

Discover how the tables in a loaded DuckDB relate. Reads the database (read-only
discovery), scores candidate foreign keys on naming, value overlap and parent
key-likeness, then VERIFIES each against the data. Verified high-confidence edges
auto-accept (no manual step); coincidences and broken references are rejected;
plausible-but-unprovable edges are surfaced for review. Prints ACCEPT/REVIEW
status and constraint DDL for the accepted edges.

ARGS:
    <duckdb-path>           A DuckDB database the analyst has already loaded.

OPTIONS:
    --out <path>            Also write a datapackage.json with the discovered
                           foreignKeys inside each resource's Table Schema.
";

fn run_relate(args: Vec<String>) -> ExitCode {
    use dovetail_core::relate::{constraint_ddl, run_path, EdgeStatus};
    if wants_help(&args) {
        print!("{RELATE_HELP}");
        return ExitCode::SUCCESS;
    }
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

const JAQ_HELP: &str = "\
dovetail jaq <program> <file>

Run a jq program through embedded, vendored jaq, emitting NDJSON. True
passthrough: the program is run unchanged, so a reviewer reads exactly what ran.
The embedded engine is pinned, so discovery and execution use the same bytes.

ARGS:
    <program>      A jq program (e.g. '.results[]').
    <file>         A JSON input file (one or many top-level values).

OPTIONS:
    --version      Print the embedded jaq-core version.
    -h, --help     Show this help.
";

/// `dovetail jaq <program> <file>` — run a jq program through embedded jaq,
/// emitting NDJSON. `dovetail jaq --version` stamps the embedded jaq version.
fn run_jaq_cmd(args: Vec<String>) -> ExitCode {
    if wants_help(&args) {
        print!("{JAQ_HELP}");
        return ExitCode::SUCCESS;
    }
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
