//! dovetail — thin CLI over dovetail-core.
//!
//! The command line lives here as [`cli_main`], so the `dovetail` binary and a
//! host that runs dovetail in its own child process share one entry point.
//!
//! `dovetail survey <paths...>` discovers how to load each file: it detects the
//! format and row-level structure, reports which fallback-ladder rung it chose
//! and why, and prints the emitted standalone load. Under-confident
//! detections route to suggest-and-confirm rather than emit blind.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dovetail_core::emit::DuplicatePolicy;
use dovetail_core::nominations::Nominations;
use dovetail_core::survey::{survey_file, Outcome};
use dovetail_core::transform::{run_jaq, JAQ_CORE_VERSION};

#[cfg(feature = "finetype-guided")]
use dovetail_core::FinetypeGuidedDetector;
#[cfg(not(feature = "finetype-guided"))]
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
    model --db <duckdb> --pass <sample|full> --out <file>
                                  Write an Ossie semantic model of a loaded
                                  DuckDB: its datasets, fields and joins.

OPTIONS:
    -h, --help                    Show this help.

Run `dovetail <command> --help` for command-specific usage.
";

/// Run the command line over the process arguments and return its exit code.
pub fn cli_main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("survey") => run_survey_cmd(args.collect()),
        Some("jaq") => run_jaq_cmd(args.collect()),
        Some("relate") => run_relate(args.collect()),
        Some("model") => run_model_cmd(args.collect()),
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
dovetail survey <paths...> [--nominations <file>]

Discover how to load each file into DuckDB. Detects the format and row-level
structure, emits a standalone .sql load plus a datapackage.json descriptor, and
reports which fallback-ladder rung it chose and why. Under-confident detections
are surfaced for confirmation rather than emitted blind.

ARGS:
    <paths...>                One or more input files (csv, tsv, parquet, ndjson, json).

OPTIONS:
    --nominations <file>       A declaration file naming a column's type by file
                               stem, the same file finetype reads. A nominated
                               label is taken as given: it replaces whatever was
                               detected and carries its taxonomy bounds into the
                               field's constraints.
";

