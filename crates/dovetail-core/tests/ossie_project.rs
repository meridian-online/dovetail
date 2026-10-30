//! The projection from a role model to an Ossie model, over the retail fixture as a
//! packages session.

mod common;

use std::path::Path;

use common::*;
use dovetail_core::evidence::{Session, SessionTyping};
use dovetail_core::finetype_ext::TypingRequest;
use dovetail_core::ossie::{project, read, write, Grade, Model, Provenance, Role, Verified};
use dovetail_core::packages::{read_packages, SourceTable};
use dovetail_core::roles::run_session;

const SCHEMA: &str = include_str!("../../../vendor/ossie/ossie-schema.json");
const GOLDEN: &str = "../../tests/ossie-fixtures/retail.sample.ossie.yaml";

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
        project(&roles, &tables, &session, "retail", &provenance),
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
    // The answer key calls categories a reference folded into products. Over the packages
    // fixture the roles score it an entity and fold nothing: the fixture leaves
    // `categories.name` unlabelled, so the table has no label column to be a reference by.
    // The projection writes the call the roles made.
    assert_eq!(categories.meta.role, Role::Entity);
    assert_eq!(categories.meta.folded_into, None);
    assert_eq!(categories.meta.fold_column, None);
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
    let m = project(&roles, &tables, &session, "m", &provenance);
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
    let m = project(&roles, &tables, &session, "empty", &provenance);
    let d = dataset(&m, "notes");
    assert_eq!(d.meta.role, Role::Unclear);
    assert_eq!(d.meta.rows, Some(0));
    assert_eq!(d.fields.len(), 2);
    assert_eq!(m.meta.typing.as_ref().unwrap().source, "floor");
    validate(&write(&m));
}

#[test]
fn golden_matches() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = retail(dir.path());
    let yaml = write(&m);
    validate(&yaml);
    assert_eq!(read(&yaml).unwrap(), m);

    let dir2 = tempfile::tempdir().unwrap();
    let (again, _) = retail(dir2.path());
    assert_eq!(write(&again), yaml, "project is deterministic");

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN);
    if std::env::var_os("DOVETAIL_WRITE_OSSIE_GOLDEN").is_some() {
        std::fs::write(&path, &yaml).unwrap();
    }
    let golden = std::fs::read_to_string(&path).expect("the golden exists");
    assert_eq!(yaml, golden);
    validate(&golden);
}
