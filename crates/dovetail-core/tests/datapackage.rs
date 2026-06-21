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

fn fixture_format_token(name: &str) -> &'static str {
    match name {
        "tsv-simple" => "tsv",
        "parquet-simple" => "parquet",
        "ndjson-simple" => "ndjson",
        "json-array" | "json-object" => "json",
        _ => "csv",
    }
}
