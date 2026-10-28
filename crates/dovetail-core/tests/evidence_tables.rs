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
