//! ac-08 — every emitted datapackage.json validates against the vendored
//! Frictionless profile, and the custom load-recipe property does not break
//! conformance.

use std::path::{Path, PathBuf};

use dovetail_core::datapackage::assemble;
use dovetail_core::eval::load_corpus;
use dovetail_core::{Detector, SampledInput, ShapeHeuristicDetector};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn profile() -> serde_json::Value {
    let path = repo_root().join("vendor/frictionless/datapackage-profile.json");
    let text = std::fs::read_to_string(path).expect("read vendored profile");
    serde_json::from_str(&text).expect("parse profile")
}

#[test]
fn every_emitted_descriptor_validates_against_the_frictionless_profile() {
    let schema = profile();
    let validator = jsonschema::validator_for(&schema).expect("compile schema");
    let corpus = load_corpus(repo_root().join("tests/fixtures")).unwrap();

    let mut failures = Vec::new();
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
        let json = serde_json::to_value(&dp).unwrap();

        // The custom recipe property is present...
        assert!(
            json["resources"][0].get("x-dovetailLoadRecipe").is_some(),
            "{}: recipe property missing", fx.manifest.name
        );

        // ...and the descriptor still conforms.
        let errors: Vec<String> =
            validator.iter_errors(&json).map(|e| format!("{} at {}", e, e.instance_path)).collect();
        if !errors.is_empty() {
            failures.push(format!("{}:\n  {}", fx.manifest.name, errors.join("\n  ")));
        }
    }

    assert!(failures.is_empty(), "conformance failures:\n{}", failures.join("\n"));
}
