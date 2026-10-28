# Role fixtures

Each directory holds a deterministic `build.sql` and an `answer-key.json` that says what role every table should get. Later tests score dovetail's role calls against the key.

- `retail/` is a small retail business at realistic size, using Brightfield's stand-in column names. Its tables cover every role, plus three pairs that combine: a partition (`suppliers_north`/`suppliers_south`), and two snapshots (`price_list_2024`/`price_list_2025`, `members_2024`/`members_2025`).

## Answer-key format

- `tables`: the table names as they stand after combination, each with a `role` (`entity`, `event`, `reference`, `bridge`, `snapshot`, `unclear`) and a one-line `why`. Names match case-insensitively.
- `folds`: a `reference` table folded `into` the table that alone points at it.
- `combinations`: a combined `name`, its `parts`, and a `kind` (`partition`, `snapshot`, `duplicate`).
