//! role_corpus — load the fetched role corpus into evidence sessions.
//!
//! The corpus is one Parquet file per table (`tests/role-corpus/README.md`
//! describes its layout). [`load_corpus_set`] copies a directory of them into
//! one session, a database to score whole; [`load_corpus_table`] copies one
//! file into a session of its own, a table scored alone.
//!
//! FineType's profile is the slow part of a run, so a profiled session's
//! column labels are cached beside its data: `<set>/.profile.json` for a set,
//! `.<file>.profile.json` beside a lone table. Each cache holds the
//! [`profile_cache_stamp`] it was made under, and is used only when that stamp
//! is the session's; otherwise the columns are profiled again and the cache
//! rewritten. The floor is cheap and is never cached.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::evidence::{column_labels, EvidenceError, LabelKey, Session};
use crate::finetype_ext::{finetype_commit, TypingRequest, TypingSource};
use crate::relate::quote;

/// The cache file of a set's profile, inside the set's directory.
const SET_CACHE: &str = ".profile.json";

/// A session whose `src` holds one table per Parquet file in `dir`, named
/// after the file's stem, typed as `typing` asks.
pub fn load_corpus_set(dir: &Path, typing: TypingRequest) -> Result<Session, EvidenceError> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| EvidenceError::Session(format!("{}: {e}", dir.display())))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "parquet"))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(EvidenceError::Session(format!(
            "{} holds no Parquet files",
            dir.display()
        )));
    }
    load(&files, &dir.join(SET_CACHE), typing)
}

/// A session holding the one table in `file`, named after its stem, typed as
/// `typing` asks. Scored alone, it has no joins and no combinations.
pub fn load_corpus_table(file: &Path, typing: TypingRequest) -> Result<Session, EvidenceError> {
    let name = file
        .file_name()
        .ok_or_else(|| EvidenceError::Session(format!("{} names no file", file.display())))?;
    let cache = file.with_file_name(format!(".{}.profile.json", name.to_string_lossy()));
    load(&[file.to_path_buf()], &cache, typing)
}

/// What a profile cache was made under: the extension's `ft_version()`, the
/// model directory's SHA-256, and the finetype commit dovetail's `Cargo.lock`
/// resolved, each labelled. `None` for the floor, which is not cached.
pub fn profile_cache_stamp(typing: &TypingSource) -> Option<String> {
    match typing {
        TypingSource::Profiled {
            extension_version,
            model_sha256,
        } => Some(format!(
            "extension {extension_version}; model sha256 {model_sha256}; dovetail's finetype commit {}",
            finetype_commit()
        )),
        TypingSource::Floor { .. } => None,
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ProfileCache {
    stamp: String,
    columns: Vec<CachedLabel>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedLabel {
    table: String,
    column: String,
    duckdb_type: String,
    label: Option<String>,
}

fn load(files: &[PathBuf], cache: &Path, typing: TypingRequest) -> Result<Session, EvidenceError> {
    let mut sql = String::new();
    for f in files {
        let stem = f
            .file_stem()
            .map(|s| s.to_string_lossy())
            .ok_or_else(|| EvidenceError::Session(format!("{} has no file name", f.display())))?;
        let path = f.to_str().ok_or_else(|| {
            EvidenceError::Session(format!("the path {} is not valid UTF-8", f.display()))
        })?;
        sql.push_str(&format!(
            "CREATE TABLE {} AS FROM read_parquet('{}');\n",
            quote(&stem),
            path.replace('\'', "''")
        ));
    }
    let mut session = Session::from_sql(&sql, typing)?;
    let Some(stamp) = profile_cache_stamp(&session.typing) else {
        return Ok(session);
    };
    let labels = match read_cache(cache, &stamp) {
        Some(labels) => labels,
        None => {
            let labels = column_labels(&session)?;
            write_cache(cache, &stamp, &labels)?;
            labels
        }
    };
    session.set_known_labels(labels);
    Ok(session)
}

/// The cache's labels, when it exists, parses and carries `stamp`.
fn read_cache(cache: &Path, stamp: &str) -> Option<BTreeMap<LabelKey, Option<String>>> {
    let text = std::fs::read_to_string(cache).ok()?;
    let parsed: ProfileCache = serde_json::from_str(&text).ok()?;
    if parsed.stamp != stamp {
        return None;
    }
    Some(
        parsed
            .columns
            .into_iter()
            .map(|c| ((c.table, c.column, c.duckdb_type), c.label))
            .collect(),
    )
}

/// Write the cache through a temporary file, so an interrupted run leaves the
/// old cache or the new one, never half of one.
fn write_cache(
    cache: &Path,
    stamp: &str,
    labels: &BTreeMap<LabelKey, Option<String>>,
) -> Result<(), EvidenceError> {
    let body = ProfileCache {
        stamp: stamp.to_string(),
        columns: labels
            .iter()
            .map(|((table, column, duckdb_type), label)| CachedLabel {
                table: table.clone(),
                column: column.clone(),
                duckdb_type: duckdb_type.clone(),
                label: label.clone(),
            })
            .collect(),
    };
    let text = serde_json::to_string_pretty(&body)
        .map_err(|e| EvidenceError::Session(format!("profile cache: {e}")))?;
    let tmp = cache.with_extension("json.tmp");
    let io = |e: std::io::Error| EvidenceError::Session(format!("{}: {e}", cache.display()));
    std::fs::write(&tmp, text).map_err(io)?;
    std::fs::rename(&tmp, cache).map_err(io)?;
    Ok(())
}
