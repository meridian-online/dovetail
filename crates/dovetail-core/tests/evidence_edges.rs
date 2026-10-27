//! Edge evidence: candidate joins measured with set-based SQL inside DuckDB,
//! and `relate`'s edges scored from those rows.
//!
//! The golden descriptors pin `relate`'s output byte for byte under the floor
//! typing. Set `DOVETAIL_BLESS=1` to rewrite them, and review the diff.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Build `fixture` into a DuckDB file named `name` in `dir`.
fn build_db(dir: &Path, fixture: &str, name: &str) -> PathBuf {
    let sql = std::fs::read_to_string(repo_root().join("tests/relate-fixtures").join(fixture))
        .expect("read fixture");
    let db = dir.join(name);
    let conn = duckdb::Connection::open(&db).expect("create db file");
    conn.execute_batch(&sql).expect("build fixture");
    db
}

fn descriptor_for(db: &Path) -> serde_json::Value {
    dovetail_core::relate::run_path(db.to_str().unwrap())
        .expect("relate run")
        .descriptor
}

#[test]
fn descriptor_is_byte_identical_to_baseline() {
    let dir = tempfile::tempdir().unwrap();
    for (fixture, db_name, golden) in [
        (
            "build.sql",
            "fixture.duckdb",
            "build.descriptor.golden.json",
        ),
        (
            "identifier-keys.build.sql",
            "identifier-keys.duckdb",
            "identifier-keys.descriptor.golden.json",
        ),
    ] {
        let db = build_db(dir.path(), fixture, db_name);
        let got = serde_json::to_string_pretty(&descriptor_for(&db)).unwrap() + "\n";
        let path = repo_root().join("tests/relate-fixtures").join(golden);
        if std::env::var_os("DOVETAIL_BLESS").is_some() {
            std::fs::write(&path, &got).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&path).expect("read golden descriptor");
        assert!(
            got == want,
            "{golden} differs from the descriptor relate writes now:\n{got}"
        );
    }
}
