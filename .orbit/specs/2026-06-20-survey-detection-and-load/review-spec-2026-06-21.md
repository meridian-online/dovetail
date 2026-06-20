# Spec Review

**Date:** 2026-06-21
**Reviewer:** Context-separated agent (fresh session)
**Spec:** 2026-06-20-survey-detection-and-load
**Verdict:** APPROVE

---

## Review Depth

| Pass | Triggered by | Findings |
|------|-------------|----------|
| 1 — Structural scan | always | 2 |
| 2 — Assumption & failure | content signals (eval dataset / ground-truth corpus; cross-system DuckDB + finetype boundary) | 2 |
| 3 — Adversarial | not triggered | — |

Pass 3 was not reached: no contradicted assumptions, no cascading failure mode, no untestable AC. The Pass 1/2 findings are LOW–MEDIUM and addressable in implementation without reworking the plan.

---

## Findings

### [MEDIUM] The 90% gate bar is hard-coded in ac-10 but ac-04 is told to set it
**Category:** constraint-conflict
**Pass:** 1
**Description:** ac-04 instructs the implementer to "record the choice with the hit-rates that justify it **and the bar it must clear in ac-10**", i.e. ac-04's decision note nominates the bar. But ac-10 already pins the bar at a literal `>= 90%`. So the bar is both derived (ac-04) and fixed (ac-10). If the eval lands the chosen detector at, say, 87% and the implementer decides 85% is the defensible bar, ac-04 and ac-10 now disagree and the gate fires on a number ac-04 was authorised to move.
**Evidence:** ac-04 text ("the bar it must clear in ac-10"); ac-10 text ("correct row-structure rate is >= 90%"). The tabletop frames 90% as an example, not a commitment — Q6.1 reads "≥ a threshold (pin in /orb:spec, e.g. ≥90%)" and Q10 names the kill condition as "stays below the bar … even after iterating" without pinning a number.
**Recommendation:** Treat 90% as fixed and reword ac-04 so it justifies *clearing the pinned 90% bar* (not *setting* it) — i.e. ac-04 records which detector clears 90% and why, and if none does, that is the kill-condition pivot to suggest-and-confirm (already ac-10's else-branch). Alternatively, if the bar is genuinely meant to be chosen from eval numbers, make ac-10 reference "the bar recorded in ac-04" rather than a literal. Either is fine; pick one so the two ACs cannot disagree. Low-cost wording fix, not a structural change.

### [LOW] "Correct row-structure rate" denominator is undefined for nested/recursive structures
**Category:** test-gap
**Pass:** 1
**Description:** ac-10's hit-rate is "correct structure / total fixtures" — one boolean per fixture. But the finetype-guided detector (ac-02a) recurses: a fixture can have a top-level table *plus* a nested hidden table. Is such a fixture "correct" only when every level is right, or scored per-level? The metric's grain is unspecified, which makes the 90% gate's meaning ambiguous on exactly the cases (nested records) the detector is built to handle.
**Evidence:** ac-02 ("when a column profiles as records … descend and re-profile"); ac-03 ("correct structure / total fixtures"); ac-01 includes "a JSON file that is an array of flat records" and "a single top-level object" but the manifest grain for nested cases is not stated.
**Recommendation:** In ac-03/ac-10 implementation, define "correct structure" as exact match of the full detected Structure (all levels) against the fixture manifest — all-or-nothing per fixture. Record this in the ac-03 results table so a partial-credit reading can't creep in. No spec edit strictly required; capture it in the eval harness and the ac-04 decision note.

### [LOW] Detection sampling vs. emit-don't-execute grey area is noted but not bounded by an AC
**Category:** assumption
**Pass:** 2
**Description:** The tabletop (Q5) explicitly flags that a scratch-load "for detection sampling only" is a grey area against choice 0001 (emit, don't execute) and says it "must not become the emit path." No AC enforces that the *detector* reads bytes/samples directly rather than scratch-loading via DuckDB. The constraint is sound and the round-trip (ac-07) guarantees DuckDB-in-tests-only for the *load path*, but nothing pins the *detection* path away from execution.
**Evidence:** Tabletop Q5 ("A scratch-load for detection sampling only is a grey area … it must not become the emit path"); spec Constraints ("DuckDB runs only in the test round-trip, never as survey's load path") — phrased around the *load* path, silent on detection sampling.
**Recommendation:** Acceptable as-is for an MVP — the finetype-guided detector profiles sampled rows in-process (choice 0012), so the natural implementation already avoids DuckDB at detection time. If the implementer reaches for a scratch DuckDB to sample, that is the moment to surface it. No AC needed; flagging so the reviewer of the PR watches for a DuckDB dependency leaking into survey's detection code path.

### [LOW] ac-11 duplicate-policy default is unpinned across formats
**Category:** missing-requirement
**Pass:** 2
**Description:** ac-11 requires duplication to be *surfaced* with an *explicit recorded policy* (keep-last / keep-first / collect-to-array / rename) — good, and it correctly forbids silent drops. It does not say which policy is the default, nor whether the default differs by format (DuckDB's `read_csv` and `read_json` have their own native duplicate-column behaviours). The AC as written is satisfiable by recording *a* policy, so this is not a gap in testability — only in determinism of the chosen default.
**Evidence:** ac-11 text; ac-01 deliberately includes "a CSV with duplicate column names" as a fixture, so the case is exercised. Failure mode Q4 ("Silently picks a duplicate-key/column policy → data loss. Halt-worthy").
**Recommendation:** Pick a conservative default (rename — it loses no data and is the least surprising in emitted SQL) and record it in the datapackage custom properties so the round-trip (ac-07) column-set check is deterministic. Implementation detail; ac-11 as written already prevents the halt-worthy silent-drop, which is what matters.

---

## Honest Assessment

This plan is ready to implement. It is unusually well-formed: it leads with the make-or-break (detection), proves it with a measured head-to-head before building on the winner, and bakes the kill condition into a gate AC (ac-10) with a pre-committed pivot (suggest-and-confirm) rather than an open-ended halt. The spike→build sequencing, the emit-don't-execute discipline (DuckDB in tests only), and the conformance check against vendored Frictionless profiles all give the build real, runnable verification rather than stand-ins. All three gate ACs (ac-04, ac-07, ac-10) pass the deterministic description checks — non-empty, no placeholder token, comfortably over the length floor — and each names a concrete, inspectable artefact.

The biggest risk is not in the spec's structure but in the one number it pins: the 90% bar (Finding 1). Because ac-04 is told to *justify* the bar while ac-10 *fixes* it, an honest eval result in the 85–89% band creates an ambiguity about whether the gate fires or the bar moves. That is a one-line wording reconciliation, not a reason to send the spec back — hence APPROVE rather than REQUEST_CHANGES. The other findings (metric grain on nested structures, detection-sampling boundary, duplicate-policy default) are implementation-time decisions the build should record in the ac-03 results table and the datapackage properties; none blocks starting.
