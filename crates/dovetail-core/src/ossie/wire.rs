//! The YAML shape of an Ossie document, and the conversion to and from [`Model`].
//!
//! Struct field order here is the key order `write` emits, so output is deterministic.

use serde::{Deserialize, Serialize};

use super::meridian;
use super::{
    Dataset, Datatype, Expression, Extension, Extras, Field, Model, ModelMeta, OssieError,
    Relationship, OSSIE_VERSION,
};
use super::{DatasetMeta, FieldMeta};

#[derive(Serialize, Deserialize)]
struct WireModel {
    #[serde(default = "default_version")]
    version: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default)]
    datasets: Vec<WireDataset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    relationships: Vec<WireRelationship>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    metrics: Vec<serde_yaml::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    custom_extensions: Vec<Extension>,
    #[serde(flatten)]
    extras: Extras,
}

fn default_version() -> String {
    OSSIE_VERSION.to_string()
}

#[derive(Serialize, Deserialize)]
struct WireDataset {
    name: String,
    source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    primary_key: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    fields: Vec<WireField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    metrics: Vec<serde_yaml::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    custom_extensions: Vec<Extension>,
    #[serde(flatten)]
    extras: Extras,
}

#[derive(Serialize, Deserialize)]
struct WireField {
    name: String,
    expression: Expression,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    datatype: Option<Datatype>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    custom_extensions: Vec<Extension>,
    #[serde(flatten)]
    extras: Extras,
}

#[derive(Serialize, Deserialize)]
struct WireRelationship {
    name: String,
    from: String,
    to: String,
    from_columns: Vec<String>,
    to_columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    custom_extensions: Vec<Extension>,
    #[serde(flatten)]
    extras: Extras,
}

/// Parse an Ossie document. An element with no Meridian extension is read with
/// default metadata; keys this module does not model are kept.
pub fn read(yaml: &str) -> Result<Model, OssieError> {
    let w: WireModel = serde_yaml::from_str(yaml).map_err(|e| {
        let loc = e.location();
        OssieError::Yaml {
            message: e.to_string(),
            line: loc.as_ref().map_or(0, |l| l.line()),
            column: loc.as_ref().map_or(0, |l| l.column()),
        }
    })?;
    from_wire(w)
}

fn meridian_error(what: &str) -> impl FnOnce(String) -> OssieError + '_ {
    move |message| OssieError::Meridian {
        what: what.to_string(),
        message,
    }
}

fn from_wire(w: WireModel) -> Result<Model, OssieError> {
    let (meta, extensions) = meridian::split::<ModelMeta>(w.custom_extensions, ModelMeta::default)
        .map_err(meridian_error("model"))?;
    let datasets = w
        .datasets
        .into_iter()
        .map(dataset_from_wire)
        .collect::<Result<_, _>>()?;
    let relationships = w
        .relationships
        .into_iter()
        .map(relationship_from_wire)
        .collect::<Result<_, _>>()?;
    Ok(Model {
        version: w.version,
        name: w.name,
        description: w.description,
        datasets,
        relationships,
        metrics: w.metrics,
        meta,
        extensions,
        extras: w.extras,
    })
}

fn dataset_from_wire(d: WireDataset) -> Result<Dataset, OssieError> {
    let source = d.source.clone();
    let (meta, extensions) = meridian::split::<DatasetMeta>(d.custom_extensions, || DatasetMeta {
        source_file: source,
        ..DatasetMeta::default()
    })
    .map_err(meridian_error(&format!("dataset {}", d.name)))?;
    let fields = d
        .fields
        .into_iter()
        .map(|f| field_from_wire(&d.name, f))
        .collect::<Result<_, _>>()?;
    Ok(Dataset {
        name: d.name,
        source: d.source,
        description: d.description,
        primary_key: d.primary_key,
        fields,
        metrics: d.metrics,
        meta,
        extensions,
        extras: d.extras,
    })
}

fn field_from_wire(dataset: &str, f: WireField) -> Result<Field, OssieError> {
    let what = format!("field {dataset}.{}", f.name);
    if f.expression.sql.is_none() && f.expression.dialects.is_empty() {
        return Err(OssieError::NoExpression { what });
    }
    let (meta, extensions) = meridian::split::<FieldMeta>(f.custom_extensions, FieldMeta::default)
        .map_err(meridian_error(&what))?;
    Ok(Field {
        name: f.name,
        expression: f.expression,
        datatype: f.datatype,
        meta,
        extensions,
        extras: f.extras,
    })
}

