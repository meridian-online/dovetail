//! relate discovers edges from finetype's types, not from column-name spelling.
//!
//! The fixture is a set of registries and a link table whose join keys carry
//! **raw registry names** — `lei`, `isin`, `entity_ref` — none of which ends in
//! `id` / `_id` / `_key` / `code`. Under name-spelling alone every one of them
//! scores 0.44 against a 0.60 auto-accept line, so the relationships are only
//! discovered if someone renames the columns first.
//!
//! ## The control
//!
//! Every test here has a paired run against `SemanticTypes::default()` — an
//! empty typing. Because the feature fails closed, an empty typing reproduces
//! *exactly* the old scoring: no key-likeness floor, no shared-type term, no
//! demotion. So the same fixture, the same code path and the same assertions
//! give the before-and-after in one run, and a change that applied the floor
//! unconditionally would redden here rather than pass quietly.

use std::path::{Path, PathBuf};

use dovetail_core::relate::{
    accepted, build_descriptor_with_types, discover_with_types, run_path, semantic_types,
    EdgeStatus, SemanticTypes, CONF_HIGH,
};
use serde::Deserialize;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn fixture_sql() -> String {
    std::fs::read_to_string(repo_root().join("tests/relate-fixtures/identifier-keys.build.sql"))
        .expect("identifier-keys fixture")
}

fn build_fixture() -> duckdb::Connection {
    let conn = duckdb::Connection::open_in_memory().expect("open duckdb");
    conn.execute_batch(&fixture_sql()).expect("build identifier-keys fixture");
    conn
}

#[derive(Debug, Deserialize)]
struct Manifest {
    expected_edges: Vec<ExpectedEdge>,
    accepted_edges: Vec<String>,
}
#[derive(Debug, Deserialize)]
struct ExpectedEdge {
    case: String,
    child: String,
    parent: String,
    status: String,
    status_before: String,
}

