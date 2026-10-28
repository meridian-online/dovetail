//! The role model: roles, folds and combinations carried together, the
//! summary `dovetail relate` prints from them, and the descriptor left as
//! `relate` writes it without them.

use std::path::{Path, PathBuf};

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
        eprintln!("{a}");
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
