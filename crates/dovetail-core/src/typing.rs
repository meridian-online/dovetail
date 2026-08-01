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
//!
//! ## The identifier probe
//!
//! finetype's fast path is deliberately narrow: `CONCLUSIVE_LEAVES` is the set
//! whose validators are structurally exclusive *against the whole taxonomy*, and
//! it leaves out checksum-bearing identifier types that matter to relate. An LEI
//! is the worked example — the leaf exists, its definition carries both an
//! ISO 17442 pattern and an ISO 7064 checksum, and the fast path still abstains,
//! so a column of legal-entity identifiers types as plain `string`.
//!
//! [`deterministic_semantic_type`] therefore falls back to an **identifier
//! probe** over [`crate::identifiers::IDENTIFIER_LEAVES`]. The probe is not a
//! second opinion about what an identifier is — every rule it applies is
//! finetype's:
//!
//! - finetype's compiled **pattern** validator for the leaf,
//! - finetype's **checksum** implementation named by the leaf's own
//!   `checksum:` directive (`finetype_core::checksum::resolve`),
//! - finetype's **≥90% of sample** agreement bar, and
//! - an **exactly-one-match** gate: two allowlisted leaves both passing means
//!   the sample is not conclusive, so the probe abstains.
//!
//! The checksum is the part that makes this safe. `finetype-core`'s validator
//! checks pattern, length and enum only — it does **not** apply the `checksum:`
//! directive (that happens later, in the model crate's post-sharpen guard), so
//! pattern-only agreement would confirm any 20-character alphanumeric string as
//! an LEI. Applying the check digits here is what turns shape into substance.

use std::sync::OnceLock;

use finetype_core::Taxonomy;

/// Fraction of a sample's non-empty values that must agree before a leaf is
/// taken. Mirrors finetype's own `label_validates_sample` bar so the probe and
/// the fast path answer to the same threshold.
const SAMPLE_AGREEMENT: f64 = 0.9;

/// The compile-time-embedded taxonomy with its validators compiled, parsed once.
/// `deterministic_fast_path` runs each candidate leaf's validator over the sample,
/// so the validators must be compiled first.
pub(crate) fn taxonomy() -> &'static Taxonomy {
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
///
/// Two stages, in order: finetype's own conclusive fast path, then the
/// checksum-substantiated identifier probe (see the module docs). A column that
/// resolves in the first stage never reaches the second, so the probe can only
/// ever add resolutions — it cannot change one.
pub fn deterministic_semantic_type(values: &[String]) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    let tax = taxonomy();
    finetype_core::deterministic_fast_path(tax, values).or_else(|| identifier_probe(tax, values))
}

/// Resolve an allowlisted identifier leaf from the sample, or abstain.
///
/// Abstains — returns `None` — when nothing passes, and equally when *two*
/// allowlisted leaves pass: an ambiguous sample is not a conclusive one. This is
/// the same exclusivity gate finetype's fast path applies, and it is what stops
/// a widened list from mislabelling by admitting overlapping validators.
fn identifier_probe(tax: &Taxonomy, values: &[String]) -> Option<String> {
    let mut matched: Option<&str> = None;
    for leaf in crate::identifiers::IDENTIFIER_LEAVES {
        if !substantiates(tax, leaf, values) {
            continue;
        }
        if matched.is_some() {
            return None;
        }
        matched = Some(leaf);
    }
    matched.map(str::to_string)
}

