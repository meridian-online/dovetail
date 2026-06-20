# Tabletop — Survey: detection-first SQL-native load

**Date:** 2026-06-20
**Cards in scope:** 0001-survey
**Output spec:** .orbit/specs/2026-06-20-survey-detection-and-load/spec.yaml (to be crystallised by /orb:spec)

---

## Capability ambition

Turn a pile of unfamiliar files into a reviewable, deterministic recipe that lands each cleanly into DuckDB — preferring SQL, and recording what it chose and why. The make-or-break underneath: **detection**. If survey guesses the structure wrong, the analyst does the work by hand and the package loses its reason to exist.

## Goal (Q1)

`dovetail survey <paths>` on the SQL-native format set (CSV, TSV, Parquet, NDJSON, top-level JSON) emits, per file, a `datapackage.json` resource plus a standalone `.sql` load that DuckDB can run unattended, and reports that it took the SQL rung. The first cut proves detection → descriptor → emitted SQL end-to-end. jaq escalation, calcard, calamine are deferred to later survey specs.

## Values (Q2)

- **Load-bearing (capability precondition): detection reliability.** Everything rests on survey identifying format and row-level structure well enough to emit unattended.
- **Output value: legibility of the emitted recipe.** "The plan is the artifact." When a clever-but-opaque recipe competes with a plain readable one, the readable one wins — even if longer.
- Supporting: determinism / discovery-execution parity (choice 0006), least-invasive rung (choice 0004).

## Trade-offs (Q3)

- **Format breadth → depth.** SQL-native set only; jaq/calcard/calamine deferred. *Acceptable.*
- **Ship-a-build-fast → an upfront detection spike.** *Expensive-but-worth-it* — detection is the whole point, so it earns the comparison.
- **Clever recipe → plain recipe.** Reject opaque SQL even when shorter. *Acceptable* (it is the value).
- **No abstraction → a `Detector` trait before a second algorithm ships.** *Acceptable* — the trait is the vehicle for the exploration, not speculative generality.

Verification posture: every scenario here `verifies: capability` — survey emits real artifacts (`.sql`, `datapackage.json`) we run against real DuckDB and validate against the real Frictionless profiles. No stand-ins.

## Failure modes (Q4)

- **Misdetects row-level structure → emits a recipe that loads the wrong or empty table.** *Halt-worthy* — the core risk. Trigger: round-trip row-count / column-set mismatch against a fixture's expected manifest.
- **Silently picks a duplicate-key / duplicate-column policy → data loss.** *Halt-worthy* — must surface and make the policy explicit, never silently choose.
- **Emits `.sql` that doesn't parse or run.** *Halt-worthy.*
- finetype profiling slow on large samples. *Hygiene* — cap sample size.
- `Detector` trait churn / over-abstraction. *Hygiene.*

## Lateral approaches (Q5) — held in reserve

- **Load into DuckDB and introspect the loaded table.** Rejected as the default: violates emit-don't-execute (choice 0001). A scratch-load *for detection sampling only* is a grey area — note it if recursion needs it; it must not become the emit path.
- **Pure shape-heuristic detection, no finetype.** Not rejected — kept as a second `Detector` impl for the head-to-head. That comparison is the point of the trait.
- **Suggest-and-confirm (analyst confirms structure).** Not the default — it is the *pivot path* if the detection kill condition fires (see Q10).

## Success criteria (Q6) — binary, measurable

1. On the fixture corpus, the chosen detector identifies the correct row-level structure for ≥ a threshold (pin in /orb:spec, e.g. ≥90% of SQL-native fixtures).
2. For each supported input, the emitted `.sql` runs against DuckDB and yields a table whose row count and column set match the fixture's expected manifest (round-trip).
3. Every emitted `datapackage.json` validates against the vendored Frictionless profiles (in the test suite).
4. survey prints the chosen rung (SQL) and the reason for every input.
5. The `Detector` trait has ≥2 implementations evaluated head-to-head, and the winner is justified by the eval numbers.

