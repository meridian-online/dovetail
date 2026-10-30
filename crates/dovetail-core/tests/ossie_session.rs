//! Source tables and the session built from them: the data packages name the
//! datasets and type their fields, the lineage columns are never measured, and
//! only the named tables are in the session.

mod common;

use common::*;
use dovetail_core::evidence::{column_evidence, table_evidence, Dataset, Session, SessionTyping};
use dovetail_core::finetype_ext::{TypingRequest, TypingSource};
use dovetail_core::packages::{read_packages, SourceTable};

const RETAIL: [&str; 15] = [
    "categories",
    "customer_segments",
    "customers",
    "loyalty_visits",
    "member_visits",
    "members_2024",
    "members_2025",
    "price_list_2024",
    "price_list_2025",
    "products",
    "promo_codes",
    "stores",
    "suppliers_north",
    "suppliers_south",
    "transactions",
];

fn retail_session(dir: &std::path::Path, sample: Option<u64>) -> Session {
    over_retail(&build_raw_retail_db(dir), sample)
}

fn over_retail(db: &std::path::Path, sample: Option<u64>) -> Session {
    let (tables, warnings) = read_packages(&retail_packages_dir(), &table_names(db)).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    Session::over(db, &tables, sample, SessionTyping::Packages).unwrap()
}

fn plain(name: &str) -> SourceTable {
    SourceTable {
        name: name.to_string(),
        relation: format!("raw_{name}"),
        source_file: None,
        source_part: None,
        fields: Vec::new(),
    }
}

#[test]
fn packages_name_the_datasets() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_retail_db(dir.path());
    let db_tables = table_names(&db);
    let (tables, warnings) = read_packages(&retail_packages_dir(), &db_tables).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, RETAIL);
    for t in &tables {
        assert_eq!(t.relation, format!("raw_{}", t.name));
        assert_eq!(t.source_file, Some(format!("source/{}.csv", t.name)));
        assert_eq!(t.source_part, None);
        assert!(!t.fields.is_empty());
    }
    let customers = tables.iter().find(|t| t.name == "customers").unwrap();
    assert_eq!(customers.fields[0].name, "customer_id");
    assert_eq!(customers.fields[0].frictionless_type, "integer");
    assert_eq!(
        customers.fields[0].finetype.as_deref(),
        Some("representation.numeric.integer_number")
    );

    // A package copied beside the fixture, naming a table the database lacks.
    let copy = dir.path().join("packages");
    std::fs::create_dir_all(copy.join("ghost")).unwrap();
    std::fs::write(
        copy.join("ghost/datapackage.json"),
        r#"{"name":"ghost","resources":[{"name":"ghost","path":"ghost.csv","schema":{"fields":[]}}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(copy.join("stores")).unwrap();
    std::fs::copy(
        retail_packages_dir().join("stores/datapackage.json"),
        copy.join("stores/datapackage.json"),
    )
    .unwrap();
    let (tables, warnings) = read_packages(&copy, &db_tables).unwrap();
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].name, "stores");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("ghost"), "{warnings:?}");
}

#[test]
fn lineage_is_never_measured() {
    let dir = tempfile::tempdir().unwrap();
    let s = retail_session(dir.path(), None);
    let cols = column_evidence(&s).unwrap();
    assert!(!cols.is_empty());
    assert!(
        cols.iter().all(|c| !c.column.starts_with("__source_")),
        "{:?}",
        cols.iter()
            .filter(|c| c.column.starts_with("__source_"))
            .collect::<Vec<_>>()
    );
    let datasets: Vec<Dataset> = RETAIL.iter().map(|t| Dataset::source(t)).collect();
    let evidence = table_evidence(&s, &cols, &[], &datasets).unwrap();
    assert_eq!(evidence.len(), RETAIL.len());
    for t in &evidence {
        assert!(!t.key.contains(&"__source_row".to_string()), "{}", t.name);
    }
    assert_eq!(s.typing, TypingSource::Packages);
    assert_eq!(s.sampled(), None);
}

#[test]
fn unlabelled_field_is_untyped() {
    let dir = tempfile::tempdir().unwrap();
    let s = retail_session(dir.path(), None);
    let cols = column_evidence(&s).unwrap();
    let label = |table: &str, column: &str| {
        cols.iter()
            .find(|c| c.table == table && c.column == column)
            .unwrap()
            .label
            .clone()
    };
    assert_eq!(label("customers", "email"), None);
    assert_eq!(
        label("customers", "joined_on").as_deref(),
        Some("datetime.date.iso")
    );

    // The floor would type that column: the package, not the floor, decides.
    let db = dir.path().join("retail.duckdb");
    let floor = Session::over(
        &db,
        &[plain("customers")],
        None,
        SessionTyping::Request(TypingRequest::Floor),
    )
    .unwrap();
    let floor_cols = column_evidence(&floor).unwrap();
    let floor_email = floor_cols
        .iter()
        .find(|c| c.column == "email")
        .unwrap()
        .label
        .clone();
    assert!(
        floor_email.is_some_and(|l| l.ends_with(".email")),
        "the floor should type customers.email"
    );
}

