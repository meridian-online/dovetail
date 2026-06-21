# jaq ↔ jq divergence notes (ac-07)

**Spec:** 2026-06-21-jaq-passthrough-shim
**Embedded:** jaq-core 3.1.0 · **Reference:** system jq 1.8.2

The parity claim is scoped: *embedded jaq is byte-identical to canonical jq on
the curated jaq/jq-equivalent corpus* (`tests/jaq-corpus/cases.json`), **not**
"equivalent to jq" across the whole language. jaq is an independent
reimplementation, so the general claim is known-false; this note records the
boundary.

## Result on the curated subset

All 9 corpus cases produce **byte-identical** output to system jq 1.8.2
(`reference_parity_vs_system_jq`, ac-06). **Zero divergences** within the subset.
The covered idioms: identity, array iteration, field access, `.results[]` hoist,
`select`, `map`, `keys`, object construction, NDJSON streams — the ingestion
operations survey's `.yaml` rung will lean on.

## Deliberately excluded from the parity subset

These construct families are kept out of the corpus because embedded jaq and
canonical jq can legitimately diverge; including them would assert a known-false
equality. They are out of scope for the ingestion-idiom claim, not bugs.

| Family | Why excluded |
|---|---|
| Regex builtins (`test`, `match`, `sub`, `gsub`, `scan`) | jaq and jq use different regex engines; flag/semantics differences are documented upstream. |
| Date/time builtins (`now`, `strftime`, `gmtime`, `mktime`) | Locale/clock-dependent and non-deterministic; would also break reproducibility (ac-05). |
| Extreme-precision / very large numbers | Number representation and rounding can differ between the two implementations. |
| `@`-format edge cases (`@base64d` on malformed input, `@uri`) | Encoding edge-case behaviour is implementation-specific. |
| SQL-style and advanced builtins (`INDEX`, `GROUP_BY`, `getpath`/`setpath` corners) | Outside the ingestion-idiom scope; equivalence not asserted. |

## How to extend

To widen the parity claim, add cases to `tests/jaq-corpus/cases.json` with
`jq_equivalent: true` and run `cargo test -p dovetail-core --test jaq_transform`.
A case that diverges from jq fails `reference_parity_vs_system_jq` — either fix
the wrapper, or move the construct here with a reason and drop it from the
corpus. The test is the enforcement; this note is the record.
