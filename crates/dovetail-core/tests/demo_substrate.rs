//! The substrate the shipped end-to-end demo runs on, gated here rather than
//! only in the runner that drives it.
//!
//! `descriptor_stitch` drives a deliberately messy fixture: VARCHAR timestamps,
//! UUIDs, IP addresses. The demo's first scenario drives a different shape —
//! two tables, native `TIMESTAMP` columns, and one foreign key carried by an
//! INTEGER surrogate key. No test in this repo covered that shape, so a change
//! that reached only it would leave this repo green while the demo's first step
//! stopped producing the edge the rest of the pipeline is generated from.
//!
//! Two properties the demo depends on, and this file pins both:
//!
//! 1. `logins.account_id -> accounts.id` is discovered and **accepted** — the
//!    runner reads `x-dovetailStatus: accepted` out of the descriptor, and the
//!    generated pipeline's join step is compiled from it.
//! 2. The `TIMESTAMP` columns type as Frictionless `datetime`. The descriptor is
//!    the whole input to the generation step; a column that arrives as `string`
//!    changes what is generated downstream.
//!
//! The runner builds the CLI `--no-default-features`, so the demo's semantic
//! types come from the value-only classifier and no model is loaded. The test
//! asserts the same outcome under either feature set: `typing.rs` is model-free
//! on both, and a divergence between them is itself worth reddening.
//!
//! The fixture mirrors `arcform/tests/fixtures/build.sql`, the file the runner
//! loads. It is inlined rather than read from the sibling checkout: a test that
//! reaches across repositories passes or fails on whether that checkout is
//! present, which is not the property under test.

use dovetail_core::relate::{accepted, build_descriptor, constraint_ddl, discover, Edge};

/// The demo's two-table substrate: `accounts.id` is a uniquely-valued key,
/// `logins.account_id` references it with no orphan rows.
fn build_demo_fixture() -> duckdb::Connection {
    let conn = duckdb::Connection::open_in_memory().expect("open duckdb");
    conn.execute_batch(
        "CREATE TABLE accounts (
             id         INTEGER,
             email      VARCHAR,
             created_at TIMESTAMP
         );
         INSERT INTO accounts VALUES
             (1, 'ada@example.com',     '2026-01-02 09:00:00'),
             (2, 'alan@example.com',    '2026-01-03 10:30:00'),
             (3, 'grace@example.com',   '2026-01-05 14:15:00'),
             (4, 'edsger@example.com',  '2026-01-07 08:45:00'),
             (5, 'barbara@example.com', '2026-01-09 16:20:00');

         CREATE TABLE logins (
             id           INTEGER,
             account_id   INTEGER,
             logged_in_at TIMESTAMP
         );
         INSERT INTO logins VALUES
             (100, 1, '2026-01-02 09:05:00'),
             (101, 2, '2026-01-03 11:00:00'),
             (102, 1, '2026-01-04 07:30:00'),
             (103, 3, '2026-01-06 12:00:00'),
             (104, 5, '2026-01-10 18:00:00');",
    )
    .expect("build demo fixture");
    conn
}

/// One line per candidate edge: the evidence terms `score_edge` builds
/// `confidence` from, and the `orphanCount`, `childTotal`, `parentUnique` and
/// `semanticMismatch` that `assign_status` and `demote_on_mismatch` read
/// alongside it — what a failure here has to say in place of a bare absence.
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
                 semanticMismatch={} parentUnique={} orphanCount={} childTotal={}",
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
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Property 1: the one edge the demo's first scenario is generated from is
/// discovered, accepted, and the only accepted edge in the substrate.
#[test]
fn the_demo_substrate_yields_exactly_the_accepted_edge_the_pipeline_is_built_from() {
    let conn = build_demo_fixture();
    let edges = discover(&conn).expect("discover");

    let accepted_edges = accepted(&edges);
    let named: Vec<String> = accepted_edges
        .iter()
        .map(|e| format!("{} -> {}", e.child.qualified(), e.parent.qualified()))
        .collect();
    assert_eq!(
        named,
        vec!["logins.account_id -> accounts.id".to_string()],
        "relate scored:\n{}",
        edge_report(&edges)
    );

    // The descriptor is what the generation step reads, so the edge has to
    // arrive inside the child resource's Table Schema, not merely in `edges`.
    let descriptor = build_descriptor(&conn, &edges, "signups.duckdb").expect("descriptor");
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
    assert_eq!(fks.len(), 1, "logins foreignKeys: {logins}");
    let fk = &fks[0];
    assert_eq!(fk["fields"][0], "account_id");
    assert_eq!(fk["reference"]["resource"], "accounts");
    assert_eq!(fk["reference"]["fields"][0], "id");
    assert_eq!(
        fk["x-dovetailStatus"],
        "accepted",
        "the runner selects on this value; relate scored:\n{}",
        edge_report(&edges)
    );

    // And it compiles to constraint DDL naming both sides.
    let ddl = constraint_ddl(&edges);
    assert!(
        ddl.contains(r#"ALTER TABLE "logins""#) && ddl.contains(r#"REFERENCES "accounts" ("id")"#),
        "constraint DDL missing the demo's edge:\n{ddl}"
    );
}

/// Property 2: the demo's `TIMESTAMP` columns reach the descriptor as
/// Frictionless `datetime`, and the surrogate keys stay `integer`.
#[test]
fn the_demo_substrates_timestamp_columns_type_as_datetime() {
    let conn = build_demo_fixture();
    let edges = discover(&conn).expect("discover");
    let descriptor = build_descriptor(&conn, &edges, "signups.duckdb").expect("descriptor");

    let field = |resource: &str, name: &str| -> serde_json::Value {
        descriptor["resources"]
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
            .clone()
    };

    for (resource, column) in [("accounts", "created_at"), ("logins", "logged_in_at")] {
        let f = field(resource, column);
        assert_eq!(
            f["type"], "datetime",
            "{resource}.{column} must reach the descriptor as datetime, got {f}"
        );
    }

    // A surrogate key the value-only classifier declines to type keeps the SQL
    // family, and carries no semantic type it did not earn.
    for (resource, column) in [
        ("accounts", "id"),
        ("logins", "id"),
        ("logins", "account_id"),
    ] {
        let f = field(resource, column);
        assert_eq!(f["type"], "integer", "{resource}.{column} is {f}");
        assert!(
            f.get("x-dovetailSemanticType").is_none(),
            "{resource}.{column} is {f}"
        );
    }

    // The email column is the one the classifier does resolve, model-free.
    let email = field("accounts", "email");
    assert_eq!(email["type"], "string");
    assert_eq!(email["format"], "email");
    assert_eq!(email["x-dovetailSemanticType"], "identity.person.email");
}