#[test]
fn quoted_names_survive() {
    let dir = tempfile::tempdir().unwrap();
    let sql = r#"CREATE TABLE "Order Lines" AS
        SELECT CAST(i AS INTEGER) AS line_id, 'x' || i AS "o'brien" FROM range(1, 6) t(i);"#;
    let db = build_raw_db_from(dir.path(), sql);
    assert_eq!(table_names(&db), vec!["raw_Order Lines"]);
    let table = SourceTable {
        name: "Order Lines".to_string(),
        relation: "raw_Order Lines".to_string(),
        source_file: None,
        source_part: None,
        fields: Vec::new(),
    };
    let s = Session::over(
        &db,
        &[table],
        None,
        SessionTyping::Request(TypingRequest::Floor),
    )
    .unwrap();
    let cols = column_evidence(&s).unwrap();
    let measured: Vec<(&str, &str, i64)> = cols
        .iter()
        .map(|c| (c.table.as_str(), c.column.as_str(), c.rows))
        .collect();
    assert_eq!(
        measured,
        vec![("Order Lines", "line_id", 5), ("Order Lines", "o'brien", 5)]
    );
    assert_eq!(
        s.full_relation("Order Lines"),
        Some("\"full\".main.\"raw_Order Lines\"")
    );
}

#[test]
fn other_tables_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let sql = format!(
        "{} CREATE TABLE silver_customers AS SELECT * FROM customers;",
        retail_build_sql()
    );
    let db = build_raw_db_from(dir.path(), &sql);
    // A silver table is not a raw one: keep it as a plain table.
    let conn = duckdb::Connection::open(&db).unwrap();
    conn.execute_batch(
        "ALTER TABLE raw_silver_customers RENAME TO silver_customers;
         ALTER TABLE silver_customers DROP COLUMN __source_file;",
    )
    .unwrap();
    drop(conn);
    let db_tables = table_names(&db);
    assert!(db_tables.contains(&"silver_customers".to_string()));
    let (tables, _) = read_packages(&retail_packages_dir(), &db_tables).unwrap();
    let s = Session::over(&db, &tables, None, SessionTyping::Packages).unwrap();
    let cols = column_evidence(&s).unwrap();
    assert!(cols.iter().all(|c| c.table != "silver_customers"));
    assert!(s.full_relation("silver_customers").is_none());
    assert_eq!(
        cols.iter()
            .map(|c| c.table.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        RETAIL.len()
    );
}

#[test]
fn sample_is_a_repeatable_copy_and_full_data_stays_readable() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_retail_db(dir.path());
    let a = over_retail(&db, Some(100));
    let b = over_retail(&db, Some(100));
    let rows =
        |s: &Session, sql: &str| -> i64 { s.conn().query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(rows(&a, "SELECT count(*) FROM src.main.transactions"), 100);
    assert_eq!(rows(&a, "SELECT count(*) FROM src.main.stores"), 12);
    let full = a.full_relation("transactions").unwrap();
    assert_eq!(rows(&a, &format!("SELECT count(*) FROM {full}")), 5000);
    let sum = "SELECT sum(txn_id) FROM src.main.transactions";
    assert_eq!(rows(&a, sum), rows(&b, sum));
    assert_eq!(a.sampled(), Some(100));
}

#[test]
fn columns_a_package_does_not_label_stay_untyped() {
    let dir = tempfile::tempdir().unwrap();
    let s = retail_session(dir.path(), None);
    s.conn()
        .execute_batch(
            "CREATE VIEW src.main.extra AS
             SELECT email FROM src.main.customers",
        )
        .unwrap();
    let cols = column_evidence(&s).unwrap();
    let extra = cols.iter().find(|c| c.table == "extra").unwrap();
    assert_eq!(extra.label, None);
}

#[test]
fn colliding_resources_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("packages");
    for (pkg, name) in [("a", "Stores"), ("b", "stores")] {
        std::fs::create_dir_all(copy.join(pkg)).unwrap();
        std::fs::write(
            copy.join(pkg).join("datapackage.json"),
            format!(r#"{{"resources":[{{"name":"{name}","path":"s.csv"}}]}}"#),
        )
        .unwrap();
    }
    let err = read_packages(&copy, &["raw_stores".to_string()]).unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains("a/datapackage.json") && text.contains("b/datapackage.json"),
        "{text}"
    );
}

#[test]
fn a_table_without_lineage_is_measured() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("plain.duckdb");
    let conn = duckdb::Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE people AS SELECT i AS id, 'n' || i AS name FROM range(1, 4) t(i);",
    )
    .unwrap();
    drop(conn);
    let tables = dovetail_core::packages::all_tables(&db).unwrap();
    let s = Session::over(
        &db,
        &tables,
        None,
        SessionTyping::Request(TypingRequest::Floor),
    )
    .unwrap();
    let cols = column_evidence(&s).unwrap();
    let measured: Vec<(&str, &str, i64)> = cols
        .iter()
        .map(|c| (c.table.as_str(), c.column.as_str(), c.rows))
        .collect();
    assert_eq!(measured, vec![("people", "id", 3), ("people", "name", 3)]);
}

#[test]
fn a_sample_of_zero_rows_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_retail_db(dir.path());
    let err = Session::over(&db, &[plain("stores")], Some(0), SessionTyping::Packages).err();
    assert!(err.is_some_and(|e| e.to_string().contains("0 rows")));
}

#[test]
fn x_part_becomes_source_part() {
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("packages");
    std::fs::create_dir_all(copy.join("book")).unwrap();
    std::fs::write(
        copy.join("book/datapackage.json"),
        r#"{"resources":[{"name":"stores","path":"book.xlsx","x-part":"Sheet1"}]}"#,
    )
    .unwrap();
    let (tables, _) = read_packages(&copy, &["raw_stores".to_string()]).unwrap();
    assert_eq!(tables[0].source_part.as_deref(), Some("Sheet1"));
}
