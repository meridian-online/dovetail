//! The retail role fixture: it builds at the stated size, its answer key loads,
//! and `relate` finds the joins the stand-in schema implies.

use std::path::{Path, PathBuf};

use dovetail_core::evidence::Session;
use dovetail_core::finetype_ext::TypingRequest;
use dovetail_core::relate::{run_session, EdgeStatus};
use dovetail_core::role_key::{load_answer_key, CombinationKind, Role};

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
    let roles: Vec<(&str, Role)> = key
        .tables
        .iter()
        .map(|(n, t)| (n.as_str(), t.role))
        .collect();
    assert_eq!(
        roles,
        vec![
            ("categories", Role::Reference),
            ("customer_segments", Role::Snapshot),
            ("customers", Role::Entity),
            ("loyalty_visits", Role::Event),
            ("member_visits", Role::Event),
            ("members", Role::Snapshot),
            ("price_list", Role::Snapshot),
            ("products", Role::Entity),
            ("promo_codes", Role::Unclear),
            ("stores", Role::Entity),
            ("suppliers", Role::Entity),
            ("transactions", Role::Event),
        ]
    );
    for (name, t) in &key.tables {
        assert!(!t.why.trim().is_empty(), "{name}: empty why");
        assert!(!t.why.contains('\n'), "{name}: why spans lines");
    }
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

    // Every name the key mentions exists in the built fixture.
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch(&build_sql()).unwrap();
    let built: Vec<String> = conn
        .prepare("SELECT table_name FROM information_schema.tables")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let combined: Vec<&str> = key.combinations.iter().map(|c| c.name.as_str()).collect();
    for name in key
        .tables
        .keys()
        .filter(|n| !combined.contains(&n.as_str()))
    {
        assert!(built.contains(name), "key table {name} not built");
    }
    for part in key.combinations.iter().flat_map(|c| &c.parts) {
        assert!(built.contains(part), "combination part {part} not built");
    }
}

#[test]
fn fixture_carries_the_evidence_its_key_claims() {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch(&build_sql()).unwrap();
    let n = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };

    assert_eq!(
        n("SELECT count(DISTINCT assessed_on) FROM customer_segments"),
        6
    );
    assert_eq!(
        n("SELECT count(*) FROM (SELECT DISTINCT customer_id, assessed_on FROM customer_segments)"),
        n("SELECT count(*) FROM customer_segments")
    );
    assert_eq!(
        n("SELECT count(*) FROM suppliers_north JOIN suppliers_south USING (supplier_id)"),
        0
    );
    let shared = n("SELECT count(*) FROM members_2024 JOIN members_2025 USING (member_id)");
    assert!(
        shared * 2 >= n("SELECT count(*) FROM members_2024"),
        "overlap {shared}"
    );
    assert!(
        n(
            "SELECT count(*) FROM (SELECT customer_id FROM transactions GROUP BY 1
           HAVING count(DISTINCT purchased_at) > 1)"
        ) > 0
    );

    let s = Session::from_sql(&build_sql(), TypingRequest::Floor).unwrap();
    let run = run_session(&s).unwrap();
    let touching: Vec<_> = run
        .edges
        .iter()
        .filter(|e| {
            e.status != EdgeStatus::Rejected
                && (e.child.table == "promo_codes" || e.parent.table == "promo_codes")
        })
        .map(|e| format!("{} -> {}", e.child.qualified(), e.parent.qualified()))
        .collect();
    assert!(touching.is_empty(), "promo_codes has edges: {touching:?}");
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
