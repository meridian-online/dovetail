//! Combinations: tables that are one kind of thing split across files are
//! found, combined through a view, typed from their parts, and measured in
//! their parts' place, under the floor.

use std::path::Path;

use dovetail_core::combine::{run_combined, CombinedRun};
use dovetail_core::evidence::{Relation, Session};
use dovetail_core::finetype_ext::TypingRequest;
use dovetail_core::relate::EdgeStatus;
use dovetail_core::roles::{score_table, CombinationKind, Role};

fn combined(sql: &str) -> (Session, CombinedRun) {
    let s = Session::from_sql(sql, TypingRequest::Floor).unwrap();
    let run = run_combined(&s).unwrap();
    (s, run)
}

fn retail() -> (Session, CombinedRun) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/role-fixtures/retail");
    combined(&std::fs::read_to_string(dir.join("build.sql")).unwrap())
}

const SUPPLIERS: &str = "
    CREATE TABLE suppliers_north AS SELECT CAST(i AS INTEGER) AS supplier_id,
        'North Supplier ' || CAST(i AS VARCHAR) AS name, 'UK' AS country
    FROM range(1, 31) t(i);
    CREATE TABLE suppliers_south AS SELECT CAST(i AS INTEGER) AS supplier_id,
        'South Supplier ' || CAST(i AS VARCHAR) AS name, 'ES' AS country
    FROM range(101, 131) t(i);";

const PRICE_LIST: &str = "
    CREATE TABLE price_list_2024 AS SELECT 'BEV-' || lpad(CAST(i AS VARCHAR), 3, '0') AS sku,
        CAST(round(2 + (i % 37) * 0.75, 2) AS DOUBLE) AS unit_price
    FROM range(1, 101) t(i);
    CREATE TABLE price_list_2025 AS SELECT 'BEV-' || lpad(CAST(i AS VARCHAR), 3, '0') AS sku,
        CAST(round(2.5 + (i % 37) * 0.8, 2) AS DOUBLE) AS unit_price
    FROM range(1, 101) t(i);";

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[test]
fn partition_pair() {
    let (_, run) = combined(SUPPLIERS);
    assert_eq!(run.combinations.len(), 1, "{:#?}", run.combinations);
    let c = &run.combinations[0];
    assert_eq!(c.name, "suppliers");
    assert_eq!(c.parts, vec!["suppliers_north", "suppliers_south"]);
    assert_eq!(c.kind, CombinationKind::Partition);
    let token = c.token_column.as_ref().unwrap();
    assert_eq!(token.duckdb_type, "VARCHAR");
    assert_eq!(
        token.values,
        pairs(&[("suppliers_north", "north"), ("suppliers_south", "south")])
    );
    assert!(
        c.source_sql.contains("UNION ALL BY NAME"),
        "{}",
        c.source_sql
    );
    assert!(!c.evidence.is_empty());
    let suppliers = run.tables.iter().find(|t| t.name == "suppliers").unwrap();
    assert_eq!(suppliers.rows, 60);
    assert_eq!(suppliers.key, vec!["supplier_id"]);
}

#[test]
fn snapshot_pair() {
    let (s, run) = combined(PRICE_LIST);
    assert_eq!(run.combinations.len(), 1, "{:#?}", run.combinations);
    let c = &run.combinations[0];
    assert_eq!(c.name, "price_list");
    assert_eq!(c.kind, CombinationKind::Snapshot);
    let token = c.token_column.as_ref().unwrap();
    assert_eq!(token.duckdb_type, "INTEGER");
    assert_eq!(
        token.values,
        pairs(&[("price_list_2024", "2024"), ("price_list_2025", "2025")])
    );
    let t = run.tables.iter().find(|t| t.name == "price_list").unwrap();
    assert_eq!(t.key, vec!["sku".to_string(), token.name.clone()]);
    let non_audit: Vec<&str> = t
        .time_columns
        .iter()
        .filter(|c| !c.audit)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(non_audit, vec![token.name.as_str()]);
    let call = score_table(t, &run.edges, &run.carried, &s.typing);
    assert_eq!(call.role, Role::Snapshot, "{call:#?}");
}

