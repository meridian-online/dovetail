//! Helpers shared by the test files that model the retail role fixture the way
//! Brightfield holds it: each table loaded as `raw_<t>` with the three lineage
//! columns, beside data packages that name and type the tables.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use dovetail_core::ossie::LINEAGE;
use dovetail_core::packages::RAW_PREFIX;

/// The retail role fixture's directory.
pub fn retail_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/role-fixtures/retail")
}

/// The retail fixture's `build.sql`.
pub fn retail_build_sql() -> String {
    std::fs::read_to_string(retail_dir().join("build.sql")).unwrap()
}

/// The data packages that name and type the retail tables.
pub fn retail_packages_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/ossie-fixtures/retail-packages")
}

/// Run `build_sql` into a new DuckDB file in `dir`, then rename every table
/// `t` it made to `raw_<t>` and add the lineage columns after its own: the
/// file name, no part and a dense row number from 1.
pub fn build_raw_db_from(dir: &Path, build_sql: &str) -> PathBuf {
    let path = dir.join("retail.duckdb");
    let conn = duckdb::Connection::open(&path).unwrap();
    conn.execute_batch(build_sql).unwrap();
    let tables: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT table_name FROM information_schema.tables
                 WHERE table_schema = 'main' AND table_type = 'BASE TABLE'
                 ORDER BY table_name",
            )
            .unwrap();
        let rows = stmt.query_map([], |r| r.get(0)).unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    };
    let [file, part, row] = LINEAGE;
    for t in tables {
        let q = t.replace('"', "\"\"");
        conn.execute_batch(&format!(
            "CREATE TABLE \"{RAW_PREFIX}{q}\" AS
                 SELECT *, '{t}.csv' AS {file}, CAST(NULL AS VARCHAR) AS {part},
                        CAST(row_number() OVER () AS BIGINT) AS {row}
                 FROM \"{q}\";
             DROP TABLE \"{q}\";"
        ))
        .unwrap();
    }
    path
}

/// The retail fixture as `raw_<t>` tables with lineage columns, in a new file
/// in `dir`.
pub fn build_raw_retail_db(dir: &Path) -> PathBuf {
    build_raw_db_from(dir, &retail_build_sql())
}

/// Every base table of the DuckDB file at `db`, sorted.
pub fn table_names(db: &Path) -> Vec<String> {
    dovetail_core::packages::all_tables(db)
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect()
}
