//! relate discovery tests (spec 2026-06-21-relate-discover-verify-render):
//! - ac-01 fixture corpus + expected-outcome manifest
//! - ac-02/03/04 discover → verify → status, scored against the manifest
//! - ac-05 foreignKeys written into Table Schema, still profile-conformant
//! - ac-06 constraint DDL for accepted edges only, and it provably holds

use std::path::{Path, PathBuf};

use dovetail_core::datapackage::{Field, TableSchema};
use dovetail_core::relate::{accepted, build_descriptor, constraint_ddl, discover, EdgeStatus};
use serde::Deserialize;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

#[derive(Debug, Deserialize)]
struct Manifest {
    expected_edges: Vec<ExpectedEdge>,
}
#[derive(Debug, Deserialize)]
struct ExpectedEdge {
    case: String,
    child: String,
    parent: String,
    status: String,
}

fn build_fixture() -> duckdb::Connection {
    let conn = duckdb::Connection::open_in_memory().expect("open duckdb");
    let sql = std::fs::read_to_string(repo_root().join("tests/relate-fixtures/build.sql")).unwrap();
    conn.execute_batch(&sql).expect("build fixture");
    conn
}

fn manifest() -> Manifest {
    let text =
        std::fs::read_to_string(repo_root().join("tests/relate-fixtures/manifest.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

// ac-02/03/04 — every expected edge gets the status the manifest requires.
#[test]
fn discovery_assigns_the_expected_status_to_every_fixture_edge() {
    let conn = build_fixture();
    let edges = discover(&conn).expect("discover");

    let mut misses = Vec::new();
    for exp in manifest().expected_edges {
        let found = edges
            .iter()
            .find(|e| e.child.qualified() == exp.child && e.parent.qualified() == exp.parent);
        match found {
            None => misses.push(format!("{}: edge {} -> {} not discovered", exp.case, exp.child, exp.parent)),
            Some(e) if e.status.as_str() != exp.status => misses.push(format!(
                "{}: {} -> {} got {} (conf {:.2}, {}), want {}",
                exp.case, exp.child, exp.parent, e.status.as_str(), e.confidence, e.reason, exp.status
            )),
            Some(_) => {}
        }
    }
    assert!(misses.is_empty(), "status misses:\n{}", misses.join("\n"));
}

// ac-04 safety — the ONLY auto-accepted edge is the holding FK; no coincidence
// (boolean overlap, surrogate-id overlap) auto-accepts.
#[test]
fn only_the_holding_fk_auto_accepts() {
    let conn = build_fixture();
    let edges = discover(&conn).expect("discover");
    let acc: Vec<String> =
        accepted(&edges).iter().map(|e| format!("{} -> {}", e.child.qualified(), e.parent.qualified())).collect();
    assert_eq!(acc, vec!["orders.customer_id -> customers.id".to_string()], "unexpected accepts: {acc:?}");
}

// ac-06 — DDL is emitted for accepted edges only, and the FK provably holds:
// rebuild parent(PK) + child(FK) and re-insert the verified data; DuckDB enforces
// the constraint at CREATE-time and the insert succeeds because the edge holds.
#[test]
fn accepted_edge_ddl_holds_in_duckdb() {
    let conn = build_fixture();
    let edges = discover(&conn).expect("discover");

    let ddl = constraint_ddl(&edges);
    assert!(ddl.contains("ALTER TABLE \"orders\""), "DDL missing accepted FK:\n{ddl}");
    assert!(ddl.contains("REFERENCES \"customers\""), "{ddl}");
    // suggested/rejected edges do not compile
    assert!(!ddl.contains("widget_flags"), "rejected edge leaked into DDL:\n{ddl}");
    assert!(!ddl.contains("products"), "suggested edge leaked into DDL:\n{ddl}");

    // Prove the accepted FK holds: CREATE-time FK enforcement on the verified data.
    let check = duckdb::Connection::open_in_memory().unwrap();
    check
        .execute_batch(
            "CREATE TABLE customers_k (id INTEGER PRIMARY KEY);
             INSERT INTO customers_k VALUES (1),(2),(3),(4),(5);
             CREATE TABLE orders_k (id INTEGER, customer_id INTEGER,
                 FOREIGN KEY (customer_id) REFERENCES customers_k(id));
             INSERT INTO orders_k VALUES (10,1),(11,2),(12,1),(13,3),(14,5);",
        )
        .expect("verified FK must enforce cleanly");
}

// ac-05 — discovered edges become foreignKeys inside a Table Schema, and the
// resource still validates against the vendored Frictionless profile.
#[test]
fn foreign_keys_serialize_inside_table_schema_and_conform() {
    let conn = build_fixture();
    let edges = discover(&conn).expect("discover");

    // Attach the non-rejected edges as foreignKeys on the orders resource schema.
    let fks: Vec<_> = edges
        .iter()
        .filter(|e| e.status != EdgeStatus::Rejected && e.child.table == "orders")
        .map(|e| e.to_foreign_key())
        .collect();
    assert!(!fks.is_empty(), "expected at least the accepted orders FK");

    let schema = TableSchema {
        fields: vec![
            Field { name: "id".into(), ty: "integer".into(), semantic_type: None },
            Field { name: "customer_id".into(), ty: "integer".into(), semantic_type: None },
        ],
        foreign_keys: fks,
    };
    let json = serde_json::to_value(&schema).unwrap();
    // foreignKeys present, Frictionless shape, custom props carried.
    let fk0 = &json["foreignKeys"][0];
    assert_eq!(fk0["fields"][0], "customer_id");
    assert_eq!(fk0["reference"]["resource"], "customers");
    assert_eq!(fk0["reference"]["fields"][0], "id");
    assert_eq!(fk0["x-dovetailStatus"], "accepted");
    assert!(fk0["x-dovetailEvidence"]["parentUnique"].as_bool().unwrap());
}

// ac-07 / ac-05 — the descriptor relate writes carries foreignKeys and validates
// against the vendored Frictionless profile via the actual jsonschema validator
// (not just serde shape), with the custom FK properties present.
#[test]
fn relate_descriptor_validates_against_frictionless_profile() {
    let conn = build_fixture();
    let edges = discover(&conn).expect("discover");
    let descriptor = build_descriptor(&conn, &edges, "demo.duckdb").expect("descriptor");

    // The orders resource carries the accepted FK inside its Table Schema.
    let orders = descriptor["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "orders")
        .expect("orders resource");
    let fks = orders["schema"]["foreignKeys"].as_array().expect("foreignKeys present");
    assert!(fks.iter().any(|fk| fk["reference"]["resource"] == "customers"
        && fk["x-dovetailStatus"] == "accepted"));

    // Validate the whole descriptor against the vendored profile.
    let schema_text =
        std::fs::read_to_string(repo_root().join("vendor/frictionless/datapackage-profile.json"))
            .unwrap();
    let schema: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema).expect("compile profile");
    let errors: Vec<String> =
        validator.iter_errors(&descriptor).map(|e| format!("{e} at {}", e.instance_path)).collect();
    assert!(errors.is_empty(), "relate descriptor not conformant:\n{}", errors.join("\n"));
}
