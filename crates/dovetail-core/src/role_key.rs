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
    let mut tables = BTreeMap::new();
    for (name, t) in key.tables {
        let lower = name.to_lowercase();
        if tables.insert(lower.clone(), t).is_some() {
            return Err(format!(
                "{}: table `{lower}` appears twice, ignoring case",
                path.display()
            ));
        }
    }
    key.tables = tables;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write(tag: &str, body: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("role_key_{}_{tag}.json", std::process::id()));
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn mixed_case_names_are_found_lower_cased() {
        let p = write(
            "ok",
            r#"{"name":"t","tables":{"Orders":{"role":"event","why":"x"}},
                "folds":[{"reference":"Cats","into":"Orders"}],
                "combinations":[{"name":"Sup","parts":["Sup_A","SUP_B"],"kind":"partition"}]}"#,
        );
        let key = load_answer_key(&p).unwrap();
        std::fs::remove_file(&p).ok();
        assert!(key.tables.contains_key("orders"));
        assert_eq!(key.folds[0].reference, "cats");
        assert_eq!(key.combinations[0].parts, vec!["sup_a", "sup_b"]);
    }

    #[test]
    fn a_case_insensitive_collision_is_rejected() {
        let p = write(
            "dup",
            r#"{"name":"t","tables":{"Orders":{"role":"event","why":"x"},
                "orders":{"role":"entity","why":"y"}},"folds":[],"combinations":[]}"#,
        );
        let err = load_answer_key(&p).unwrap_err();
        std::fs::remove_file(&p).ok();
        assert!(err.contains("orders"), "{err}");
    }
}
