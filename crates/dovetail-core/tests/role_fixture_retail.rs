//! The retail role fixture: it builds at the stated size, its answer key loads,
//! `relate` finds the joins the stand-in schema implies, and the stand-in
//! tables score to the roles the key gives them.

use std::path::{Path, PathBuf};

use dovetail_core::evidence::{column_evidence, table_evidence, Session};
use dovetail_core::finetype_ext::{TypingRequest, TypingSource};
use dovetail_core::relate::{run_session, run_with, EdgeStatus};
use dovetail_core::role_key::{load_answer_key, CombinationKind, Role};
use dovetail_core::roles::{score_table, RoleCall, TableEvidence};

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

/// The tables that stand in for Brightfield's own; the combined parts and the
/// visits that point at a combination wait for combinations.
const STAND_INS: [&str; 8] = [
    "customers",
    "stores",
    "categories",
    "products",
    "transactions",
    "loyalty_visits",
    "customer_segments",
    "promo_codes",
];

/// Score the stand-in tables under `typing`, with each table's evidence.
fn score_stand_ins(typing: TypingRequest) -> (TypingSource, Vec<(TableEvidence, RoleCall)>) {
    let s = Session::from_sql(&build_sql(), typing).unwrap();
    let cols = column_evidence(&s).unwrap();
    let mut all: Vec<String> = cols.iter().map(|c| c.table.clone()).collect();
    all.dedup();
    let run = run_with(&s, &cols, &all).unwrap();
    let tables: Vec<String> = STAND_INS.iter().map(|t| t.to_string()).collect();
    let evidence = table_evidence(&s, &cols, &run.edges, &tables).unwrap();
    let written: Vec<String> = s
        .conn()
        .prepare("SELECT \"table\" FROM memory.main.table_evidence")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(written, tables, "memory.table_evidence");
    let calls = evidence
        .into_iter()
        .map(|t| {
            let call = score_table(&t, &run.edges, &[], &s.typing);
            (t, call)
        })
        .collect();
    (s.typing.clone(), calls)
}

#[test]
fn retail_roles_match_answer_key() {
    let (typing, calls) = score_stand_ins(TypingRequest::Best);
    if !matches!(typing, TypingSource::Profiled { .. }) {
        eprintln!("skipping: FineType's profile is unavailable: {typing:?}");
        return;
    }
    let key = load_answer_key(&fixture_dir().join("answer-key.json")).unwrap();
    let mut wrong = Vec::new();
    for (t, call) in &calls {
        eprintln!("{}: {:?} {:?}", t.name, call.role, call.scores);
        let want = key.tables[&t.name].role;
        if call.role != want {
            wrong.push(format!(
                "{}: got {:?}, want {want:?}\n  scores {:?}\n  evidence {:#?}\n  table {t:#?}",
                t.name, call.role, call.scores, call.evidence
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert_eq!(calls.len(), STAND_INS.len());
}

#[test]
fn retail_roles_floor_recorded() {
    let (_, calls) = score_stand_ins(TypingRequest::Floor);
    assert_eq!(calls.len(), STAND_INS.len());
    for (t, call) in &calls {
        eprintln!("{}: {:?} {:?}", t.name, call.role, call.scores);
        assert!(
            call.typing.starts_with("floor"),
            "{}: {}",
            t.name,
            call.typing
        );
    }
}
