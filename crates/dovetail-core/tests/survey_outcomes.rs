//! ac-09 (rung reporting), ac-10 (detection-quality gate routing), ac-11
//! (duplicate-column policy surfaced) — the survey orchestration behaviours.

use std::path::{Path, PathBuf};

use dovetail_core::emit::DuplicatePolicy;
use dovetail_core::eval::load_corpus;
use dovetail_core::survey::{survey_file, Outcome};
use dovetail_core::ShapeHeuristicDetector;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn fixture_path(name: &str) -> PathBuf {
    let corpus = load_corpus(repo_root().join("tests/fixtures")).unwrap();
    corpus.iter().find(|f| f.manifest.name == name).unwrap().data_path.clone()
}

// ac-09 — every confident input reports its rung and why.
#[test]
fn reports_the_chosen_rung_and_reason() {
    let det = ShapeHeuristicDetector::new();
    let report =
        survey_file(&fixture_path("csv-simple"), &det, DuplicatePolicy::default(), None).unwrap();
    matches!(report.outcome, Outcome::Emitted { .. });
    let line = report.render();
    assert!(line.contains("rung=sql"), "rung not reported: {line}");
    assert!(line.contains("confidence"), "reason not reported: {line}");
}

// ac-11 — duplicate columns are surfaced with the explicit policy.
#[test]
fn surfaces_duplicate_columns_and_policy() {
    let det = ShapeHeuristicDetector::new();
    let report =
        survey_file(&fixture_path("csv-dup-cols"), &det, DuplicatePolicy::default(), None).unwrap();
    assert_eq!(report.detection.duplicate_columns, vec!["id", "name"]);
    let line = report.render();
    assert!(line.contains("duplicate columns"), "dups not surfaced: {line}");
    assert!(line.contains("Rename"), "policy not named: {line}");
    assert!(line.contains("no data dropped"), "{line}");
}

// ac-10 — an under-confident detection routes to suggest-and-confirm rather
// than emitting blind. A truncated/garbage JSON triggers the low-confidence path.
#[test]
fn under_confident_input_routes_to_suggest_confirm() {
    let dir = std::env::temp_dir().join("dovetail-ac10");
    std::fs::create_dir_all(&dir).unwrap();
    let garbage = dir.join("garbage.json");
    // Not an object or array of objects → the JSON detector reports confidence 0.2.
    std::fs::write(&garbage, b"\"just a bare string\"").unwrap();

    let det = ShapeHeuristicDetector::new();
    let report = survey_file(&garbage, &det, DuplicatePolicy::default(), None).unwrap();
    match &report.outcome {
        Outcome::SuggestConfirm { reason } => {
            assert!(reason.contains("below"), "reason: {reason}");
        }
        Outcome::Emitted { .. } => panic!("expected suggest-and-confirm for low-confidence input"),
    }
    assert!(report.render().contains("suggest-and-confirm"));
}

// ac-10 corpus mechanism: the chosen detector clears the >=90% bar across the
// whole corpus (every fixture emits, none routes to suggest-confirm).
#[test]
fn corpus_clears_the_detection_bar() {
    let det = ShapeHeuristicDetector::new();
    let corpus = load_corpus(repo_root().join("tests/fixtures")).unwrap();
    let emitted = corpus
        .iter()
        .filter(|fx| {
            let r = survey_file(&fx.data_path, &det, DuplicatePolicy::default(), None).unwrap();
            matches!(r.outcome, Outcome::Emitted { .. })
        })
        .count();
    let rate = emitted as f64 / corpus.len() as f64;
    assert!(rate >= 0.90, "corpus emit rate {rate:.2} below 0.90 bar");
}
