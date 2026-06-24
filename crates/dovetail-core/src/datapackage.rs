//! Data Package assembly (ac-06). survey serialises both models into one
//! Frictionless Data Package descriptor (`datapackage.json`) — the canonical
//! artifact (choice 0002). This module builds the per-resource half: load
//! recipe reference, Table Schema, and resource-level provenance carried on the
//! standard fields (`bytes`, `hash`, `format`, `mediatype`).
//!
//! `foreignKeys` (the relationship half) is out of scope here — it belongs to
//! relate (card 0002-relate).

use std::path::Path;

use serde::Serialize;

use crate::structure::{Column, Detection, Format};

/// A Frictionless Table Schema field. `type` is a Frictionless type string
/// (`string`, `integer`, `number`, `boolean`, `date`, `datetime`, ...).
#[derive(Debug, Clone, Serialize)]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// Frictionless `format` for the type, when finetype's map supplies one
    /// (e.g. `email` for a string, `%d/%m/%Y` for a date). Frictionless field
    /// order is name → type → format → custom `x-`.
    #[serde(rename = "format", skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// dovetail's finetype semantic type, retained as a namespaced custom
    /// property alongside the standard `type`.
    #[serde(rename = "x-dovetailSemanticType", skip_serializing_if = "Option::is_none")]
    pub semantic_type: Option<String>,
}

/// A Frictionless Table Schema foreign key. Shape per the spec:
/// `{fields, reference: {resource, fields}}`. relate's evidence, confidence and
/// status ride as namespaced custom properties (choice 0003).
#[derive(Debug, Clone, Serialize)]
pub struct ForeignKey {
    pub fields: Vec<String>,
    pub reference: ForeignKeyReference,
    #[serde(rename = "x-dovetailStatus")]
    pub status: String,
    #[serde(rename = "x-dovetailConfidence")]
    pub confidence: f64,
    #[serde(rename = "x-dovetailEvidence")]
    pub evidence: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignKeyReference {
    pub resource: String,
    pub fields: Vec<String>,
}

/// A Frictionless Table Schema.
#[derive(Debug, Clone, Serialize)]
pub struct TableSchema {
    pub fields: Vec<Field>,
    /// foreignKeys live INSIDE the Table Schema (Frictionless), not at package
    /// level. Omitted when empty so survey-only descriptors stay unchanged.
    #[serde(rename = "foreignKeys", skip_serializing_if = "Vec::is_empty")]
    pub foreign_keys: Vec<ForeignKey>,
}

/// dovetail's load recipe, carried as a namespaced custom property on the
/// resource. The `rung` records which fallback-ladder rung was chosen (choice
/// 0004); `sql` references the emitted standalone load.
#[derive(Debug, Clone, Serialize)]
pub struct LoadRecipe {
    pub rung: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
}

/// A Frictionless Data Resource.
#[derive(Debug, Clone, Serialize)]
pub struct Resource {
    pub name: String,
    pub path: String,
    pub format: String,
    pub mediatype: String,
    pub bytes: u64,
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    pub schema: TableSchema,
    #[serde(rename = "x-dovetailLoadRecipe")]
    pub load_recipe: LoadRecipe,
}

/// A Frictionless Data Package.
#[derive(Debug, Clone, Serialize)]
pub struct DataPackage {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub resources: Vec<Resource>,
}

const DATAPACKAGE_PROFILE: &str = "https://datapackage.org/profiles/1.0/datapackage.json";

impl Format {
    fn mediatype(self) -> &'static str {
        match self {
            Format::Csv => "text/csv",
            Format::Tsv => "text/tab-separated-values",
            Format::Parquet => "application/vnd.apache.parquet",
            Format::Ndjson => "application/x-ndjson",
            Format::Json => "application/json",
        }
    }

    fn token(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Tsv => "tsv",
            Format::Parquet => "parquet",
            Format::Ndjson => "ndjson",
            Format::Json => "json",
        }
    }
}

/// Build a Table Schema field from a column, reading finetype's authoritative
/// Frictionless map (`frictionless_for`) for the `type`/`format` pair. Columns
/// with no semantic type (the shape-heuristic detector) — and any label the map
/// doesn't carry — fall back to `string`/no-format, the always-loadable default.
fn field_of(col: &Column) -> Field {
    let fx = col.semantic_type.as_deref().and_then(finetype_core::frictionless_for);
    Field {
        name: col.name.clone(),
        ty: fx.as_ref().map_or_else(|| "string".into(), |f| f.ftype.clone()),
        format: fx.and_then(|f| f.format),
        semantic_type: col.semantic_type.clone(),
    }
}

/// Assemble a single-resource Data Package descriptor for a surveyed file.
///
/// `created` is injected (rather than read from the clock) so callers control
/// determinism; pass `None` to omit it.
pub fn assemble(
    det: &Detection,
    source_path: &Path,
    resource_name: &str,
    sql_recipe_ref: Option<&str>,
    created: Option<String>,
) -> std::io::Result<DataPackage> {
    let bytes_data = std::fs::read(source_path)?;
    let bytes = bytes_data.len() as u64;
    let hash = sha256_hex(&bytes_data);

    let schema = TableSchema {
        fields: det.columns.iter().map(field_of).collect(),
        foreign_keys: Vec::new(),
    };

    let resource = Resource {
        name: resource_name.to_string(),
        path: source_path.to_string_lossy().to_string(),
        format: det.format.token().to_string(),
        mediatype: det.format.mediatype().to_string(),
        bytes,
        hash: format!("sha256:{hash}"),
        created,
        schema,
        load_recipe: LoadRecipe {
            rung: "sql".to_string(),
            sql: sql_recipe_ref.map(|s| s.to_string()),
        },
    };

    Ok(DataPackage { schema: DATAPACKAGE_PROFILE.to_string(), resources: vec![resource] })
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
