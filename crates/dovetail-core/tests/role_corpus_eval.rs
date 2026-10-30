//! The role corpus: its answer keys checked against themselves, and the
//! baseline of tiers 2 and 3 recorded from the fetched data.
//!
//! `answer_keys_are_consistent` needs no data and runs everywhere.
//! `corpus_baseline` needs the corpus that `scripts/fetch-role-corpus.sh`
//! fetches into `DOVETAIL_CORPUS`, and FineType's extension and model
//! (`scripts/fetch-finetype.sh`); without either it says why and skips. It
//! scores every CTU database (tier 2) and every sampled GitTables table
//! (tier 3), floor-only and profiled, and rewrites the corpus part of
//! `tests/role-corpus/RESULTS.md`, from its corpus heading on, leaving tier 1's
//! part to `role_eval_harness`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use dovetail_core::evidence::{column_evidence, Session};
use dovetail_core::finetype_ext::{TypingRequest, TypingSource};
use dovetail_core::relate::EdgeStatus;
use dovetail_core::role_corpus::{load_corpus_set, load_corpus_table};
use dovetail_core::role_eval::{
    render_results, role_name, score_set, split_record, stamp, RoleScorer, ScorePerRole, SetResult,
    CORPUS_HEADING,
};
use dovetail_core::role_key::{load_answer_key, AnswerKey, Role};
use dovetail_core::roles::RoleModel;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn results_path() -> PathBuf {
    repo_root().join("tests/role-corpus/RESULTS.md")
}

fn keys_dir() -> PathBuf {
    repo_root().join("tests/role-corpus/ctu")
}

