//! The Apache Ossie model dovetail writes: Ossie's core keys, typed, plus the
//! `MERIDIAN` extension on each element that Brightfield reads.
//!
//! [`read`] and [`write`] are the only way in and out of a document. Reading is
//! lenient: keys this module does not model are kept and written back, and an
//! element with no `MERIDIAN` entry reads with default metadata. Writing is
//! deterministic: keys come out in the wire order, so the same model always gives
//! the same bytes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod meridian;
mod wire;

pub use meridian::{
    Combination, DatasetMeta, DecidedBy, Evidence, FieldMeta, Grade, ModelMeta, RelationshipMeta,
    Role, SkippedSource, Typing, Verified,
};
pub use wire::{read, write};

/// The Ossie specification version dovetail writes.
pub const OSSIE_VERSION: &str = "0.2.0.dev0";

/// The vendor name of dovetail's and Brightfield's extension entry.
pub const VENDOR: &str = "MERIDIAN";

/// The lineage columns a load adds to every table. They are never measured and never fields.
pub const LINEAGE: [&str; 3] = ["__source_file", "__source_part", "__source_row"];

/// Schema-valid Ossie content this module does not model (`ai_context`, `unique_keys`, a
/// field's `label` and `dimension`, and so on), keyed by its YAML key and carried through
/// a read and write unchanged.
pub type Extras = BTreeMap<String, serde_yaml::Value>;

/// A vendor extension entry this module does not interpret: carried and written back unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extension {
    pub vendor_name: String,
    pub data: String,
}

/// One entry of an expression's `dialects` list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dialect {
    pub dialect: String,
    pub expression: String,
}

/// Ossie's logical data types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Datatype {
    String,
    Integer,
    Decimal,
    Float,
    Boolean,
    Date,
    Time,
    DateTime,
    DateTimeTz,
    Opaque,
}

/// A field's expression: the portable `sql`, and any per-dialect forms.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expression {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dialects: Vec<Dialect>,
}

impl Expression {
    /// An expression written as portable SQL alone.
    pub fn sql(sql: impl Into<String>) -> Self {
        Expression {
            sql: Some(sql.into()),
            dialects: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    pub datasets: Vec<Dataset>,
    pub relationships: Vec<Relationship>,
    /// Metrics, kept as written.
    pub metrics: Vec<serde_yaml::Value>,
    pub meta: ModelMeta,
    /// Other vendors' extension entries, in order.
    pub extensions: Vec<Extension>,
    pub extras: Extras,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dataset {
    pub name: String,
    pub source: String,
    pub description: Option<String>,
    pub primary_key: Vec<String>,
    pub fields: Vec<Field>,
    pub metrics: Vec<serde_yaml::Value>,
    pub meta: DatasetMeta,
    pub extensions: Vec<Extension>,
    pub extras: Extras,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub expression: Expression,
    pub datatype: Option<Datatype>,
    pub meta: FieldMeta,
    pub extensions: Vec<Extension>,
    pub extras: Extras,
}

/// A join between two datasets: `from` is the many side, `to` the one side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relationship {
    pub name: String,
    pub from: String,
    pub to: String,
    pub from_columns: Vec<String>,
    pub to_columns: Vec<String>,
    #[serde(default)]
    pub meta: RelationshipMeta,
    #[serde(default, skip_serializing_if = "Extras::is_empty")]
    pub extras: Extras,
    // Brightfield's name for it in a `rejected` entry.
    #[serde(
        default,
        rename = "other_extensions",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub extensions: Vec<Extension>,
}

/// A document that could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OssieError {
    /// The text is not an Ossie document. `line` and `column` are 1-based, 0 when unknown.
    #[error("{message} (line {line}, column {column})")]
    Yaml {
        message: String,
        line: usize,
        column: usize,
    },
    /// A `MERIDIAN` entry whose `data` is not valid for its element.
    #[error("{what}: bad {VENDOR} extension data: {message}")]
    Meridian { what: String, message: String },
    /// A field with neither `sql` nor any dialect expression.
    #[error("{what}: no expression")]
    NoExpression { what: String },
}
