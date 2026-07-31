//! The identifier allowlist — which finetype leaves count as a *join key*.
//!
//! relate scores a candidate foreign key on how key-like the parent column is.
//! Before this module that judgement was made entirely on how the column was
//! *spelled*: `id` / `*_id` / `*_key` / `*code` scored high, everything else
//! scored 0.2. A column of legal-entity identifiers named `lei` was, to relate,
//! indistinguishable from a column of free text.
//!
//! This module is the grounding: an **explicit, versioned allowlist of finetype
//! taxonomy leaves that name a durable identifier of a thing** — the kind of
//! value a join key holds. A column whose sample resolves to one of these leaves
//! earns a floor on its key-likeness ([`crate::relate::SEMANTIC_KEY_FLOOR`]) and can
//! match a differently-named column carrying the same leaf.
//!
//! ## It fails closed
//!
//! An unknown leaf, an unresolved column, or a leaf absent from this list
//! contributes **nothing** — the score falls back to exactly today's
//! name-spelling behaviour. Nothing here can lower a score, block an edge or
//! raise an error. The worst a missing entry costs is a missed edge.
//!
//! ## Admission rule — a leaf must be SUBSTANTIATED
//!
//! A pattern is not evidence: `^[0-9]+$` "confirms" every integer column ever
//! written. A leaf is admitted only when finetype can substantiate the value
//! itself, by one of two routes:
//!
//! 1. the leaf carries a `checksum:` directive in the taxonomy, so its check
//!    digits prove the type (an arbitrary 20-character string is not an LEI); or
//! 2. `finetype_core::deterministic_fast_path` already resolves it — finetype's
//!    own guarantee that the leaf's validator is structurally exclusive.
//!
//! [`admission_route`] evaluates that rule against the live taxonomy, and
//! `identifiers::every_allowlisted_leaf_is_substantiated` enforces it as a test,
//! so the list cannot quietly grow an entry backed by shape alone.
//!
//! ## Deliberately excluded
//!
//! - **`representation.identifier.increment` and
//!   `representation.identifier.numeric_code`** — both validate `^[0-9]+$`.
//!   Admitting either would declare every integer column in the database
//!   key-like, and since the two patterns are identical they are not even
//!   distinguishable from one another. This is why a bare numeric registry
//!   number (a company filer number, an account sequence) cannot be typed from
//!   its values: nothing in the digits says what the number counts.
//! - **`finance.payment.credit_card_number`** — checksum-anchored, and still not
//!   a join key. A card number is a secret; rewarding joins across it is a
//!   feature nobody should get by default.
//! - **`technology.identifier.tsid` / `snowflake_id`** — integer-shaped, same
//!   problem as `increment`. **`ulid`** is excluded by finetype's own fast path
//!   for over-firing on short generic shapes; this list defers to that
//!   judgement rather than second-guessing it.
//! - **`geography.transportation.iso6346`** — checksum-anchored and otherwise
//!   admissible, but four of its five taxonomy samples fail its own mod-11 check
//!   digit, leaving one worked example to test exclusivity against. A shipping
//!   container code is also not a join key anywhere in this product's reach.
//!   Excluded until the samples can substantiate it.
//!
//! ## Versioning
//!
//! [`ALLOWLIST_VERSION`] moves whenever the membership changes. finetype's
//! taxonomy is not frozen — leaves get renamed and re-homed between releases —
//! and when that happens the repair here is a **data change to
//! [`IDENTIFIER_LEAVES`], not a code change**. The version is what lets a
//! descriptor consumer tell which vintage of the list produced a score.

/// Version of the allowlist's *membership*. Bump on every add, remove or rename.
///
/// v1 — 2026-07-31, written against the finetype taxonomy dovetail pins today.
pub const ALLOWLIST_VERSION: u32 = 1;

