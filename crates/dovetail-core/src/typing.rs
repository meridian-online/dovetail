//! Model-free finetype semantic typing.
//!
//! Assigns a finetype semantic label to a column from a sample of its values,
//! using finetype-core's deterministic value-only classifier
//! (`deterministic_fast_path`) against the compile-time-embedded taxonomy — no
//! neural model, no runtime `labels/` directory. This is the always-available
//! typing floor: the shipped `survey`/`relate` flow types fields with it even
//! when no model directory is configured. The finetype-guided detector layers
//! the neural model on top when one is (`detect/finetype_guided.rs`), and both
//! paths funnel the label back through `finetype_core::frictionless_for` to reach
//! the same authoritative Frictionless `{type, format}` (choice 0105).
//!
//! The label is a 3-level taxonomy leaf — `identity.person.email`,
//! `datetime.timestamp.iso_8601`, `representation.identifier.uuid` — exactly the
//! key `frictionless_for` maps. Deterministic typing only fires where a value
//! *provably* is its type (an email, a delimited ISO timestamp, a UUID), so it is
//! never a mislabel; anything that needs the neural model returns `None` and
//! falls back to the structural type.

use std::sync::OnceLock;

use finetype_core::Taxonomy;

/// The compile-time-embedded taxonomy with its validators compiled, parsed once.
/// `deterministic_fast_path` runs each candidate leaf's validator over the sample,
/// so the validators must be compiled first.
fn taxonomy() -> &'static Taxonomy {
    static TAX: OnceLock<Taxonomy> = OnceLock::new();
    TAX.get_or_init(|| {
        let mut t = Taxonomy::embedded().expect("embedded taxonomy must parse");
        t.compile_validators();
        t
    })
}

/// Assign a finetype semantic label to a column from a value sample, model-free.
///
/// Returns the 3-level taxonomy leaf (e.g. `identity.person.email`) when the
/// sample structurally resolves to exactly one conclusive type; `None` when the
/// values need the neural model, or are plain / empty. The returned leaf is the
/// key [`finetype_core::frictionless_for`] maps to a Frictionless `{type, format}`
/// pair.
pub fn deterministic_semantic_type(values: &[String]) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    finetype_core::deterministic_fast_path(taxonomy(), values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    // The typing floor resolves the conclusive leaves finetype-core guarantees,
    // and each resolves to a real (non-string) Frictionless mapping where the
    // type warrants it — proving the shipped flow can emit semantic types with no
    // neural model loaded.
    #[test]
    fn resolves_email_to_string_email_format() {
        let label = deterministic_semantic_type(&s(&["jane@example.com", "bob@corp.co.uk"]))
            .expect("email must resolve");
        assert_eq!(label, "identity.person.email");
        let fx = finetype_core::frictionless_for(&label).expect("email has a frictionless map");
        assert_eq!(fx.ftype, "string");
        assert_eq!(fx.format.as_deref(), Some("email"));
    }

    #[test]
    fn resolves_iso_datetime_to_datetime_type() {
        let label =
            deterministic_semantic_type(&s(&["2024-01-15T14:30:00Z", "2024-02-20T09:00:00Z"]))
                .expect("iso datetime must resolve");
        assert!(label.starts_with("datetime."), "got {label}");
        let fx = finetype_core::frictionless_for(&label).expect("datetime has a frictionless map");
        assert_eq!(fx.ftype, "datetime", "an ISO timestamp must type as datetime, not string");
    }

    #[test]
    fn resolves_uuid() {
        let label = deterministic_semantic_type(&s(&[
            "550e8400-e29b-41d4-a716-446655440000",
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
        ]))
        .expect("uuid must resolve");
        assert_eq!(label, "representation.identifier.uuid");
    }

    #[test]
    fn declines_plain_integers_and_words() {
        assert_eq!(deterministic_semantic_type(&s(&["1", "2", "3"])), None);
        assert_eq!(deterministic_semantic_type(&s(&["Ada", "Grace", "Alan"])), None);
    }

    #[test]
    fn declines_empty_sample() {
        assert_eq!(deterministic_semantic_type(&[]), None);
    }
}
