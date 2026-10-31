//! The projection from a role model to an Ossie model, over the retail fixture as a
//! packages session.

mod common;

use std::path::Path;

use common::*;
use dovetail_core::evidence::{Session, SessionTyping};
use dovetail_core::finetype_ext::TypingRequest;
use dovetail_core::ossie::{project, write, Grade, Model, Provenance, Role, Verified};
use dovetail_core::packages::{read_packages, SourceTable};
use dovetail_core::roles::run_session;

const SCHEMA: &str = include_str!("../../../vendor/ossie/ossie-schema.json");

fn validate(yaml: &str) {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
    let doc: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&doc)
        .map(|e| format!("{} at {}", e, e.instance_path))
        .collect();
    assert!(errors.is_empty(), "schema errors: {errors:#?}");
}

fn retail(dir: &Path) -> (Model, std::path::PathBuf) {
    let db = build_raw_retail_db(dir);
    let (tables, warnings) = read_packages(&retail_packages_dir(), &table_names(&db)).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let session = Session::over(&db, &tables, None, SessionTyping::Packages).unwrap();
    let roles = run_session(&session).unwrap();
    let provenance = Provenance {
        pass: Verified::Sample,
        typing: roles.typing.clone(),
        sample_rows: None,
    };
    (
        project(&roles, &tables, &session, "retail", &provenance).unwrap(),
        db,
    )
}

fn dataset<'a>(m: &'a Model, name: &str) -> &'a dovetail_core::ossie::Dataset {
    m.datasets.iter().find(|d| d.name == name).unwrap()
}

