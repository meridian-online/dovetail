//! The detected shape of an input — the output of the detection layer and the
//! ground-truth vocabulary the fixture manifests are written against.

use serde::{Deserialize, Serialize};

/// The on-disk format of an input. The SQL-native set for the survey MVP; jaq /
/// calcard / calamine formats are out of scope here (spec
/// 2026-06-20-survey-detection-and-load).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Csv,
    Tsv,
    Parquet,
    Ndjson,
    Json,
}

impl Format {
    /// Best-effort format guess from a file extension. Content sniffing in the
    /// detector refines this; the extension is only the first signal.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "csv" => Some(Format::Csv),
            "tsv" => Some(Format::Tsv),
            "parquet" => Some(Format::Parquet),
            "ndjson" | "jsonl" => Some(Format::Ndjson),
            "json" => Some(Format::Json),
            _ => None,
        }
    }
}

/// The row-level structure of an input — where the rows actually are. This is
/// the make-or-break detection output: get it wrong and the emitted recipe
/// loads the wrong table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Structure {
    /// Already row-shaped: CSV/TSV/Parquet/NDJSON, or a JSON array of flat
    /// records. Loads natively.
    FlatTable,
    /// A top-level JSON array of objects; rows are the array elements.
    RecordsArray,
    /// A JSON file that is a single object; one row.
    SingleObject,
}

/// A column discovered during detection, with the semantic type the
/// finetype-guided detector assigned it (None for the shape-heuristic detector,
/// which does not type columns).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_type: Option<String>,
}

impl Column {
    pub fn untyped(name: impl Into<String>) -> Self {
        Column { name: name.into(), semantic_type: None }
    }
}

/// The full detection result for one input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    pub format: Format,
    pub structure: Structure,
    pub columns: Vec<Column>,
    /// Detector confidence in [0,1]. The detection-quality gate (ac-10) routes
    /// inputs below threshold to suggest-and-confirm.
    pub confidence: f32,
    /// Duplicate column names observed, if any. Survey surfaces an explicit
    /// policy for these rather than letting a parser drop data silently (ac-11).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub duplicate_columns: Vec<String>,
}

impl Detection {
    /// The column names in detected order.
    pub fn column_names(&self) -> Vec<String> {
        self.columns.iter().map(|c| c.name.clone()).collect()
    }

    /// Whether two detections agree on the load-bearing structural facts —
    /// format, structure, and the ordered column set. Semantic types and
    /// confidence are deliberately excluded: the eval scores structure, and the
    /// two detectors are not expected to agree on leaf types. Exact
    /// full-structure match, per review-spec finding 2.
    pub fn structurally_matches(&self, other: &Detection) -> bool {
        self.format == other.format
            && self.structure == other.structure
            && self.column_names() == other.column_names()
    }
}