/// Whether `values` substantiate `leaf`: at least [`SAMPLE_AGREEMENT`] of the
/// non-empty values pass BOTH finetype's compiled pattern validator for the leaf
/// AND, where the leaf's taxonomy definition names one, finetype's checksum for
/// that scheme.
///
/// A leaf whose definition names no checksum is pattern-only here; the allowlist's
/// admission rule is what guarantees such a leaf was already proven structurally
/// exclusive by finetype's own fast path.
fn substantiates(tax: &Taxonomy, leaf: &str, values: &[String]) -> bool {
    let checksum = tax
        .get(leaf)
        .and_then(|d| d.checksum.as_deref())
        .and_then(finetype_core::checksum::resolve);

    let mut checked = 0usize;
    let mut passed = 0usize;
    for v in values {
        let t = v.trim();
        if t.is_empty() {
            continue;
        }
        // An unknown leaf makes `validate_value_for_label` err; leaving `checked`
        // at zero is what makes the probe fail closed on a renamed taxonomy entry.
        let Ok(res) = finetype_core::validate_value_for_label(t, leaf, tax) else {
            continue;
        };
        checked += 1;
        if res.is_valid && checksum.is_none_or(|verify| verify(t)) {
            passed += 1;
        }
    }
    checked > 0 && (passed as f64) / (checked as f64) >= SAMPLE_AGREEMENT
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
        assert_eq!(
            fx.ftype, "datetime",
            "an ISO timestamp must type as datetime, not string"
        );
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
        assert_eq!(
            deterministic_semantic_type(&s(&["Ada", "Grace", "Alan"])),
            None
        );
    }

    #[test]
    fn declines_empty_sample() {
        assert_eq!(deterministic_semantic_type(&[]), None);
    }

    // --- the identifier probe ------------------------------------------------

    /// The gap the probe exists to close: finetype's fast path abstains on LEI —
    /// the leaf is not in its `CONCLUSIVE_LEAVES` — so before the probe a column
    /// of legal-entity identifiers typed as nothing at all. This test fails if
    /// the probe is deleted AND fails if finetype later adopts the leaf itself
    /// (at which point the allowlist entry is redundant, which is worth knowing).
    #[test]
    fn probe_resolves_lei_where_finetypes_fast_path_abstains() {
        let sample = s(&[
            "529900T8BM49AURSDO55",
            "213800WSGIIZCXF1P572",
            "549300MLUDYVRQOOXS22",
            "0439PH4JW8BY7PLFUQ18",
        ]);
        assert_eq!(
            finetype_core::deterministic_fast_path(taxonomy(), &sample),
            None,
            "premise moved: finetype's own fast path now resolves LEI"
        );
        assert_eq!(
            deterministic_semantic_type(&sample).as_deref(),
            Some("finance.securities.lei")
        );
    }

    /// The checksum is the substance. These strings match the LEI pattern
    /// exactly — 4 digits, 14 alphanumerics, 2 digits — and carry wrong check
    /// digits. Pattern-only agreement would confirm all four as LEIs.
    #[test]
    fn probe_rejects_pattern_shaped_values_with_bad_check_digits() {
        let forged = s(&[
            "529900T8BM49AURSDO00",
            "213800WSGIIZCXF1P500",
            "549300MLUDYVRQOOXS00",
            "0439PH4JW8BY7PLFUQ00",
        ]);
        for v in &forged {
            assert!(
                finetype_core::validate_value_for_label(v, "finance.securities.lei", taxonomy())
                    .expect("lei leaf exists")
                    .is_valid,
                "premise moved: {v} no longer matches the LEI pattern"
            );
        }
        assert_eq!(
            deterministic_semantic_type(&forged),
            None,
            "check digits are the only thing separating these from real LEIs"
        );
    }

    /// Fails closed on the values that actually flow through a database: plain
    /// integers, zero-padded numeric codes, words, decimals. None of these may
    /// acquire an identifier type, because an identifier type buys a column a
    /// key-likeness floor in relate.
    #[test]
    fn probe_abstains_on_ordinary_column_values() {
        for sample in [
            s(&["1750", "1800", "2098", "2186", "2230", "2488"]),
            s(&["0000001750", "0000001800", "0000002098", "0000002186"]),
            s(&["1", "2", "3", "4", "5", "6", "7", "8"]),
            s(&["840", "036", "5112", "06075"]),
            s(&["hammer", "lamp", "rake", "wrench"]),
            s(&["9.90", "4.50", "3.30", "8.00"]),
            s(&["true", "false", "true", "false"]),
        ] {
            assert_eq!(
                deterministic_semantic_type(&sample),
                None,
                "ordinary values must not resolve to an identifier: {sample:?}"
            );
        }
    }

    /// **Known limitation, pinned so it cannot be forgotten.**
    ///
    /// finetype's LEI validation pattern is `^[0-9]{4}[A-Z0-9]{14}[0-9]{2}$` and
    /// its description reads "4-digit LOU prefix". ISO 17442 makes characters
    /// 1–4 the LOU prefix, which is **alphanumeric** — a great many real LEIs
    /// begin with letters. Measured against 400 LEIs from a live registry slice:
    /// **400 of 400 pass finetype's own ISO 7064 check digits; 131 (32.8%) match
    /// its pattern.** A real LEI column therefore sits far below the 90%
    /// agreement bar and types as nothing at all, so the identifier probe cannot
    /// reach it and relate scores the column on spelling alone.
    ///
    /// dovetail deliberately does **not** patch around this. The taxonomy is
    /// finetype's to define, and a pattern override here would be a second
    /// source of truth about what an LEI is — drifting against the one that
    /// already exists, in a repo that consumes finetype as a pinned dependency.
    ///
    /// The values below carry valid ISO 7064 check digits and letter LOU
    /// prefixes. **When this test reddens, the upstream pattern has been fixed**
    /// — at which point widen `identifier-keys.build.sql`, whose LEIs are all
    /// digit-prefixed today precisely because that is all finetype accepts.
    #[test]
    fn letter_prefixed_leis_do_not_resolve_upstream_pattern_defect() {
        let real_shaped = s(&[
            "XA7TGNO0H4AJ34N48G95",
            "IQTSPMI222PTMTCVH081",
            "VRMB7XBEVMA3R4300091",
            "SZ3TGTSWMARXSQQ2JH31",
        ]);
        let lei = finetype_core::checksum::resolve("lei").expect("the lei checksum exists");
        for v in &real_shaped {
            assert!(
                lei(v),
                "{v} must carry valid ISO 7064 check digits for this test to mean anything"
            );
            assert!(
                !finetype_core::validate_value_for_label(v, "finance.securities.lei", taxonomy())
                    .expect("lei leaf exists")
                    .is_valid,
                "UPSTREAM FIXED: {v} now matches finetype's LEI pattern. Delete this test and \
                 widen the identifier-keys fixture to letter-prefixed LEIs."
            );
        }
        assert_eq!(
            deterministic_semantic_type(&real_shaped),
            None,
            "a column of genuinely-shaped LEIs types as nothing while the pattern is wrong"
        );
    }

    /// A sample that is only *mostly* LEIs falls below finetype's agreement bar.
    /// 3 of 6 valid is 50%, well under 90%.
    #[test]
    fn probe_abstains_when_the_sample_does_not_agree() {
        let mixed = s(&[
            "529900T8BM49AURSDO55",
            "213800WSGIIZCXF1P572",
            "549300MLUDYVRQOOXS22",
            "not-an-lei",
            "12345",
            "another one",
        ]);
        assert_eq!(deterministic_semantic_type(&mixed), None);
    }
}
