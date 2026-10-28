//! `table_evidence` on small tables built in SQL, under the floor: NULLs in
//! the pointer or time, as-of dates, and self-references.

use dovetail_core::evidence::{column_evidence, table_evidence, Session};
use dovetail_core::finetype_ext::TypingRequest;
use dovetail_core::relate::run_with;
use dovetail_core::roles::{points_at, score_table, Role, RoleCall, TableEvidence};

/// Build `sql`, then measure and score `table`.
fn measure(sql: &str, table: &str) -> (TableEvidence, RoleCall) {
    let s = Session::from_sql(sql, TypingRequest::Floor).unwrap();
    let cols = column_evidence(&s).unwrap();
    let mut all: Vec<String> = cols.iter().map(|c| c.table.clone()).collect();
    all.dedup();
    let run = run_with(&s, &cols, &all).unwrap();
    let t = table_evidence(&s, &cols, &run.edges, &[table.to_string()])
        .unwrap()
        .remove(0);
    let call = score_table(&t, &run.edges, &[], &s.typing);
    (t, call)
}

#[test]
fn nulls_do_not_make_a_time_vary_within_its_parent() {
    // Each user's visits share one date; a NULL user and a NULL date would
    // each read as one more (user, date) pair if counted.
    let sql = "
        CREATE TABLE users AS SELECT CAST(i AS INTEGER) AS user_id, 'User ' || i AS name
        FROM range(1, 31) t(i);
        CREATE TABLE visits AS
        SELECT CAST(i % 30 + 1 AS INTEGER) AS user_id,
               DATE '2024-01-01' + CAST(i % 30 + 1 AS INTEGER) AS visited_on
        FROM range(0, 300) t(i)
        UNION ALL SELECT NULL, DATE '2024-06-01'
        UNION ALL SELECT 1, NULL;";
    let s = Session::from_sql(sql, TypingRequest::Floor).unwrap();
    let cols = column_evidence(&s).unwrap();
    let all = vec!["users".to_string(), "visits".to_string()];
    let run = run_with(&s, &cols, &all).unwrap();
    assert!(
        points_at(&run.edges).any(|e| e.child.qualified() == "visits.user_id"),
        "the test needs visits.user_id as a pointer"
    );
    let t = table_evidence(&s, &cols, &run.edges, &["visits".to_string()])
        .unwrap()
        .remove(0);
    assert_eq!(t.time_columns.len(), 1);
    assert!(!t.time_columns[0].varies_within_parent, "{t:#?}");
}

#[test]
fn audit_stamp_is_not_an_as_of_date() {
    let base = "CREATE TABLE status AS SELECT CAST(i AS INTEGER) AS status_id,
                CASE WHEN i % 2 = 0 THEN 'on' ELSE 'off' END AS state, {col}
                FROM range(1, 11) t(i);";
    let sql = base.replace("{col}", "DATE '2024-05-01' AS loaded_at");
    let (t, _) = measure(&sql, "status");
    assert_eq!(t.constant_date, None, "{t:#?}");
    let sql = base.replace("{col}", "DATE '2024-05-01' AS as_of");
    let (t, _) = measure(&sql, "status");
    assert_eq!(t.constant_date.as_deref(), Some("as_of"));

    // One row is no as-of date either.
    let sql = "CREATE TABLE one AS SELECT 1 AS one_id, DATE '2024-05-01' AS as_of;";
    let (t, _) = measure(sql, "one");
    assert_eq!(t.constant_date, None, "{t:#?}");
}

#[test]
fn reports_to_is_a_self_reference() {
    let sql = "CREATE TABLE employees AS SELECT
                   CAST(i AS INTEGER) AS employee_id,
                   'Name ' || i AS last_name,
                   'Clerk' AS title,
                   CASE WHEN i = 1 THEN NULL ELSE CAST((i - 1) / 3 + 1 AS INTEGER) END AS reports_to,
                   DATE '2020-01-01' + CAST(i * 40 AS INTEGER) AS hire_date
               FROM range(1, 9) t(i);";
    let (t, call) = measure(sql, "employees");
    assert_eq!(t.key, vec!["employee_id"]);
    assert_eq!(t.self_references, vec!["reports_to"]);
    assert_eq!(
        call.role,
        Role::Entity,
        "{:?} {:#?}",
        call.scores,
        call.evidence
    );
}

