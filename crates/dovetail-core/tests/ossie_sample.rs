//! A sampled session samples what is read row by row and measures a parent's
//! whole key, and each table's row count, on the full table.

mod common;

use std::path::Path;

use common::*;
use dovetail_core::evidence::{
    column_evidence, edge_evidence, table_evidence, Dataset, EdgeEvidence, Pass, Session,
    SessionTyping,
};
use dovetail_core::packages::SourceTable;
use dovetail_core::relate::{edge_from_evidence, Edge, EdgeStatus};

fn plain(name: &str) -> SourceTable {
    SourceTable {
        name: name.to_string(),
        relation: format!("raw_{name}"),
        source_file: None,
        source_part: None,
        fields: Vec::new(),
    }
}

/// 2,000 customers; 20,000 orders, each pointing at a customer.
const SHOP: &str = "
    CREATE TABLE customers AS
        SELECT i AS customer_id, 'customer ' || i AS customer_name FROM range(1, 2001) t(i);
    CREATE TABLE orders AS
        SELECT i AS order_id, (i % 2000) + 1 AS customer_id FROM range(1, 20001) t(i);";

fn session(db: &Path, tables: &[&str], sample: Option<u64>) -> Session {
    let tables: Vec<SourceTable> = tables.iter().map(|t| plain(t)).collect();
    Session::over(db, &tables, sample, SessionTyping::Packages).unwrap()
}

fn measure(s: &Session, names: &[&str]) -> (Vec<EdgeEvidence>, Vec<Edge>) {
    let cols = column_evidence(s).unwrap();
    let names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    let measured = edge_evidence(s, &cols, &Dataset::sources(&names), Pass::Parts).unwrap();
    let edges = measured.iter().map(edge_from_evidence).collect();
    (measured, edges)
}

fn shop(dir: &Path) -> std::path::PathBuf {
    build_raw_db_from(dir, SHOP)
}

#[test]
fn sampled_parent_key_still_joins() {
    let dir = tempfile::tempdir().unwrap();
    let s = session(&shop(dir.path()), &["customers", "orders"], Some(500));
    let (_, edges) = measure(&s, &["customers", "orders"]);
    let e = edges
        .iter()
        .find(|e| {
            e.child.qualified() == "orders.customer_id"
                && e.parent.qualified() == "customers.customer_id"
        })
        .expect("the orders to customers edge");
    assert_eq!(e.status, EdgeStatus::Accepted, "{e:?}");
    assert_eq!(e.verification.orphan_count, 0);
    assert!(e.verification.parent_unique);
    assert_eq!(e.evidence.value_overlap, 1.0);
    assert_eq!(e.evidence.parent_total, 2000);
}

#[test]
fn sampled_rows_are_full_counts() {
    let dir = tempfile::tempdir().unwrap();
    let s = session(&shop(dir.path()), &["customers", "orders"], Some(500));
    let cols = column_evidence(&s).unwrap();
    let (_, edges) = measure(&s, &["customers", "orders"]);
    let datasets = Dataset::sources(&["customers".to_string(), "orders".to_string()]);
    let tables = table_evidence(&s, &cols, &edges, &datasets).unwrap();
    let rows: Vec<(&str, i64, i64)> = tables
        .iter()
        .map(|t| (t.name.as_str(), t.rows, t.sample_rows))
        .collect();
    assert_eq!(
        rows,
        [("customers", 2000, 500), ("orders", 20000, 500)],
        "the full count, beside the rows the rest was measured on"
    );
}

#[test]
fn an_unsampled_session_reads_every_row() {
    let dir = tempfile::tempdir().unwrap();
    let s = session(&shop(dir.path()), &["customers", "orders"], None);
    let cols = column_evidence(&s).unwrap();
    let (_, edges) = measure(&s, &["customers", "orders"]);
    let datasets = Dataset::sources(&["customers".to_string(), "orders".to_string()]);
    let tables = table_evidence(&s, &cols, &edges, &datasets).unwrap();
    assert_eq!(
        tables
            .iter()
            .map(|t| (t.rows, t.sample_rows))
            .collect::<Vec<_>>(),
        [(2000, 2000), (20000, 20000)]
    );
}

#[test]
fn sample_is_repeatable() {
    let dir = tempfile::tempdir().unwrap();
    let db = shop(dir.path());
    let run = || {
        let s = session(&db, &["customers", "orders"], Some(500));
        let cols = column_evidence(&s).unwrap();
        let (measured, _) = measure(&s, &["customers", "orders"]);
        (cols, measured)
    };
    assert_eq!(run(), run());
}

#[test]
fn parent_duplicates_counted() {
    // 1,000 parent rows over 997 keys: 1, 2 and 3 each appear twice, and a
    // NULL key appears twice and counts for nothing.
    let sql = "
        CREATE TABLE parents AS
            SELECT i AS parent_id FROM range(1, 998) t(i)
            UNION ALL SELECT i FROM range(1, 4) t(i)
            UNION ALL SELECT NULL FROM range(0, 2);
        CREATE TABLE kids AS
            SELECT i AS kid_id, (i % 997) + 1 AS parent_id FROM range(1, 3001) t(i);";
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_db_from(dir.path(), sql);
    for sample in [None, Some(100)] {
        let s = session(&db, &["parents", "kids"], sample);
        let (measured, edges) = measure(&s, &["parents", "kids"]);
        let find = |edges: &[Edge]| {
            edges
                .iter()
                .position(|e| {
                    e.child.qualified() == "kids.parent_id"
                        && e.parent.qualified() == "parents.parent_id"
                })
                .expect("the kids to parents edge")
        };
        let at = find(&edges);
        assert_eq!(measured[at].parent_duplicates, 3, "{sample:?}");
        assert_eq!(edges[at].evidence.parent_duplicates, 3, "{sample:?}");
        assert!(!edges[at].verification.parent_unique, "{sample:?}");
    }
}
