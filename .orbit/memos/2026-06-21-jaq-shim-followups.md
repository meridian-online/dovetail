# jaq shim — review-pr follow-ups

Non-blocking findings from review-pr 2026-06-21 (spec
2026-06-21-jaq-passthrough-shim APPROVE). Candidates for a follow-up spec against
card 0003-transform-shim.

1. **ac-04's "multi-format input" is narrower than worded.** `run_jaq` ingests
   JSON only (`jaq_json::read::parse_many`). The YAML test converts YAML→JSON in
   the harness via serde_yaml, then feeds JSON to the engine — jaq itself never
   reads YAML/TOML/XML, and TOML/XML aren't exercised at all. The spec's goal
   (NDJSON passthrough) is met, but the brief's "jaq handles YAML/TOML/XML in
   addition to JSON" is really a *conversion layer in front of jaq*, not jaq
   itself. Follow-up: either wire a format-conversion front-end into the shim
   (serde_yaml / toml / quick-xml → JSON Val) or reword the capability so the
   conversion responsibility is explicit.

2. **jaq version stamp is hardcoded** (`JAQ_CORE_VERSION = "3.1.0"`). Tracks the
   Cargo.lock pin but can drift on upgrade. Auto-derive it from Cargo.lock at
   build time — ties to choice 0013 (provenance header format).

Pairs with the calamine/calcard half of card 0003 (choice 0011, still open) — the
natural next transform-shim spec.
