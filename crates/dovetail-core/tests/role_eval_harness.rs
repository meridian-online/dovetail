//! Score role calls against answer keys, and hold tier 1 (the retail fixture)
//! to its targets: exact under the profiled typing, and recorded floor-only.
//!
//! `tests/role-corpus/RESULTS.md` is the committed record. The profiled run
//! checks it is current; with `DOVETAIL_WRITE_ROLE_RESULTS=1` it rewrites it.
//! With `DOVETAIL_REQUIRE_PROFILED=1` a test that would skip for want of
//! FineType's profile fails instead, so a CI step that sets it cannot pass
//! without running.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use dovetail_core::combine::Combination;
use dovetail_core::evidence::Session;
use dovetail_core::finetype_ext::{TypingRequest, TypingSource};
use dovetail_core::relate::RelateRun;
use dovetail_core::role_eval::{
    render_report, score_set, split_record, stamp, RoleScorer, ScorePerRole,
};
use dovetail_core::role_key::{
    load_answer_key, AnswerKey, CombinationKind, ExpectedCombination, ExpectedFold, ExpectedTable,
    Role,
};
use dovetail_core::roles::{Fold, RoleCall, RoleModel};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn results_path() -> PathBuf {
    repo_root().join("tests/role-corpus/RESULTS.md")
}

fn call(table: &str, role: Role) -> RoleCall {
    RoleCall {
        table: table.into(),
        role,
        scores: vec![],
        evidence: vec![],
        primary_key: vec![],
        time_field: None,
        audit_time_fields: vec![],
        typing: "floor: test".into(),
    }
}

fn hand_model() -> RoleModel {
    RoleModel {
        typing: TypingSource::Floor {
            reason: "test".into(),
        },
        relate: RelateRun {
            edges: vec![],
            descriptor: serde_json::Value::Null,
            measured: vec![],
        },
        tables: vec![
            call("Categories", Role::Reference),
            call("customers", Role::Entity),
            call("orders", Role::Event),
            call("suppliers", Role::Unclear),
        ],
        folds: vec![],
        combinations: vec![Combination {
            name: "suppliers".into(),
            parts: vec!["suppliers_north".into(), "suppliers_south".into()],
            kind: CombinationKind::Partition,
            token_column: None,
            source_sql: String::new(),
            key: vec![],
            evidence: vec![],
        }],
        carried_keys: vec![],
        edges: vec![],
    }
}

fn hand_key() -> AnswerKey {
    let t = |role| ExpectedTable {
        role,
        why: "x".into(),
    };
    AnswerKey {
        name: "hand".into(),
        tables: BTreeMap::from([
            ("categories".into(), t(Role::Reference)),
            ("customers".into(), t(Role::Entity)),
            ("orders".into(), t(Role::Event)),
            ("suppliers".into(), t(Role::Entity)),
        ]),
        folds: vec![ExpectedFold {
            reference: "categories".into(),
            into: "orders".into(),
        }],
        combinations: vec![ExpectedCombination {
            name: "suppliers".into(),
            parts: vec!["suppliers_north".into(), "suppliers_south".into()],
            kind: CombinationKind::Partition,
        }],
    }
}

#[test]
fn score_set_counts() {
    let r = score_set(&hand_model(), &hand_key(), 2);
    assert_eq!(
        (r.set.as_str(), r.tier, r.typing.as_str()),
        ("hand", 2, "floor")
    );
    assert_eq!((r.tables, r.correct), (4, 3));
    assert_eq!(r.confusion.get(&(Role::Entity, Role::Unclear)), Some(&1));
    assert_eq!(r.confusion.get(&(Role::Entity, Role::Entity)), Some(&1));
    assert_eq!(
        (r.folds.true_pos, r.folds.false_pos, r.folds.false_neg),
        (0, 0, 1)
    );
    assert_eq!(
        (
            r.combinations.true_pos,
            r.combinations.false_pos,
            r.combinations.false_neg
        ),
        (1, 0, 0)
    );
    assert_eq!(r.misses.len(), 2, "{:?}", r.misses);
    assert!(r.misses.iter().any(|m| m.contains("suppliers")));
    assert!(r.misses.iter().any(|m| m.contains("categories")));
}

#[test]
fn a_spurious_fold_and_a_wrong_kind_count_against_precision() {
    let mut model = hand_model();
    model.folds.push(Fold {
        reference: "categories".into(),
        host: "customers".into(),
        child_column: "category_id".into(),
        parent_column: "category_id".into(),
        struct_column: "category".into(),
    });
    model.combinations[0].kind = CombinationKind::Snapshot;
    let r = score_set(&model, &hand_key(), 2);
    assert_eq!(
        (r.folds.true_pos, r.folds.false_pos, r.folds.false_neg),
        (0, 1, 1)
    );
    assert_eq!(
        (
            r.combinations.true_pos,
            r.combinations.false_pos,
            r.combinations.false_neg
        ),
        (0, 1, 1)
    );
}