/// The databases `scripts/fetch-role-corpus.sh` lists in `CTU_DBS`, in its
/// order.
fn ctu_dbs_in_script() -> Vec<String> {
    let script = std::fs::read_to_string(repo_root().join("scripts/fetch-role-corpus.sh"))
        .expect("read fetch-role-corpus.sh");
    let start = script.find("\nCTU_DBS=(").expect("CTU_DBS in the script") + 10;
    let end = start + script[start..].find(')').expect("CTU_DBS closes");
    script[start..end]
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// A fresh directory holding two small Parquet tables, `Orders` (with an
/// awkward column name) and `customers`.
fn tiny_set(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("role_corpus_{}_{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let conn = duckdb::Connection::open_in_memory().unwrap();
    let out = |name: &str| dir.join(name).display().to_string().replace('\'', "''");
    conn.execute_batch(&format!(
        "COPY (SELECT i AS customer_id, 'c' || i || '@example.com' AS email FROM range(1, 41) t(i))
           TO '{}' (FORMAT parquet);
         COPY (SELECT i AS \"Order ID\", 1 + i % 40 AS customer_id FROM range(1, 201) t(i))
           TO '{}' (FORMAT parquet);",
        out("customers.parquet"),
        out("Orders.parquet"),
    ))
    .unwrap();
    std::fs::write(dir.join("declared-keys.csv"), "not a table\n").unwrap();
    dir
}

/// A set loads one table per Parquet file, named after its stem; a lone
/// table loads alone. The floor writes no profile cache.
#[test]
fn corpus_files_load_as_tables() {
    let dir = tiny_set("floor");
    let s = load_corpus_set(&dir, TypingRequest::Floor).expect("load the set");
    let tables: BTreeSet<String> = column_evidence(&s)
        .unwrap()
        .into_iter()
        .map(|c| c.table)
        .collect();
    assert_eq!(
        tables,
        BTreeSet::from(["Orders".into(), "customers".into()])
    );
    let lone = load_corpus_table(&dir.join("Orders.parquet"), TypingRequest::Floor).unwrap();
    let cols: Vec<String> = column_evidence(&lone)
        .unwrap()
        .into_iter()
        .map(|c| format!("{}.{}", c.table, c.column))
        .collect();
    assert_eq!(cols, ["Orders.Order ID", "Orders.customer_id"]);
    assert!(!dir.join(".profile.json").exists());
    std::fs::remove_dir_all(&dir).ok();
}

/// A profiled set writes its cache, a second load reads it and types the
/// columns the same, and a cache with another stamp is profiled again.
#[test]
fn a_profile_is_cached_and_restamped() {
    let dir = tiny_set("profiled");
    let first = load_corpus_set(&dir, TypingRequest::Best).expect("load the set");
    let Some(stamp) = dovetail_core::role_corpus::profile_cache_stamp(&first.typing) else {
        eprintln!(
            "skipping: FineType's profile is unavailable: {:?}",
            first.typing
        );
        std::fs::remove_dir_all(&dir).ok();
        return;
    };
    let cache = dir.join(".profile.json");
    let written = std::fs::read_to_string(&cache).expect("the cache is written");
    assert!(written.contains(&stamp), "{written}");
    let profiled = column_evidence(&first).unwrap();

    let second = load_corpus_set(&dir, TypingRequest::Best).unwrap();
    assert_eq!(column_evidence(&second).unwrap(), profiled);

    // A label planted in the cache is used under the session's stamp, and not
    // under another.
    let plant = |stamp: &str| {
        let mut v: serde_json::Value = serde_json::from_str(&written).unwrap();
        v["stamp"] = stamp.into();
        for c in v["columns"].as_array_mut().unwrap() {
            if c["column"] == "email" {
                c["label"] = "planted.label".into();
            }
        }
        std::fs::write(&cache, v.to_string()).unwrap();
    };
    let email = |s: &Session| {
        column_evidence(s)
            .unwrap()
            .into_iter()
            .find(|c| c.column == "email")
            .unwrap()
            .label
    };
    plant(&stamp);
    let planted = load_corpus_set(&dir, TypingRequest::Best).unwrap();
    assert_eq!(email(&planted).as_deref(), Some("planted.label"));
    plant("another stamp");
    let restamped = load_corpus_set(&dir, TypingRequest::Best).unwrap();
    assert_eq!(column_evidence(&restamped).unwrap(), profiled);
    assert!(std::fs::read_to_string(&cache).unwrap().contains(&stamp));
    std::fs::remove_dir_all(&dir).ok();
}

/// Every committed CTU answer key, by file stem.
fn answer_keys() -> BTreeMap<String, AnswerKey> {
    let mut keys = BTreeMap::new();
    for entry in std::fs::read_dir(keys_dir()).expect("read the keys directory") {
        let path = entry.expect("directory entry").path();
        if path.extension().is_some_and(|x| x == "json") {
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            let key = load_answer_key(&path).unwrap_or_else(|e| panic!("{e}"));
            keys.insert(stem, key);
        }
    }
    keys
}

/// Each key loads, is named after its file, and agrees with itself: a fold's
/// reference is a table the key calls a reference, its host is a table of the
/// key, and a combination's name is a table of the key. The keys are exactly
/// the databases the fetch script fetches.
#[test]
fn answer_keys_are_consistent() {
    let keys = answer_keys();
    let mut problems = Vec::new();
    for (stem, key) in &keys {
        if &key.name != stem {
            problems.push(format!("{stem}.json: named `{}`", key.name));
        }
        for f in &key.folds {
            match key.tables.get(&f.reference) {
                Some(t) if t.role == Role::Reference => {}
                Some(t) => problems.push(format!(
                    "{stem}: fold of `{}` into `{}`: the key calls `{}` {}, not reference",
                    f.reference,
                    f.into,
                    f.reference,
                    role_name(t.role)
                )),
                None => problems.push(format!(
                    "{stem}: fold of `{}`: no such table in the key",
                    f.reference
                )),
            }
            if !key.tables.contains_key(&f.into) {
                problems.push(format!(
                    "{stem}: fold of `{}` into `{}`: no such host in the key",
                    f.reference, f.into
                ));
            }
        }
        for c in &key.combinations {
            if !key.tables.contains_key(&c.name) {
                problems.push(format!(
                    "{stem}: combination `{}`: no such table in the key",
                    c.name
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));

    let mut script = ctu_dbs_in_script();
    script.sort();
    let stems: Vec<String> = keys.keys().cloned().collect();
    assert_eq!(
        stems, script,
        "the answer keys are not the fetch script's CTU_DBS"
    );
}

/// The checkout's commit, `-dirty` when it has uncommitted changes, or
/// `unknown` outside a git checkout.
fn dovetail_commit() -> String {
    Command::new("git")
        .args(["describe", "--always", "--dirty"])
        .current_dir(repo_root())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// The fields of one CSV line, honouring double quotes.
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                fields.last_mut().unwrap().push('"');
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(String::new()),
            c => fields.last_mut().unwrap().push(c),
        }
    }
    fields
}

/// The rows of a CSV file with a header, as maps from header to field.
fn read_csv(path: &Path) -> Vec<BTreeMap<String, String>> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut lines = text.lines().filter(|l| !l.is_empty());
    let header = csv_fields(lines.next().expect("a header"));
    lines
        .map(|l| header.iter().cloned().zip(csv_fields(l)).collect())
        .collect()
}

/// A declared foreign key, lower-cased: child table and column, parent table
/// and column.
type ForeignKey = (String, String, String, String);

/// A database's declared foreign keys, sorted into the ones join discovery is
/// scored on (single-column, between two tables) and the rest.
#[derive(Default)]
struct DeclaredKeys {
    single: BTreeSet<ForeignKey>,
    self_references: BTreeSet<ForeignKey>,
    /// One per constraint: its column pairs.
    composite: Vec<Vec<ForeignKey>>,
}

fn declared_keys(path: &Path) -> DeclaredKeys {
    let mut constraints: BTreeMap<(String, String), Vec<ForeignKey>> = BTreeMap::new();
    for row in read_csv(path) {
        if row["kind"] != "foreign_key" {
            continue;
        }
        let fk = (
            row["table_name"].to_lowercase(),
            row["column_name"].to_lowercase(),
            row["referenced_table_name"].to_lowercase(),
            row["referenced_column_name"].to_lowercase(),
        );
        constraints
            .entry((fk.0.clone(), row["constraint_name"].clone()))
            .or_default()
            .push(fk);
    }
    let mut keys = DeclaredKeys::default();
    for (_, pairs) in constraints {
        if pairs.len() > 1 {
            keys.composite.push(pairs);
        } else if pairs[0].0 == pairs[0].2 {
            keys.self_references.insert(pairs[0].clone());
        } else {
            keys.single.insert(pairs[0].clone());
        }
    }
    keys
}

/// Join discovery on one database under one typing.
struct JoinRecall {
    set: String,
    typing: String,
    declared: usize,
    /// Proposed as an accepted or suggested edge, child to parent.
    found: usize,
    accepted: usize,
    self_references: usize,
    composite: usize,
    /// Composite keys with at least one column pair found.
    composite_touched: usize,
    missed: Vec<String>,
}

fn join_recall(set: &str, typing: &str, model: &RoleModel, keys: &DeclaredKeys) -> JoinRecall {
    let mut proposed: BTreeMap<ForeignKey, EdgeStatus> = BTreeMap::new();
    for e in &model.relate.edges {
        if e.status == EdgeStatus::Rejected {
            continue;
        }
        let fk = (
            e.child.table.to_lowercase(),
            e.child.column.to_lowercase(),
            e.parent.table.to_lowercase(),
            e.parent.column.to_lowercase(),
        );
        let entry = proposed.entry(fk).or_insert(e.status);
        if e.status == EdgeStatus::Accepted {
            *entry = EdgeStatus::Accepted;
        }
    }
    let mut found = 0;
    let mut accepted = 0;
    let mut missed = Vec::new();
    for fk in &keys.single {
        match proposed.get(fk) {
            Some(status) => {
                found += 1;
                if *status == EdgeStatus::Accepted {
                    accepted += 1;
                }
            }
            None => missed.push(format!("`{}.{}` -> `{}.{}`", fk.0, fk.1, fk.2, fk.3)),
        }
    }
    let composite_touched = keys
        .composite
        .iter()
        .filter(|pairs| pairs.iter().any(|p| proposed.contains_key(p)))
        .count();
    JoinRecall {
        set: set.to_string(),
        typing: typing.to_string(),
        declared: keys.single.len(),
        found,
        accepted,
        self_references: keys.self_references.len(),
        composite: keys.composite.len(),
        composite_touched,
        missed,
    }
}

fn typing_label(t: &TypingSource) -> &'static str {
    match t {
        TypingSource::Profiled { .. } => "profiled",
        TypingSource::Floor { .. } => "floor",
        TypingSource::Packages => "packages",
    }
}

fn pct(n: usize, d: usize) -> String {
    if d == 0 {
        "-".to_string()
    } else {
        format!("{:.1}%", n as f64 * 100.0 / d as f64)
    }
}

/// One GitTables table's call under one typing.
struct LoneCall {
    typing: String,
    role: Role,
}

/// A table or database that could not be scored, and why.
struct Skipped {
    what: String,
    typing: String,
    why: String,
}

/// The most columns a lone table may have to be scored. Measuring a table's
/// columns is one statement holding a `count(DISTINCT …)` per column, which
/// DuckDB plans as a join per column; at well over a thousand columns that
/// planning ran for more than ten minutes without finishing, so a wider table
/// is skipped and listed rather than left to stall the baseline.
const MAX_LONE_COLUMNS: usize = 1000;

/// Why `file` is too wide to score, or `None` when it is not (or its width
/// cannot be read, which loading it will then report).
fn too_wide(file: &Path) -> Option<String> {
    let conn = duckdb::Connection::open_in_memory().ok()?;
    let sql = format!(
        "SELECT count(*) FROM (DESCRIBE FROM read_parquet('{}'))",
        file.to_str()?.replace('\'', "''")
    );
    let n: i64 = conn.query_row(&sql, [], |r| r.get(0)).ok()?;
    (n as usize > MAX_LONE_COLUMNS).then(|| {
        format!("{n} columns, more than the {MAX_LONE_COLUMNS} a lone table is scored with")
    })
}

/// The first line of an error, which is all the record needs.
fn first_line(e: impl std::fmt::Display) -> String {
    e.to_string().lines().next().unwrap_or_default().to_string()
}

/// Load and score a set, turning an error or a panic into the reason it is
/// skipped, so one bad table does not sink the baseline. The corpus's own path
/// is written `$DOVETAIL_CORPUS` in the reason.
fn score<E: std::fmt::Display>(
    corpus: &Path,
    load: impl FnOnce() -> Result<Session, E>,
) -> Result<RoleModel, String> {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let session = load().map_err(first_line)?;
        ScorePerRole.score(&session).map_err(first_line)
    }));
    let why = match outcome {
        Ok(Ok(model)) => return Ok(model),
        Ok(Err(why)) => why,
        Err(panic) => {
            let msg = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            format!("panicked: {}", first_line(msg))
        }
    };
    Err(why.replace(&corpus.display().to_string(), "$DOVETAIL_CORPUS"))
}

