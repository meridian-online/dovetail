//! The retail role fixture: it builds at the stated size, its answer key loads,
//! and `relate` finds the joins the stand-in schema implies.

use std::path::{Path, PathBuf};

use dovetail_core::evidence::Session;
use dovetail_core::finetype_ext::TypingRequest;
use dovetail_core::relate::{run_session, EdgeStatus};
use dovetail_core::role_key::{load_answer_key, CombinationKind};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/role-fixtures/retail")
}

fn build_sql() -> String {
    std::fs::read_to_string(fixture_dir().join("build.sql")).unwrap()
}

#[test]
fn fixture_builds_with_expected_counts() {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch(&build_sql()).expect("build fixture");
    let expected = [
        ("customers", 400),
        ("stores", 12),
        ("categories", 8),
        ("products", 120),
        ("transactions", 5000),
        ("loyalty_visits", 1500),
        ("customer_segments", 2400),
        ("promo_codes", 20),
        ("suppliers_north", 30),
        ("suppliers_south", 30),
        ("price_list_2024", 100),
        ("price_list_2025", 100),
        ("members_2024", 300),
        ("members_2025", 300),
        ("member_visits", 2000),
    ];
    for (table, rows) in expected {
        let n: i64 = conn
            .query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, rows, "{table}");
    }
}

#[test]
fn answer_key_loads() {
    let key = load_answer_key(&fixture_dir().join("answer-key.json")).unwrap();
    assert_eq!(key.tables.len(), 12);
    assert_eq!(key.folds.len(), 1);
    assert_eq!(key.folds[0].reference, "categories");
    assert_eq!(key.folds[0].into, "products");
    let kinds: Vec<_> = key
        .combinations
        .iter()
        .map(|c| (c.name.as_str(), c.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("suppliers", CombinationKind::Partition),
            ("price_list", CombinationKind::Snapshot),
            ("members", CombinationKind::Snapshot),
        ]
    );
}

#[test]
fn relate_finds_the_stand_in_joins() {
    let s = Session::from_sql(&build_sql(), TypingRequest::Floor).unwrap();
    let run = run_session(&s).unwrap();
    for e in &run.edges {
        eprintln!(
            "{} -> {} [{:?}]",
            e.child.qualified(),
            e.parent.qualified(),
            e.status
        );
    }
    let has = |child: &str, parent: &str| {
        run.edges.iter().any(|e| {
            e.status == EdgeStatus::Accepted
                && e.child.qualified() == child
                && e.parent.qualified() == parent
        })
    };
    for (c, p) in [
        ("transactions.customer_id", "customers.customer_id"),
        ("transactions.store_id", "stores.store_id"),
        ("transactions.sku", "products.sku"),
        ("products.category_id", "categories.category_id"),
        ("loyalty_visits.customer_id", "customers.customer_id"),
        ("customer_segments.customer_id", "customers.customer_id"),
    ] {
        assert!(has(c, p), "missing accepted edge {c} -> {p}");
    }
    let between = run.edges.iter().any(|e| {
        let (c, p) = (e.child.table.as_str(), e.parent.table.as_str());
        (c == "loyalty_visits" && p == "member_visits")
            || (c == "member_visits" && p == "loyalty_visits")
    });
    assert!(
        !between,
        "an edge runs between loyalty_visits and member_visits"
    );
}
