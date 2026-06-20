# Follow-up: model-backed detection eval

Discovered while implementing spec 2026-06-20-survey-detection-and-load (ac-03/ac-04).

The detection eval currently runs the finetype-guided detector in **degraded
(structural-only) mode** because no finetype model artifacts are wired into the
dovetail build environment, and the SQL-native fixture corpus is mostly flat —
so the model-backed arm's two real advantages never show in the numbers:

1. **Semantic column typing** — needs `DOVETAIL_FINETYPE_MODEL_DIR` pointed at a
   real finetype model dir (fusion layout: `fusion_manifest.json` →
   value_model / mb_model / head).
2. **Recursive descent into nested records** — needs fixtures with a column that
   is itself an array of objects (a hidden table), which the SQL-native MVP set
   deliberately excludes.

A follow-up spec against card 0001-survey would: add nested-structure fixtures
with manifests, wire a model dir into the eval (or a CI artifact), and re-run
the head-to-head so the finetype-guided arm can be measured *outperforming* the
shape baseline rather than merely matching it.

Not blocking the MVP: on the SQL-native structural axis both detectors clear the
≥90% bar, and finetype-guided strictly dominates once artifacts are present.