#[test]
fn every_source_table_is_a_dataset() {
    let dir = tempfile::tempdir().unwrap();
    let (m, db) = retail(dir.path());
    let mut names: Vec<&str> = m.datasets.iter().map(|d| d.name.as_str()).collect();
    let sorted = {
        let mut s = names.clone();
        s.sort();
        s
    };
    assert_eq!(names, sorted, "datasets are sorted by name");
    for t in table_names(&db) {
        // The retail tables are all packaged: one dataset each.
        let t = t.strip_prefix("raw_").unwrap_or(&t).to_string();
        assert!(names.contains(&t.as_str()), "{t} is not a dataset");
    }
    names.retain(|n| ["suppliers", "price_list", "members"].contains(n));
    assert_eq!(names, ["members", "price_list", "suppliers"]);

    let categories = dataset(&m, "categories");
    assert_eq!(categories.meta.role, Role::Reference);
    assert_eq!(categories.meta.folded_into.as_deref(), Some("products"));
    assert_eq!(categories.meta.fold_column.as_deref(), Some("category"));
    assert_eq!(categories.source, "raw_categories");
    assert_eq!(categories.meta.rows, Some(8));
    assert_eq!(
        m.meta.generator,
        format!("dovetail {}", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(m.meta.pass, Some(Verified::Sample));
    assert_eq!(m.meta.typing.as_ref().unwrap().source, "packages");
}

#[test]
fn relationships_point_many_to_one() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = retail(dir.path());
    let r = m
        .relationships
        .iter()
        .find(|r| r.name == "transactions_customers")
        .unwrap();
    assert_eq!(
        (r.from.as_str(), r.to.as_str()),
        ("transactions", "customers")
    );
    assert_eq!(r.meta.grade, Grade::Likely);
    assert_eq!(r.meta.cardinality, "N:1");
    assert_eq!(r.meta.evidence.child_type, r.meta.evidence.parent_type);
    assert!(r.meta.evidence.child_type.is_some());
    assert!(r.meta.evidence.types_agree);
    assert_eq!(r.meta.verified, Verified::Sample);
    let names: Vec<&str> = m.relationships.iter().map(|r| r.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn combinations_are_datasets_with_parts() {
    let dir = tempfile::tempdir().unwrap();
    let (m, db) = retail(dir.path());
    let conn = duckdb::Connection::open_with_flags(
        &db,
        duckdb::Config::default()
            .access_mode(duckdb::AccessMode::ReadOnly)
            .unwrap(),
    )
    .unwrap();
    let mut parts = Vec::new();
    for (name, kind) in [
        ("suppliers", "partition"),
        ("price_list", "snapshot"),
        ("members", "snapshot"),
    ] {
        let d = dataset(&m, name);
        assert!(d.source.starts_with("SELECT"), "{}", d.source);
        assert!(d.source.contains("\"raw_"), "{}", d.source);
        assert!(!d.source.contains("src.main"), "{}", d.source);
        // Runs against the analyst's database alone, and gives the rows the dataset counts.
        let n: i64 = conn
            .query_row(&format!("SELECT count(*) FROM ({})", d.source), [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(n > 0);
        assert_eq!(Some(n as u64), d.meta.rows, "{name}");
        let c = d.meta.combination.as_ref().unwrap();
        assert_eq!(c.kind, kind);
        let files: Vec<&str> = c
            .parts
            .iter()
            .map(|p| dataset(&m, p).meta.source_file.as_str())
            .collect();
        assert_eq!(d.meta.source_file, files.join(", "));
        assert!(!d.meta.source_file.is_empty());
        for p in &c.parts {
            let part = dataset(&m, p);
            assert_eq!(part.meta.combined_into.as_deref(), Some(name));
            assert_eq!(part.meta.role, d.meta.role);
            assert_eq!(part.meta.role_evidence, [format!("combined into {name}")]);
            parts.push(p.clone());
        }
    }
    assert_eq!(parts.len(), 6);
    for r in &m.relationships {
        assert!(
            !parts.contains(&r.from) && !parts.contains(&r.to),
            "{}",
            r.name
        );
    }
}

#[test]
fn time_fields_are_marked() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = retail(dir.path());
    let dim = |d: &str, f: &str| {
        let field = dataset(&m, d).fields.iter().find(|x| x.name == f).unwrap();
        field.extras.get("dimension").cloned()
    };
    let t = dim("transactions", "purchased_at").expect("the event time is marked");
    assert_eq!(t["is_time"], serde_yaml::Value::Bool(true));
    // A field that is not a time field writes none.
    assert_eq!(dim("transactions", "txn_id"), None);
}

#[test]
fn audit_times_are_not_time_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_db_from(
        dir.path(),
        "CREATE TABLE customers AS
             SELECT i AS customer_id, 'customer ' || i AS name,
                    DATE '2024-01-01' + CAST(i AS INTEGER) AS joined_on,
                    TIMESTAMP '2024-01-01 00:00:00' AS created_at
             FROM range(1, 51) t(i);",
    );
    let tables = vec![SourceTable {
        name: "customers".into(),
        relation: "raw_customers".into(),
        source_file: None,
        source_part: None,
        fields: Vec::new(),
    }];
    let session = Session::over(
        &db,
        &tables,
        None,
        SessionTyping::Request(TypingRequest::Floor),
    )
    .unwrap();
    let roles = run_session(&session).unwrap();
    assert_eq!(roles.tables[0].audit_time_fields, ["created_at"]);
    let provenance = Provenance {
        pass: Verified::Sample,
        typing: roles.typing.clone(),
        sample_rows: None,
    };
    let m = project(&roles, &tables, &session, "m", &provenance).unwrap();
    let dim = |f: &str| {
        dataset(&m, "customers")
            .fields
            .iter()
            .find(|x| x.name == f)
            .unwrap()
            .extras
            .get("dimension")
            .cloned()
    };
    assert_eq!(
        dim("created_at").unwrap()["is_time"],
        serde_yaml::Value::Bool(false)
    );
    assert_eq!(dim("joined_on"), None);
}

#[test]
fn no_lineage_fields() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = retail(dir.path());
    for d in &m.datasets {
        for f in &d.fields {
            assert!(!f.name.starts_with("__source"), "{}.{}", d.name, f.name);
        }
    }
}

#[test]
fn typed_fields_carry_datatypes_and_untyped_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = retail(dir.path());
    let products = dataset(&m, "products");
    let field = |n: &str| products.fields.iter().find(|f| f.name == n).unwrap();
    // No label: untyped, so no datatype either.
    assert_eq!(field("sku").meta.finetype, None);
    assert_eq!(field("sku").datatype, None);
    assert_eq!(
        field("category_id").datatype,
        Some(dovetail_core::ossie::Datatype::Integer)
    );
    assert!(field("unit_price").datatype.is_some());
}

#[test]
fn empty_table_is_unclear_with_zero_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_db_from(
        dir.path(),
        "CREATE TABLE notes (note_id INTEGER, body VARCHAR);",
    );
    let tables = vec![SourceTable {
        name: "notes".into(),
        relation: "raw_notes".into(),
        source_file: Some("notes.csv".into()),
        source_part: None,
        fields: Vec::new(),
    }];
    let session = Session::over(
        &db,
        &tables,
        None,
        SessionTyping::Request(TypingRequest::Floor),
    )
    .unwrap();
    let roles = run_session(&session).unwrap();
    let provenance = Provenance {
        pass: Verified::Sample,
        typing: roles.typing.clone(),
        sample_rows: None,
    };
    let m = project(&roles, &tables, &session, "empty", &provenance).unwrap();
    let d = dataset(&m, "notes");
    assert_eq!(d.meta.role, Role::Unclear);
    assert_eq!(d.meta.rows, Some(0));
    assert_eq!(d.fields.len(), 2);
    assert_eq!(m.meta.typing.as_ref().unwrap().source, "floor");
    validate(&write(&m));
}

