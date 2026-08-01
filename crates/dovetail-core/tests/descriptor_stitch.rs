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

use dovetail_core::relate::{build_descriptor, discover, Edge, CONF_HIGH};

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

/// One line per candidate edge relate scored: the evidence terms `score_edge`
/// builds `confidence` from, and the terms `assign_status` and
/// `demote_on_mismatch` read alongside it.
///
/// A missing foreign key is reported by its absence, which says the key is not
/// there and stops. Whether it is there depends on `confidence` and on the
/// `orphanCount`, `childTotal`, `parentUnique` and `semanticMismatch` those two
/// functions read, so a bare absence leaves a reader to guess which of them
/// moved.
fn edge_report(edges: &[Edge]) -> String {
    if edges.is_empty() {
        return "  relate scored no candidate edge at all".to_string();
    }
    edges
        .iter()
        .map(|e| {
            format!(
                "  {} -> {}  status={} confidence={:.3} nameSimilarity={:.3} \
                 semanticSimilarity={:.3} valueOverlap={:.3} parentKeyLikeness={:.3} \
                 semanticMismatch={} parentUnique={} orphanCount={} childTotal={} \
                 childType={:?} parentType={:?}",
                e.child.qualified(),
                e.parent.qualified(),
                e.status.as_str(),
                e.confidence,
                e.evidence.name_similarity,
                e.evidence.semantic_similarity,
                e.evidence.value_overlap,
                e.evidence.parent_key_likeness,
                e.evidence.semantic_mismatch,
                e.verification.parent_unique,
                e.verification.orphan_count,
                e.verification.child_total,
                e.evidence.child_semantic_type,
                e.evidence.parent_semantic_type,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
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
        .unwrap_or_else(|| {
            panic!(
                "logins carries no foreignKeys; relate scored:\n{}",
                edge_report(&edges)
            )
        });
    let fk = fks
        .iter()
        .find(|fk| fk["reference"]["resource"] == "accounts")
        .unwrap_or_else(|| {
            panic!(
                "no logins.account_id -> accounts.id foreign key; relate scored:\n{}",
                edge_report(&edges)
            )
        });

    assert_eq!(fk["fields"][0], "account_id");
    assert_eq!(fk["reference"]["fields"][0], "id");
    assert_eq!(
        fk["x-dovetailStatus"],
        "accepted",
        "relate scored:\n{}",
        edge_report(&edges)
    );

    // Confidence + evidence ride on the foreign key, and this fixture determines
    // each term exactly: the child column is spelled `<parent singular>_id`, its
    // four distinct values are all present in the parent, and the parent is a
    // uniquely-valued column named `id`. Pinning the terms rather than asserting
    // each is above zero is what makes a shifted score report which input moved.
    // The descriptor rounds them to two places, so the comparisons are exact.
    let ev = &fk["x-dovetailEvidence"];
    let terms: [(&str, f64); 5] = [
        (
            "x-dovetailConfidence",
            fk["x-dovetailConfidence"].as_f64().unwrap(),
        ),
        ("nameSimilarity", ev["nameSimilarity"].as_f64().unwrap()),
        (
            "semanticSimilarity",
            ev["semanticSimilarity"].as_f64().unwrap(),
        ),
        ("valueOverlap", ev["valueOverlap"].as_f64().unwrap()),
        (
            "parentKeyLikeness",
            ev["parentKeyLikeness"].as_f64().unwrap(),
        ),
    ];
    let expected: [(&str, f64); 5] = [
        ("x-dovetailConfidence", 0.95),
        ("nameSimilarity", 0.9),
        ("semanticSimilarity", 0.0),
        ("valueOverlap", 1.0),
        ("parentKeyLikeness", 1.0),
    ];
    assert_eq!(
        terms,
        expected,
        "the scoring terms this fixture determines have moved; relate scored:\n{}",
        edge_report(&edges)
    );
    assert!(
        fk["x-dovetailConfidence"].as_f64().unwrap() >= CONF_HIGH,
        "this fixture's edge must clear the auto-accept gate; relate scored:\n{}",
        edge_report(&edges)
    );
    assert!(
        ev["parentUnique"].as_bool().unwrap(),
        "accounts.id is the unique parent key"
    );
    assert_eq!(
        ev["orphanCount"].as_i64().unwrap(),
        0,
        "the FK holds — no orphans"
    );

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
