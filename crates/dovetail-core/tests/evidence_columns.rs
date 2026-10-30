//! Column evidence: every column of the analyst's database measured and typed
//! inside DuckDB, from an in-memory session that attaches the database
//! read-only and writes its own evidence tables to itself.
//!
//! Tests that need FineType's profile run only when `DOVETAIL_FINETYPE_EXTENSION`
//! and `FINETYPE_MODEL_DIR` are set; `scripts/fetch-finetype.sh` prints both.

use dovetail_core::evidence::{
    column_evidence, is_audit_name, ColumnEvidence, Session, AUDIT_TIME_NAMES,
};
use dovetail_core::finetype_ext::{TypingRequest, TypingSource};

fn find<'a>(cols: &'a [ColumnEvidence], table: &str, column: &str) -> &'a ColumnEvidence {
    cols.iter()
        .find(|c| c.table == table && c.column == column)
        .unwrap_or_else(|| panic!("no evidence for {table}.{column} in {cols:#?}"))
}

fn tables(cols: &[ColumnEvidence]) -> Vec<String> {
    let mut names: Vec<String> = cols.iter().map(|c| c.table.clone()).collect();
    names.dedup();
    names
}

#[test]
fn counts_and_types() {
    let s = Session::from_sql(
        "CREATE TABLE t (id INTEGER, name VARCHAR);
         INSERT INTO t VALUES (1, 'a'), (2, NULL), (3, 'a');",
        TypingRequest::Floor,
    )
    .unwrap();
    let cols = column_evidence(&s).unwrap();
    let name = find(&cols, "t", "name");
    assert_eq!((name.rows, name.non_null, name.distinct), (3, 2, 1));
    assert_eq!(name.duckdb_type, "VARCHAR");
    let id = find(&cols, "t", "id");
    assert_eq!((id.rows, id.non_null, id.distinct), (3, 3, 3));
    assert_eq!(id.duckdb_type, "INTEGER");

    let s = Session::from_sql(
        "CREATE TABLE t (v INTEGER); INSERT INTO t VALUES (1), (NULL), (2);",
        TypingRequest::Floor,
    )
    .unwrap();
    let v = &column_evidence(&s).unwrap()[0];
    assert_eq!((v.rows, v.non_null, v.distinct), (3, 2, 2));
}

#[test]
fn awkward_names_are_quoted() {
    // Best, so the profiled path's SQL meets the same names when it is there.
    let s = Session::from_sql(
        r#"CREATE TABLE "Order ID" ("Order ID" INTEGER, "select" VARCHAR);
           CREATE TABLE "select" ("a""b" VARCHAR, "o'k" INTEGER);
           CREATE TABLE "a""b" ("from" INTEGER);
           CREATE TABLE "InvoiceLine" ("InvoiceLineId" INTEGER, "UnitPrice" DOUBLE);
           INSERT INTO "Order ID" VALUES (1, 'x'), (2, 'y');
           INSERT INTO "select" VALUES ('p', 1);
           INSERT INTO "a""b" VALUES (7);
           INSERT INTO "InvoiceLine" VALUES (1, 0.99), (2, 1.99);"#,
        TypingRequest::Best,
    )
    .unwrap();
    let cols = column_evidence(&s).unwrap();
    for (table, column, rows) in [
        ("Order ID", "Order ID", 2),
        ("Order ID", "select", 2),
        ("select", "a\"b", 1),
        ("select", "o'k", 1),
        ("a\"b", "from", 1),
        ("InvoiceLine", "InvoiceLineId", 2),
        ("InvoiceLine", "UnitPrice", 2),
    ] {
        let c = find(&cols, table, column);
        assert_eq!(c.rows, rows, "{table}.{column}");
        assert_eq!(c.non_null, rows, "{table}.{column}");
    }
    assert_eq!(cols.len(), 7);
}

#[test]
fn empty_table_has_zero_counts() {
    let s = Session::from_sql(
        "CREATE TABLE empty (id INTEGER, email VARCHAR);",
        TypingRequest::Best,
    )
    .unwrap();
    let cols = column_evidence(&s).unwrap();
    assert_eq!(cols.len(), 2);
    for c in &cols {
        assert_eq!((c.rows, c.non_null, c.distinct), (0, 0, 0), "{c:?}");
        assert_eq!(c.label, None, "{c:?}");
    }
}

#[test]
fn analyst_db_is_not_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("analyst.duckdb");
    let table_list = |conn: &duckdb::Connection| -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT table_name FROM information_schema.tables ORDER BY table_name")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    let before = {
        let conn = duckdb::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE customers (id INTEGER, email VARCHAR);
             INSERT INTO customers VALUES (1, 'a@example.com'), (2, 'b@example.com');
             CREATE TABLE orders (id INTEGER, customer_id INTEGER);
             INSERT INTO orders VALUES (1, 1), (2, 2);",
        )
        .unwrap();
        table_list(&conn)
    };

    {
        let s = Session::attach(&path, TypingRequest::Best).unwrap();
        let cols = column_evidence(&s).unwrap();
        assert_eq!(tables(&cols), ["customers", "orders"]);
        // The session holds its evidence itself.
        let held: i64 = s
            .conn()
            .query_row("SELECT count(*) FROM memory.column_evidence", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(held, 4);
    }

    let conn = duckdb::Connection::open(&path).unwrap();
    let after = table_list(&conn);
    assert_eq!(after, before);
    assert!(!after.iter().any(|t| t == "column_evidence"));
}

