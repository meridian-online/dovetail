//! ac-02 / ac-03 groundwork: the shape-heuristic detector must read the correct
//! row-level structure and column set for every fixture in the corpus. Scored
//! here as an exact full-structure match against each fixture's manifest.

use std::path::{Path, PathBuf};

use dovetail_core::structure::{Format, Structure};
use dovetail_core::{Detector, SampledInput, ShapeHeuristicDetector};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Manifest {
    name: String,
    file: String,
    format: Format,
    structure: Structure,
    #[allow(dead_code)]
    row_count: usize,
    columns: Vec<String>,
    #[serde(default)]
    duplicate_columns: Vec<String>,
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .canonicalize()
        .expect("fixtures dir")
}

fn load_manifests() -> Vec<(Manifest, PathBuf)> {
    let dir = fixtures_dir();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("read fixtures") {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_dir() {
            continue;
        }
        let manifest_path = entry.path().join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&manifest_path).unwrap();
        let manifest: Manifest = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("parse {manifest_path:?}: {e}"));
        let data_path = entry.path().join(&manifest.file);
        out.push((manifest, data_path));
    }
    out.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    out
}

#[test]
fn shape_detector_matches_every_fixture_manifest() {
    let detector = ShapeHeuristicDetector::new();
    let manifests = load_manifests();
    assert!(manifests.len() >= 7, "expected the full fixture corpus");

    let mut misses = Vec::new();
    for (manifest, data_path) in &manifests {
        let input = SampledInput::from_path(data_path).expect("sample");
        let det = detector.detect(&input);
        let ok = det.format == manifest.format
            && det.structure == manifest.structure
            && det.column_names() == manifest.columns;
        if !ok {
            misses.push(format!(
                "{}: got format={:?} structure={:?} cols={:?}; want format={:?} structure={:?} cols={:?}",
                manifest.name,
                det.format,
                det.structure,
                det.column_names(),
                manifest.format,
                manifest.structure,
                manifest.columns,
            ));
        }
    }
    assert!(misses.is_empty(), "structure misses:\n{}", misses.join("\n"));
}

#[test]
fn duplicate_columns_are_surfaced() {
    let detector = ShapeHeuristicDetector::new();
    for (manifest, data_path) in load_manifests() {
        if manifest.duplicate_columns.is_empty() {
            continue;
        }
        let input = SampledInput::from_path(&data_path).expect("sample");
        let det = detector.detect(&input);
        assert_eq!(
            det.duplicate_columns, manifest.duplicate_columns,
            "{}: duplicate columns not surfaced", manifest.name
        );
    }
}
