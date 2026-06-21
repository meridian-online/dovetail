# jaq parity corpus

Ground-truth `(program, input)` pairs for the jaq passthrough shim (spec
`2026-06-21-jaq-passthrough-shim`). Each case is a curated jq program drawn from
the common ingestion idioms — identity, array iteration, field access, the
`.results[]` hoist, `select`, `map`, `keys`, object construction, NDJSON streams.

Every case is marked `jq_equivalent: true`: embedded jaq and canonical jq are
expected to produce byte-identical output for it. That equivalence is what the
reference-parity test (ac-06) asserts. Programs where jaq and jq are known to
diverge are deliberately **excluded** from this corpus and recorded in
[`../../.orbit/specs/2026-06-21-jaq-passthrough-shim/divergence-notes.md`](../../.orbit/specs/2026-06-21-jaq-passthrough-shim/divergence-notes.md)
(ac-07) — the parity claim is "equivalent on this stated subset", not "equivalent
to jq" in general.
