//! The two passes: the sample pass proposes a model, and the full pass verifies that
//! proposal on every row, rewriting only grades, evidence, warnings, row counts and keys
//! that do not hold, or discovers afresh when there is nothing current to verify.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dovetail_core::evidence::{Session, SessionTyping};
use dovetail_core::finetype_ext::TypingRequest;
use dovetail_core::ossie::{
    project, read, run_model, verify_full, write, Grade, Model, ModelRequest, Provenance, Role,
    Verified,
};
use dovetail_core::packages::SourceTable;
use dovetail_core::roles::run_session;

fn plain(name: &str) -> SourceTable {
    SourceTable {
        name: name.to_string(),
        relation: format!("raw_{name}"),
        source_file: None,
        source_part: None,
        fields: Vec::new(),
    }
}

/// 2,000 customers, each key once; 20,000 orders, each pointing at a customer.
const CLEAN: &str = "
    CREATE TABLE customers AS
        SELECT i AS customer_id, 'customer ' || i AS customer_name FROM range(1, 2001) t(i);
    CREATE TABLE orders AS
        SELECT i AS order_id, (i % 2000) + 1 AS customer_id FROM range(1, 20001) t(i);";

/// The same, after 2,000 orders pointing at no customer and a second customer 7
/// arrived.
const BROKEN: &str = "
    CREATE TABLE customers AS
        SELECT i AS customer_id, 'customer ' || i AS customer_name FROM range(1, 2001) t(i)
        UNION ALL SELECT 7, 'customer 7 again';
    CREATE TABLE orders AS
        SELECT i AS order_id, (i % 2000) + 1 AS customer_id FROM range(1, 20001) t(i)
        UNION ALL SELECT i, i + 5000 FROM range(20001, 22001) t(i);";

/// The sample pass's proposal over `db`'s customers and orders, from 500-row samples.
fn sample_proposal(db: &Path) -> Model {
    let tables = [plain("customers"), plain("orders")];
    let s = Session::over(db, &tables, Some(500), SessionTyping::Packages).unwrap();
    let roles = run_session(&s).unwrap();
    let provenance = Provenance {
        pass: Verified::Sample,
        typing: roles.typing.clone(),
        sample_rows: Some(500),
    };
    project(&roles, &tables, &s, "shop", &provenance).unwrap()
}

/// What the analyst is looking at: every name, role, fold, combination and join, in
/// order.
fn layout(m: &Model) -> Vec<String> {
    let mut out = Vec::new();
    for d in &m.datasets {
        let fields: Vec<&str> = d.fields.iter().map(|f| f.name.as_str()).collect();
        out.push(format!(
            "dataset {} source {} role {:?} fold {:?}/{:?} combined_into {:?} combination {:?} fields {fields:?}",
            d.name,
            d.source,
            d.meta.role,
            d.meta.folded_into,
            d.meta.fold_column,
            d.meta.combined_into,
            d.meta.combination,
        ));
    }
    for r in &m.relationships {
        out.push(format!(
            "join {} {}.{:?} -> {}.{:?}",
            r.name, r.from, r.from_columns, r.to, r.to_columns
        ));
    }
    out
}

#[test]
fn full_pass_rearranges_nothing() {
    // A sampled session measures a parent's key on its full table, so one database
    // cannot hold a join whose sample is clean beside a parent key the full data
    // repeats. The full data here is the sample's database after more rows arrived.
    let dir = tempfile::tempdir().unwrap();
    let clean = build_raw_db_from(&subdir(dir.path(), "clean"), CLEAN);
    let broken = build_raw_db_from(&subdir(dir.path(), "broken"), BROKEN);

    let sample = sample_proposal(&clean);
    let customers = |m: &Model| m.datasets.iter().find(|d| d.name == "customers").cloned();
    let join = |m: &Model| {
        m.relationships
            .iter()
            .find(|r| r.from == "orders" && r.to == "customers")
            .cloned()
            .expect("the orders to customers join")
    };
    assert_eq!(join(&sample).meta.grade, Grade::Likely);
    assert_eq!(join(&sample).meta.verified, Verified::Sample);
    assert_eq!(customers(&sample).unwrap().primary_key, ["customer_id"]);

    let tables = [plain("customers"), plain("orders")];
    let s = Session::over(&broken, &tables, None, SessionTyping::Packages).unwrap();
    let full = verify_full(sample.clone(), &s);

    assert_eq!(layout(&full), layout(&sample));
    assert!(full
        .relationships
        .iter()
        .all(|r| r.meta.verified == Verified::Full));
    assert_eq!(full.meta.pass, Some(Verified::Full));

    let j = join(&full);
    assert_eq!(j.meta.grade, Grade::Possible);
    assert_eq!(j.meta.evidence.orphans, 2000);
    assert_eq!(j.meta.evidence.child_rows, 22000);
    assert_eq!(j.meta.evidence.parent_duplicates, 1);
    let warning = j.meta.warning.expect("a warning");
    assert!(warning.contains("2,000 of 22,000"), "{warning}");
    assert!(warning.contains("customers"), "{warning}");

    let c = customers(&full).unwrap();
    assert!(c.primary_key.is_empty(), "{:?}", c.primary_key);
    assert_eq!(c.meta.role, customers(&sample).unwrap().meta.role);
    assert_eq!(
        c.meta.role_evidence.last().map(String::as_str),
        Some("customer_id repeats in 2 rows of the full table")
    );
    assert_eq!(c.meta.rows, Some(2001));
    let orders = full.datasets.iter().find(|d| d.name == "orders").unwrap();
    assert_eq!(orders.meta.rows, Some(22000));
}