fn manifest() -> Manifest {
    let text = std::fs::read_to_string(
        repo_root().join("tests/relate-fixtures/identifier-keys.manifest.json"),
    )
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// Every candidate edge in the fixture gets the status the manifest requires —
/// and, run against an empty typing, the status the manifest records as the
/// behaviour before typing reached the gate.
#[test]
fn identifier_typing_moves_every_fixture_edge_to_its_expected_status() {
    let conn = build_fixture();
    let types = semantic_types(&conn).expect("type the columns");
    let after = discover_with_types(&conn, &types).expect("discover");
    let before = discover_with_types(&conn, &SemanticTypes::default()).expect("discover, untyped");

    let status_of = |edges: &[dovetail_core::relate::Edge], child: &str, parent: &str| {
        edges
            .iter()
            .find(|e| e.child.qualified() == child && e.parent.qualified() == parent)
            .map_or_else(|| "not discovered".to_string(), |e| e.status.as_str().to_string())
    };

    let mut misses = Vec::new();
    for exp in manifest().expected_edges {
        let got_after = status_of(&after, &exp.child, &exp.parent);
        if got_after != exp.status {
            misses.push(format!(
                "{}: {} -> {} is {got_after}, want {}",
                exp.case, exp.child, exp.parent, exp.status
            ));
        }
        let got_before = status_of(&before, &exp.child, &exp.parent);
        if got_before != exp.status_before {
            misses.push(format!(
                "{}: without typing, {} -> {} is {got_before}, want {} — the control has moved, \
                 so the after-status is no longer evidence of anything",
                exp.case, exp.child, exp.parent, exp.status_before
            ));
        }
    }
    assert!(misses.is_empty(), "status misses:\n{}", misses.join("\n"));
}

/// The complete accepted set, both ways round. Naming the whole set is what
/// stops a floor that is too generous from passing: a new accept anywhere in the
/// fixture reddens this.
#[test]
fn the_accepted_set_is_exactly_the_manifests_and_is_empty_without_typing() {
    let conn = build_fixture();
    let types = semantic_types(&conn).expect("type the columns");

    let mut got: Vec<String> = accepted(&discover_with_types(&conn, &types).unwrap())
        .iter()
        .map(|e| format!("{} -> {}", e.child.qualified(), e.parent.qualified()))
        .collect();
    got.sort();
    assert_eq!(got, manifest().accepted_edges, "accepted set differs from the manifest");

    let untyped = discover_with_types(&conn, &SemanticTypes::default()).unwrap();
    let untyped_accepts: Vec<String> = accepted(&untyped)
        .iter()
        .map(|e| format!("{} -> {}", e.child.qualified(), e.parent.qualified()))
        .collect();
    assert!(
        untyped_accepts.is_empty(),
        "these raw-named keys must NOT auto-discover without typing — if they do, \
         the fixture is not testing what it claims: {untyped_accepts:?}"
    );
}

/// A semantic mismatch DEMOTES. All three accept gates pass — zero orphans, a
/// unique parent, confidence over the line — and the edge is `Suggested`, still
/// present, carrying both types and a reason that names them.
#[test]
fn a_semantic_mismatch_demotes_to_suggested_and_never_prunes() {
    let conn = build_fixture();
    let types = semantic_types(&conn).expect("type the columns");
    let edges = discover_with_types(&conn, &types).expect("discover");

    let edge = edges
        .iter()
        .find(|e| {
            e.child.qualified() == "mixed_ref.security_ref"
                && e.parent.qualified() == "instrument_ref.security_ref"
        })
        .expect("the mismatched edge must still be present — demotion, not pruning");

    assert!(edge.verification.integrity_holds(), "gate 1: {} orphans", edge.verification.orphan_count);
    assert!(edge.verification.parent_unique, "gate 2: parent must be unique");
    assert!(
        edge.confidence >= CONF_HIGH,
        "gate 3: confidence {:.3} must clear {CONF_HIGH}",
        edge.confidence
    );
    assert_eq!(
        edge.status,
        EdgeStatus::Suggested,
        "every accept gate passes, so only the type disagreement can be holding this back"
    );

    assert!(edge.evidence.semantic_mismatch);
    assert_eq!(edge.evidence.child_semantic_type.as_deref(), Some("finance.securities.lei"));
    assert_eq!(edge.evidence.parent_semantic_type.as_deref(), Some("finance.securities.isin"));
    assert_eq!(edge.evidence.semantic_similarity, 0.0);
    assert!(
        edge.reason.contains("finance.securities.lei")
            && edge.reason.contains("finance.securities.isin"),
        "the reason must name both types, or the demotion is invisible: {}",
        edge.reason
    );

    // Not pruned means it reaches the artifact an analyst reads.
    let descriptor = build_descriptor_with_types(&conn, &edges, "fixture.duckdb", &types).unwrap();
    let fks = descriptor["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "mixed_ref")
        .expect("mixed_ref resource")["schema"]["foreignKeys"]
        .as_array()
        .expect("the demoted edge is written to the descriptor");
    let fk = fks
        .iter()
        .find(|fk| fk["reference"]["resource"] == "instrument_ref")
        .expect("demoted foreign key present");
    assert_eq!(fk["x-dovetailStatus"], "suggested");
    assert_eq!(fk["x-dovetailEvidence"]["semanticMismatch"], true);
    assert_eq!(fk["x-dovetailEvidence"]["childSemanticType"], "finance.securities.lei");
    assert_eq!(fk["x-dovetailEvidence"]["parentSemanticType"], "finance.securities.isin");
}

/// A pair whose names share nothing and whose types share everything is scored,
/// not pruned — and the shared-type term is what carries it, not the naming.
#[test]
fn columns_with_unrelated_names_but_one_identifier_type_are_rewarded() {
    let conn = build_fixture();
    let types = semantic_types(&conn).expect("type the columns");
    let edges = discover_with_types(&conn, &types).expect("discover");

    let edge = edges
        .iter()
        .find(|e| {
            e.child.qualified() == "filings.entity_ref"
                && e.parent.qualified() == "entity_registry.lei"
        })
        .expect("edge must be discovered");

    assert_eq!(edge.status, EdgeStatus::Accepted);
    assert_eq!(
        edge.evidence.name_similarity, 0.0,
        "`entity_ref` and `lei` share no substring — if this is non-zero the test proves nothing"
    );
    assert_eq!(edge.evidence.semantic_similarity, 1.0);
    assert_eq!(edge.evidence.child_semantic_type.as_deref(), Some("finance.securities.lei"));
    assert_eq!(edge.evidence.parent_semantic_type.as_deref(), Some("finance.securities.lei"));

    let untyped = discover_with_types(&conn, &SemanticTypes::default()).unwrap();
    assert!(
        !untyped.iter().any(|e| e.child.qualified() == "filings.entity_ref"
            && e.parent.qualified() == "entity_registry.lei"),
        "without typing this pair is dropped by the candidate prune, never scored"
    );
}

/// One semantic type per column per run. Every `x-dovetailSemanticType` the
/// descriptor publishes for a column is byte-identical to the type the gate
/// scored that same column on, as recorded in the foreign-key evidence.
///
/// Driven through `run_path` — the whole CLI path, not a hand-assembled pair of
/// calls — because the disagreement this rules out was an ordering bug in
/// `run_path` itself.
#[test]
fn the_descriptor_and_the_gate_agree_about_every_column() {
    let db = std::env::temp_dir()
        .join(format!("dovetail-identifier-keys-{}-{}.duckdb", std::process::id(), line!()));
    let _ = std::fs::remove_file(&db);
    {
        let conn = duckdb::Connection::open(&db).expect("create db file");
        conn.execute_batch(&fixture_sql()).expect("build fixture");
    }

    let run = run_path(db.to_str().unwrap()).expect("relate run");

    // What the descriptor publishes, per column.
    let mut published: std::collections::HashMap<(String, String), Option<String>> =
        std::collections::HashMap::new();
    for resource in run.descriptor["resources"].as_array().unwrap() {
        let table = resource["name"].as_str().unwrap().to_string();
        for field in resource["schema"]["fields"].as_array().unwrap() {
            published.insert(
                (table.clone(), field["name"].as_str().unwrap().to_string()),
                field
                    .get("x-dovetailSemanticType")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            );
        }
    }
    assert!(!published.is_empty(), "descriptor published no fields");

    // What the gate scored on, per column, read back out of the edges.
    let mut disagreements = Vec::new();
    let mut compared = 0usize;
    for edge in &run.edges {
        for (col, scored_on) in [
            (&edge.child, &edge.evidence.child_semantic_type),
            (&edge.parent, &edge.evidence.parent_semantic_type),
        ] {
            let key = (col.table.clone(), col.column.clone());
            let published = published.get(&key).unwrap_or_else(|| {
                panic!("{} is scored but not described", col.qualified())
            });
            compared += 1;
            if published != scored_on {
                disagreements.push(format!(
                    "{}: descriptor says {published:?}, the gate scored on {scored_on:?}",
                    col.qualified()
                ));
            }
        }
    }
    let _ = std::fs::remove_file(&db);

    assert!(compared > 0, "no columns compared — the run discovered nothing");
    assert!(
        disagreements.is_empty(),
        "the descriptor disagrees with the gate about {} of {compared} column readings:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );

    // And the typing is doing something: at least the identifier columns resolved.
    assert!(
        published
            .iter()
            .filter(|(_, v)| v.as_deref() == Some("finance.securities.lei"))
            .count()
            >= 3,
        "expected the LEI-bearing columns to type as LEI: {published:#?}"
    );
}

/// The raw registry names auto-accept with no rename — the whole point — and the
/// evidence written to the descriptor says so in a form an analyst can audit.
#[test]
fn raw_registry_names_auto_accept_with_no_rename() {
    let conn = build_fixture();
    let types = semantic_types(&conn).expect("type the columns");
    let edges = discover_with_types(&conn, &types).expect("discover");
    let descriptor = build_descriptor_with_types(&conn, &edges, "fixture.duckdb", &types).unwrap();

    let link = descriptor["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "link")
        .expect("link resource");
    let fks = link["schema"]["foreignKeys"].as_array().expect("foreignKeys present");

    for (child_col, parent_table, leaf) in [
        ("lei", "entity_registry", "finance.securities.lei"),
        ("isin", "security_registry", "finance.securities.isin"),
    ] {
        let fk = fks
            .iter()
            .find(|fk| fk["fields"][0] == child_col && fk["reference"]["resource"] == parent_table)
            .unwrap_or_else(|| panic!("no discovered key for link.{child_col}"));
        assert_eq!(fk["x-dovetailStatus"], "accepted", "link.{child_col} must auto-accept");
        let ev = &fk["x-dovetailEvidence"];
        assert_eq!(ev["childSemanticType"], leaf);
        assert_eq!(ev["parentSemanticType"], leaf);
        assert_eq!(ev["semanticSimilarity"], 1.0);
        assert_eq!(ev["semanticMismatch"], false);
        assert_eq!(ev["orphanCount"], 0);
        assert_eq!(ev["parentUnique"], true);
        // The floor, visible in the artifact: 0.6*0.85 + 0.4*1.0.
        assert_eq!(ev["parentKeyLikeness"], 0.91);
        assert_eq!(
            ev["identifierAllowlistVersion"],
            dovetail_core::identifiers::ALLOWLIST_VERSION
        );
    }
}
