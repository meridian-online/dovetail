//! The shape-heuristic detector — pure structural inspection, no finetype. It
//! sniffs the format and reads the row-level structure straight from the bytes:
//! CSV/TSV headers, NDJSON first line, JSON top-level shape, Parquet footer.
//! This is the no-finetype arm of the ac-02 head-to-head; it does not assign
//! semantic types to columns.

use std::path::Path;

use crate::detect::{Detector, SampledInput};
use crate::structure::{Column, Detection, Format, Structure};

#[derive(Debug, Default)]
pub struct ShapeHeuristicDetector;

impl ShapeHeuristicDetector {
    pub fn new() -> Self {
        ShapeHeuristicDetector
    }
}

impl Detector for ShapeHeuristicDetector {
    fn name(&self) -> &str {
        "shape-heuristic"
    }

    fn detect(&self, input: &SampledInput) -> Detection {
        let format = sniff_format(input);
        match format {
            Format::Csv => delimited(input, b',', Format::Csv),
            Format::Tsv => delimited(input, b'\t', Format::Tsv),
            Format::Ndjson => ndjson(input),
            Format::Json => json(input),
            Format::Parquet => parquet(&input.path),
        }
    }
}

/// Resolve the format from the extension hint, falling back to content shape.
fn sniff_format(input: &SampledInput) -> Format {
    if let Some(f) = input.extension_hint {
        return f;
    }
    let head = input.head_str();
    let trimmed = head.trim_start();
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        // One object per line → ndjson; otherwise a single JSON document.
        if head.lines().take(2).filter(|l| l.trim_start().starts_with('{')).count() >= 2 {
            Format::Ndjson
        } else {
            Format::Json
        }
    } else if head.contains('\t') {
        Format::Tsv
    } else {
        Format::Csv
    }
}

/// CSV / TSV: the header row is the column set; duplicates are surfaced.
fn delimited(input: &SampledInput, delim: u8, format: Format) -> Detection {
    let head = input.head_str();
    let header_line = head.lines().next().unwrap_or("");
    let names: Vec<String> = header_line
        .split(delim as char)
        .map(|s| s.trim().trim_matches('"').to_string())
        .collect();
    let duplicate_columns = duplicates(&names);
    Detection {
        format,
        structure: Structure::FlatTable,
        columns: names.into_iter().map(Column::untyped).collect(),
        confidence: 0.95,
        duplicate_columns,
    }
}

/// NDJSON: one JSON object per line; the first object's keys are the columns.
fn ndjson(input: &SampledInput) -> Detection {
    let head = input.head_str();
    let first = head.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let names = object_keys(first);
    let duplicate_columns = duplicates(&names);
    Detection {
        format: Format::Ndjson,
        structure: Structure::FlatTable,
        columns: names.into_iter().map(Column::untyped).collect(),
        confidence: 0.95,
        duplicate_columns,
    }
}

/// JSON: a top-level array of objects is a records-array (rows = elements); a
/// top-level object is a single-object (one row).
fn json(input: &SampledInput) -> Detection {
    let head = input.head_str();
    let value: Result<serde_json::Value, _> = serde_json::from_str(head.trim());
    match value {
        Ok(serde_json::Value::Array(items)) => {
            let names = items
                .first()
                .and_then(|v| v.as_object())
                .map(|o| o.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            Detection {
                format: Format::Json,
                structure: Structure::RecordsArray,
                columns: names.into_iter().map(Column::untyped).collect(),
                confidence: 0.9,
                duplicate_columns: Vec::new(),
            }
        }
        Ok(serde_json::Value::Object(map)) => Detection {
            format: Format::Json,
            structure: Structure::SingleObject,
            columns: map.keys().cloned().map(Column::untyped).collect(),
            confidence: 0.9,
            duplicate_columns: Vec::new(),
        },
        // Truncated sample or non-object JSON: report low confidence so the
        // detection-quality gate can route to suggest-and-confirm.
        _ => Detection {
            format: Format::Json,
            structure: Structure::SingleObject,
            columns: Vec::new(),
            confidence: 0.2,
            duplicate_columns: Vec::new(),
        },
    }
}

/// Parquet: the footer carries a self-describing schema. Read the top-level
/// column names from it; structure is always a flat table.
fn parquet(path: &Path) -> Detection {
    use parquet::file::reader::{FileReader, SerializedFileReader};
    let names = std::fs::File::open(path)
        .ok()
        .and_then(|f| SerializedFileReader::new(f).ok())
        .map(|reader| {
            reader
                .metadata()
                .file_metadata()
                .schema_descr()
                .root_schema()
                .get_fields()
                .iter()
                .map(|f| f.name().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let confidence = if names.is_empty() { 0.2 } else { 0.98 };
    Detection {
        format: Format::Parquet,
        structure: Structure::FlatTable,
        columns: names.into_iter().map(Column::untyped).collect(),
        confidence,
        duplicate_columns: Vec::new(),
    }
}

/// Parse a JSON object literal and return its keys in order.
fn object_keys(s: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(s.trim())
        .ok()
        .and_then(|v| v.as_object().map(|o| o.keys().cloned().collect()))
        .unwrap_or_default()
}

/// Names that appear more than once, each reported once, in first-seen order.
fn duplicates(names: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashMap::new();
    for n in names {
        *seen.entry(n.clone()).or_insert(0) += 1;
    }
    let mut out = Vec::new();
    let mut emitted = std::collections::HashSet::new();
    for n in names {
        if seen[n] > 1 && emitted.insert(n.clone()) {
            out.push(n.clone());
        }
    }
    out
}
