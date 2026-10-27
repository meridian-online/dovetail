//! FineType's DuckDB extension, loaded from the file attached to the finetype
//! release dovetail pins, with the floor typing as the fallback.
//!
//! The typing a run uses is requested ([`TypingRequest`]) and reported
//! ([`TypingSource`]). [`TypingRequest::Best`] loads the extension named by
//! `DOVETAIL_FINETYPE_EXTENSION` and its model from `FINETYPE_MODEL_DIR`, and
//! keeps it only when every check passes; any failure is the floor, with a
//! reason naming what was expected and what was found. The extension is never
//! installed from a repository, which cannot honour a pin, and with
//! `FINETYPE_MODEL_DIR` checked first it never fetches a model itself.
//! `scripts/fetch-finetype.sh` fetches both files and prints the two variables.
//!
//! The extension's answers, as FineType's `crates/finetype-duckdb` defines them
//! at the pinned tag:
//!
//! - `ft_version()` is `VARCHAR` `finetype <version>`, the crate version with no
//!   leading `v`.
//! - `ft_detail(col)` is an aggregate returning a `VARCHAR` holding a JSON
//!   object: `type` (the label, such as `identity.person.email`), `confidence`,
//!   `duckdb_type`, `samples`, `votes` (an object of up to five labels to their
//!   vote shares), and `disambiguation` when a rule decided. A group with
//!   nothing to classify is `{"type":"unknown",...,"samples":0}`, and a failed
//!   classification (a model that would not load, say) is the same with an
//!   `error` key naming why.
//! - `ft_profile(col)` is the same classification as
//!   `STRUCT(type, confidence, duckdb_type)`.
//!
//! The extension loads its model once per process, on its first
//! classification, and keeps it.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The variable naming the release's `.duckdb_extension` file.
pub const EXTENSION_VAR: &str = "DOVETAIL_FINETYPE_EXTENSION";
/// The variable naming the model directory; the extension reads it too.
pub const MODEL_DIR_VAR: &str = "FINETYPE_MODEL_DIR";

/// The files FineType's extension requires in its model directory.
const MODEL_FILES: &[&str] = &["model.safetensors", "label_map.json", "config.json"];

/// The name the extension must be loaded under: DuckDB derives the entry point
/// (`finetype_init_c_api`) from the file name, so the release's own name,
/// `finetype-<tag>-<target>.duckdb_extension`, does not load.
const LOAD_NAME: &str = "finetype.duckdb_extension";

/// Five addresses the extension must type as an email.
const PROBE_SQL: &str = "SELECT ft_detail(v) FROM (VALUES \
     ('alice@example.com'), ('bob.smith@example.org'), ('carol@mail.example.net'), \
     ('dave_92@example.co.uk'), ('erin+news@example.com')) AS probe(v)";

/// The finetype tag the workspace pins, such as `v0.6.64`.
pub fn finetype_tag() -> &'static str {
    env!("DOVETAIL_FINETYPE_TAG")
}

/// Which typing a run asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypingRequest {
    /// FineType's profile when the pinned extension and its model load and
    /// answer as expected, else the floor.
    Best,
    /// The floor only; the extension is not touched.
    Floor,
}

/// Which typing a run got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypingSource {
    /// The pinned extension loaded and passed the known-answer probe.
    Profiled {
        /// What `ft_version()` reported, such as `finetype 0.6.64`.
        extension_version: String,
        /// SHA-256 over the model directory, as [`load`] describes.
        model_sha256: String,
    },
    /// The model-free floor typing, and why.
    Floor { reason: String },
}

/// Load FineType's extension into `conn` as `request` asks.
///
/// With [`TypingRequest::Best`], in order: the extension file and the model
/// directory must exist, the model directory must hold the extension's model
/// files, the connection must allow unsigned extensions, the file must load,
/// `ft_version()` must be the pinned tag's version, and `ft_detail` must type
/// five email addresses with a label ending `.email`. The first that fails is
/// the floor's reason.
///
/// The release extension is unsigned, and DuckDB refuses `SET
/// allow_unsigned_extensions` once a database is running, so `conn` must have
/// been opened with `duckdb::Config::default().allow_unsigned_extensions()`;
/// one that was not falls back to the floor, saying so.
///
/// The model's identity is a SHA-256 over every file under the directory,
/// taken in order of their `/`-separated relative paths, each contributing its
/// path, a zero byte, its length as a little-endian `u64`, and its bytes.
pub fn load(conn: &duckdb::Connection, request: TypingRequest) -> TypingSource {
    if request == TypingRequest::Floor {
        return floor("floor requested");
    }
    match load_best(conn) {
        Ok(source) => source,
        Err(reason) => TypingSource::Floor { reason },
    }
}