/// The finetype taxonomy leaves that count as a join key.
///
/// Sorted by taxonomy path so a diff of this list reads as a diff of the
/// taxonomy. Every entry must satisfy [`admission_route`].
pub const IDENTIFIER_LEAVES: &[&str] = &[
    // ── checksum-anchored: the value's own check digits prove the type ──────
    "finance.banking.iban",             // ISO 13616 + mod-97
    "finance.securities.cusip",         // mod-10 with alpha weights
    "finance.securities.figi",          // mod-10 over the 12-char code
    "finance.securities.isin",          // ISO 6166 mod-10
    "finance.securities.lei",           // ISO 17442, ISO 7064 mod-97-10
    "finance.securities.sedol",         // weighted mod-10
    "identity.academic.orcid",          // ISO 7064 mod-11-2
    "identity.commerce.isbn",           // ISBN-10 mod-11 / ISBN-13 mod-10
    "identity.commerce.issn",           // mod-11
    "identity.government.abn",          // mod-89
    "identity.medical.npi",             // Luhn over the 80840 prefix
    // ── structurally conclusive: finetype's own fast path resolves these ────
    "finance.crypto.ethereum_address", // `0x` + 40 hex
    "identity.person.email",           // the natural account key, in practice
    "representation.identifier.uuid",  // 8-4-4-4-12 hex
    "technology.code.doi",             // `10.NNNN/…` registrant prefix
];

/// Whether `leaf` is on the allowlist.
pub fn is_identifier_leaf(leaf: &str) -> bool {
    IDENTIFIER_LEAVES.contains(&leaf)
}

/// How a leaf earns its place on the allowlist — the admission rule, evaluated
/// against the live taxonomy rather than asserted in a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionRoute {
    /// The taxonomy names a `checksum:` for this leaf and finetype implements it.
    Checksum,
    /// `deterministic_fast_path` resolves the leaf from its own taxonomy samples.
    StructurallyConclusive,
    /// Neither — shape alone. Not admissible.
    ShapeOnly,
}

