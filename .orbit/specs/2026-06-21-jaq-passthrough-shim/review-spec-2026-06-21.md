# Spec review — jaq passthrough shim

**Spec:** 2026-06-21-jaq-passthrough-shim
**Card:** 0003-transform-shim
**Reviewer:** review-spec
**Date:** 2026-06-21
**Cycle:** review_spec_cycle 0

**Verdict:** APPROVE

## One-line

Sound, testable, and faithful to the choices it cites — the two-parity split (self vs reference) is correctly carved into separate ACs, and the few soft edges below are notes for implement, not blockers.

## What this spec gets right

- **The parity split is the hard part and it's handled.** The tabletop hot-wash named conflating self-parity with reference-parity as the trap that sets up a known-false `jaq == jq` test. The ACs honour the split cleanly: ac-05 (reproducibility, byte-identical to *itself*, structural per choice 0006) and ac-06 (byte-identical to *canonical jq* on a curated subset, per choice 0005). These are genuinely different tests with different evidence, and the spec keeps them apart. This is the single most important thing for this spec to get right, and it does.
- **The gate is the right gate.** ac-02 (byte-equal passthrough) is marked `gate: true` and is the correct keystone — a rewriting shim fails the whole legibility premise (brief principle 4). Gating the rest on it is correct; if passthrough silently normalises, nothing downstream is worth measuring.
- **The reference-parity dependency is handled, not assumed.** ac-06 requires system jq on the test machine and explicitly specifies skip-with-notice when absent, keeping CI dependency-free. Without that clause this AC would be a flaky or environment-coupled test; with it, it's clean.
- **Divergences are documented, not chased.** ac-07 (`ac_type: doc`) records excluded constructs with reasons. This is the correct discipline for a reimplementation — the claim becomes "equivalent on this stated subset", and the boundary is written down rather than discovered later in the field.
- **Scope carving is defended.** calamine/calcard are confirmed in principle (choice 0011) but explicitly deferred to their own spec, with the carving rationale stated (parity is the hard part, prove it alone first). The CLI surface seam for survey's future `.yaml` rung is flagged as a contract with no code coupling — correct posture.
- **Codebase fit is real.** The repo already has the `dovetail-core` crate and a thin `dovetail` bin with a manual `survey` subcommand (`crates/dovetail/src/main.rs`); ac-01 (core transform module) and ac-03 (`jaq` subcommand alongside `survey`) land exactly where the spec says they will.

## Soft edges — implement-time notes, not blockers

1. **ac-04 / ac-05 / ac-06 all lean on "the corpus" but the corpus isn't enumerated in the spec.** The idioms are named (`.results[]`, `select`, `map`, hoist) and at least one of YAML/TOML/XML is required for ac-04, which is enough to make the ACs testable. But the exact (program, input) pairs are left to implement. That's acceptable for a spec of this size — just flag that the corpus is itself a deliverable, and the reviewer at review-pr should expect to see it committed (it's the evidence substrate for three ACs).

2. **ac-02's assertion mechanism is "exposes/echoes for review".** The AC says the program dovetail "exposes/echoes" must equal the input byte-for-byte, but doesn't pin *how* that surface is exposed (a `--show-program` flag? an echo to stderr? a field in the run output?). The kill-condition pivot (Q10 secondary) already anticipates the case where jaq forces an AST round-trip — preserve the raw program string and emit it verbatim. So the intent is clear; the surface is an implement detour. Fine to leave open, but the implementer must produce a concrete, assertable artefact for the byte-equal check, not just an internal invariant.

3. **ac-08 (version stamp) shape is "compatible with choice 0013" — and 0013 is still Proposed/open.** This is a forward-compatibility ask against an unsettled choice, which is the right call (don't block on 0013), but it means ac-08 can't be fully validated against a fixed schema yet. The AC is satisfiable as written (version retrievable via `--version` or a run-output stamp); just note that "compatible with 0013" is a soft constraint that 0013's eventual resolution could revisit. No action now.

4. **ac_type coverage.** Only ac-07 carries an explicit `ac_type` (`doc`); the rest default to `code`. That's correct per the taxonomy — ac-01 through ac-06 and ac-08 all close on passing tests / functional artefacts, and ac-07 closes on a written divergence note. No deferrable (`ops`/`observation`) ACs here, which fits a self-contained build spec. Nothing to change.

## Choice alignment

- **Choice 0005** (embedded jaq default, jq optional reference) — ac-01 (no system jq on the path), ac-06 (`--engine jq` as the reference path). Faithful; this spec is 0005's first concrete realisation as the tabletop claims.
- **Choice 0006** (discovery-execution parity) — ac-05 makes the self-parity claim structural via vendoring. Faithful.
- **Choice 0011** (shim CLI surface) — ac-03/ac-04 settle the jaq half (`dovetail jaq <program> <file>` → NDJSON default). Faithful; calamine/calcard correctly left open.
- **Choice 0013** (provenance header) — ac-08 keeps the stamp shape forward-compatible. Faithful given 0013 is still open.

## Recommendation

Proceed to implement. Carry the four soft edges as implement-time awareness — specifically, commit the parity corpus as a reviewable artefact and produce a concrete assertable surface for ac-02's byte-equal check. None of these rise to a spec change.