#[test]
fn duplicate_checked_first() {
    let (_, run) = combined(
        "CREATE TABLE stock_2024 AS SELECT CAST(i AS INTEGER) AS item_id, i * 2 AS qty
         FROM range(1, 51) t(i);
         CREATE TABLE stock_2025 AS SELECT * FROM stock_2024;",
    );
    assert_eq!(run.combinations.len(), 1, "{:#?}", run.combinations);
    let c = &run.combinations[0];
    assert_eq!(c.kind, CombinationKind::Duplicate);
    assert_eq!(c.name, "stock");
    assert!(c.token_column.is_none());
    assert!(!c.source_sql.contains("UNION"), "{}", c.source_sql);
    let t = run.tables.iter().find(|t| t.name == "stock").unwrap();
    assert_eq!(t.rows, 50);
}

#[test]
fn different_columns_never_combine() {
    let (_, run) = combined(
        "CREATE TABLE sales_2024 AS SELECT CAST(i AS INTEGER) AS id, i * 1.5 AS amount
         FROM range(1, 51) t(i);
         CREATE TABLE sales_2025 AS SELECT CAST(i AS INTEGER) AS id, i * 1.5 AS total
         FROM range(1, 51) t(i);
         CREATE TABLE stock_2024 AS SELECT CAST(i AS INTEGER) AS id, i * 1.5 AS qty
         FROM range(1, 51) t(i);
         CREATE TABLE stock_2025 AS SELECT CAST(i AS INTEGER) AS id, 'n' || i AS qty
         FROM range(1, 51) t(i);",
    );
    assert!(run.combinations.is_empty(), "{:#?}", run.combinations);
    let names: Vec<&str> = run.datasets.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["sales_2024", "sales_2025", "stock_2024", "stock_2025"]
    );
}

#[test]
fn no_edges_between_parts() {
    let (_, run) = retail();
    let parts: Vec<&str> = run
        .combinations
        .iter()
        .flat_map(|c| c.parts.iter().map(|p| p.as_str()))
        .collect();
    assert_eq!(parts.len(), 6, "{:#?}", run.combinations);
    for e in &run.edges {
        assert!(
            !parts.contains(&e.child.table.as_str()) && !parts.contains(&e.parent.table.as_str()),
            "{} -> {}",
            e.child.qualified(),
            e.parent.qualified()
        );
    }
    for d in &run.datasets {
        assert!(!parts.contains(&d.name.as_str()), "{} is measured", d.name);
    }
    let views: Vec<&str> = run
        .datasets
        .iter()
        .filter(|d| d.relation == Relation::View)
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(views, vec!["members", "price_list", "suppliers"]);
}

#[test]
fn edges_point_at_combined_view() {
    let (_, run) = combined(&format!(
        "{SUPPLIERS}
         CREATE TABLE orders AS SELECT CAST(i AS INTEGER) AS order_id,
             CAST(CASE WHEN i % 2 = 0 THEN i % 30 + 1 ELSE i % 30 + 101 END AS INTEGER) AS supplier_id,
             DATE '2024-01-01' + CAST(i % 200 AS INTEGER) AS ordered_on
         FROM range(1, 501) t(i);"
    ));
    let to_suppliers: Vec<_> = run
        .edges
        .iter()
        .filter(|e| e.child.qualified() == "orders.supplier_id" && e.status != EdgeStatus::Rejected)
        .collect();
    assert_eq!(to_suppliers.len(), 1, "{:#?}", run.edges);
    let e = to_suppliers[0];
    assert_eq!(e.parent.qualified(), "suppliers.supplier_id");
    assert_eq!(e.status, EdgeStatus::Accepted, "{}", e.reason);
    assert_eq!(e.verification.orphan_count, 0);
}