fn floor(reason: &str) -> TypingSource {
    TypingSource::Floor {
        reason: reason.to_string(),
    }
}

fn load_best(conn: &duckdb::Connection) -> Result<TypingSource, String> {
    let tag = finetype_tag();
    let extension = std::env::var_os(EXTENSION_VAR).map(PathBuf::from).ok_or_else(|| {
        format!("{EXTENSION_VAR} is not set; expected the path of FineType {tag}'s release .duckdb_extension")
    })?;
    if !extension.is_file() {
        return Err(format!(
            "{EXTENSION_VAR} names {}, which is not a file; expected FineType {tag}'s release .duckdb_extension",
            extension.display()
        ));
    }

    let model_dir = std::env::var_os(MODEL_DIR_VAR)
        .map(PathBuf::from)
        .ok_or_else(|| {
            format!("{MODEL_DIR_VAR} is not set; expected the directory of FineType's model")
        })?;
    check_model_dir(&model_dir)?;

    allow_unsigned(conn)?;
    let loadable = loadable_path(&extension)?;
    let quoted = loadable.display().to_string().replace('\'', "''");
    conn.execute_batch(&format!("LOAD '{quoted}'"))
        .map_err(|e| {
            format!(
                "FineType {tag}'s extension at {} did not load: {e}",
                extension.display()
            )
        })?;

    let reported: String = conn
        .query_row("SELECT ft_version()", [], |row| row.get(0))
        .map_err(|e| {
            format!("expected ft_version() to answer after loading the extension; it failed: {e}")
        })?;
    check_version(tag, &reported)?;

    let model_sha256 = hash_model_dir(&model_dir)?;
    probe(conn)?;
    Ok(TypingSource::Profiled {
        extension_version: reported,
        model_sha256,
    })
}

/// The model directory exists and holds the files the extension loads.
fn check_model_dir(dir: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(format!(
            "{MODEL_DIR_VAR} names {}, which is not a directory; expected FineType's model",
            dir.display()
        ));
    }
    let missing: Vec<&str> = MODEL_FILES
        .iter()
        .copied()
        .filter(|file| !dir.join(file).is_file())
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    let found = if found.is_empty() {
        "nothing".to_string()
    } else {
        found.join(", ")
    };
    Err(format!(
        "{MODEL_DIR_VAR} names {}, which does not hold FineType's model: expected {}, missing {}, found {found}",
        dir.display(),
        MODEL_FILES.join(", "),
        missing.join(", ")
    ))
}

/// The connection allows unsigned extensions. DuckDB fixes the setting when the
/// database starts (`SET allow_unsigned_extensions` on a running one is an
/// error), so this only reads it.
fn allow_unsigned(conn: &duckdb::Connection) -> Result<(), String> {
    let enabled: String = conn
        .query_row(
            "SELECT CAST(current_setting('allow_unsigned_extensions') AS VARCHAR)",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("could not read allow_unsigned_extensions: {e}"))?;
    if enabled.eq_ignore_ascii_case("true") {
        Ok(())
    } else {
        Err(format!(
            "expected a connection opened with allow_unsigned_extensions, as FineType's release \
             extension is unsigned; found allow_unsigned_extensions = {enabled}"
        ))
    }
}

