//! ac-03 — run the detection eval over the fixture corpus, write the
//! reproducible results table to the spec folder, and assert the chosen
//! detector clears the detection-quality bar (ac-10, fixed at >=90%).

use std::path::{Path, PathBuf};

use dovetail_core::detect::ShapeHeuristicDetector;
use dovetail_core::eval::{eval_detector, load_corpus, render_report, EvalResult};

#[cfg(feature = "finetype-guided")]
use dovetail_core::detect::FinetypeGuidedDetector;

/// The >=90% floor from ac-10, treated as fixed (review-spec finding 1).
const DETECTION_BAR: f64 = 0.90;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repo root")
}

#[test]
fn detection_eval_runs_and_clears_the_bar() {
    let root = repo_root();
    let corpus = load_corpus(root.join("tests/fixtures")).expect("load corpus");
    assert!(corpus.len() >= 7, "expected the full fixture corpus");

    let mut results: Vec<EvalResult> = Vec::new();

    let shape = ShapeHeuristicDetector::new();
    results.push(eval_detector(&shape, &corpus, "structural"));

    // The finetype-guided arm runs in whatever mode the environment affords: a
    // configured model dir loads the classifier, otherwise it degrades to the
    // structural read. Either way it produces a scorable structure result.
    #[cfg(feature = "finetype-guided")]
    {
        let ft = FinetypeGuidedDetector::from_env();
        let mode = if std::env::var_os("DOVETAIL_FINETYPE_MODEL_DIR").is_some() {
            "model-backed"
        } else {
            "degraded (no model dir)"
        };
        results.push(eval_detector(&ft, &corpus, mode));
    }

    // Write the reproducible eval record to the build-output dir (target/, which
    // is gitignored — the report is regenerated on every run). The date is read
    // from a SOURCE_DATE_EPOCH-style override or left as a stable placeholder so
    // the test stays deterministic.
    let date = std::env::var("DOVETAIL_EVAL_DATE").unwrap_or_else(|_| "unstamped".to_string());
    let report = render_report(&results, "tests/fixtures (7 SQL-native fixtures)", &date);
    let out = root.join("target/eval-results.md");
    std::fs::write(&out, &report).expect("write eval report");

    // The shape-heuristic detector is the structural baseline and must clear the
    // bar on its own.
    let shape_result = &results[0];
    assert!(
        shape_result.hit_rate() >= DETECTION_BAR,
        "shape-heuristic hit-rate {:.1}% below the {:.0}% bar\n{}",
        shape_result.hit_rate() * 100.0,
        DETECTION_BAR * 100.0,
        report,
    );
}
