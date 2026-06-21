//! A sampled input — the bytes (or a bounded head of them) a detector reads.
//! Sampling stays in-process and never loads into DuckDB; DuckDB executes only
//! in the round-trip test (keeps choice 0001 clean; review-spec finding 3).

use std::path::{Path, PathBuf};

use crate::structure::Format;

/// An input file presented to a detector: its path, an extension-based format
/// hint, and a bounded sample of its bytes.
#[derive(Debug, Clone)]
pub struct SampledInput {
    pub path: PathBuf,
    /// Format guessed from the extension; the detector refines it by content.
    pub extension_hint: Option<Format>,
    /// A bounded head of the file's bytes. Text formats parse from this; the
    /// Parquet detector reads the footer schema from `path` directly.
    pub head: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum SampleError {
    #[error("reading {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
}

/// How many bytes to read as the sample head. Enough to cover a JSON array's
/// first records or a CSV header plus a few rows.
const SAMPLE_BYTES: usize = 64 * 1024;

impl SampledInput {
    /// Read a bounded sample from a path.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, SampleError> {
        let path = path.as_ref().to_path_buf();
        let bytes = read_head(&path, SAMPLE_BYTES)
            .map_err(|source| SampleError::Io { path: path.clone(), source })?;
        let extension_hint = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(Format::from_extension);
        Ok(SampledInput { path, extension_hint, head: bytes })
    }

    /// The sample head decoded as UTF-8 (lossy), for the text-format detectors.
    pub fn head_str(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.head)
    }
}

fn read_head(path: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let f = std::fs::File::open(path)?;
    let mut buf = Vec::with_capacity(limit.min(8192));
    f.take(limit as u64).read_to_end(&mut buf)?;
    Ok(buf)
}
