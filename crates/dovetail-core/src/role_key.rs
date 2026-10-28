//! The answer-key format for role fixtures: what role each table should get,
//! which references fold, and which tables combine. Tests score calls against it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// What a table is for. Serialised lower-case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Entity,
    Event,
    Reference,
    Bridge,
    Snapshot,
    Unclear,
}

/// How several tables combine into one. Serialised lower-case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CombinationKind {
    Partition,
    Snapshot,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedTable {
    pub role: Role,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedFold {
    pub reference: String,
    pub into: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedCombination {
    pub name: String,
    pub parts: Vec<String>,
    pub kind: CombinationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerKey {
    pub name: String,
    pub tables: BTreeMap<String, ExpectedTable>,
    pub folds: Vec<ExpectedFold>,
    pub combinations: Vec<ExpectedCombination>,
}

/// Read an answer key. Table names are lower-cased throughout so lookups match
/// case-insensitively; look them up with a lower-cased name.
pub fn load_answer_key(path: &Path) -> Result<AnswerKey, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut key: AnswerKey =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    key.tables = key
        .tables
        .into_iter()
        .map(|(name, t)| (name.to_lowercase(), t))
        .collect();
    for f in &mut key.folds {
        f.reference = f.reference.to_lowercase();
        f.into = f.into.to_lowercase();
    }
    for c in &mut key.combinations {
        c.name = c.name.to_lowercase();
        c.parts = c.parts.iter().map(|p| p.to_lowercase()).collect();
    }
    Ok(key)
}
