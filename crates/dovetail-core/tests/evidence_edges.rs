//! Edge evidence: candidate joins measured with set-based SQL inside DuckDB,
//! and `relate`'s edges scored from those rows.
//!
//! The golden descriptors pin `relate`'s output byte for byte under the floor
//! typing. Set `DOVETAIL_BLESS=1` to rewrite them, and review the diff.

use std::path::{Path, PathBuf};

use dovetail_core::evidence::{column_evidence, edge_evidence, EdgeEvidence, Session};
use dovetail_core::finetype_ext::TypingRequest;
use dovetail_core::relate::{discover, edge_from_evidence, run_session, ColumnRef, EdgeStatus};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Build `fixture` into a DuckDB file named `name` in `dir`.
fn build_db(dir: &Path, fixture: &str, name: &str) -> PathBuf {
    let sql = fixture_sql(fixture);
    let db = dir.join(name);
    let conn = duckdb::Connection::open(&db).expect("create db file");
    conn.execute_batch(&sql).expect("build fixture");
    db
}

fn fixture_sql(fixture: &str) -> String {
    std::fs::read_to_string(repo_root().join("tests/relate-fixtures").join(fixture))
        .expect("read fixture")
}

/// What `run_path` writes, with the floor typing rather than the best available.
fn descriptor_for(db: &Path) -> serde_json::Value {
    let s = Session::attach(db, TypingRequest::Floor).expect("attach");
    run_session(&s).expect("relate run").descriptor
}

fn col(table: &str, column: &str) -> ColumnRef {
    ColumnRef {
        table: table.to_string(),
        column: column.to_string(),
    }
}

fn find<'a>(edges: &'a [EdgeEvidence], child: &str, parent: &str) -> &'a EdgeEvidence {
    edges
        .iter()
        .find(|e| e.child.qualified() == child && e.parent.qualified() == parent)
        .unwrap_or_else(|| panic!("no evidence for {child} -> {parent} in {edges:#?}"))
}

fn measured(build_sql: &str) -> (Session, Vec<EdgeEvidence>) {
    let s = Session::from_sql(build_sql, TypingRequest::Floor).expect("session");
    let cols = column_evidence(&s).expect("column evidence");
    let mut tables: Vec<String> = cols.iter().map(|c| c.table.clone()).collect();
    tables.dedup();
    let edges = edge_evidence(&s, &cols, &tables).expect("edge evidence");
    (s, edges)
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

#[test]
fn edge_evidence_matches_manifest_counts() {
    let (s, edges) = measured(&fixture_sql("build.sql"));

    // (a) the holding foreign key: every distinct child value is in the parent.
    let a = find(&edges, "orders.customer_id", "customers.id");
    assert_eq!(a.orphan_count, 0);
    assert_eq!(a.overlap_distinct, a.child_distinct);
    assert_eq!((a.child_total, a.child_distinct), (5, 4));
    assert_eq!((a.parent_total, a.parent_distinct), (5, 5));

    // (b) the near miss: one shipment points at an order that is not there.
    let b = find(&edges, "shipments.order_id", "orders.id");
    assert_eq!(b.orphan_count, 1);
    assert_eq!(b.overlap_distinct, 2);

    // Every row is kept in the session, under the descriptor's pass.
    let (rows, parts): (i64, i64) = s
        .conn()
        .query_row(
            "SELECT count(*), count(*) FILTER (WHERE pass = 'parts')
             FROM memory.main.edge_evidence",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((rows, parts), (edges.len() as i64, edges.len() as i64));
}

#[test]
fn edge_from_evidence_is_pure() {
    // Case (a) by hand: no database behind it.
    let e = EdgeEvidence {
        child: col("orders", "customer_id"),
        parent: col("customers", "id"),
        child_total: 5,
        child_distinct: 4,
        parent_total: 5,
        parent_distinct: 5,
        overlap_distinct: 4,
        orphan_count: 0,
        name_similarity: 0.9,
        semantic_similarity: 0.0,
        child_label: None,
        parent_label: None,
        parent_key_shaped: false,
    };
    let edge = edge_from_evidence(&e);
    assert_eq!(edge.status, EdgeStatus::Accepted);

    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch(&fixture_sql("build.sql")).unwrap();
    let today = discover(&conn)
        .unwrap()
        .into_iter()
        .find(|t| t.child == e.child && t.parent == e.parent)
        .expect("discover finds case (a)");
    assert_eq!(
        format!("{:.2}", edge.confidence),
        format!("{:.2}", today.confidence)
    );
    assert_eq!(format!("{:.2}", edge.confidence), "0.95");
    assert_eq!(edge.reason, today.reason);
}

#[test]
fn all_null_child_yields_no_edge() {
    let (_s, edges) = measured(
        "CREATE TABLE owners (id INTEGER);
         INSERT INTO owners VALUES (1), (2), (3);
         CREATE TABLE pets (id INTEGER, owner_id INTEGER);
         INSERT INTO pets VALUES (10, NULL), (11, NULL);",
    );
    assert!(
        edges.iter().all(|e| e.child != col("pets", "owner_id")),
        "an all-NULL child is no candidate: {edges:#?}"
    );
}

#[test]
fn discover_twice_leaves_tables_unchanged() {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch(&fixture_sql("build.sql")).unwrap();
    let tables = |conn: &duckdb::Connection| -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT database_name || '.' || schema_name || '.' || table_name
                 FROM duckdb_tables() ORDER BY ALL",
            )
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<duckdb::Result<_>>()
            .unwrap()
    };
    let before = tables(&conn);
    let first = discover(&conn).unwrap();
    let second = discover(&conn).unwrap();
    assert_eq!(tables(&conn), before);
    assert_eq!(first.len(), second.len());
}