## Escalation triggers (Q7)

- **Detectors disagree on a fixture's structure, or confidence is below threshold** → halt; surface the file and what each detector saw; propose suggest-and-confirm rather than guess. (Operational form of the kill condition.)
- **Round-trip mismatch during the build phase** → halt; surface input + emitted SQL + expected-vs-actual; do not paper over.
- **finetype's column profile is ambiguous for a candidate table** (table-of-records vs scalar column) → surface the recursion decision rather than picking silently.

## Adjacent code (Q8)

- `dovetail-core`: new `detect` module (`Detector` trait + ≥2 impls, finetype-guided recursive descent first); `emit` (SQL writer + `datapackage.json` assembler); the plan model.
- **finetype as a library dependency** of `dovetail-core` — used in-process for column profiling and for the per-resource Table Schema (`schema` block). This resolves choice 0012 toward *library dependency*.
- **Recursion mechanism:** profile candidate rows → when a column profiles as records (a hidden table), descend and re-profile that sub-structure → assemble the resource (and any nested resources) from the converged result.
- `duckdb` crate: used by the **test** round-trip (running emitted SQL), not by survey itself — survey emits, it does not run. Keeps the choice 0001 line clean; the survey-relate seam (choice 0009) is downstream of this.
- Vendored Frictionless JSON Schema profiles (from `frictionlessdata/datapackage`) for test-time descriptor validation.
- Fixture corpus: curated SQL-native inputs each with an expected manifest (structure, row count, columns).

## Budget (Q9) — Claude-execution pace

- Detection eval phase (fixture corpus + 2–3 `Detector` impls + eval harness): ~1.5 days.
- Build phase (emit `.sql` + `datapackage.json` assembly + finetype Table Schema wiring + rung reporting): ~1.5 days.
- **Total ≈ 3 working days.**

## Kill conditions (Q10)

- **Primary — "detection is reliable enough to emit unattended."** Kill signal: the chosen detector's row-structure hit-rate stays below the bar on the fixture corpus even after iterating. **Pivot:** survey becomes *suggest-and-confirm* — emit a proposed recipe the analyst confirms or edits, rather than emit-and-trust. (Extends choice 0007's human-in-the-loop posture to survey.)
- **Secondary — "most SQL-native loads stay on the .sql rung."** Near-tautological within this MVP's scope; the real test of the SQL-preferred framing (choice 0004) is deferred to the jaq-escalation spec. Flagged so it is not mistaken for proven here.
- **Tertiary — "finetype-as-library recursion is cheap enough."** Kill signal: recursive re-profiling is too slow on nested structure. **Pivot:** cap recursion depth and sample harder.

## Feeds back to choices

- **Choice 0012 (finetype integration)** → resolve to *library dependency of dovetail-core*; finetype is survey's detection engine, used recursively. Move from `proposed` toward `accepted` once /orb:spec lands.
- **Choice 0009 (survey-relate seam)** → this spec keeps survey emit-only (DuckDB runs only in tests), so the seam stays unresolved by design; nothing here forces dovetail to execute.
- **Choice 0002 / 0003 / 0013** → the Frictionless profiles carry `dialect`, `hash`, `bytes`, `created`, and per-resource `schema` (with `foreignKeys` *inside* each Table Schema). Our custom surface shrinks to the load recipe and (for relate) the evidence/confidence/status triplet.

## Hot-wash

- **recurred:** detection quality as the hinge everything hangs on; finetype surfacing as the engine, not a sidecar.
- **surprised:** the Frictionless standard already carries `dialect` / `hash` / `schema`, shrinking our custom surface; `foreignKeys` living *inside* per-resource Table Schemas rather than at package level.
- **friction:** the Q8 "do we validate?" fork half-dissolved once finetype owns the schema — the real question was *who produces the schema*, not whether we validate.
- **meta-patterns-for-future-tabletops:** when a card names an output format that has a published standard, read the standard's profiles *before* the tabletop — it reshapes the forks. Did that here; it paid off.
