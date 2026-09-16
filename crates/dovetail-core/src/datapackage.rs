//! Data Package assembly. survey serialises both models into one
//! Frictionless Data Package descriptor (`datapackage.json`) — the canonical
//! artifact (choice 0002). This module builds the per-resource half: load
//! recipe reference, Table Schema, and resource-level provenance carried on the
//! standard fields (`bytes`, `hash`, `format`, `mediatype`).
//!
//! `foreignKeys` (the relationship half) is out of scope here — it belongs to
//! relate.

use std::path::Path;
use std::sync::OnceLock;

use serde::Serialize;

use finetype_core::{PatternSource, Taxonomy};

use crate::nominations::{nomination_stem, Nomination, Nominations};
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
    /// order is name → type → format → constraints → custom `x-`.
    #[serde(rename = "format", skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// The nominated label's taxonomy validation bounds
    /// (`minLength`/`maxLength`/`minimum`/`maximum`), present only under a
    /// nomination (nomination-carries-constraints). dovetail reads no column
    /// values for a nominated field, so this never carries `pattern` or
    /// `enum` — those are claims about the data, and dovetail makes none.
    #[serde(rename = "constraints", skip_serializing_if = "Option::is_none")]
    pub constraints: Option<serde_json::Map<String, serde_json::Value>>,
    /// dovetail's finetype semantic type, retained as a namespaced custom
    /// property alongside the standard `type`.
    #[serde(
        rename = "x-dovetailSemanticType",
        skip_serializing_if = "Option::is_none"
    )]
    pub semantic_type: Option<String>,
    /// Set when `semantic_type` came from `--nominations` rather than
    /// detection: the label was declared, not guessed, and is used as given
    /// (nomination-is-authoritative).
    #[serde(
        rename = "x-finetype-nominated",
        skip_serializing_if = "Option::is_none"
    )]
    pub nominated: Option<bool>,
}

#[derive(Debug, thiserror::Error)]
pub enum DataPackageError {
    #[error("reading {0}")]
    Io(#[from] std::io::Error),
    #[error("column {column:?} is nominated as {label:?}, which is not a label in the taxonomy")]
    UnknownNomination { column: String, label: String },
}

/// The taxonomy dovetail nominations and semantic types are resolved against —
/// the same one finetype-core embeds at compile time, parsed once and cached.
fn embedded_taxonomy() -> &'static Taxonomy {
    static TAXONOMY: OnceLock<Taxonomy> = OnceLock::new();
    TAXONOMY.get_or_init(|| Taxonomy::embedded().expect("embedded taxonomy must parse"))
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

const DATAPACKAGE_PROFILE: &str = "https://datapackage.org/profiles/2.0/datapackage.json";

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

/// Build a Table Schema field from a column.
///
/// A nomination, when present, wins outright (nomination-is-authoritative): the
/// declared label replaces whatever detection assigned, and the taxonomy's
/// `Taxonomy::publication_for` — the one function both dovetail and
/// finetype-mcp read for what a label publishes — supplies `type`, `format`
/// and `constraints`. `PatternSource::Observed(None)` because dovetail read no
/// column values under a nomination and publishes no claim about them.
///
/// With no nomination this is unchanged: finetype's authoritative Frictionless
/// map (`frictionless_for`) supplies `type`/`format`; a column with no semantic
/// type, or a label the map doesn't carry, falls back to `string`/no-format.
fn field_of(col: &Column, nomination: Option<&Nomination>) -> Result<Field, DataPackageError> {
    if let Some(nom) = nomination {
        let taxonomy = embedded_taxonomy();
        if taxonomy.get(&nom.label).is_none() {
            return Err(DataPackageError::UnknownNomination {
                column: col.name.clone(),
                label: nom.label.clone(),
            });
        }
        let published = taxonomy.publication_for(&nom.label, PatternSource::Observed(None));
        return Ok(Field {
            name: col.name.clone(),
            ty: published.ftype,
            format: published.format,
            constraints: (!published.constraints.is_empty()).then_some(published.constraints),
            semantic_type: Some(nom.label.clone()),
            nominated: Some(true),
        });
    }

    let fx = col
        .semantic_type
        .as_deref()
        .and_then(finetype_core::frictionless_for);
    Ok(Field {
        name: col.name.clone(),
        ty: fx
            .as_ref()
            .map_or_else(|| "string".into(), |f| f.ftype.clone()),
        format: fx.and_then(|f| f.format),
        constraints: None,
        semantic_type: col.semantic_type.clone(),
        nominated: None,
    })
}

/// The Frictionless `resource.path`. Frictionless 2.0 requires it to be
/// relative to the directory holding the descriptor (an absolute filesystem
/// path, `../`, `~` or `file:` are all rejected by the profile's `path`
/// pattern). dovetail writes one descriptor per source and co-locates it with
/// the data, so a single-file survey reduces to the basename — the flat case of
/// the descriptor-relative rule. A multi-file package rooted above its data
/// would carry the subpath instead; dovetail does not build that shape yet.
fn resource_path(source: &Path) -> String {
    source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.to_string_lossy().into_owned())
}

/// Assemble a single-resource Data Package descriptor for a surveyed file.
///
/// `created` is injected (rather than read from the clock) so callers control
/// determinism; pass `None` to omit it. `nominations`, when given, is
/// consulted per column against `source_path`'s file stem — the same stem
/// convention the declaration file itself uses, not `resource_name`'s
/// SQL-safe rewrite of it.
pub fn assemble(
    det: &Detection,
    source_path: &Path,
    resource_name: &str,
    sql_recipe_ref: Option<&str>,
    created: Option<String>,
    nominations: Option<&Nominations>,
) -> Result<DataPackage, DataPackageError> {
    let bytes_data = std::fs::read(source_path)?;
    let bytes = bytes_data.len() as u64;
    let hash = sha256_hex(&bytes_data);

    let stem = nomination_stem(source_path);
    let fields = det
        .columns
        .iter()
        .map(|c| {
            let nomination = nominations.and_then(|n| n.get(&stem, &c.name));
            field_of(c, nomination)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let schema = TableSchema {
        fields,
        foreign_keys: Vec::new(),
    };

    let resource = Resource {
        name: resource_name.to_string(),
        path: resource_path(source_path),
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

    Ok(DataPackage {
        schema: DATAPACKAGE_PROFILE.to_string(),
        resources: vec![resource],
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
