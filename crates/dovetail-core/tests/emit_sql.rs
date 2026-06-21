//! ac-05 — the emitted .sql uses the right native reader and the detector's
//! resolved parameters, and is plain/legible (a single SELECT per load).

use std::path::{Path, PathBuf};

use dovetail_core::emit::{emit_sql, DuplicatePolicy};
use dovetail_core::eval::load_corpus;
use dovetail_core::{Detector, SampledInput, ShapeHeuristicDetector};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn sql_for(fixture: &str) -> String {
    let corpus = load_corpus(repo_root().join("tests/fixtures")).unwrap();
    let fx = corpus.iter().find(|f| f.manifest.name == fixture).expect("fixture");
    let input = SampledInput::from_path(&fx.data_path).unwrap();
    let det = ShapeHeuristicDetector::new().detect(&input);
    emit_sql(&det, fx.data_path.to_str().unwrap(), fixture, DuplicatePolicy::default())
}

#[test]
fn csv_emits_read_csv_with_comma_delim() {
    let sql = sql_for("csv-simple");
    assert!(sql.contains("read_csv("), "{sql}");
    assert!(sql.contains("delim = ','"), "{sql}");
    assert!(sql.contains("header = true"), "{sql}");
}

#[test]
fn tsv_emits_tab_delim() {
    let sql = sql_for("tsv-simple");
    assert!(sql.contains("read_csv("), "{sql}");
    assert!(sql.contains("delim = '\\t'"), "{sql}");
}

#[test]
fn parquet_emits_read_parquet() {
    let sql = sql_for("parquet-simple");
    assert!(sql.contains("read_parquet("), "{sql}");
}

#[test]
fn ndjson_emits_newline_delimited() {
    let sql = sql_for("ndjson-simple");
    assert!(sql.contains("read_json("), "{sql}");
    assert!(sql.contains("format = 'newline_delimited'"), "{sql}");
}

#[test]
fn json_array_emits_array_format() {
    let sql = sql_for("json-array");
    assert!(sql.contains("read_json("), "{sql}");
    assert!(sql.contains("format = 'array'"), "{sql}");
}

#[test]
fn json_object_emits_auto_format() {
    let sql = sql_for("json-object");
    assert!(sql.contains("read_json("), "{sql}");
    assert!(sql.contains("format = 'auto'"), "{sql}");
}

#[test]
fn duplicate_columns_get_a_rename_projection_and_policy_note() {
    let sql = sql_for("csv-dup-cols");
    assert!(sql.contains("policy: Rename"), "policy note missing:\n{sql}");
    // The second occurrences of id/name are aliased explicitly rather than dropped.
    assert!(sql.contains("\"id_1\""), "renamed dup not projected:\n{sql}");
    assert!(sql.contains("\"name_1\""), "renamed dup not projected:\n{sql}");
}

#[test]
fn emission_is_a_single_select_per_load() {
    // Legibility: one CREATE ... AS SELECT, no procedural cleverness.
    let sql = sql_for("csv-simple");
    assert_eq!(sql.matches("SELECT").count(), 1, "expected exactly one SELECT:\n{sql}");
    assert!(sql.contains("CREATE OR REPLACE TABLE"), "{sql}");
}