/// Score tiers 2 and 3 under both typings and record them in `RESULTS.md`.
///
/// It runs on a thread with a large stack: a test thread has 2 MiB, and
/// DuckDB's planner, unoptimised in a debug build, overflows that on the
/// wide tables of some sets (PremierLeague's `Actions`, for one), which aborts
/// the process rather than failing one set.
#[test]
fn corpus_baseline() {
    std::thread::Builder::new()
        .name("corpus_baseline".into())
        .stack_size(256 << 20)
        .spawn(run_corpus_baseline)
        .expect("spawn the baseline's thread")
        .join()
        .expect("the baseline completes");
}

fn run_corpus_baseline() {
    let Some(corpus) = std::env::var_os("DOVETAIL_CORPUS").map(PathBuf::from) else {
        eprintln!("skipping: DOVETAIL_CORPUS is not set; fetch the corpus with scripts/fetch-role-corpus.sh");
        return;
    };
    let probe = Session::from_sql("", TypingRequest::Best).expect("an empty session");
    if let TypingSource::Floor { reason } = &probe.typing {
        eprintln!("skipping: FineType's profile is unavailable: {reason}");
        return;
    }
    let profiled_typing = probe.typing.clone();
    drop(probe);

    let keys = answer_keys();
    let started = Instant::now();
    let mut results: Vec<SetResult> = Vec::new();
    let mut joins: Vec<JoinRecall> = Vec::new();
    let mut lone: Vec<LoneCall> = Vec::new();
    let mut skipped: Vec<Skipped> = Vec::new();

    let manifest = read_csv(&corpus.join("gittables/sample/manifest.csv"));
    for request in [TypingRequest::Floor, TypingRequest::Best] {
        let typing = match request {
            TypingRequest::Floor => "floor",
            TypingRequest::Best => "profiled",
        };

        for (db, key) in &keys {
            let t = Instant::now();
            let dir = corpus.join("ctu").join(db);
            match score(&corpus, || load_corpus_set(&dir, request)) {
                Ok(model) => {
                    assert_eq!(
                        typing_label(&model.typing),
                        typing,
                        "{db}: {:?}",
                        model.typing
                    );
                    let r = score_set(&model, key, 2);
                    let declared = declared_keys(&dir.join("declared-keys.csv"));
                    joins.push(join_recall(db, typing, &model, &declared));
                    eprintln!(
                        "tier 2 {typing} {db}: {}/{} in {:.1}s",
                        r.correct,
                        r.tables,
                        t.elapsed().as_secs_f64()
                    );
                    results.push(r);
                }
                Err(why) => {
                    eprintln!("tier 2 {typing} {db}: skipped: {why}");
                    skipped.push(Skipped {
                        what: format!("ctu/{db}"),
                        typing: typing.to_string(),
                        why,
                    });
                }
            }
        }

        let t = Instant::now();
        for (i, row) in manifest.iter().enumerate() {
            let file = format!("{}/{}", row["topic"], row["file"]);
            let path = corpus.join("gittables/sample").join(&file);
            let outcome = too_wide(&path)
                .map_or(Ok(()), Err)
                .and_then(|()| score(&corpus, || load_corpus_table(&path, request)))
                .and_then(|model| match model.tables.as_slice() {
                    [call] => Ok(call.role),
                    calls => Err(format!("expected one call, got {}", calls.len())),
                });
            match outcome {
                Ok(role) => lone.push(LoneCall {
                    typing: typing.to_string(),
                    role,
                }),
                Err(why) => skipped.push(Skipped {
                    what: format!("gittables/{file}"),
                    typing: typing.to_string(),
                    why,
                }),
            }
            if (i + 1) % 200 == 0 {
                eprintln!(
                    "tier 3 {typing}: {} of {} in {:.0}s",
                    i + 1,
                    manifest.len(),
                    t.elapsed().as_secs_f64()
                );
            }
        }
    }

    let section = render_corpus(&results, &joins, &lone, &skipped, manifest.len(), &{
        stamp(&profiled_typing, &dovetail_commit())
    });
    println!("{section}");
    eprintln!(
        "corpus baseline took {:.0}s",
        started.elapsed().as_secs_f64()
    );

    let existing = std::fs::read_to_string(results_path()).expect("read RESULTS.md");
    let (tier1, _) = split_record(&existing);
    let mut whole = tier1.trim_end().to_string();
    whole.push_str("\n\n");
    whole.push_str(&section);
    std::fs::write(results_path(), whole).expect("write RESULTS.md");
}