/// A path to the extension under the name DuckDB loads it by: the file itself
/// when it already has that name, else a link (a copy where links are not
/// available) in a temporary directory keyed by the file's path.
fn loadable_path(extension: &Path) -> Result<PathBuf, String> {
    if extension.file_name().is_some_and(|name| name == LOAD_NAME) {
        return Ok(extension.to_path_buf());
    }
    let source = extension
        .canonicalize()
        .map_err(|e| format!("could not resolve {}: {e}", extension.display()))?;
    let key = Sha256::digest(source.to_string_lossy().as_bytes());
    let dir = std::env::temp_dir().join(format!("dovetail-finetype-{}", &hex(&key)[..16]));
    let target = dir.join(LOAD_NAME);
    let staged = |e: std::io::Error| {
        format!(
            "could not stage {} as {}: {e}",
            extension.display(),
            target.display()
        )
    };
    std::fs::create_dir_all(&dir).map_err(staged)?;

    #[cfg(unix)]
    {
        if std::fs::read_link(&target).is_ok_and(|points_at| points_at == source) {
            return Ok(target);
        }
    }
    // Built beside the target and renamed over it, so a concurrent load never
    // sees a half-written file.
    let partial = dir.join(format!(
        ".{LOAD_NAME}.{}.{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_file(&partial);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&source, &partial).map_err(staged)?;
    #[cfg(not(unix))]
    std::fs::copy(&source, &partial).map_err(staged)?;
    std::fs::rename(&partial, &target).map_err(staged)?;
    Ok(target)
}

/// `reported` is `finetype <version>` and the version is `tag`'s, less its `v`.
fn check_version(tag: &str, reported: &str) -> Result<(), String> {
    let expected = tag.strip_prefix('v').unwrap_or(tag);
    let Some(version) = reported.strip_prefix("finetype ") else {
        return Err(format!(
            "expected ft_version() to report `finetype {expected}` (the pinned tag {tag}); found `{reported}`, \
             which is not of the form `finetype <version>`"
        ));
    };
    if version == expected {
        Ok(())
    } else {
        Err(format!(
            "expected ft_version() to report `finetype {expected}` (the pinned tag {tag}); found `{reported}`"
        ))
    }
}

/// SHA-256 over the model directory, as [`load`] describes.
fn hash_model_dir(dir: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_files(dir, "", &mut files)
        .map_err(|e| format!("could not read the model directory {}: {e}", dir.display()))?;
    files.sort();
    let mut hasher = Sha256::new();
    for relative in &files {
        let bytes = std::fs::read(dir.join(relative))
            .map_err(|e| format!("could not read model file {relative}: {e}"))?;
        hasher.update(relative.as_bytes());
        hasher.update([0u8]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hex(&hasher.finalize()))
}

fn collect_files(dir: &Path, prefix: &str, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, &relative, out)?;
        } else {
            out.push(relative);
        }
    }
    Ok(())
}

/// `ft_detail` types five email addresses with a label ending `.email`.
fn probe(conn: &duckdb::Connection) -> Result<(), String> {
    let expected = "expected ft_detail over five email addresses to give a label ending `.email`";
    let detail: String = conn
        .query_row(PROBE_SQL, [], |row| row.get(0))
        .map_err(|e| format!("{expected}; the query failed: {e}"))?;
    let parsed: serde_json::Value = serde_json::from_str(&detail)
        .map_err(|e| format!("{expected}; found output that is not JSON ({e}): {detail}"))?;
    match parsed.get("type").and_then(|t| t.as_str()) {
        Some(label) if label.ends_with(".email") => Ok(()),
        Some(label) => {
            let error = parsed
                .get("error")
                .and_then(|e| e.as_str())
                .map(|e| format!(" (the extension said: {e})"))
                .unwrap_or_default();
            Err(format!("{expected}; found `{label}`{error}"))
        }
        None => Err(format!("{expected}; found no `type` in {detail}")),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::check_version;

    #[test]
    fn version_matching_the_tag_passes() {
        assert_eq!(check_version("v0.6.64", "finetype 0.6.64"), Ok(()));
    }

    #[test]
    fn a_different_version_names_both() {
        let err = check_version("v0.6.64", "finetype 0.6.65").unwrap_err();
        assert!(err.contains("0.6.64") && err.contains("0.6.65"), "{err}");
    }

    #[test]
    fn an_unexpected_shape_is_refused() {
        let err = check_version("v0.6.64", "0.6.64").unwrap_err();
        assert!(err.contains("finetype <version>"), "{err}");
    }
}