#[test]
fn snapshot_child_key_is_carried() {
    let (s, run) = combined(&format!(
        "{PRICE_LIST}
         CREATE TABLE order_lines AS SELECT CAST(i AS INTEGER) AS line_id,
             'BEV-' || lpad(CAST(i % 100 + 1 AS VARCHAR), 3, '0') AS sku,
             CAST(i % 5 + 1 AS INTEGER) AS quantity
         FROM range(1, 1001) t(i);"
    ));
    assert_eq!(run.carried.len(), 1, "{:#?}", run.carried);
    let k = &run.carried[0];
    assert_eq!(k.child.qualified(), "order_lines.sku");
    assert_eq!(k.snapshot, "price_list");
    assert_eq!(k.key, "sku");
    assert_eq!(k.overlap, 1.0);
    assert!(k.child_repeats);
    assert!(
        !run.edges.iter().any(|e| e.parent.table == "price_list"),
        "{:#?}",
        run.edges
    );
    let c = &run.combinations[0];
    assert!(
        c.evidence.iter().any(|e| e.contains("order_lines.sku")),
        "{:#?}",
        c.evidence
    );
    let t = run.tables.iter().find(|t| t.name == "price_list").unwrap();
    let call = score_table(t, &run.edges, &run.carried, &s.typing);
    assert!(
        call.evidence.iter().any(|e| e.contains("order_lines.sku")),
        "{:#?}",
        call.evidence
    );
}

#[test]
fn view_columns_are_typed() {
    let (_, run) = retail();
    for c in &run.combinations {
        let token = c.token_column.as_ref().map(|t| t.name.as_str());
        let view_cols: Vec<_> = run.columns.iter().filter(|v| v.table == c.name).collect();
        assert!(!view_cols.is_empty(), "{}: no column evidence", c.name);
        for v in view_cols {
            if Some(v.column.as_str()) == token {
                continue;
            }
            let part_labels: Vec<Option<&str>> = c
                .parts
                .iter()
                .map(|p| {
                    run.columns
                        .iter()
                        .find(|x| &x.table == p && x.column == v.column)
                        .unwrap()
                        .label
                        .as_deref()
                })
                .collect();
            let agreed = part_labels.iter().flatten().next().copied();
            assert_eq!(v.label.as_deref(), agreed, "{}.{}", c.name, v.column);
        }
    }
    let year = run
        .columns
        .iter()
        .find(|v| v.table == "members" && v.column == "year")
        .expect("members.year");
    assert_eq!(year.duckdb_type, "INTEGER");
    assert_eq!(year.label.as_deref(), Some("datetime.component.year"));
    assert_eq!(year.rows, 600);
    assert_eq!(year.distinct, 2);
}

#[test]
fn member_visits_is_event() {
    let (s, run) = retail();
    let carried: Vec<String> = run
        .carried
        .iter()
        .map(|k| format!("{} -> {}.{}", k.child.qualified(), k.snapshot, k.key))
        .collect();
    assert!(
        carried.contains(&"member_visits.member_id -> members.member_id".to_string()),
        "{carried:?}"
    );
    assert!(
        !run.edges.iter().any(|e| e.status != EdgeStatus::Rejected
            && e.child.table == "member_visits"
            && e.parent.table == "members"),
        "{:#?}",
        run.edges
    );
    let t = run
        .tables
        .iter()
        .find(|t| t.name == "member_visits")
        .unwrap();
    let call = score_table(t, &run.edges, &run.carried, &s.typing);
    assert_eq!(call.role, Role::Event, "{call:#?}");
    assert!(
        call.evidence.iter().any(|e| e.contains("snapshot members")),
        "{:#?}",
        call.evidence
    );
}

#[test]
fn pair_key_snapshot() {
    // Each student takes three courses; the pair is the key, and the same
    // enrolments are graded again the next term.
    let (_, run) = combined(
        "CREATE TABLE grades_2024 AS SELECT CAST(i AS INTEGER) AS student_id,
             CAST(c AS INTEGER) AS course_id, CAST((i + c) % 5 AS INTEGER) AS grade
         FROM range(1, 41) a(i), range(1, 4) b(c);
         CREATE TABLE grades_2025 AS SELECT CAST(i AS INTEGER) AS student_id,
             CAST(c AS INTEGER) AS course_id, CAST((i * c) % 5 AS INTEGER) AS grade
         FROM range(11, 51) a(i), range(1, 4) b(c);",
    );
    assert_eq!(run.combinations.len(), 1, "{:#?}", run.combinations);
    let c = &run.combinations[0];
    assert_eq!(c.name, "grades");
    assert_eq!(c.kind, CombinationKind::Snapshot, "{:#?}", c.evidence);
    assert_eq!(c.key.len(), 2, "{:?}", c.key);
    let t = run.tables.iter().find(|t| t.name == "grades").unwrap();
    assert_eq!(t.key.len(), 3, "{:?}", t.key);
    assert_eq!(t.key[2], "year");
}
