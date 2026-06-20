//! Detection eval harness (ac-03). Runs one or more [`Detector`]s over a fixture
//! corpus and scores each by row-structure identification hit-rate — an exact
//! full-structure match against each fixture's manifest (review-spec finding 2).
//! The result renders to a markdown table for the reproducible eval record.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::detect::{Detector, SampledInput};
use crate::structure::{Format, Structure};

/// One fixture's ground truth, read from its `manifest.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct FixtureManifest {
    pub name: String,
    pub file: String,
    pub format: Format,
    pub structure: Structure,
    pub row_count: usize,
    pub columns: Vec<String>,
    #[serde(default)]
    pub duplicate_columns: Vec<String>,
}

/// A fixture: its manifest plus the resolved path to the data file.
#[derive(Debug, Clone)]
pub struct Fixture {
    pub manifest: FixtureManifest,
    pub data_path: PathBuf,
}

/// Load every fixture (a directory containing a `manifest.json`) under `dir`,
/// sorted by name for stable eval output.
pub fn load_corpus(dir: impl AsRef<Path>) -> std::io::Result<Vec<Fixture>> {
    let dir = dir.as_ref();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let manifest_path = entry.path().join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&manifest_path)?;
        let manifest: FixtureManifest = serde_json::from_str(&text).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{manifest_path:?}: {e}"))
        })?;
        let data_path = entry.path().join(&manifest.file);
        out.push(Fixture { manifest, data_path });
    }
    out.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    Ok(out)
}

/// Per-fixture outcome for one detector.
#[derive(Debug, Clone)]
pub struct FixtureScore {
    pub fixture: String,
    pub matched: bool,
    /// Populated only on a miss, for the inspectable breakdown.
    pub detail: Option<String>,
}

/// One detector's eval result over the corpus.
#[derive(Debug, Clone)]
pub struct EvalResult {
    pub detector: String,
    pub mode: String,
    pub total: usize,
    pub hits: usize,
    pub scores: Vec<FixtureScore>,
}

impl EvalResult {
    pub fn hit_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.hits as f64 / self.total as f64
        }
    }
}

/// Score one detector against the corpus. `mode` is a free-text note recorded in
/// the results table (e.g. whether a model dir was loaded).
pub fn eval_detector(
    detector: &dyn Detector,
    corpus: &[Fixture],
    mode: impl Into<String>,
) -> EvalResult {
    let mut scores = Vec::new();
    let mut hits = 0;
    for fx in corpus {
        let input = match SampledInput::from_path(&fx.data_path) {
            Ok(i) => i,
            Err(e) => {
                scores.push(FixtureScore {
                    fixture: fx.manifest.name.clone(),
                    matched: false,
                    detail: Some(format!("sample error: {e}")),
                });
                continue;
            }
        };
        let det = detector.detect(&input);
        let matched = det.format == fx.manifest.format
            && det.structure == fx.manifest.structure
            && det.column_names() == fx.manifest.columns;
        if matched {
            hits += 1;
        }
        let detail = (!matched).then(|| {
            format!(
                "got {:?}/{:?} cols={:?}; want {:?}/{:?} cols={:?}",
                det.format,
                det.structure,
                det.column_names(),
                fx.manifest.format,
                fx.manifest.structure,
                fx.manifest.columns,
            )
        });
        scores.push(FixtureScore { fixture: fx.manifest.name.clone(), matched, detail });
    }
    EvalResult { detector: detector.name().to_string(), mode: mode.into(), total: corpus.len(), hits, scores }
}

/// Render eval results as a markdown report — the reproducible eval record
/// (ac-03 deliverable). `date` is passed in (the workflow stamps it).
pub fn render_report(results: &[EvalResult], corpus_label: &str, date: &str) -> String {
    let mut s = String::new();
    s.push_str(&format!("# Detection eval — {date}\n\n"));
    s.push_str(&format!("Corpus: {corpus_label}\n\n"));

    s.push_str("| detector | mode | hit-rate | hits |\n");
    s.push_str("|---|---|---|---|\n");
    for r in results {
        s.push_str(&format!(
            "| {} | {} | {:.1}% | {}/{} |\n",
            r.detector,
            r.mode,
            r.hit_rate() * 100.0,
            r.hits,
            r.total,
        ));
    }

    s.push_str("\n## Per-fixture\n\n");
    if let Some(first) = results.first() {
        s.push('|');
        s.push_str(" fixture |");
        for r in results {
            s.push_str(&format!(" {} |", r.detector));
        }
        s.push('\n');
        s.push_str("|---|");
        for _ in results {
            s.push_str("---|");
        }
        s.push('\n');
        for (i, fx) in first.scores.iter().enumerate() {
            s.push_str(&format!("| {} |", fx.fixture));
            for r in results {
                let mark = if r.scores[i].matched { "✓" } else { "✗" };
                s.push_str(&format!(" {mark} |"));
            }
            s.push('\n');
        }
    }

    let misses: Vec<&FixtureScore> =
        results.iter().flat_map(|r| r.scores.iter()).filter(|s| !s.matched).collect();
    if !misses.is_empty() {
        s.push_str("\n## Misses\n\n");
        for r in results {
            for sc in r.scores.iter().filter(|s| !s.matched) {
                if let Some(detail) = &sc.detail {
                    s.push_str(&format!("- **{}** / {}: {}\n", r.detector, sc.fixture, detail));
                }
            }
        }
    }
    s
}
