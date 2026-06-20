# ac-04 — Detector decision

**Date:** 2026-06-21
**Spec:** 2026-06-20-survey-detection-and-load
**Eval record:** [eval-results.md](./eval-results.md)

## Decision

**survey builds on `FinetypeGuidedDetector` as the canonical detector**, with
`ShapeHeuristicDetector` retained as its structural core and as the no-finetype
comparison arm.

The finetype-guided detector is a strict superset of the shape detector: it
takes the same structural read (format, row-level structure, column set) and
adds a semantic type per column from finetype-model when a model directory is
configured. With no model dir it degrades to *exactly* the shape detector's
behaviour. So choosing it costs nothing on the structural axis the eval scores,
and gains semantic typing for the resource Table Schema (ac-06) and, later,
relate's evidence (card 0002-relate). This realises choice 0012.

## Numbers that justify it

From the eval over the 7-fixture SQL-native corpus:

| detector | mode | row-structure hit-rate |
|---|---|---|
| shape-heuristic | structural | 100% (7/7) |
| finetype-guided | degraded (no model dir) | 100% (7/7) |

Both **clear the fixed ≥90% detection-quality bar** set in ac-10 (the bar is
fixed there; this note confirms the chosen detector clears it, per review-spec
finding 1). Structural parity is expected — finetype-guided composes the shape
detector for structure.

## Honest limitation (carried forward)

The head-to-head does **not** yet show the model-backed arm *outperforming* the
structural one, for two reasons:

1. The MVP corpus is the SQL-native set, which is mostly flat — the recursive
   descent into nested records (the case where finetype profiling earns its
   keep) isn't stressed by these fixtures.
2. No finetype model artifacts are wired in this environment, so the
   finetype-guided arm ran in **degraded (structural-only)** mode.

A model-backed eval — set `DOVETAIL_FINETYPE_MODEL_DIR` to real artifacts and
add nested-structure fixtures — is the follow-up that would let the semantic
and recursive-descent advantages show in the numbers. Logged as a memo for
distill rather than expanded here, to keep the MVP scoped.

The decision stands regardless: on structure both are equal and both clear the
bar, and finetype-guided strictly dominates once artifacts are present.
