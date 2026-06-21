# Survey MVP — review-pr follow-ups

Non-blocking LOW findings from review-pr 2026-06-21 (spec
2026-06-20-survey-detection-and-load APPROVE). Candidates for a follow-up spec
against card 0001-survey.

1. **CLI ships the shape detector, not the canonical finetype-guided one.**
   `crates/dovetail/Cargo.toml` pins `dovetail-core` with `default-features =
   false`, so `dovetail survey` runs `ShapeHeuristicDetector`. Behaviourally a
   no-op today (finetype-guided degrades to exactly the shape read with no model
   dir), but it's the gap between the detector ac-04 picked and the detector that
   ships. Close it by wiring a `--finetype-guided` / model-dir flag into the CLI.

2. **ac-11's JSON duplicate-*key* half is unexercised.** The CSV duplicate-
   *column* case is tested and handled (rename projection). serde_json collapses
   repeated object keys, and there's no JSON dup-key fixture, so the "never
   silently drop" guarantee isn't tested on the JSON side. Add a fixture + a
   raw-key pass that surfaces JSON dup keys.

3. **All fixtures derive table name `data`** from the file stem — harmless in the
   corpus, would collide on same-stem real files. Consider qualifying the
   resource/table name by parent dir or a uniquifier.

Pairs with [the model-backed detection eval memo](./2026-06-21-model-backed-detection-eval.md):
both want a follow-up survey spec once the MVP lands.
