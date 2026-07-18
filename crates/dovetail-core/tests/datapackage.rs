//! ac-06 — survey assembles a Data Package descriptor per file: resource with
//! source path, format, load-recipe reference, Table Schema from the columns,
//! and provenance on the standard fields (bytes, hash).

use std::path::{Path, PathBuf};

use dovetail_core::datapackage::assemble;
use dovetail_core::eval::load_corpus;
use dovetail_core::{Detector, SampledInput, ShapeHeuristicDetector};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

#[test]
fn assembles_a_conformant_resource_per_fixture() {
    let corpus = load_corpus(repo_root().join("tests/fixtures")).unwrap();
    for fx in &corpus {
        let input = SampledInput::from_path(&fx.data_path).unwrap();
        let det = ShapeHeuristicDetector::new().detect(&input);
        let dp = assemble(
            &det,
            &fx.data_path,
            &fx.manifest.name,
            Some(&format!("{}.sql", fx.manifest.name)),
            Some("2026-06-21T00:00:00Z".into()),
        )
        .unwrap();

        assert_eq!(dp.resources.len(), 1, "{}", fx.manifest.name);
        let r = &dp.resources[0];

        // Table Schema fields mirror the detected columns, in order.
        let field_names: Vec<&str> = r.schema.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(field_names, fx.manifest.columns, "{} schema fields", fx.manifest.name);

        // Provenance on the standard fields.
        assert!(r.bytes > 0, "{} bytes", fx.manifest.name);
        assert!(r.hash.starts_with("sha256:"), "{} hash", fx.manifest.name);
        assert_eq!(r.format, fixture_format_token(&fx.manifest.name));

        // Load recipe references the emitted SQL on the SQL rung.
        assert_eq!(r.load_recipe.rung, "sql");
        assert!(r.load_recipe.sql.is_some());

        // The whole thing serialises to valid JSON.
        let json = serde_json::to_string_pretty(&dp).unwrap();
        assert!(json.contains("\"$schema\""), "{} $schema", fx.manifest.name);
        assert!(json.contains("x-dovetailLoadRecipe"), "{} recipe", fx.manifest.name);
    }
}

// The survey path types fields with finetype: surveying via the SAME detector
// the shipped CLI uses (`FinetypeGuidedDetector::from_env()`), the assembled
// descriptor carries semantic types — model-free, from finetype's deterministic
// typing floor — so a normal survey no longer emits all-string schemas.
#[cfg(feature = "finetype-guided")]
#[test]
fn survey_descriptor_carries_finetype_semantic_types() {
    use dovetail_core::FinetypeGuidedDetector;

    let dir = std::env::temp_dir().join("dovetail-survey-typing");
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("signups.csv");
    std::fs::write(
        &csv,
        "id,email,opened_at\n\
         1,ada@example.com,2024-01-15T14:30:00Z\n\
         2,grace@navy.mil,2024-02-20T09:00:00Z\n\
         3,alan@bletchley.uk,2024-03-01T18:45:00Z\n",
    )
    .unwrap();

    // The detector the shipped `dovetail survey` runs (no model dir → the
    // deterministic typing floor).
    let det = FinetypeGuidedDetector::from_env().detect(&SampledInput::from_path(&csv).unwrap());
    let dp = assemble(&det, &csv, "signups", Some("signups.sql"), None).unwrap();

    let fields = &dp.resources[0].schema.fields;
    let email = fields.iter().find(|f| f.name == "email").expect("email field");
    assert_eq!(email.ty, "string");
    assert_eq!(email.format.as_deref(), Some("email"));
    assert_eq!(email.semantic_type.as_deref(), Some("identity.person.email"));

    let opened = fields.iter().find(|f| f.name == "opened_at").expect("opened_at field");
    assert_eq!(opened.ty, "datetime", "ISO timestamp must type as datetime, not string");
    assert!(opened.semantic_type.as_deref().unwrap().starts_with("datetime."));

    let _ = std::fs::remove_dir_all(&dir);
}

fn fixture_format_token(name: &str) -> &'static str {
    match name {
        "tsv-simple" => "tsv",
        "parquet-simple" => "parquet",
        "ndjson-simple" => "ndjson",
        "json-array" | "json-object" => "json",
        _ => "csv",
    }
}
