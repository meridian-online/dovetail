//! The role model: roles, folds and combinations carried together, the
//! summary `dovetail relate` prints from them, and the descriptor left as
//! `relate` writes it without them.

use std::path::{Path, PathBuf};

use dovetail_core::combine::run_combined;
use dovetail_core::evidence::Session;
use dovetail_core::finetype_ext::TypingRequest;
use dovetail_core::roles::{self, Role};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn retail_sql() -> String {
    std::fs::read_to_string(repo_root().join("tests/role-fixtures/retail/build.sql")).unwrap()
}

fn retail_summary(typing: TypingRequest) -> String {
    let s = Session::from_sql(&retail_sql(), typing).unwrap();
    roles::run_session(&s).unwrap().render_summary()
}

#[test]
fn retail_model_is_deterministic() {
    for typing in [TypingRequest::Floor, TypingRequest::Best] {
        let a = retail_summary(typing);
        let b = retail_summary(typing);
        assert_eq!(a, b);
    }
}

#[test]
fn summary_names_typing() {
    let floor = retail_summary(TypingRequest::Floor);
    let first = floor.lines().next().unwrap();
    assert!(
        first.starts_with("typing: floor"),
        "first line is {first:?}"
    );
    let best = retail_summary(TypingRequest::Best);
    let first = best.lines().next().unwrap();
    assert!(
        first.starts_with("typing: profiled") || first.starts_with("typing: floor"),
        "first line is {first:?}"
    );
}

/// The model carries every combined dataset scored, the fold and the
/// combinations, and the summary prints one line for each.
#[test]
fn retail_model_carries_roles_folds_and_combinations() {
    let s = Session::from_sql(&retail_sql(), TypingRequest::Floor).unwrap();
    let model = roles::run_session(&s).unwrap();
    let names: Vec<&str> = model.tables.iter().map(|c| c.table.as_str()).collect();
    for want in [
        "customers",
        "stores",
        "products",
        "categories",
        "transactions",
        "loyalty_visits",
        "customer_segments",
        "promo_codes",
        "member_visits",
        "suppliers",
        "price_list",
        "members",
    ] {
        assert!(names.contains(&want), "{want} missing from {names:?}");
    }
    assert_eq!(model.tables.len(), 12, "{names:?}");
    let combos: Vec<&str> = model.combinations.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(combos, vec!["members", "price_list", "suppliers"]);

    let summary = model.render_summary();
    let lines: Vec<&str> = summary.lines().collect();
    assert_eq!(
        lines.len(),
        1 + model.tables.len() + model.folds.len() + model.combinations.len(),
        "{summary}"
    );
    // Table lines are sorted by role, then name.
    let mut sorted: Vec<(Role, &str)> = model
        .tables
        .iter()
        .map(|c| (c.role, c.table.as_str()))
        .collect();
    sorted.sort();
    for (line, (_, name)) in lines[1..=model.tables.len()].iter().zip(&sorted) {
        assert!(line.contains(name), "{line:?} should name {name}");
    }
    for f in &model.folds {
        assert!(
            summary.contains(&format!("fold {} into {}", f.reference, f.host)),
            "{summary}"
        );
    }
    for c in &model.combinations {
        assert!(
            summary.contains(&format!("combine {} from", c.name)),
            "{summary}"
        );
    }
}

/// The descriptor the role model carries is the one `relate` writes alone:
/// the golden files pin it byte for byte under the floor typing.
#[test]
fn relate_output_keeps_descriptor() {
    let dir = tempfile::tempdir().unwrap();
    // The descriptor names its resources after the database file, so the
    // files are named as the golden test names them.
    for (fixture, db_name, golden) in [
        (
            "build.sql",
            "fixture.duckdb",
            "build.descriptor.golden.json",
        ),
        (
            "identifier-keys.build.sql",
            "identifier-keys.duckdb",
            "identifier-keys.descriptor.golden.json",
        ),
    ] {
        let fixtures = repo_root().join("tests/relate-fixtures");
        let sql = std::fs::read_to_string(fixtures.join(fixture)).unwrap();
        let db = dir.path().join(db_name);
        duckdb::Connection::open(&db)
            .unwrap()
            .execute_batch(&sql)
            .unwrap();
        let model = roles::run_path(&db, TypingRequest::Floor).unwrap();
        let got = serde_json::to_string_pretty(&model.relate.descriptor).unwrap() + "\n";
        let want = std::fs::read_to_string(fixtures.join(golden)).unwrap();
        assert!(got == want, "{golden} differs:\n{got}");
    }
}

