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

/// Two partitioned suppliers, a source table of products pointing at their
/// combination, and two partitioned shipments pointing at a source table of
/// stores: a combination as a parent, and as a child.
const COMBINED: &str = "
    CREATE TABLE suppliers_north AS
        SELECT i AS supplier_id, 'North Supplier ' || i AS name FROM range(1, 201) t(i);
    CREATE TABLE suppliers_south AS
        SELECT i AS supplier_id, 'South Supplier ' || i AS name FROM range(1001, 1201) t(i);
    CREATE TABLE products AS
        SELECT i AS product_id,
               CASE WHEN i % 2 = 0 THEN (i % 200) + 1 ELSE (i % 200) + 1001 END AS supplier_id
        FROM range(1, 301) t(i);
    CREATE TABLE stores AS
        SELECT i AS store_id, 'Store ' || i AS store_name FROM range(1, 501) t(i);
    CREATE TABLE shipments_east AS
        SELECT i AS shipment_id, (i % 500) + 1 AS store_id FROM range(1, 301) t(i);
    CREATE TABLE shipments_west AS
        SELECT i AS shipment_id, (i % 500) + 1 AS store_id FROM range(1001, 1301) t(i);";

/// For each pointer, whether the child's rows outnumber the parent's, as
/// `score_table` reads them: the child's `rows` against the edge's full
/// figure for the parent. Also the rows of every dataset.
type Outnumbering = (Vec<(String, i64)>, Vec<(String, String, i64, bool)>);

fn outnumbering(sample: Option<u64>) -> Outnumbering {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_db_from(dir.path(), COMBINED);
    let names = [
        "suppliers_north",
        "suppliers_south",
        "products",
        "stores",
        "shipments_east",
        "shipments_west",
    ];
    let s = session(&db, &names, sample);
    let run = dovetail_core::combine::run_combined(&s).unwrap();
    let mut combined: Vec<&str> = run.combinations.iter().map(|c| c.name.as_str()).collect();
    combined.sort();
    assert_eq!(combined, ["shipments", "suppliers"]);
    let rows = run
        .tables
        .iter()
        .map(|t| (t.name.clone(), t.rows))
        .collect();
    let rows_of = |name: &str| run.tables.iter().find(|t| t.name == name).unwrap().rows;
    let mut out = Vec::new();
    for (child, parent) in [("products", "suppliers"), ("shipments", "stores")] {
        let e = run
            .edges
            .iter()
            .find(|e| e.child.table == child && e.parent.table == parent)
            .unwrap_or_else(|| panic!("{child} to {parent} in {:#?}", run.edges));
        out.push((
            child.to_string(),
            parent.to_string(),
            e.evidence.parent_full_total,
            rows_of(child) > e.evidence.parent_full_total,
        ));
    }
    (rows, out)
}

#[test]
fn row_counts_compare_on_the_full_basis() {
    let (full_rows, full_answer) = outnumbering(None);
    let (rows, answer) = outnumbering(Some(50));
    // A combination holds the rows of all its parts; a source table all its own.
    assert_eq!(
        rows,
        [
            ("products".to_string(), 300),
            ("shipments".to_string(), 600),
            ("stores".to_string(), 500),
            ("suppliers".to_string(), 400),
        ]
    );
    assert_eq!(rows, full_rows);
    assert_eq!(answer, full_answer);
    // Products (300) sit between the view's sampled rows (about 100) and its
    // full 400, so only the full figure says they do not outnumber it;
    // shipments (600) outnumber the 500 stores.
    let row = |c: &str, p: &str, parent_rows: i64, outnumbers: bool| {
        (c.to_string(), p.to_string(), parent_rows, outnumbers)
    };
    assert_eq!(
        answer,
        [
            row("products", "suppliers", 400, false),
            row("shipments", "stores", 500, true)
        ]
    );
}

#[test]
fn a_duplicate_combination_counts_the_part_it_reads() {
    // Two identical 1,000-row price lists are one table, kept once.
    let sql = "
        CREATE TABLE price_list_2024 AS
            SELECT 'SKU-' || lpad(CAST(i AS VARCHAR), 4, '0') AS sku, i % 37 AS unit_price
            FROM range(1, 1001) t(i);
        CREATE TABLE price_list_2025 AS SELECT * FROM price_list_2024;
        CREATE TABLE sale_lines AS
            SELECT i AS line_id, 'SKU-' || lpad(CAST((i % 1000) + 1 AS VARCHAR), 4, '0') AS sku
            FROM range(1, 201) t(i);";
    let measure = |sample: Option<u64>| {
        let dir = tempfile::tempdir().unwrap();
        let db = build_raw_db_from(dir.path(), sql);
        let s = session(
            &db,
            &["price_list_2024", "price_list_2025", "sale_lines"],
            sample,
        );
        let run = dovetail_core::combine::run_combined(&s).unwrap();
        assert_eq!(run.combinations.len(), 1, "{:#?}", run.combinations);
        assert_eq!(
            run.combinations[0].kind,
            dovetail_core::roles::CombinationKind::Duplicate
        );
        let name = run.combinations[0].name.clone();
        let rows = run.tables.iter().find(|t| t.name == name).unwrap().rows;
        let parent_full_total = run
            .edges
            .iter()
            .find(|e| e.child.table == "sale_lines" && e.parent.table == name)
            .unwrap_or_else(|| panic!("sale_lines to {name} in {:#?}", run.edges))
            .evidence
            .parent_full_total;
        (rows, parent_full_total)
    };
    assert_eq!(measure(None), (1000, 1000));
    assert_eq!(measure(Some(100)), (1000, 1000));
}

#[test]
fn a_sampled_join_to_a_combination_reads_its_whole_key() {
    // Each supplier part holds 200 rows, four times the sample: the sampled view
    // holds a quarter of the supplier ids, the parts' full tables every one.
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_db_from(dir.path(), COMBINED);
    let names = [
        "suppliers_north",
        "suppliers_south",
        "products",
        "stores",
        "shipments_east",
        "shipments_west",
    ];
    let s = session(&db, &names, Some(50));
    let run = dovetail_core::combine::run_combined(&s).unwrap();
    let suppliers = run
        .combinations
        .iter()
        .find(|c| c.name == "suppliers")
        .expect("the suppliers combination");
    assert_eq!(
        suppliers.kind,
        dovetail_core::roles::CombinationKind::Partition
    );
    let e = run
        .edges
        .iter()
        .find(|e| {
            e.child.qualified() == "products.supplier_id"
                && e.parent.qualified() == "suppliers.supplier_id"
        })
        .unwrap_or_else(|| panic!("products to suppliers in {:#?}", run.edges));
    assert_eq!(e.status, EdgeStatus::Accepted, "{e:?}");
    assert_eq!(e.verification.orphan_count, 0);
    assert!(e.verification.parent_unique);
    assert_eq!(e.evidence.value_overlap, 1.0);
    assert_eq!(e.evidence.parent_total, 400);
    assert_eq!(e.evidence.parent_distinct, 400);
}
