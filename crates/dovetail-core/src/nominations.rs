//! Type nominations: what a column IS, declared by the person surveying it.
//!
//! A nomination is taken as given (nomination-is-authoritative): survey does
//! not check it against the data and does not let detection overturn it. The
//! file is the same declaration finetype itself reads — JSON, keyed on the
//! **file stem** of the input it describes:
//!
//! ```json
//! {
//!   "resources": {
//!     "edgar": {
//!       "corpus": { "label": "representation.text.plain_text", "why": "free-form prose" }
//!     }
//!   }
//! }
//! ```
//!
//! `resources` is required; `label` is required; `why` is free text, recorded
//! and never interpreted. Whether a declared label is one the taxonomy carries
//! is checked where the label is used — `datapackage::assemble` — since that is
//! the one place already holding the taxonomy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// One declared column type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nomination {
    /// The taxonomy label this column is declared to be.
    pub label: String,
    /// Free text from the author. Recorded, never interpreted.
    pub why: Option<String>,
}

/// A parsed nominations file: resource stem → column name → nomination.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Nominations {
    resources: BTreeMap<String, BTreeMap<String, Nomination>>,
}

#[derive(Debug, thiserror::Error)]
pub enum NominationsError {
    #[error("reading --nominations {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("--nominations {origin}: {source}")]
    Parse {
        origin: String,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Deserialize)]
struct RawNominations {
    #[serde(default)]
    resources: BTreeMap<String, BTreeMap<String, RawNomination>>,
}

#[derive(Deserialize)]
struct RawNomination {
    label: String,
    #[serde(default)]
    why: Option<String>,
}

impl Nominations {
    /// The nomination for one column of one resource, if there is one. A stem
    /// with no entry is profiled entirely by detection.
    pub fn get(&self, stem: &str, column: &str) -> Option<&Nomination> {
        self.resources.get(stem)?.get(column)
    }

    /// Read and parse a nominations file.
    pub fn load(path: &Path) -> Result<Self, NominationsError> {
        let text = std::fs::read_to_string(path).map_err(|source| NominationsError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text, &path.display().to_string())
    }

    /// Parse nominations JSON. `origin` names the file in errors.
    pub fn parse(text: &str, origin: &str) -> Result<Self, NominationsError> {
        let raw: RawNominations =
            serde_json::from_str(text).map_err(|source| NominationsError::Parse {
                origin: origin.to_string(),
                source,
            })?;
        let resources = raw
            .resources
            .into_iter()
            .map(|(stem, columns)| {
                let columns = columns
                    .into_iter()
                    .map(|(column, n)| {
                        (
                            column,
                            Nomination {
                                label: n.label,
                                why: n.why,
                            },
                        )
                    })
                    .collect();
                (stem, columns)
            })
            .collect();
        Ok(Nominations { resources })
    }
}

/// The stem a nominations file keys resources on: the file stem, unsanitised —
/// the same string finetype's own nominations reader uses, and not
/// `survey::resource_name`'s SQL-safe rewrite of it.
pub fn nomination_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_file_parses() {
        let text = r#"{
          "resources": {
            "naics": {
              "description": { "label": "representation.text.plain_text", "why": "prose" },
              "corpus": { "label": "representation.text.plain_text" }
            }
          }
        }"#;
        let n = Nominations::parse(text, "n.json").expect("parses");
        assert_eq!(
            n.get("naics", "description").map(|d| d.label.as_str()),
            Some("representation.text.plain_text")
        );
        assert_eq!(
            n.get("naics", "description").and_then(|d| d.why.as_deref()),
            Some("prose")
        );
        assert_eq!(
            n.get("naics", "corpus").and_then(|d| d.why.as_deref()),
            None
        );
        assert_eq!(n.get("naics", "absent"), None);
        assert_eq!(n.get("other", "corpus"), None);
    }

    #[test]
    fn a_missing_label_is_refused() {
        let text = r#"{"resources": {"s": {"c": {"why": "no label here"}}}}"#;
        let e = Nominations::parse(text, "n.json").expect_err("label is required");
        assert!(e.to_string().contains("n.json"), "{e}");
    }

    #[test]
    fn not_json_is_refused() {
        let e = Nominations::parse("not json", "n.json").expect_err("invalid JSON is refused");
        assert!(e.to_string().contains("n.json"), "{e}");
    }

    #[test]
    fn nomination_stem_is_the_raw_file_stem_not_a_sql_rewrite() {
        assert_eq!(nomination_stem(Path::new("edgar.csv")), "edgar");
        assert_eq!(nomination_stem(Path::new("my-report.csv")), "my-report");
    }
}
