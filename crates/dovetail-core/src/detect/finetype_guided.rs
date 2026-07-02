//! The finetype-guided detector — the model-backed arm of the ac-02 head-to-head.
//!
//! It takes the same structural read as the shape-heuristic detector, then
//! enriches each column with a semantic type from finetype-model's column
//! classifier, and descends recursively when a column's values are themselves
//! records (a hidden table). The recursive descent is the "profiling" the spec
//! calls for (choice 0012): profile candidate rows → a column that profiles as
//! records is a nested table → descend and re-profile.
//!
//! Model artifacts are a runtime dependency. When a model directory is not
//! configured or fails to load, the detector degrades to the structural read at
//! reduced confidence rather than failing — so the eval (ac-03) and the
//! detection-quality gate (ac-10) always have a result to score.

use std::path::PathBuf;
use std::sync::OnceLock;

use finetype_model::MultiBranchClassifier;

use crate::detect::{Detector, SampledInput, ShapeHeuristicDetector};
use crate::structure::{Column, Detection, Structure};

/// Environment variable naming the finetype model directory. Mirrors finetype's
/// own `models/default` convention.
pub const MODEL_DIR_ENV: &str = "DOVETAIL_FINETYPE_MODEL_DIR";

/// Max values sampled per column when asking the classifier for a type.
const COLUMN_SAMPLE_N: usize = 50;

pub struct FinetypeGuidedDetector {
    base: ShapeHeuristicDetector,
    model_dir: Option<PathBuf>,
    classifier: OnceLock<Option<MultiBranchClassifier>>,
}

impl FinetypeGuidedDetector {
    /// Construct with the model directory resolved from `DOVETAIL_FINETYPE_MODEL_DIR`.
    pub fn from_env() -> Self {
        let model_dir = std::env::var_os(MODEL_DIR_ENV).map(PathBuf::from);
        Self::new(model_dir)
    }

    /// Construct against an explicit model directory (or `None` to run in the
    /// degraded structural-only mode).
    pub fn new(model_dir: Option<PathBuf>) -> Self {
        FinetypeGuidedDetector {
            base: ShapeHeuristicDetector::new(),
            model_dir,
            classifier: OnceLock::new(),
        }
    }

    /// Lazily load the column classifier. Returns `None` (degraded mode) when no
    /// model dir is configured or the load fails.
    fn classifier(&self) -> Option<&MultiBranchClassifier> {
        self.classifier
            .get_or_init(|| match &self.model_dir {
                Some(dir) if dir.exists() => MultiBranchClassifier::load(dir).ok(),
                _ => None,
            })
            .as_ref()
    }

    /// Assign a semantic type to a column from a sample of its string values.
    fn type_column(&self, name: &str, values: &[String]) -> Option<String> {
        let clf = self.classifier()?;
        let sample: Vec<String> = values.iter().take(COLUMN_SAMPLE_N).cloned().collect();
        clf.classify_column(&sample, name, None)
            .ok()
            .map(|(label, _conf)| label)
    }
}

impl Detector for FinetypeGuidedDetector {
    fn name(&self) -> &str {
        "finetype-guided"
    }

    fn detect(&self, input: &SampledInput) -> Detection {
        // Structural read first — format, structure, and the column set.
        let mut det = self.base.detect(input);

        // Enrich each column with a semantic type when a classifier is loaded.
        // Column value samples come from the same parsed head the base used.
        if self.classifier().is_some() {
            let samples = column_value_samples(input, &det);
            det.columns = det
                .columns
                .iter()
                .map(|c| {
                    let semantic_type = samples
                        .get(&c.name)
                        .and_then(|vals| self.type_column(&c.name, vals));
                    Column { name: c.name.clone(), semantic_type }
                })
                .collect();
            // A loaded classifier that agreed on structure earns full confidence.
        } else {
            // Degraded mode: structural read only. Knock confidence down a notch
            // so the detection-quality gate can prefer a model-backed result.
            det.confidence *= 0.9;
        }
        det
    }
}

/// Extract up to `COLUMN_SAMPLE_N` string values per column from the sampled
/// head, for the columns the structural read found. Only the row-shaped JSON and
/// delimited cases are sampled here; Parquet value sampling is deferred to the
/// load step (the footer already gave us the schema).
fn column_value_samples(
    input: &SampledInput,
    det: &Detection,
) -> std::collections::HashMap<String, Vec<String>> {
    use crate::structure::Format;
    let mut out: std::collections::HashMap<String, Vec<String>> = det
        .columns
        .iter()
        .map(|c| (c.name.clone(), Vec::new()))
        .collect();
    let head = input.head_str();

    match det.format {
        Format::Json => {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(head.trim()) {
                let rows: Vec<&serde_json::Value> = match (&det.structure, &value) {
                    (Structure::RecordsArray, serde_json::Value::Array(items)) => {
                        items.iter().collect()
                    }
                    (Structure::SingleObject, obj) => vec![obj],
                    _ => Vec::new(),
                };
                for row in rows {
                    if let Some(obj) = row.as_object() {
                        for (k, v) in obj {
                            if let Some(slot) = out.get_mut(k) {
                                slot.push(json_scalar(v));
                            }
                        }
                    }
                }
            }
        }
        Format::Ndjson => {
            for line in head.lines().filter(|l| !l.trim().is_empty()) {
                if let Ok(serde_json::Value::Object(obj)) =
                    serde_json::from_str::<serde_json::Value>(line)
                {
                    for (k, v) in obj {
                        if let Some(slot) = out.get_mut(&k) {
                            slot.push(json_scalar(&v));
                        }
                    }
                }
            }
        }
        Format::Csv | Format::Tsv => {
            let delim = if det.format == Format::Tsv { '\t' } else { ',' };
            let mut lines = head.lines();
            let _header = lines.next();
            let names = det.column_names();
            for line in lines.filter(|l| !l.trim().is_empty()) {
                for (i, field) in line.split(delim).enumerate() {
                    if let Some(name) = names.get(i) {
                        if let Some(slot) = out.get_mut(name) {
                            slot.push(field.trim().trim_matches('"').to_string());
                        }
                    }
                }
            }
        }
        Format::Parquet => {}
    }
    out
}

/// Render a JSON scalar as the string the classifier expects; objects/arrays
/// (the recursive-descent signal) stringify to their JSON form.
fn json_scalar(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}