/// Parts of one combination hold one kind of thing and do not refer to each
/// other: no edge between them is measured, scored or written.
#[test]
fn no_edge_links_parts_of_one_combination() {
    let s = Session::from_sql(&retail_sql(), TypingRequest::Floor).unwrap();
    let model = roles::run_session(&s).unwrap();
    assert!(!model.combinations.is_empty());
    let siblings = |a: &str, b: &str| {
        model
            .combinations
            .iter()
            .any(|c| c.parts.iter().any(|p| p == a) && c.parts.iter().any(|p| p == b))
    };
    for e in &model.relate.edges {
        assert!(
            !siblings(&e.child.table, &e.parent.table),
            "{} -> {} links two parts",
            e.child.qualified(),
            e.parent.qualified()
        );
    }
    for e in &model.relate.measured {
        assert!(!siblings(&e.child.table, &e.parent.table));
    }
    for r in model.relate.descriptor["resources"].as_array().unwrap() {
        let child = r["name"].as_str().unwrap();
        for fk in r["schema"]["foreignKeys"].as_array().into_iter().flatten() {
            let parent = fk["reference"]["resource"].as_str().unwrap();
            assert!(
                !siblings(child, parent),
                "{child} -> {parent} in the descriptor"
            );
        }
    }
    let rows: i64 = s
        .conn()
        .query_row(
            "SELECT count(*) FROM memory.main.edge_evidence
             WHERE pass = 'parts' AND child_table LIKE 'price_list_%'
               AND parent_table LIKE 'price_list_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0);
    // A part still joins what is not its sibling.
    assert!(model
        .relate
        .edges
        .iter()
        .any(|e| e.child.table == "price_list_2024" && e.parent.table == "products"));
}

/// The two stages give what one call gives, and the first stage alone holds
/// the descriptor, so a caller can write it before the roles are scored.
#[test]
fn staged_run_matches_one_call() {
    let s = Session::from_sql(&retail_sql(), TypingRequest::Floor).unwrap();
    let parts = roles::run_parts(&s).unwrap();
    let descriptor = parts.relate.descriptor.clone();
    let names: Vec<&str> = parts.combinations.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["members", "price_list", "suppliers"]);
    let staged = roles::run_roles(&s, parts).unwrap();
    assert_eq!(staged.relate.descriptor, descriptor);

    let fresh = Session::from_sql(&retail_sql(), TypingRequest::Floor).unwrap();
    let one = roles::run_session(&fresh).unwrap();
    assert_eq!(staged.render_summary(), one.render_summary());
    assert_eq!(staged.relate.descriptor, one.relate.descriptor);
}

/// Reusing the parts pass's rows for untouched tables gives the combined
/// edges that measuring every pair again gives, with and without
/// combinations.
#[test]
fn combined_edges_reuse_the_parts_pass() {
    let relate_sql = std::fs::read_to_string(
        repo_root().join("tests/relate-fixtures/identifier-keys.build.sql"),
    )
    .unwrap();
    for (sql, combines) in [(retail_sql(), true), (relate_sql, false)] {
        let s = Session::from_sql(&sql, TypingRequest::Floor).unwrap();
        let model = roles::run_session(&s).unwrap();
        assert_eq!(!model.combinations.is_empty(), combines);
        let fresh = Session::from_sql(&sql, TypingRequest::Floor).unwrap();
        let measured = run_combined(&fresh).unwrap();
        assert_eq!(
            format!("{:?}", model.edges),
            format!("{:?}", measured.edges)
        );
        let combined_rows = |s: &Session| -> i64 {
            s.conn()
                .query_row(
                    "SELECT count(*) FROM memory.main.edge_evidence WHERE pass = 'combined'",
                    [],
                    |r| r.get(0),
                )
                .unwrap()
        };
        assert_eq!(combined_rows(&s), combined_rows(&fresh));
        // A carried key leaves the edges, not the rows.
        assert_eq!(
            combined_rows(&s),
            (model.edges.len() + model.carried_keys.len()) as i64
        );
    }
}
