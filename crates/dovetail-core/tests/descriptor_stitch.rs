//! The self-assembling descriptor: one flow, one Frictionless descriptor that
//! carries BOTH halves of dovetail's model —
//!   (a) fields typed with finetype semantic types (not coarse all-string), and
//!   (b) discovered foreignKeys with the evidence + confidence relate computes.
//!
//! Before this stitch these were two never-merged paths: survey typed fields but
//! emitted no foreignKeys, relate emitted foreignKeys but typed fields coarsely
//! from the SQL information_schema. This test drives the merged flow — relate over
//! a messy multi-table database — and asserts one descriptor holds both, still
//! conformant to the vendored Frictionless profile.

use std::path::{Path, PathBuf};

use dovetail_core::relate::{build_descriptor, discover};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// A messy, multi-table fixture: value-conclusive semantic columns (email, ISO
/// timestamps, UUIDs, IPs) plus a holding foreign-key relationship
/// (`logins.account_id` ⊆ the unique `accounts.id`).
fn build_messy_fixture() -> duckdb::Connection {
    let conn = duckdb::Connection::open_in_memory().expect("open duckdb");
    conn.execute_batch(
        "CREATE TABLE accounts (
             id           INTEGER,
             email        VARCHAR,
             account_uuid VARCHAR,
             opened_at    VARCHAR
         );
         INSERT INTO accounts VALUES
             (1, 'ada@example.com',    '550e8400-e29b-41d4-a716-446655440000', '2024-01-15T14:30:00Z'),
             (2, 'grace@navy.mil',     '6ba7b810-9dad-11d1-80b4-00c04fd430c8', '2024-02-20T09:00:00Z'),
             (3, 'alan@bletchley.uk',  '6ba7b811-9dad-11d1-80b4-00c04fd430c8', '2024-03-01T18:45:00Z'),
             (4, 'edsger@utexas.edu',  '6ba7b812-9dad-11d1-80b4-00c04fd430c8', '2024-04-10T07:15:00Z'),
             (5, 'barbara@nasa.gov',   '6ba7b813-9dad-11d1-80b4-00c04fd430c8', '2024-05-05T12:00:00Z');

         CREATE TABLE logins (
             id          INTEGER,
             account_id  INTEGER,
             ip_address  VARCHAR,
             logged_in_at VARCHAR
         );
         INSERT INTO logins VALUES
             (10, 1, '192.168.1.10', '2024-06-01T08:00:00Z'),
             (11, 2, '10.0.0.4',     '2024-06-01T09:30:00Z'),
             (12, 1, '172.16.0.9',   '2024-06-02T14:20:00Z'),
             (13, 3, '192.168.1.55', '2024-06-03T11:11:00Z'),
             (14, 5, '10.0.0.7',     '2024-06-04T16:45:00Z');",
    )
    .expect("build messy fixture");
    conn
}

/// Find a resource's field by name in the assembled descriptor.
fn field<'a>(desc: &'a serde_json::Value, resource: &str, name: &str) -> &'a serde_json::Value {
    desc["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == resource)
        .unwrap_or_else(|| panic!("resource {resource} missing"))["schema"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap_or_else(|| panic!("field {resource}.{name} missing"))
}

// The stitch: ONE relate flow yields ONE descriptor whose fields carry finetype
// semantic types AND whose schema carries evidence-bearing foreignKeys.
#[test]
fn one_flow_yields_semantic_types_and_evidence_bearing_foreign_keys() {
    let conn = build_messy_fixture();

    // Single flow: discover the relationships, then assemble the descriptor.
    let edges = discover(&conn).expect("discover");
    let descriptor = build_descriptor(&conn, &edges, "signups.duckdb").expect("descriptor");

    // --- (a) fields carry finetype SEMANTIC types, not coarse all-string --------

    // Email → Frictionless string/email, semantic type recorded.
    let email = field(&descriptor, "accounts", "email");
    assert_eq!(email["type"], "string");
    assert_eq!(email["format"], "email");
    assert_eq!(email["x-dovetailSemanticType"], "identity.person.email");

    // ISO timestamp → Frictionless `datetime` — a genuinely non-string type the
    // coarse SQL-family mapping (VARCHAR → string) could never have produced.
    let opened = field(&descriptor, "accounts", "opened_at");
    assert_eq!(
        opened["type"], "datetime",
        "ISO timestamp must type as datetime, got {opened}"
    );
    assert!(opened["x-dovetailSemanticType"]
        .as_str()
        .unwrap()
        .starts_with("datetime."));

    // UUID → string/uuid with the semantic leaf recorded.
    let uuid = field(&descriptor, "accounts", "account_uuid");
    assert_eq!(uuid["format"], "uuid");
    assert_eq!(
        uuid["x-dovetailSemanticType"],
        "representation.identifier.uuid"
    );

    // A plain surrogate key finetype cannot conclusively type stays `integer`
    // (SQL-family fallback), never mistyped — the id is still a clean integer.
    let id = field(&descriptor, "accounts", "id");
    assert_eq!(id["type"], "integer");
    assert!(id.get("x-dovetailSemanticType").is_none());

    // Proof of the "not all-string" claim: at least one field is semantically
    // typed to something other than string.
    let semantic_nonstring = descriptor["resources"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["schema"]["fields"].as_array().unwrap())
        .any(|f| f.get("x-dovetailSemanticType").is_some() && f["type"] != "string");
    assert!(
        semantic_nonstring,
        "expected at least one non-string finetype semantic field"
    );

    // --- (b) the SAME descriptor carries evidence-bearing foreignKeys -----------

    let logins = descriptor["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "logins")
        .expect("logins resource");
    let fks = logins["schema"]["foreignKeys"]
        .as_array()
        .expect("foreignKeys present");
    let fk = fks
        .iter()
        .find(|fk| fk["reference"]["resource"] == "accounts")
        .expect("logins.account_id -> accounts.id foreign key");

    assert_eq!(fk["fields"][0], "account_id");
    assert_eq!(fk["reference"]["fields"][0], "id");
    assert_eq!(fk["x-dovetailStatus"], "accepted");
    // Confidence + evidence ride on the foreign key.
    assert!(fk["x-dovetailConfidence"].as_f64().unwrap() > 0.0);
    let ev = &fk["x-dovetailEvidence"];
    assert!(
        ev["parentUnique"].as_bool().unwrap(),
        "accounts.id is the unique parent key"
    );
    assert_eq!(
        ev["orphanCount"].as_i64().unwrap(),
        0,
        "the FK holds — no orphans"
    );
    assert!(ev["valueOverlap"].as_f64().unwrap() > 0.0);
    assert!(ev["nameSimilarity"].as_f64().unwrap() > 0.0);

    // --- one descriptor, still Frictionless-conformant --------------------------

    let schema_text =
        std::fs::read_to_string(repo_root().join("vendor/frictionless/datapackage-profile.json"))
            .unwrap();
    let schema: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema).expect("compile profile");
    let errors: Vec<String> = validator
        .iter_errors(&descriptor)
        .map(|e| format!("{e} at {}", e.instance_path))
        .collect();
    assert!(
        errors.is_empty(),
        "stitched descriptor not conformant:\n{}",
        errors.join("\n")
    );
}