#[test]
fn report_is_deterministic() {
    let a = score_set(&hand_model(), &hand_key(), 2);
    let mut b = a.clone();
    b.typing = "profiled".into();
    let one = render_report(&[a.clone(), b.clone()], "stamp");
    let two = render_report(&[b, a], "stamp");
    assert_eq!(one, two);
    assert!(one.contains("stamp"));
    assert!(
        one.contains("| hand | 2 | floor | 4 | 3/4 (75.0%) |"),
        "{one}"
    );
}

/// The retail fixture scored under `request`, with its answer key.
fn retail(request: TypingRequest) -> (RoleModel, AnswerKey) {
    let dir = repo_root().join("tests/role-fixtures/retail");
    let sql = std::fs::read_to_string(dir.join("build.sql")).expect("build.sql");
    let session = Session::from_sql(&sql, request).expect("build session");
    let model = ScorePerRole.score(&session).expect("score");
    let key = load_answer_key(&dir.join("answer-key.json")).expect("answer key");
    (model, key)
}

/// Whether the model was typed with FineType's profile. When it was not, say
/// why and skip, unless `DOVETAIL_REQUIRE_PROFILED` makes that a failure.
fn profiled_or_skip(model: &RoleModel) -> bool {
    if matches!(model.typing, TypingSource::Profiled { .. }) {
        return true;
    }
    if std::env::var_os("DOVETAIL_REQUIRE_PROFILED").is_some() {
        panic!(
            "DOVETAIL_REQUIRE_PROFILED is set but FineType's profile is unavailable: {:?}",
            model.typing
        );
    }
    eprintln!(
        "skipping: FineType's profile is unavailable: {:?}",
        model.typing
    );
    false
}

#[test]
fn tier1_retail_is_exact() {
    let (model, key) = retail(TypingRequest::Best);
    if !profiled_or_skip(&model) {
        return;
    }
    let r = score_set(&model, &key, 1);
    assert_eq!(r.typing, "profiled");
    assert_eq!(r.correct, r.tables, "{:?}", r.misses);
    assert_eq!(
        (r.folds.false_pos, r.folds.false_neg),
        (0, 0),
        "{:?}",
        r.misses
    );
    assert_eq!(
        (r.combinations.false_pos, r.combinations.false_neg),
        (0, 0),
        "{:?}",
        r.misses
    );
    assert!(r.misses.is_empty(), "{:?}", r.misses);
}

/// The lines of `report` from `heading` up to the next heading of its level
/// or above.
fn section<'a>(report: &'a str, heading: &str) -> &'a str {
    let start = report
        .find(heading)
        .unwrap_or_else(|| panic!("no {heading}"));
    let rest = &report[start + heading.len()..];
    let end = rest.find("\n#").unwrap_or(rest.len());
    &report[start..start + heading.len() + end]
}

#[test]
fn tier1_retail_floor_recorded() {
    let (model, key) = retail(TypingRequest::Floor);
    let r = score_set(&model, &key, 1);
    assert_eq!(r.typing, "floor");
    assert_eq!(r.tables, key.tables.len());
    let report = render_report(std::slice::from_ref(&r), "floor only");
    let row = report
        .lines()
        .find(|l| l.starts_with("| retail | 1 | floor |"))
        .unwrap_or_else(|| panic!("no floor row\n{report}"));

    // The floor's run is the committed record's, row and section alike.
    let record = std::fs::read_to_string(results_path()).expect("read RESULTS.md");
    assert!(
        record.lines().any(|l| l == row),
        "RESULTS.md does not record the floor's run:\n{row}"
    );
    let floor = section(&report, "## Tier 1, floor");
    assert!(
        record.contains(floor),
        "RESULTS.md does not record the floor's run:\n{floor}"
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

/// Both typings' tier-1 runs, rendered, match the committed record's tier-1
/// part apart from its stamp. `DOVETAIL_WRITE_ROLE_RESULTS=1` writes that part
/// instead. The corpus part below it is `role_corpus_eval`'s, and is kept as
/// it is either way.
#[test]
fn tier1_record_is_current() {
    let (profiled, key) = retail(TypingRequest::Best);
    if !profiled_or_skip(&profiled) {
        return;
    }
    let (floor, _) = retail(TypingRequest::Floor);
    let results = [score_set(&floor, &key, 1), score_set(&profiled, &key, 1)];
    let stamp = stamp(&profiled.typing, &dovetail_commit());
    let report = render_report(&results, &stamp);
    let existing = std::fs::read_to_string(results_path()).unwrap_or_default();
    let (record, corpus) = split_record(&existing);
    if std::env::var_os("DOVETAIL_WRITE_ROLE_RESULTS").is_some() {
        std::fs::create_dir_all(results_path().parent().unwrap()).unwrap();
        let mut whole = report.clone();
        if !corpus.is_empty() {
            whole.push('\n');
            whole.push_str(corpus);
        }
        std::fs::write(results_path(), whole).expect("write RESULTS.md");
        return;
    }
    assert!(!existing.is_empty(), "RESULTS.md is missing");
    let unstamped = |s: &str| -> Vec<String> {
        s.lines()
            .filter(|l| !l.starts_with("Stamp: "))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        unstamped(record.trim_end()),
        unstamped(report.trim_end()),
        "RESULTS.md is stale; rerun with DOVETAIL_WRITE_ROLE_RESULTS=1\n{report}"
    );
}