/// Evaluate the admission rule for `leaf` against `tax`.
///
/// `tax` must have had `compile_validators()` called on it.
pub fn admission_route(tax: &finetype_core::Taxonomy, leaf: &str) -> AdmissionRoute {
    let Some(def) = tax.get(leaf) else {
        return AdmissionRoute::ShapeOnly;
    };
    if def.checksum.as_deref().and_then(finetype_core::checksum::resolve).is_some() {
        return AdmissionRoute::Checksum;
    }
    // Route 2: finetype's own fast path resolves this leaf from its own samples.
    let samples: Vec<String> =
        def.samples.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
    if !samples.is_empty()
        && finetype_core::deterministic_fast_path(tax, &samples).as_deref() == Some(leaf)
    {
        return AdmissionRoute::StructurallyConclusive;
    }
    AdmissionRoute::ShapeOnly
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typing::{deterministic_semantic_type, taxonomy};

    fn samples_of(leaf: &str) -> Vec<String> {
        taxonomy()
            .get(leaf)
            .unwrap_or_else(|| panic!("{leaf} is not in the taxonomy dovetail pins"))
            .samples
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    }

    /// A leaf's own taxonomy samples, filtered to the ones that actually pass its
    /// own check digits.
    ///
    /// The filter is not a convenience. Several checksum-bearing leaves ship
    /// *invalid* worked examples — the ISBN-10 `0-13-110362-9`, the ORCID
    /// `0000-0003-1234-5678`, two of four ABNs, one of three NPIs. Testing the
    /// probe against a sample set that is only 67% valid would measure
    /// finetype's sample hygiene, not this allowlist's exclusivity, and would
    /// redden here on an upstream change dovetail does not control.
    fn valid_samples_of(leaf: &str) -> Vec<String> {
        let tax = taxonomy();
        let checksum =
            tax.get(leaf).and_then(|d| d.checksum.as_deref()).and_then(finetype_core::checksum::resolve);
        samples_of(leaf)
            .into_iter()
            .filter(|s| checksum.is_none_or(|verify| verify(s)))
            .collect()
    }

    /// The admission rule, enforced. A leaf on this list must be substantiated by
    /// a checksum or by finetype's own conclusive fast path — never by shape
    /// alone. Adding `representation.identifier.numeric_code` (pattern
    /// `^[0-9]+$`) reddens this test, which is the point.
    #[test]
    fn every_allowlisted_leaf_is_substantiated() {
        let tax = taxonomy();
        let unsubstantiated: Vec<&str> = IDENTIFIER_LEAVES
            .iter()
            .copied()
            .filter(|l| admission_route(tax, l) == AdmissionRoute::ShapeOnly)
            .collect();
        assert!(
            unsubstantiated.is_empty(),
            "these leaves are backed by shape alone — a pattern is not evidence: {unsubstantiated:?}"
        );
    }

    /// Every entry must still exist in the taxonomy dovetail pins. A finetype
    /// release that renames or re-homes a leaf silently disarms that entry —
    /// the probe fails closed, so nothing breaks and nobody finds out.
    #[test]
    fn every_allowlisted_leaf_exists_in_the_pinned_taxonomy() {
        let tax = taxonomy();
        let missing: Vec<&str> =
            IDENTIFIER_LEAVES.iter().copied().filter(|l| tax.get(l).is_none()).collect();
        assert!(missing.is_empty(), "allowlist has drifted from the taxonomy: {missing:?}");
    }

    /// Exclusivity, proven by round trip: each leaf's own taxonomy samples must
    /// resolve back to that leaf and no other. If two allowlisted validators
    /// overlap, the exactly-one-match gate abstains and both entries lose their
    /// resolution — this test is how that shows up as a failure rather than as a
    /// quiet coverage hole.
    #[test]
    fn each_allowlisted_leaf_resolves_from_its_own_taxonomy_samples() {
        let mut misses = Vec::new();
        for leaf in IDENTIFIER_LEAVES {
            let samples = valid_samples_of(leaf);
            if samples.len() < 2 {
                misses.push(format!(
                    "{leaf}: only {} substantiated sample(s) — too few to prove exclusivity",
                    samples.len()
                ));
                continue;
            }
            match deterministic_semantic_type(&samples).as_deref() {
                Some(got) if got == *leaf => {}
                other => misses.push(format!("{leaf}: resolved to {other:?}")),
            }
        }
        assert!(misses.is_empty(), "allowlist entries not exclusive:\n{}", misses.join("\n"));
    }

    /// The list must not fire on the rest of the taxonomy. Every NON-allowlisted
    /// leaf's own samples are run through the full resolver; landing on an
    /// allowlisted identifier would mean relate hands a key-likeness floor to a
    /// column that is not a key.
    #[test]
    fn no_other_taxonomy_leaf_resolves_to_an_allowlisted_identifier() {
        let tax = taxonomy();
        let mut cross_fire = Vec::new();
        for label in tax.label_to_index().keys() {
            if is_identifier_leaf(label) {
                continue;
            }
            let samples = samples_of(label);
            if samples.is_empty() {
                continue;
            }
            if let Some(got) = deterministic_semantic_type(&samples) {
                if is_identifier_leaf(&got) {
                    cross_fire.push(format!("{label} samples resolved to {got}"));
                }
            }
        }
        cross_fire.sort();
        assert!(
            cross_fire.is_empty(),
            "allowlisted identifiers fire on other types:\n{}",
            cross_fire.join("\n")
        );
    }

    /// The two leaves that would break the whole scheme, named so a future
    /// addition has to argue with a test rather than with a comment.
    #[test]
    fn the_integer_shaped_identifier_leaves_stay_out() {
        for leaf in ["representation.identifier.increment", "representation.identifier.numeric_code"]
        {
            assert!(
                !is_identifier_leaf(leaf),
                "{leaf} validates ^[0-9]+$ — admitting it makes every integer column a key"
            );
            assert_eq!(
                admission_route(taxonomy(), leaf),
                AdmissionRoute::ShapeOnly,
                "{leaf} is shape-only; if that changed, re-read the admission rule before adding it"
            );
        }
    }
}