#[test]
fn a_table_named_child_is_still_a_parent() {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE child (id INTEGER, name VARCHAR);
         INSERT INTO child VALUES (1, 'Ada'), (2, 'Alan'), (3, 'Grace');
         CREATE TABLE toys (id INTEGER, child_id INTEGER);
         INSERT INTO toys VALUES (10, 1), (11, 2), (12, 3), (13, 1);",
    )
    .unwrap();
    let edges = discover(&conn).expect("discover");
    let edge = edges
        .iter()
        .find(|e| e.child == col("toys", "child_id") && e.parent == col("child", "id"))
        .expect("toys.child_id -> child.id is discovered");
    assert_eq!(edge.status, EdgeStatus::Accepted);
    assert_eq!(edge.verification.orphan_count, 0);
}

/// A pair that passes every basic condition for the exact-name key credit:
/// the same name, a unique parent of 25, full overlap.
fn credit_candidate(parent_label: Option<&str>, shaped: bool) -> EdgeEvidence {
    EdgeEvidence {
        child: col("orders", "sku"),
        parent: col("items", "sku"),
        child_total: 30,
        child_distinct: 20,
        parent_total: 25,
        parent_distinct: 25,
        overlap_distinct: 20,
        orphan_count: 0,
        name_similarity: 0.7,
        semantic_similarity: 0.0,
        child_label: None,
        parent_label: parent_label.map(str::to_string),
        parent_key_shaped: shaped,
    }
}

#[test]
fn typed_parent_credit_follows_the_excluded_families() {
    for label in [
        "identity.person.full_name",
        "representation.numeric.decimal_number",
        "finance.currency.amount",
        "finance.currency.amount_lakh",
        "finance.rate.yield",
        "representation.file.file_size",
        "datetime.date.iso",
    ] {
        // Key-shaped values do not rescue an excluded label.
        let edge = edge_from_evidence(&credit_candidate(Some(label), true));
        assert_eq!(edge.status, EdgeStatus::Rejected, "{label}");
    }
    for label in [
        "representation.numeric.integer_number",
        "finance.currency.currency_code",
    ] {
        let edge = edge_from_evidence(&credit_candidate(Some(label), false));
        assert_eq!(edge.status, EdgeStatus::Accepted, "{label}");
    }
}