fn run_survey_cmd(args: Vec<String>) -> ExitCode {
    if wants_help(&args) {
        print!("{SURVEY_HELP}");
        return ExitCode::SUCCESS;
    }

    let mut paths: Vec<PathBuf> = Vec::new();
    let mut nominations_path: Option<PathBuf> = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--nominations" {
            let Some(value) = args.next() else {
                eprintln!("dovetail survey: --nominations requires a file path");
                return ExitCode::from(2);
            };
            nominations_path = Some(PathBuf::from(value));
        } else {
            paths.push(PathBuf::from(arg));
        }
    }

    if paths.is_empty() {
        eprintln!(
            "usage: dovetail survey <paths...> [--nominations <file>]   (see: dovetail survey --help)"
        );
        return ExitCode::from(2);
    }

    let nominations = match nominations_path {
        Some(path) => match Nominations::load(&path) {
            Ok(n) => Some(n),
            Err(e) => {
                eprintln!("dovetail survey: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    run_survey(&paths, nominations.as_ref())
}

/// `dovetail relate <duckdb-path>` — read a loaded DuckDB, discover and verify
/// candidate foreign keys, report accepted + to-review edges (rejected noise is
/// suppressed), print constraint DDL for the auto-accepted edges, then each
/// table's role, the folds and the combinations.
const RELATE_HELP: &str = "\
dovetail relate <duckdb-path> [--out <datapackage.json>]

Discover how the tables in a loaded DuckDB relate. Reads the database (read-only
discovery), scores candidate foreign keys on naming, value overlap and parent
key-likeness, then VERIFIES each against the data. Verified high-confidence edges
auto-accept (no manual step); coincidences and broken references are rejected;
plausible-but-unprovable edges are surfaced for review. Prints ACCEPT/REVIEW
status and constraint DDL for the accepted edges, then what each table is for
(entity, event, reference, bridge, snapshot or unclear), the lookups that fold
into the one table using them, and the tables that combine into one dataset.

ARGS:
    <duckdb-path>           A DuckDB database the analyst has already loaded.

OPTIONS:
    --out <path>            Also write a datapackage.json with the discovered
                           foreignKeys inside each resource's Table Schema.
";

fn run_relate(args: Vec<String>) -> ExitCode {
    use dovetail_core::evidence::Session;
    use dovetail_core::finetype_ext::TypingRequest;
    use dovetail_core::relate::{constraint_ddl, EdgeStatus};
    use dovetail_core::roles::{run_parts, run_roles};
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
    // One session and one typing: the descriptor and the roles read the same
    // column evidence. The descriptor and join summary are out before the
    // roles are scored, so a roles failure leaves them in place.
    let parts = match Session::attach(std::path::Path::new(db), TypingRequest::Best)
        .and_then(|s| run_parts(&s).map(|p| (s, p)))
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("dovetail relate: {db}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (session, parts) = parts;
    let run = &parts.relate;
    let edges = &run.edges;

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
    for e in edges {
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
        println!(
            "{mark}  {} -> {}  ({})",
            e.child.qualified(),
            e.parent.qualified(),
            e.reason
        );
    }
    eprintln!("\ndovetail relate: {accepted_n} accepted, {review_n} to review");
    if accepted_n > 0 {
        println!("\n{}", constraint_ddl(edges).trim_end());
    }
    match run_roles(&session, parts) {
        Ok(model) => println!("\n{}", model.render_summary().trim_end()),
        Err(e) => {
            eprintln!("dovetail relate: {db}: scoring table roles: {e}");
            return ExitCode::FAILURE;
        }
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

fn run_survey(paths: &[PathBuf], nominations: Option<&Nominations>) -> ExitCode {
    // The shipped CLI surveys with the finetype-guided detector: it types each
    // column with the neural classifier when `DOVETAIL_FINETYPE_MODEL_DIR` is
    // configured, and with finetype's deterministic value-only typing floor
    // otherwise — so emitted descriptors carry semantic field types either way.
    // A `--no-default-features` build drops the ML stack and surveys structure
    // only via the shape-heuristic detector.
    #[cfg(feature = "finetype-guided")]
    let detector = FinetypeGuidedDetector::from_env();
    #[cfg(not(feature = "finetype-guided"))]
    let detector = ShapeHeuristicDetector::new();
    let mut had_error = false;

    for path in paths {
        match survey_file(
            path,
            &detector,
            DuplicatePolicy::default(),
            None,
            nominations,
        ) {
            Ok(report) => {
                print!("{}", report.render());
                if let Outcome::Emitted {
                    sql, descriptor, ..
                } = &report.outcome
                {
                    println!("{sql}");
                    // The Data Package descriptor the survey assembled: its Table
                    // Schema carries the finetype semantic types the guided typer
                    // assigned each column (survey's half of the model).
                    match serde_json::to_string_pretty(descriptor) {
                        Ok(json) => println!("\n-- datapackage.json\n{json}"),
                        Err(e) => eprintln!("dovetail survey: descriptor serialization: {e}"),
                    }
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

const MODEL_USAGE: &str = "usage: dovetail model --db <duckdb> --pass <sample|full> --out <file> [--packages <dir>] [--base <file>] [--verify <file>] [--name <name>]";

const MODEL_HELP: &str = "\
dovetail model --db <duckdb> --pass <sample|full> --out <file>
               [--packages <dir>] [--base <file>] [--verify <file>] [--name <name>]

Write an Ossie semantic model of the tables in a loaded DuckDB: its datasets and
their fields, the joins between them, what each table is for, and the evidence
behind each decision. The database is only read.

The sample pass discovers on a sample of each table and proposes the model. The
full pass checks that proposal against every row, rewriting only grades and
evidence; without a usable --verify it discovers afresh on the full data.

OPTIONS:
    --db <duckdb>           The DuckDB holding the tables.
    --pass <sample|full>    Which pass to run.
    --out <file>            Where to write the model. It is written whole or not
                            at all, and its directory is created if missing.
    --packages <dir>        A directory of data packages that name and type the
                            datasets. Without it every table of the database is
                            a dataset, typed by FineType when it is available.
    --base <file>           The analyst's model, whose decisions are carried
                            through. It is never written.
    --verify <file>         For the full pass, the sample pass's model to check.
    --name <name>           The model's name. Defaults to the database file's
                            name without its extension.

Warnings go to stderr, each line prefixed `dovetail model:`.
";

/// `dovetail model` — run one pass of the Ossie model and write it to `--out`.
fn run_model_cmd(args: Vec<String>) -> ExitCode {
    use dovetail_core::finetype_ext::TypingRequest;
    use dovetail_core::ossie::{run_model, write, ModelRequest, Verified};
    if wants_help(&args) {
        print!("{MODEL_HELP}");
        return ExitCode::SUCCESS;
    }
    let usage = |why: &str| {
        eprintln!("dovetail model: {why}");
        eprintln!("{MODEL_USAGE}");
        ExitCode::from(2)
    };

    let (mut db, mut pass, mut out) = (None, None, None);
    let (mut packages, mut base, mut verify, mut name) = (None, None, None, None);
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        let slot = match flag.as_str() {
            "--db" => &mut db,
            "--pass" => &mut pass,
            "--out" => &mut out,
            "--packages" => &mut packages,
            "--base" => &mut base,
            "--verify" => &mut verify,
            "--name" => &mut name,
            other => return usage(&format!("unexpected argument {other:?}")),
        };
        let Some(value) = args.next() else {
            return usage(&format!("{flag} requires a value"));
        };
        *slot = Some(value);
    }
    let Some(db) = db.map(PathBuf::from) else {
        return usage("--db is required");
    };
    let Some(out) = out.map(PathBuf::from) else {
        return usage("--out is required");
    };
    let pass = match pass.as_deref() {
        Some("sample") => Verified::Sample,
        Some("full") => Verified::Full,
        Some(other) => return usage(&format!("--pass is sample or full, not {other:?}")),
        None => return usage("--pass is required"),
    };
    if verify.is_some() && pass == Verified::Sample {
        eprintln!("dovetail model: --verify applies to the full pass only; ignored");
        verify = None;
    }
    for (flag, other) in [
        ("--db", Some(db.as_path())),
        ("--base", base.as_ref().map(Path::new)),
        ("--verify", verify.as_ref().map(Path::new)),
    ] {
        if let Some(other) = other {
            if same_path(&out, other) {
                return usage(&format!(
                    "--out is the same file as {flag}; name a different path"
                ));
            }
        }
    }
    let name = name.unwrap_or_else(|| {
        db.file_stem()
            .map_or_else(|| "model".to_owned(), |s| s.to_string_lossy().into_owned())
    });

    let req = ModelRequest {
        db,
        packages: packages.map(PathBuf::from),
        pass,
        base: base.map(PathBuf::from),
        verify: verify.map(PathBuf::from),
        name,
        typing: TypingRequest::Best,
    };
    let (model, warnings) = match run_model(&req) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("dovetail model: {e}");
            return ExitCode::FAILURE;
        }
    };
    for w in &warnings {
        eprintln!("dovetail model: {w}");
    }
    if let Err(e) = write_atomically(&out, &write(&model)) {
        eprintln!("dovetail model: writing {}: {e}", out.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Whether two paths name the same file, by absolute path: canonical where the
/// path exists, lexically absolute where it does not.
fn same_path(a: &Path, b: &Path) -> bool {
    let abs = |p: &Path| {
        std::fs::canonicalize(p)
            .or_else(|_| std::path::absolute(p))
            .unwrap_or_else(|_| p.to_path_buf())
    };
    abs(a) == abs(b)
}

/// Write `text` to `<out>.writing` beside `out`, then rename it over `out`, so a
/// reader sees the whole model or none. A failure removes the `.writing` file.
fn write_atomically(out: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = out.as_os_str().to_owned();
    tmp.push(".writing");
    let tmp = PathBuf::from(tmp);
    let done = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, out));
    if done.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    done
}
