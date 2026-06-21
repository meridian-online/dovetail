//! survey orchestration — tie detection, the fallback-ladder rung decision,
//! SQL emission, and Data Package assembly into one outcome per input, plus the
//! human-facing report.
//!
//! This module is the home of three behaviours:
//! - **Rung reporting (ac-09):** every input reports the chosen rung and why.
//! - **Detection-quality gate (ac-10):** an under-confident detection routes to
//!   suggest-and-confirm instead of emit-and-trust.
//! - **Duplicate-column policy (ac-11):** duplicates are surfaced with the
//!   explicit policy applied, never dropped silently.

use std::path::{Path, PathBuf};

use crate::datapackage::{assemble, DataPackage};
use crate::detect::{Detector, SampledInput};
use crate::emit::{emit_sql, DuplicatePolicy};
use crate::structure::Detection;

/// The emitted-output rung (choice 0004). The survey MVP only reaches the SQL
/// rung; the arcform `.yaml` rung arrives with the jaq-escalation spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rung {
    Sql,
}

impl Rung {
    fn label(self) -> &'static str {
        match self {
            Rung::Sql => "sql",
        }
    }
}

/// Per-input confidence floor for the detection-quality gate. A detection below
/// this routes to suggest-and-confirm. The corpus-level ≥90% bar is asserted
/// separately by the eval (ac-10's two mechanisms).
pub const DETECTION_CONFIDENCE_FLOOR: f32 = 0.5;

/// What survey decided for one input.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// Confident detection: an emitted load and descriptor.
    Emitted { rung: Rung, sql: String, descriptor: DataPackage },
    /// Under-confident detection: survey proposes rather than emits, and asks
    /// the analyst to confirm (the kill-condition pivot).
    SuggestConfirm { reason: String },
}

/// The full result of surveying one file.
#[derive(Debug, Clone)]
pub struct SurveyReport {
    pub source: PathBuf,
    pub detection: Detection,
    pub policy: DuplicatePolicy,
    pub outcome: Outcome,
}

/// Survey one file: detect, gate on confidence, then emit + assemble (or route
/// to suggest-and-confirm). `created` is injected for descriptor provenance.
pub fn survey_file(
    path: &Path,
    detector: &dyn Detector,
    policy: DuplicatePolicy,
    created: Option<String>,
) -> std::io::Result<SurveyReport> {
    let input = SampledInput::from_path(path)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let detection = detector.detect(&input);
    let name = resource_name(path);

    let outcome = if detection.confidence < DETECTION_CONFIDENCE_FLOOR
        || detection.columns.is_empty()
    {
        Outcome::SuggestConfirm {
            reason: format!(
                "detection confidence {:.0}% below the {:.0}% floor — proposing a recipe for you to confirm rather than emitting blind",
                detection.confidence * 100.0,
                DETECTION_CONFIDENCE_FLOOR * 100.0,
            ),
        }
    } else {
        let source = path.to_string_lossy();
        let sql = emit_sql(&detection, &source, &name, policy);
        let descriptor =
            assemble(&detection, path, &name, Some(&format!("{name}.sql")), created)?;
        Outcome::Emitted { rung: Rung::Sql, sql, descriptor }
    };

    Ok(SurveyReport { source: path.to_path_buf(), detection, policy, outcome })
}

impl SurveyReport {
    /// The human-facing report line(s): the chosen rung and why, any duplicate
    /// columns and the policy applied, or the suggest-and-confirm reason.
    pub fn render(&self) -> String {
        let mut s = String::new();
        let src = self.source.display();
        match &self.outcome {
            Outcome::Emitted { rung, .. } => {
                s.push_str(&format!(
                    "{src}: rung={} — {:?} {:?} detected at {:.0}% confidence; loadable by DuckDB natively\n",
                    rung.label(),
                    self.detection.format,
                    self.detection.structure,
                    self.detection.confidence * 100.0,
                ));
                if !self.detection.duplicate_columns.is_empty() {
                    s.push_str(&format!(
                        "  duplicate columns {:?} → policy {:?} (no data dropped)\n",
                        self.detection.duplicate_columns, self.policy,
                    ));
                }
            }
            Outcome::SuggestConfirm { reason } => {
                s.push_str(&format!("{src}: suggest-and-confirm — {reason}\n"));
            }
        }
        s
    }
}

/// Resource/table name from a file stem, sanitised to a SQL-safe identifier.
fn resource_name(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("resource");
    let cleaned: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if cleaned.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true) {
        format!("t_{cleaned}")
    } else {
        cleaned
    }
}