fn relationship_from_wire(r: WireRelationship) -> Result<Relationship, OssieError> {
    let (meta, extensions) = meridian::split(r.custom_extensions, Default::default)
        .map_err(meridian_error(&format!("relationship {}", r.name)))?;
    Ok(Relationship {
        name: r.name,
        from: r.from,
        to: r.to,
        from_columns: r.from_columns,
        to_columns: r.to_columns,
        meta,
        extras: r.extras,
        extensions,
    })
}

/// Serialise a model as an Ossie document. Keys come out in a fixed order, so the same
/// model always gives the same bytes. A non-finite evidence number (NaN or infinite) is
/// written as 0.0, since YAML and JSON cannot carry it and a null would make the file
/// unreadable. The extras of a rejected relationship are normalised to JSON-safe keys
/// (a non-string key becomes its YAML text); keys that collide after that, such as `1`
/// and `"1"`, are merged.
pub fn write(model: &Model) -> String {
    let mut meta = model.meta.clone();
    meta.rejected = meta.rejected.iter().map(for_json).collect();
    let w = WireModel {
        version: model.version.clone(),
        name: model.name.clone(),
        description: model.description.clone(),
        datasets: model.datasets.iter().map(dataset_to_wire).collect(),
        relationships: model
            .relationships
            .iter()
            .map(|r| relationship_to_wire(&finite_relationship(r)))
            .collect(),
        metrics: model.metrics.clone(),
        custom_extensions: meridian::join(&meta, &model.extensions),
        extras: model.extras.clone(),
    };
    serde_yaml::to_string(&w).expect("a model serialises to YAML")
}

/// A rejected relationship as it is stored: evidence clamped, and extras made
/// JSON-safe, since the `rejected` list is a JSON string in the Meridian extension.
fn for_json(r: &Relationship) -> Relationship {
    let mut r = finite_relationship(r);
    r.extras = r
        .extras
        .into_iter()
        .map(|(k, v)| (k, string_keys(v)))
        .collect();
    r
}

fn finite_relationship(r: &Relationship) -> Relationship {
    let mut r = r.clone();
    let e = &mut r.meta.evidence;
    for v in [&mut e.value_overlap, &mut e.name_similarity] {
        if !v.is_finite() {
            *v = 0.0;
        }
    }
    r
}

/// `v` with every mapping key as a string (a non-string key becomes its YAML text) and
/// every non-finite float as its YAML text (`.nan`, `.inf`, `-.inf`), since JSON has
/// neither non-string keys nor those floats.
fn string_keys(v: serde_yaml::Value) -> serde_yaml::Value {
    use serde_yaml::Value;
    match v {
        Value::Mapping(m) => Value::Mapping(
            m.into_iter()
                .map(|(k, v)| {
                    let k = match k {
                        Value::String(s) => s,
                        other => serde_yaml::to_string(&other)
                            .map(|s| s.trim().to_string())
                            .unwrap_or_default(),
                    };
                    (Value::String(k), string_keys(v))
                })
                .collect(),
        ),
        Value::Sequence(s) => Value::Sequence(s.into_iter().map(string_keys).collect()),
        Value::Tagged(t) => Value::Tagged(Box::new(serde_yaml::value::TaggedValue {
            tag: t.tag,
            value: string_keys(t.value),
        })),
        Value::Number(n) if n.as_f64().is_some_and(|f| !f.is_finite()) => {
            let f = n.as_f64().unwrap_or(0.0);
            Value::String(
                if f.is_nan() {
                    ".nan"
                } else if f > 0.0 {
                    ".inf"
                } else {
                    "-.inf"
                }
                .to_string(),
            )
        }
        other => other,
    }
}

fn dataset_to_wire(d: &Dataset) -> WireDataset {
    WireDataset {
        name: d.name.clone(),
        source: d.source.clone(),
        description: d.description.clone(),
        primary_key: d.primary_key.clone(),
        fields: d
            .fields
            .iter()
            .map(|f| WireField {
                name: f.name.clone(),
                expression: f.expression.clone(),
                datatype: f.datatype,
                custom_extensions: meridian::join(&f.meta, &f.extensions),
                extras: f.extras.clone(),
            })
            .collect(),
        metrics: d.metrics.clone(),
        custom_extensions: meridian::join(&d.meta, &d.extensions),
        extras: d.extras.clone(),
    }
}

fn relationship_to_wire(r: &Relationship) -> WireRelationship {
    WireRelationship {
        name: r.name.clone(),
        from: r.from.clone(),
        to: r.to.clone(),
        from_columns: r.from_columns.clone(),
        to_columns: r.to_columns.clone(),
        custom_extensions: meridian::join(&r.meta, &r.extensions),
        extras: r.extras.clone(),
    }
}
