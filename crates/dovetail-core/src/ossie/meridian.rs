//! Meridian's extension: the metadata each element carries in its `MERIDIAN`
//! `custom_extensions` entry, whose `data` is a JSON string.
//!
//! Field order here is the JSON key order. Every meta keeps keys it does not know in
//! `extra`, so an element an analyst or another tool has added to round-trips.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{Extension, Relationship, VENDOR};

/// What a table is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Entity,
    Event,
    Reference,
    Bridge,
    Snapshot,
    #[default]
    Unclear,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    Likely,
    #[default]
    Possible,
}

/// Who settled an element. Discovery, verification and rerun leave an `Analyst` element as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecidedBy {
    Dovetail,
    #[default]
    Analyst,
}

/// How much of the data a relationship's evidence was measured on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verified {
    /// No discovery pass has verified the join: it was added by hand.
    #[default]
    Unmeasured,
    Sample,
    Full,
}

/// How a combined dataset was put together from its parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Combination {
    pub kind: String,
    pub parts: Vec<String>,
    pub token_column: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DatasetMeta {
    #[serde(default)]
    pub role: Role,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_evidence: Vec<String>,
    /// All six roles scored, in the role enum's order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_scores: Option<Map<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folded_into: Option<String>,
    /// The struct column a folded reference folds into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fold_column: Option<String>,
    /// The combined dataset a part belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combined_into: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combination: Option<Combination>,
    #[serde(default)]
    pub excluded: bool,
    #[serde(default)]
    pub decided_by: DecidedBy,
    #[serde(default)]
    pub source_file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_part: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finetype: Option<String>,
    #[serde(default)]
    pub decided_by: DecidedBy,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    #[serde(default)]
    pub value_overlap: f64,
    #[serde(default)]
    pub orphans: u64,
    #[serde(default)]
    pub child_rows: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_type: Option<String>,
    #[serde(default)]
    pub types_agree: bool,
    #[serde(default)]
    pub name_similarity: f64,
    /// Parent key values held by more than one parent row.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub parent_duplicates: u64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if passes a reference
fn is_zero(n: &u64) -> bool {
    *n == 0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationshipMeta {
    #[serde(default)]
    pub grade: Grade,
    #[serde(default)]
    pub evidence: Evidence,
    /// For example `N:1`.
    #[serde(default = "n_to_one")]
    pub cardinality: String,
    #[serde(default)]
    pub verified: Verified,
    #[serde(default)]
    pub decided_by: DecidedBy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn n_to_one() -> String {
    "N:1".to_string()
}

impl Default for RelationshipMeta {
    fn default() -> Self {
        RelationshipMeta {
            grade: Grade::default(),
            evidence: Evidence::default(),
            cardinality: n_to_one(),
            verified: Verified::default(),
            decided_by: DecidedBy::default(),
            warning: None,
            extra: Map::new(),
        }
    }
}

/// A source file the discovery pass left out, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedSource {
    pub path: String,
    pub reason: String,
}

/// Where a run's column types came from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Typing {
    /// `packages`, `profiled` or `floor`.
    #[serde(default)]
    pub source: String,
    /// The FineType release tag, when the types were profiled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finetype_tag: Option<String>,
    /// The classifier model's hash, when the types were profiled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_hash: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelMeta {
    #[serde(default = "hand_written")]
    pub generator: String,
    /// Dropped relationships live here, not in `relationships`, so the core
    /// document only asserts joins the model holds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected: Vec<Relationship>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_sources: Vec<SkippedSource>,
    /// The pass that last wrote the model: `sample` or `full`.
    #[serde(default, rename = "pass", skip_serializing_if = "Option::is_none")]
    pub pass: Option<Verified>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typing: Option<Typing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rows: Option<u64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn hand_written() -> String {
    "hand-written".to_string()
}

impl Default for ModelMeta {
    fn default() -> Self {
        ModelMeta {
            generator: hand_written(),
            rejected: Vec::new(),
            skipped_sources: Vec::new(),
            pass: None,
            typing: None,
            sample_rows: None,
            extra: Map::new(),
        }
    }
}

/// Encode `meta` as a `MERIDIAN` entry.
pub(super) fn encode<T: Serialize>(meta: &T) -> Extension {
    Extension {
        vendor_name: VENDOR.to_string(),
        // Total for these types: their only free-form parts are the extras of rejected
        // relationships, which `write` first gives string keys and finite floats.
        data: serde_json::to_string(meta).expect("meridian metadata serialises"),
    }
}

/// Split `exts` into the decoded Meridian entry (the first one) and the other
/// vendors' entries, in order. An element with no Meridian entry gets `default()`.
pub(super) fn split<T: DeserializeOwned>(
    exts: Vec<Extension>,
    default: impl FnOnce() -> T,
) -> Result<(T, Vec<Extension>), String> {
    let mut meta = None;
    let mut others = Vec::new();
    for e in exts {
        if meta.is_none() && e.vendor_name == VENDOR {
            meta = Some(serde_json::from_str::<T>(&e.data).map_err(|err| err.to_string())?);
        } else {
            others.push(e);
        }
    }
    Ok((meta.unwrap_or_else(default), others))
}

/// The entries to write: Meridian's first, then the other vendors'.
pub(super) fn join<T: Serialize>(meta: &T, others: &[Extension]) -> Vec<Extension> {
    let mut v = vec![encode(meta)];
    v.extend(others.iter().cloned());
    v
}