#[test]
fn a_small_count_inside_a_dense_key_is_not_a_self_reference() {
    // sort_order's values all lie in id's, but nothing in its name says it
    // refers to another code.
    let sql = "CREATE TABLE codes AS SELECT CAST(i AS INTEGER) AS id, 'Code ' || i AS label,
                   CAST(11 - i AS INTEGER) AS sort_order
               FROM range(1, 11) t(i);";
    let (t, call) = measure(sql, "codes");
    assert_eq!(t.key, vec!["id"]);
    assert!(t.self_references.is_empty(), "{t:#?}");
    assert_eq!(
        call.role,
        Role::Unclear,
        "{:?} {:#?}",
        call.scores,
        call.evidence
    );
}

#[test]
fn self_reference_needs_a_self_pointer_name() {
    // Values inside the key, and names that share only the table's prefix.
    for (sql, table) in [
        (
            "CREATE TABLE orders AS SELECT CAST(i AS INTEGER) AS order_id,
                 CAST(i % 3 + 1 AS INTEGER) AS order_priority FROM range(1, 41) t(i);",
            "orders",
        ),
        (
            "CREATE TABLE stores AS SELECT CAST(i AS INTEGER) AS store_id,
                 CAST(41 - i AS INTEGER) AS store_number FROM range(1, 41) t(i);",
            "stores",
        ),
    ] {
        let (t, _) = measure(sql, table);
        assert!(t.self_references.is_empty(), "{table}: {t:#?}");
    }
    let sql = "CREATE TABLE categories AS SELECT CAST(i AS INTEGER) AS category_id,
                   CASE WHEN i <= 3 THEN NULL ELSE CAST(i % 3 + 1 AS INTEGER) END
                       AS parent_category_id
               FROM range(1, 13) t(i);";
    let (t, _) = measure(sql, "categories");
    assert_eq!(t.self_references, vec!["parent_category_id"]);
}

#[test]
fn table_evidence_writes_without_extensions() {
    // Writing the evidence must not need an extension DuckDB would fetch.
    let sql = "CREATE TABLE employees AS SELECT CAST(i AS INTEGER) AS employee_id,
                   'Name ' || i AS last_name,
                   CASE WHEN i = 1 THEN NULL ELSE 1 END AS reports_to,
                   DATE '2020-01-01' + CAST(i AS INTEGER) AS hire_date
               FROM range(1, 9) t(i);
               CREATE TABLE empty AS SELECT 1 AS id WHERE false;";
    let s = Session::from_sql(sql, TypingRequest::Floor).unwrap();
    s.conn()
        .execute_batch(
            "SET autoinstall_known_extensions = false; SET autoload_known_extensions = false;",
        )
        .unwrap();
    let cols = column_evidence(&s).unwrap();
    let tables = vec!["employees".to_string(), "empty".to_string()];
    let run = run_with(&s, &cols, &tables).unwrap();
    table_evidence(&s, &cols, &run.edges, &tables).unwrap();
    let (key, refs, times, constant): (String, String, String, Option<String>) = s
        .conn()
        .query_row(
            "SELECT CAST(\"key\" AS VARCHAR), CAST(self_references AS VARCHAR),
                    CAST(time_columns AS VARCHAR), constant_date
             FROM memory.main.table_evidence WHERE \"table\" = 'employees'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(key, "[employee_id]");
    assert_eq!(refs, "[reports_to]");
    assert!(times.contains("hire_date"), "{times}");
    assert_eq!(constant, None);
    let empty_key: String = s
        .conn()
        .query_row(
            "SELECT CAST(\"key\" AS VARCHAR) FROM memory.main.table_evidence
             WHERE \"table\" = 'empty'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(empty_key, "[]");
    // The guard means something only while JSON is not built in.
    let json_loaded: bool = s
        .conn()
        .query_row(
            "SELECT coalesce(bool_or(loaded), false) FROM duckdb_extensions()
             WHERE extension_name = 'json'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        !json_loaded,
        "json is loaded, so this test no longer guards it"
    );
}
