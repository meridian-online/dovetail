//! ac-07 — the emitted .sql actually loads. For each fixture, run the emitted
//! SQL through DuckDB and assert the resulting table's row count and column set
//! match the manifest. DuckDB runs ONLY here, in the test suite — never on
//! survey's own path (choice 0001). A mismatch fails loudly: it is the core
//! failure mode the whole spec guards against.

use std::path::{Path, PathBuf};

use dovetail_core::emit::{emit_sql, DuplicatePolicy};
use dovetail_core::eval::{load_corpus, Fixture};
use dovetail_core::{Detector, SampledInput, ShapeHeuristicDetector};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// The column set DuckDB will actually produce: duplicate names get `_1`, `_2`
/// suffixes (verified against DuckDB's reader). For non-dup fixtures this is the
/// manifest columns unchanged.
fn expected_loaded_columns(fx: &Fixture) -> Vec<String> {
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    fx.manifest
        .columns
        .iter()
        .map(|c| {
            let n = seen.entry(c.as_str()).or_insert(0);
            let out = if *n == 0 { c.clone() } else { format!("{c}_{n}") };
            *n += 1;
            out
        })
        .collect()
}

#[test]
fn emitted_sql_round_trips_through_duckdb() {
    let corpus = load_corpus(repo_root().join("tests/fixtures")).unwrap();
    let conn = duckdb::Connection::open_in_memory().expect("open duckdb");

    let mut failures = Vec::new();
    for fx in &corpus {
        let input = SampledInput::from_path(&fx.data_path).unwrap();
        let det = ShapeHeuristicDetector::new().detect(&input);
        let table = &fx.manifest.name;
        let sql = emit_sql(
            &det,
            fx.data_path.to_str().unwrap(),
            table,
            DuplicatePolicy::default(),
        );

        if let Err(e) = conn.execute_batch(&sql) {
            failures.push(format!("{}: SQL failed to execute: {e}\n{sql}", fx.manifest.name));
            continue;
        }

        // Column set (in order) from the loaded table.
        let cols = table_columns(&conn, table);
        let want_cols = expected_loaded_columns(fx);
        if cols != want_cols {
            failures.push(format!(
                "{}: columns {:?} != expected {:?}",
                fx.manifest.name, cols, want_cols
            ));
        }

        // Row count.
        let rows: usize = conn
            .query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| r.get::<_, i64>(0))
            .map(|n| n as usize)
            .unwrap_or(usize::MAX);
        if rows != fx.manifest.row_count {
            failures.push(format!(
                "{}: row count {} != expected {}",
                fx.manifest.name, rows, fx.manifest.row_count
            ));
        }
    }

    assert!(failures.is_empty(), "round-trip failures:\n{}", failures.join("\n"));
}

fn table_columns(conn: &duckdb::Connection, table: &str) -> Vec<String> {
    let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\" LIMIT 0")).unwrap();
    let _ = stmt.query([]).unwrap();
    stmt.column_names().iter().map(|s| s.to_string()).collect()
}
