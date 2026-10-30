# Role fixtures

Each directory holds a deterministic `build.sql` and an `answer-key.json` that says what role every table should get. Later tests score dovetail's role calls against the key.

- `retail/` is a small retail business at realistic size, using Brightfield's stand-in column names. Its tables cover every role but `bridge`, plus three pairs that combine: a partition (`suppliers_north`/`suppliers_south`), and two snapshots (`price_list_2024`/`price_list_2025`, `members_2024`/`members_2025`).

## Answer-key format

- `tables`: the table names as they stand after combination, each with a `role` (`entity`, `event`, `reference`, `bridge`, `snapshot`, `unclear`) and a one-line `why`. Names match case-insensitively.
- `folds`: a `reference` table folded `into` the table that alone points at it.
- `combinations`: a combined `name`, its `parts`, and a `kind` (`partition`, `snapshot`, `duplicate`).

## The eval record

`crates/dovetail-core/tests/role_eval_harness.rs` scores each fixture against its key and checks `tests/role-corpus/RESULTS.md` is current. Tier 1 (`retail`) must be exact under FineType's profile; its floor-only run is recorded beside it. CI runs both: the floor in the main test run, and the profile in a step that fetches the pinned extension and model with `scripts/fetch-finetype.sh`. Only the profiled run rewrites the record, and it writes both typings' results. To rewrite it after a change to the scorer, source the two exports that script prints and run `DOVETAIL_WRITE_ROLE_RESULTS=1 cargo test -p dovetail-core --test role_eval_harness tier1_record_is_current`.
