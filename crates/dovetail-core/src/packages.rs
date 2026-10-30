//! packages — the source tables a run models, read from a directory of data
//! packages or listed from the database itself.
//!
//! Brightfield loads each source as `raw_<name>` and writes one data package
//! per source (`<dir>/<source>/datapackage.json`), one resource per table,
//! each field `{name, type, x-finetype-label?}` with the lineage columns left
//! out. The packages name the datasets and type their fields; a field with no
//! label is untyped, since Brightfield leaves a label out when the column's own
//! data contradicts it.

use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::evidence::EvidenceError;

/// The prefix of a loaded table's name: Brightfield's convention, not
/// dovetail's.
pub const RAW_PREFIX: &str = "raw_";

/// One table a run models: the name the model gives it and the relation of the
/// database it is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTable {
    pub name: String,
    /// The table's name in the database, such as `raw_customers`.
    pub relation: String,
    pub source_file: Option<String>,
    pub source_part: Option<String>,
    /// The package's fields in table order; empty when typing is by request.
    pub fields: Vec<PackageField>,
}

/// A package field: a Frictionless type and, when the column's data bore it
/// out, a FineType label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageField {
    pub name: String,
    pub frictionless_type: String,
    pub finetype: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PackagesError {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("the resource {name} is in both {first} and {second}; DuckDB treats the names as one")]
    Collision {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },
    #[error("{path} is not a data package: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

#[derive(Deserialize)]
struct Package {
    #[serde(default)]
    resources: Vec<Resource>,
}

#[derive(Deserialize)]
struct Resource {
    name: String,
    #[serde(default)]
    path: Option<serde_json::Value>,
    #[serde(rename = "x-part", default)]
    part: Option<String>,
    #[serde(default)]
    schema: Schema,
}

#[derive(Deserialize, Default)]
struct Schema {
    #[serde(default)]
    fields: Vec<Field>,
}

#[derive(Deserialize)]
struct Field {
    name: String,
    #[serde(rename = "type", default = "string_type")]
    ty: String,
    #[serde(rename = "x-finetype-label", default)]
    label: Option<String>,
}

fn string_type() -> String {
    "string".to_string()
}

/// Read every `<dir>/*/datapackage.json`. Resources come back sorted by name,
/// each related as `raw_<name>`. A resource whose table is not in `db_tables`
/// is skipped, and the second value says so, one warning each.
///
/// A resource's `source_file` is its `path` resolved against its package
/// directory and, when that lies inside the directory holding `dir`, made
/// relative to it; otherwise it is the resolved path.
pub fn read_packages(
    dir: &Path,
    db_tables: &[String],
) -> Result<(Vec<SourceTable>, Vec<String>), PackagesError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| PackagesError::Io { path, source }
    };
    let dir = absolute(dir);
    let root = dir.parent().map(Path::to_path_buf);
    let mut package_dirs: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(io(&dir))? {
        let path = entry.map_err(io(&dir))?.path();
        if path.join("datapackage.json").is_file() {
            package_dirs.push(path);
        }
    }
    package_dirs.sort();

    let mut tables = Vec::new();
    let mut seen: Vec<(String, PathBuf)> = Vec::new();
    let mut warnings = Vec::new();
    for package_dir in package_dirs {
        let file = package_dir.join("datapackage.json");
        let text = std::fs::read_to_string(&file).map_err(io(&file))?;
        let package: Package =
            serde_json::from_str(&text).map_err(|source| PackagesError::Json {
                path: file.clone(),
                source,
            })?;
        for r in package.resources {
            let lower = r.name.to_lowercase();
            if let Some((_, first)) = seen.iter().find(|(n, _)| *n == lower) {
                return Err(PackagesError::Collision {
                    name: r.name,
                    first: first.clone(),
                    second: file,
                });
            }
            seen.push((lower, file.clone()));
            let relation = format!("{RAW_PREFIX}{}", r.name);
            if !db_tables.contains(&relation) {
                warnings.push(format!(
                    "the package resource {} has no table {relation} in the database; skipped",
                    r.name
                ));
                continue;
            }
            tables.push(SourceTable {
                source_file: source_file(&package_dir, root.as_deref(), r.path.as_ref()),
                source_part: r.part,
                fields: r
                    .schema
                    .fields
                    .into_iter()
                    .map(|f| PackageField {
                        name: f.name,
                        frictionless_type: f.ty,
                        finetype: f.label,
                    })
                    .collect(),
                name: r.name,
                relation,
            });
        }
    }
    tables.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((tables, warnings))
}

fn source_file(
    package_dir: &Path,
    root: Option<&Path>,
    path: Option<&serde_json::Value>,
) -> Option<String> {
    let path = match path? {
        serde_json::Value::String(s) => s.as_str(),
        serde_json::Value::Array(a) => a.first()?.as_str()?,
        _ => return None,
    };
    let resolved = normalise(&package_dir.join(path));
    let shown = match root {
        Some(root) => resolved.strip_prefix(root).unwrap_or(&resolved),
        None => &resolved,
    };
    Some(shown.to_string_lossy().replace('\\', "/"))
}

fn absolute(path: &Path) -> PathBuf {
    normalise(&std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()))
}

/// `path` with `.` and `..` folded away, without touching the file system.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Every table of the database at `db`, named and related as itself with no
/// fields: typing is by request.
pub fn all_tables(db: &Path) -> Result<Vec<SourceTable>, EvidenceError> {
    let conn = duckdb::Connection::open_with_flags(
        db,
        duckdb::Config::default().access_mode(duckdb::AccessMode::ReadOnly)?,
    )?;
    let mut stmt = conn.prepare(
        "SELECT table_name FROM information_schema.tables
         WHERE table_schema = 'main' AND table_type = 'BASE TABLE'
         ORDER BY table_name",
    )?;
    let names = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(names
        .into_iter()
        .map(|name| SourceTable {
            relation: name.clone(),
            name,
            source_file: None,
            source_part: None,
            fields: Vec::new(),
        })
        .collect())
}