#[test]
fn key_shape_collapses_runs_so_a_varying_length_still_passes() {
    // BEV-1 .. BEV-25 have two lengths (9 and 16 values), so length alone
    // does not reach 90%; every value has the pattern AAA-9.
    let s = Session::from_sql(
        "CREATE TABLE items AS SELECT 'BEV-' || CAST(i AS VARCHAR) AS sku FROM range(1, 26) t(i);
         CREATE TABLE lines AS SELECT 'BEV-' || CAST(i AS VARCHAR) AS sku FROM range(1, 11) t(i);",
        TypingRequest::Floor,
    )
    .unwrap();
    let cols = column_evidence(&s).unwrap();
    let tables = vec!["items".to_string(), "lines".to_string()];
    let edges = edge_evidence(&s, &cols, &tables).unwrap();
    let e = find(&edges, "lines.sku", "items.sku");
    assert!(e.parent_key_shaped);
    assert_eq!(edge_from_evidence(e).status, EdgeStatus::Accepted);
}

/// The edge evidence for an untyped exact-name pair: `parents.v` holds 25
/// unique values of `values` (an expression over `i`) and `kids.v` the first
/// ten.
fn untyped_exact_name(values: &str) -> EdgeEvidence {
    let s = Session::from_sql(
        &format!(
            "CREATE TABLE parents AS SELECT {values} AS v FROM range(1, 26) t(i);
             CREATE TABLE kids AS SELECT {values} AS v FROM range(1, 11) t(i);"
        ),
        TypingRequest::Floor,
    )
    .unwrap();
    let cols = column_evidence(&s).unwrap();
    let tables = vec!["parents".to_string(), "kids".to_string()];
    let edges = edge_evidence(&s, &cols, &tables).unwrap();
    find(&edges, "kids.v", "parents.v").clone()
}

#[test]
fn unique_one_word_cities_are_not_key_shaped() {
    let cities = "(['Paris','London','Rome','Berlin','Madrid','Vienna','Oslo','Lisbon','Dublin',\
        'Prague','Athens','Warsaw','Helsinki','Brussels','Zurich','Geneva','Munich','Hamburg',\
        'Milan','Naples','Seville','Porto','Lyon','Nice','Cork'])[i]";
    let e = untyped_exact_name(cities);
    assert!(!e.parent_key_shaped);
    assert_ne!(edge_from_evidence(&e).status, EdgeStatus::Accepted);
}

#[test]
fn unique_decimal_amounts_are_not_key_shaped() {
    let e = untyped_exact_name("CAST(i * 7.25 + (i % 3) * 100 AS DECIMAL(9, 2))::VARCHAR");
    assert!(!e.parent_key_shaped);
}

#[test]
fn fixed_width_prices_are_not_key_shaped_as_decimal_or_text() {
    let price = "CAST(10 + i * 3.17 AS DECIMAL(9, 2))";
    for values in [price.to_string(), format!("CAST({price} AS VARCHAR)")] {
        let e = untyped_exact_name(&values);
        assert!(!e.parent_key_shaped, "{values}");
        assert_ne!(
            edge_from_evidence(&e).status,
            EdgeStatus::Accepted,
            "{values}"
        );
    }
}

#[test]
fn formatted_text_prices_are_not_key_shaped() {
    let price = "CAST(CAST(10 + i * 3.17 AS DECIMAL(9, 2)) AS VARCHAR)";
    for values in [
        format!("'$' || {price}"),
        format!("'£' || {price}"),
        "'1,' || CAST(CAST(100 + i * 3.17 AS DECIMAL(9, 2)) AS VARCHAR)".to_string(),
        format!("'(' || {price} || ')'"),
    ] {
        let e = untyped_exact_name(&values);
        assert!(!e.parent_key_shaped, "{values}");
        assert_ne!(
            edge_from_evidence(&e).status,
            EdgeStatus::Accepted,
            "{values}"
        );
    }
}

#[test]
fn unique_untyped_bigint_numbers_are_key_shaped() {
    let e = untyped_exact_name("CAST(1000 + i AS BIGINT)");
    assert!(e.parent_key_shaped);
}