#[test]
fn email_is_labelled() {
    let s = Session::from_sql(
        "CREATE TABLE people AS
         SELECT i AS id, 'person' || i || '@example.com' AS email
         FROM range(200) r(i);",
        TypingRequest::Best,
    )
    .unwrap();
    let cols = column_evidence(&s).unwrap();
    let label = find(&cols, "people", "email").label.clone();
    assert!(
        label.as_deref().is_some_and(|l| l.ends_with(".email")),
        "typing {:?} labelled the email column {label:?}",
        s.typing
    );
    let source: String = s
        .conn()
        .query_row(
            "SELECT DISTINCT label_source FROM memory.column_evidence",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let expected = match s.typing {
        TypingSource::Profiled { .. } => "profiled",
        TypingSource::Floor { .. } => "floor",
    };
    assert_eq!(source, expected);
}

#[test]
fn audit_names() {
    for name in AUDIT_TIME_NAMES {
        assert!(is_audit_name(name), "{name}");
    }
    for name in [
        "created_at",
        "Created_At",
        "updated_at",
        "last_update",
        "created_ts",
        "updated_ts",
        "modified_at",
        "modified_ts",
        "last_update_ts",
    ] {
        assert!(is_audit_name(name), "{name}");
    }
    for name in [
        "purchased_at",
        "order_date",
        "created",
        "ts",
        "at",
        "visit_ts",
    ] {
        assert!(!is_audit_name(name), "{name}");
    }

    let s = Session::from_sql(
        "CREATE TABLE t (created_at TIMESTAMP, purchased_at TIMESTAMP);",
        TypingRequest::Floor,
    )
    .unwrap();
    let cols = column_evidence(&s).unwrap();
    assert!(find(&cols, "t", "created_at").audit_name);
    assert!(!find(&cols, "t", "purchased_at").audit_name);
}

#[test]
fn path_with_quote_attaches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("o'brien.duckdb");
    {
        let conn = duckdb::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE t AS SELECT 1 AS x;")
            .unwrap();
    }
    let s = Session::attach(&path, TypingRequest::Floor).unwrap();
    let cols = column_evidence(&s).unwrap();
    assert_eq!(find(&cols, "t", "x").rows, 1);
}

// Guards that the profile's output is stable run to run; it passes with or
// without `SET threads = 1`, so it does not prove that setting is needed.
#[test]
fn profile_is_deterministic_across_row_groups() {
    // More rows than one DuckDB row group (122,880), so a parallel scan would
    // split the table.
    let build = "CREATE TABLE big AS
         SELECT i AS id,
                'user' || i || '@example.com' AS email,
                CASE i % 3 WHEN 0 THEN 'red' WHEN 1 THEN 'green' ELSE 'blue' END AS colour
         FROM range(130000) r(i);";
    let first = Session::from_sql(build, TypingRequest::Best).unwrap();
    if !matches!(first.typing, TypingSource::Profiled { .. }) {
        eprintln!(
            "skipping: FineType's profile is unavailable: {:?}",
            first.typing
        );
        return;
    }
    let threads = |s: &Session| -> i64 {
        s.conn()
            .query_row("SELECT current_setting('threads')::BIGINT", [], |r| {
                r.get(0)
            })
            .unwrap()
    };
    let dump = |s: &Session| -> String {
        let before = threads(s);
        let cols = column_evidence(s).unwrap();
        assert_eq!(threads(s), before, "the thread count was not restored");
        let mut stmt = s
            .conn()
            .prepare("SELECT * FROM memory.column_evidence")
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((0..9)
                    .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                    .collect::<Vec<_>>()
                    .join("|"))
            })
            .unwrap();
        let table: Vec<String> = rows.map(Result::unwrap).collect();
        format!("{cols:#?}\n{}", table.join("\n"))
    };
    let a = dump(&first);
    let second = Session::from_sql(build, TypingRequest::Best).unwrap();
    let b = dump(&second);
    assert_eq!(a, b);
}

#[test]
fn evidence_tables_are_not_candidates() {
    let s = Session::from_sql(
        "CREATE TABLE a (x INTEGER); CREATE TABLE b (y INTEGER);",
        TypingRequest::Floor,
    )
    .unwrap();
    let first = column_evidence(&s).unwrap();
    let second = column_evidence(&s).unwrap();
    assert_eq!(tables(&first), ["a", "b"]);
    assert_eq!(first, second);
}