/// Run the roles over `tables` in the database at `db` and project them.
fn project_plain(
    db: &Path,
    tables: &[SourceTable],
    typing: SessionTyping,
) -> (dovetail_core::roles::RoleModel, Model) {
    let session = Session::over(db, tables, None, typing).unwrap();
    let roles = run_session(&session).unwrap();
    let provenance = Provenance {
        pass: Verified::Sample,
        typing: roles.typing.clone(),
        sample_rows: None,
    };
    let m = project(&roles, tables, &session, "m", &provenance).unwrap();
    (roles, m)
}

fn table(name: &str, relation: &str) -> SourceTable {
    SourceTable {
        name: name.into(),
        relation: relation.into(),
        source_file: None,
        source_part: None,
        fields: Vec::new(),
    }
}

fn db_from(dir: &Path, sql: &str) -> std::path::PathBuf {
    let path = dir.join("plain.duckdb");
    duckdb::Connection::open(&path)
        .unwrap()
        .execute_batch(sql)
        .unwrap();
    path
}

fn packaged(name: &str, fields: &[(&str, &str, Option<&str>)]) -> SourceTable {
    use dovetail_core::packages::PackageField;
    SourceTable {
        name: name.into(),
        relation: format!("raw_{name}"),
        source_file: None,
        source_part: None,
        fields: fields
            .iter()
            .map(|(n, t, l)| PackageField {
                name: n.to_string(),
                frictionless_type: t.to_string(),
                finetype: l.map(str::to_string),
            })
            .collect(),
    }
}

#[test]
fn a_combination_can_be_a_folded_reference() {
    const INT: &str = "representation.numeric.integer_number";
    const WORD: &str = "representation.text.word";
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_db_from(
        dir.path(),
        "CREATE TABLE colour_2024 AS SELECT i AS colour_id, 'colour ' || i AS label FROM range(1, 9) t(i);
         CREATE TABLE colour_2025 AS SELECT * FROM colour_2024;
         CREATE TABLE items AS SELECT i AS item_id, 'item ' || i AS title, (i % 8) + 1 AS colour_id
             FROM range(1, 201) t(i);",
    );
    let tables = vec![
        packaged(
            "colour_2024",
            &[
                ("colour_id", "integer", Some(INT)),
                ("label", "string", Some(WORD)),
            ],
        ),
        packaged(
            "colour_2025",
            &[
                ("colour_id", "integer", Some(INT)),
                ("label", "string", Some(WORD)),
            ],
        ),
        packaged(
            "items",
            &[
                ("item_id", "integer", Some(INT)),
                ("title", "string", Some(WORD)),
                ("colour_id", "integer", Some(INT)),
            ],
        ),
    ];
    let (roles, m) = project_plain(&db, &tables, SessionTyping::Packages);
    assert_eq!(roles.combinations.len(), 1, "{:?}", roles.combinations);
    let name = roles.combinations[0].name.clone();
    assert_eq!(roles.folds.len(), 1, "{:?}", roles.folds);
    let d = dataset(&m, &name);
    assert_eq!(d.meta.folded_into.as_deref(), Some("items"));
    // No part has a source file, so the combination lists none.
    assert_eq!(d.meta.source_file, "");
    assert_eq!(
        d.meta.fold_column.as_deref(),
        Some(roles.folds[0].struct_column.as_str())
    );
    // Its parts are not folded; they say what they were combined into.
    assert_eq!(dataset(&m, "colour_2024").meta.folded_into, None);
}