/// A new directory `name` inside `dir`.
fn subdir(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn request(db: &Path, pass: Verified, verify: Option<PathBuf>) -> ModelRequest {
    ModelRequest {
        db: db.to_path_buf(),
        packages: Some(retail_packages_dir()),
        pass,
        base: None,
        verify,
        name: "retail".to_string(),
        typing: TypingRequest::Floor,
    }
}

/// The retail joins a discovery finds: the ones the golden sample model holds.
const RETAIL_JOINS: [&str; 4] = [
    "customer_segments_customers",
    "loyalty_visits_customers",
    "loyalty_visits_stores",
    "price_list_products",
];

fn assert_discovered_on_full(m: &Model) {
    let names: Vec<&str> = m.relationships.iter().map(|r| r.name.as_str()).collect();
    for j in RETAIL_JOINS {
        assert!(names.contains(&j), "{j} not in {names:?}");
    }
    assert!(m
        .relationships
        .iter()
        .all(|r| r.meta.verified == Verified::Full));
    assert_eq!(m.meta.pass, Some(Verified::Full));
    assert_eq!(m.meta.sample_rows, None);
    assert!(m.datasets.iter().any(|d| d.meta.role != Role::Unclear));
}

/// The retail sample pass's proposal, written to `dir`.
fn retail_sample(dir: &Path, db: &Path) -> (Model, PathBuf) {
    let (m, warnings) = run_model(&request(db, Verified::Sample, None)).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let path = dir.join("sample.ossie.yaml");
    std::fs::write(&path, write(&m)).unwrap();
    (m, path)
}

#[test]
fn full_pass_verifies_the_sample() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_retail_db(dir.path());
    let (sample, path) = retail_sample(dir.path(), &db);
    let (full, warnings) = run_model(&request(&db, Verified::Full, Some(path))).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(layout(&full), layout(&sample));
    assert!(full
        .relationships
        .iter()
        .all(|r| r.meta.verified == Verified::Full));
    // The retail tables are smaller than a sample: nothing the full data says differs.
    let grades =
        |m: &Model| -> Vec<Grade> { m.relationships.iter().map(|r| r.meta.grade).collect() };
    assert_eq!(grades(&full), grades(&sample));
    // Every join was measured, those to a combined dataset through its query too.
    assert!(
        full.relationships.iter().all(|r| r.meta.warning.is_none()),
        "{:#?}",
        full.relationships
    );
    assert!(full
        .relationships
        .iter()
        .any(|r| r.to == "price_list" || r.from == "price_list"));
    let rows = |m: &Model| -> Vec<Option<u64>> { m.datasets.iter().map(|d| d.meta.rows).collect() };
    assert_eq!(rows(&full), rows(&sample));
}

#[test]
fn full_pass_without_verify_discovers() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_retail_db(dir.path());
    let (m, warnings) = run_model(&request(&db, Verified::Full, None)).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_discovered_on_full(&m);
}

#[test]
fn full_pass_on_standin_discovers() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_retail_db(dir.path());
    let (mut standin, _) = retail_sample(dir.path(), &db);
    // Brightfield's stand-in: the same datasets, every one unclear, no joins.
    standin.meta.generator = "brightfield stand-in".to_string();
    standin.relationships.clear();
    for d in &mut standin.datasets {
        d.meta.role = Role::Unclear;
        d.meta.role_evidence.clear();
        d.primary_key.clear();
    }
    let path = dir.path().join("standin.ossie.yaml");
    std::fs::write(&path, write(&standin)).unwrap();
    let (m, warnings) = run_model(&request(&db, Verified::Full, Some(path))).unwrap();
    assert_discovered_on_full(&m);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("brightfield stand-in"), "{warnings:?}");
}

#[test]
fn full_pass_on_stale_proposal_discovers() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_retail_db(dir.path());
    let (mut stale, _) = retail_sample(dir.path(), &db);
    stale.datasets.retain(|d| d.name != "stores");
    let path = dir.path().join("stale.ossie.yaml");
    std::fs::write(&path, write(&stale)).unwrap();
    let (m, warnings) = run_model(&request(&db, Verified::Full, Some(path))).unwrap();
    assert_discovered_on_full(&m);
    assert!(m.datasets.iter().any(|d| d.name == "stores"));
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("stores"), "{warnings:?}");
}

#[test]
fn full_pass_on_garbage_discovers() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_retail_db(dir.path());
    let path = dir.path().join("garbage.ossie.yaml");
    std::fs::write(&path, "not: [yaml").unwrap();
    let (m, warnings) = run_model(&request(&db, Verified::Full, Some(path))).unwrap();
    assert_discovered_on_full(&m);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("garbage.ossie.yaml"), "{warnings:?}");
}

#[test]
fn sample_pass_says_sample() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_raw_retail_db(dir.path());
    let run = || write(&run_model(&request(&db, Verified::Sample, None)).unwrap().0);
    let first = run();
    assert_eq!(first, run(), "two sample runs give the same bytes");
    let m = read(&first).unwrap();
    assert!(!m.relationships.is_empty());
    assert!(m
        .relationships
        .iter()
        .all(|r| r.meta.verified == Verified::Sample));
    assert_eq!(m.meta.pass, Some(Verified::Sample));
}