/// The corpus part of the record: tier 2's role results and join recall, then
/// tier 3's spread of calls, then what was skipped. Sorted throughout.
fn render_corpus(
    results: &[SetResult],
    joins: &[JoinRecall],
    lone: &[LoneCall],
    skipped: &[Skipped],
    sampled: usize,
    stamp: &str,
) -> String {
    let mut s = String::new();
    s.push_str(CORPUS_HEADING);
    s.push_str("\n\n");
    s.push_str("Tier 2 is each CTU database scored whole against its answer key; tier 3 is each sampled GitTables table scored alone. Written by the `role_corpus_eval` test's `corpus_baseline` from the corpus `scripts/fetch-role-corpus.sh` fetches; everything but the stamp is the same on every run of the same code over the same corpus.\n\n");
    s.push_str(&format!("Stamp: {stamp}\n\n"));

    s.push_str("## Tier 2 totals\n\n");
    s.push_str("| typing | databases | role accuracy | join recall |\n|---|---|---|---|\n");
    for typing in ["floor", "profiled"] {
        let rs: Vec<&SetResult> = results.iter().filter(|r| r.typing == typing).collect();
        let (c, t) = rs
            .iter()
            .fold((0, 0), |(c, t), r| (c + r.correct, t + r.tables));
        let js: Vec<&JoinRecall> = joins.iter().filter(|j| j.typing == typing).collect();
        let (f, d) = js
            .iter()
            .fold((0, 0), |(f, d), j| (f + j.found, d + j.declared));
        s.push_str(&format!(
            "| {typing} | {} | {c}/{t} ({}) | {f}/{d} ({}) |\n",
            rs.len(),
            pct(c, t),
            pct(f, d)
        ));
    }
    s.push('\n');
    s.push_str(&render_results(results));

    s.push_str("\n## Tier 2, join recall\n\n");
    s.push_str("A declared single-column foreign key between two tables is found when `relate` proposes the same child and parent columns as an accepted or suggested edge, in that direction. Self-references are not scored, since `relate` skips same-table pairs by design. A composite key is not scored either, since `relate` proposes single-column joins; the last column counts the composite keys with at least one of their column pairs proposed.\n\n");
    s.push_str("| set | typing | declared | found | accepted | recall | self-references | composite (a pair found) |\n|---|---|---|---|---|---|---|---|\n");
    let mut sorted: Vec<&JoinRecall> = joins.iter().collect();
    sorted.sort_by(|a, b| (&a.typing, &a.set).cmp(&(&b.typing, &b.set)));
    for j in &sorted {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} ({}) |\n",
            j.set,
            j.typing,
            j.declared,
            j.found,
            j.accepted,
            pct(j.found, j.declared),
            j.self_references,
            j.composite,
            j.composite_touched
        ));
    }
    for typing in ["floor", "profiled"] {
        let missed: Vec<String> = sorted
            .iter()
            .filter(|j| j.typing == typing)
            .flat_map(|j| j.missed.iter().map(move |m| format!("- {}: {m}", j.set)))
            .collect();
        s.push_str(&format!("\nMissed foreign keys, {typing}:"));
        if missed.is_empty() {
            s.push_str(" none.\n");
        } else {
            s.push_str("\n\n");
            for m in missed {
                s.push_str(&m);
                s.push('\n');
            }
        }
    }

    const ROLES: [Role; 6] = [
        Role::Entity,
        Role::Event,
        Role::Reference,
        Role::Bridge,
        Role::Snapshot,
        Role::Unclear,
    ];
    for typing in ["floor", "profiled"] {
        let calls: Vec<Role> = lone
            .iter()
            .filter(|c| c.typing == typing)
            .map(|c| c.role)
            .collect();
        let n = calls.len();
        let count = |r: Role| calls.iter().filter(|c| **c == r).count();
        s.push_str(&format!("\n## Tier 3, {typing}\n\n"));
        s.push_str(&format!(
            "{n} of {sampled} sampled tables scored, each alone. Unclear rate {}; reference rate {}.\n\n",
            pct(count(Role::Unclear), n),
            pct(count(Role::Reference), n)
        ));
        s.push_str("| role | tables | share |\n|---|---|---|\n");
        for r in ROLES {
            s.push_str(&format!(
                "| {} | {} | {} |\n",
                role_name(r),
                count(r),
                pct(count(r), n)
            ));
        }
    }

    let mut skipped: Vec<&Skipped> = skipped.iter().collect();
    skipped.sort_by(|a, b| (&a.what, &a.typing).cmp(&(&b.what, &b.typing)));
    s.push_str("\n## Skipped\n\nTables and databases that could not be loaded or scored, and why.");
    if skipped.is_empty() {
        s.push_str(" None.\n");
    } else {
        s.push_str("\n\n");
        for k in skipped {
            s.push_str(&format!("- {} ({}): {}\n", k.what, k.typing, k.why));
        }
    }
    s
}