#[test]
fn reserved_words_are_quoted() {
    let dir = tempfile::tempdir().unwrap();
    let db = db_from(
        dir.path(),
        r#"CREATE TABLE "group" AS SELECT i AS group_id, 'g' || i AS "order" FROM range(1, 31) t(i);
           CREATE TABLE "join" AS SELECT i AS join_id, i AS "left", i % 2 = 0 AS "full" FROM range(1, 31) t(i);"#,
    );
    let tables = vec![table("group", "group"), table("join", "join")];
    let (_, m) = project_plain(&db, &tables, SessionTyping::Request(TypingRequest::Floor));
    let d = dataset(&m, "group");
    assert_eq!(d.source, "\"group\"");
    let sql = |n: &str| {
        d.fields
            .iter()
            .find(|f| f.name == n)
            .unwrap()
            .expression
            .sql
            .clone()
    };
    assert_eq!(sql("order").as_deref(), Some("\"order\""));
    assert_eq!(sql("group_id").as_deref(), Some("group_id"));
    // The source and the expression run as written.
    let conn = duckdb::Connection::open(&db).unwrap();
    let n: i64 = conn
        .query_row(
            &format!("SELECT count({}) FROM {}", sql("order").unwrap(), d.source),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 30);

    // Keywords that are not "reserved" in DuckDB's own words still cannot stand bare.
    let j = dataset(&m, "join");
    assert_eq!(j.source, "\"join\"");
    let jsql = |n: &str| {
        j.fields
            .iter()
            .find(|f| f.name == n)
            .unwrap()
            .expression
            .sql
            .clone()
            .unwrap()
    };
    assert_eq!(jsql("left"), "\"left\"");
    assert_eq!(jsql("full"), "\"full\"");
    assert_eq!(jsql("join_id"), "join_id");
    let n: i64 = conn
        .query_row(
            &format!(
                "SELECT count({}) FROM {} WHERE {}",
                jsql("left"),
                j.source,
                jsql("full")
            ),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 15);
}

#[test]
fn relationship_names_are_unique() {
    let dir = tempfile::tempdir().unwrap();
    let db = db_from(
        dir.path(),
        "CREATE TABLE things AS SELECT i AS thing_id, i AS other_id, 'thing ' || i AS label FROM range(1, 41) t(i);
         CREATE TABLE links AS SELECT i AS link_id, (i % 40) + 1 AS thing_id FROM range(1, 401) t(i);",
    );
    let tables = vec![table("things", "things"), table("links", "links")];
    let (_, m) = project_plain(&db, &tables, SessionTyping::Request(TypingRequest::Floor));
    let from_links: Vec<_> = m
        .relationships
        .iter()
        .filter(|r| r.from == "links")
        .collect();
    assert!(
        from_links.len() >= 2,
        "{:?}",
        m.relationships.iter().map(|r| &r.name).collect::<Vec<_>>()
    );
    let mut names: Vec<&str> = m.relationships.iter().map(|r| r.name.as_str()).collect();
    let all = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), all, "{names:?}");
}

#[test]
fn a_unique_child_column_with_nulls_is_one_to_one() {
    let dir = tempfile::tempdir().unwrap();
    let db = db_from(
        dir.path(),
        "CREATE TABLE parents AS SELECT i AS parent_id, 'parent ' || i AS label FROM range(1, 41) t(i);
         CREATE TABLE kids AS SELECT i AS kid_id, CASE WHEN i <= 20 THEN i END AS parent_id FROM range(1, 41) t(i);",
    );
    let tables = vec![table("parents", "parents"), table("kids", "kids")];
    let (_, m) = project_plain(&db, &tables, SessionTyping::Request(TypingRequest::Floor));
    let r = m
        .relationships
        .iter()
        .find(|r| r.from == "kids" && r.to == "parents")
        .expect("kids point at parents");
    assert_eq!(r.meta.cardinality, "1:1");
}
