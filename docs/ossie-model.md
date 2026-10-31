# The Ossie model

`dovetail model` writes one Apache Ossie model across a DuckDB's tables: datasets with their keys and fields, relationships with their keys, and Meridian's vendor extension on each, carrying every table's role, every join's grade and evidence, and the folds and combinations that table roles propose. It runs in two passes. A quick pass over a sample writes the model first; a full pass then re-measures it on every row, rewriting only grades and evidence. Nothing an analyst decided is changed. The code is `crates/dovetail-core/src/ossie/` (`wire.rs` the wire types, `project.rs` the projection from the role model, `run.rs` the passes, `verify.rs` the full pass, `carry.rs` the analyst's decisions), `packages.rs` (data packages and typing) and the `model` subcommand in `crates/dovetail`. Role scoring is described in [table-roles.md](table-roles.md).

Brightfield opens an analyst's own files, loads each table, profiles it with FineType, and draws a model the analyst reshapes. dovetail is its discoverer: Brightfield links dovetail into its binary and runs it as a child process, and the two talk through files. Data packages go in, an Ossie model comes out. The sample pass exists so the analyst sees a first model within seconds of opening their files, whatever their size. The full pass exists so that what they see is then settled on every row without the model jumping about under them: it confirms or weakens what the sample proposed and never rearranges it.

The model is Ossie `0.2.0.dev0`, core schema at `apache/ossie` commit `d13bb6a534a100e792a048dd7a14d4d37c723b2c`, vendored under `vendor/ossie/` with its licence for the conformance test. `from` is the many side; `from_columns` and `to_columns` name declared fields; everything Meridian-specific goes in `custom_extensions` entries of `{vendor_name: MERIDIAN, data: <JSON string>}`. The Ossie model is a projection of the run's result, as the Frictionless descriptor `dovetail relate --out` writes is. Neither is derived from the other.

## The command

```
dovetail model --db <duckdb> --pass sample|full --out <file>
               [--packages <dir>] [--base <model.ossie.yaml>]
               [--verify <sample proposal>] [--name <model name>]
```

- `--db`: the DuckDB holding the tables, attached read-only as everywhere else.
- `--pass`: `sample` or `full`.
- `--out`: where the proposal goes. dovetail writes `<out>.writing` beside it and renames it into place, so a reader sees the old file or the new one, never part of either. A write that fails removes `<out>.writing`; a run killed while writing can leave a stale `<out>.writing` beside an untouched `--out`, and the next run overwrites it.
- `--packages`: a directory of data packages (`<dir>/*/datapackage.json`). When given, the packages name the datasets and type the fields.
- `--base`: the model the caller holds. Both passes carry its analyst decisions through; nothing else is read from it.
- `--verify`: for the full pass, the sample pass's own proposal, which it verifies. With the sample pass it is ignored, with a warning.
- `--name`: the model's name; defaults to the `--db` file's stem.

Exit codes follow the other commands: `0` when the model is written, `1` when it could not be (the reason on stderr, prefixed `dovetail model:`), `2` for a usage error. Warnings about the run (typing fell back to the floor, a package resource with no table, `--verify` ignored) go to stderr and do not change the exit code.

The `dovetail` crate also exposes `pub fn cli_main() -> ExitCode`, which is the binary's `main`, so a host that links dovetail can hand its child process to the CLI. Built with `default-features = false`, everything `dovetail model` does works without the candle classifier.

## Datasets and typing

**With `--packages`** the packages are the list of tables. Each resource is one dataset:

- `name` is the resource's `name`.
- `source` is `raw_<name>`, which must be a table in `--db`; a resource with no table is skipped with a warning. The prefix is one named constant, `RAW_PREFIX`, since it is Brightfield's loading convention and not dovetail's.
- `source_file` is the resource's `path` resolved against its package directory, made relative to the directory holding `packages/` when it lies inside it; `source_part` is the resource's `x-part`, naming a sheet or database table.
- The other tables in the database (silver and failed tables, anything else) are not datasets and are not measured.

A field's FineType type is its `x-finetype-label`. The run's typing source is recorded as `packages`, beside profiled and floor, and every role's evidence names the source it used.

**Without `--packages`** every table in the database is a dataset named and sourced as itself, typed as `dovetail relate` types it: FineType's extension when the pin, the model and the probe all hold, else the floor.

**Lineage columns are never measured.** The three lineage columns (`__source_file`, `__source_part`, `__source_row`) are dropped before any evidence is taken, in both modes, and never become fields.

**Fields.** One field per remaining column, in table order: `expression.sql` is the column name, and the Meridian extension is `{finetype?, decided_by: dovetail}`. `datatype` is written only for a field with a FineType type, from the package's `type`:

| Package type | `datatype` |
|---|---|
| `string` | `String` |
| `integer` | `Integer` |
| `number` | `Decimal` when the DuckDB column is `DECIMAL`, else `Float` |
| `boolean` | `Boolean` |
| `date` | `Date` |
| `time` | `Time` |
| `datetime` | `DateTime`, or `DateTimeTz` when the DuckDB column is `TIMESTAMP WITH TIME ZONE` |
| anything else | none |

Without packages the same map runs over the Frictionless type of FineType's label, as `frictionless_for` gives it.

**Time fields.** A role's event time or snapshot date gets `dimension: {is_time: true}`; each audit time field gets `dimension: {is_time: false}`. Other fields write no `dimension`, leaving Ossie's default.

**Names in SQL.** A name that is a DuckDB keyword, in any category but unreserved, or that is not a plain identifier, is quoted in `source` and `expression.sql`.

## The Meridian extension on each element

dovetail writes every key Brightfield's `brightfield-ossie` crate defines, with the same names, plus the keys marked *new*, which Brightfield ignores until it reads them. The wire order is the crate's: model `version, name, description, datasets, relationships, metrics, custom_extensions`; dataset `name, source, description, primary_key, fields, metrics, custom_extensions`; field `name, expression, datatype, custom_extensions`; relationship `name, from, to, from_columns, to_columns, custom_extensions`.

**Dataset**: `role`; `role_evidence` (the role call's sentences); *new* `role_scores` (all six roles, in the role enum's order); `folded_into` (the host, for a folded reference); *new* `fold_column` (the struct column it folds into); *new* `combined_into` (the combined dataset, for a part); *new* `combination` (`{kind, parts, token_column}`, on a combined dataset); `excluded: false`; `decided_by: dovetail`; `source_file`; `source_part`; `rows`. `primary_key`, which is core Ossie and not the extension, is the role call's key.

**Relationship**: `grade`; `evidence` `{value_overlap, orphans, child_rows, child_type?, parent_type?, types_agree, name_similarity, parent_duplicates}`; `cardinality`; `verified` (`sample` or `full`); `decided_by: dovetail`; `warning`.

- `child_type` and `parent_type` are the FineType types of the two columns, omitted when a side is untyped. `types_agree` is true when both are typed with the same type, or both untyped with the same DuckDB type.
- `cardinality` is `N:1`, or `1:1` when the child column is also unique.
- `name_similarity` is `relate`'s. `parent_duplicates` counts parent key values held by more than one row, as Brightfield's own measuring does.
- The name is `<from>_<to>`; when one pair has more than one join, each takes `_<from column>` after it.

**Model**: Brightfield's `ModelMeta` (`generator`, `rejected`, `skipped_sources`) plus, *new*, the provenance every artifact carries: `pass`, `typing` (the source, and the FineType tag and model hash when profiled) and `sample_rows`. The dovetail version is inside `generator`, which is `dovetail <version>`. `sample_rows` is the sample size the roles were scored on: `50000` from the sample pass, kept by a full pass that verifies that sample, since the roles it carries were scored there, and absent when the full pass discovers afresh on every row.

## Combinations and folds

- A **combined dataset** is a dataset of its own. Its `source` is the combination's `UNION ALL BY NAME` query written over the parts' `raw_` tables (not over dovetail's sampled copies); its fields are the combined columns plus the token column; its role and key are the combined call's; its `source_file` is the parts' files joined by `, ` in part order. Its **parts are datasets too**, each with `combined_into` set, its own fields, and the combined dataset's role with the evidence sentence "combined into <name>". Relationships point at the combined dataset, never at a part, because a join to one part alone is the wrong measurement: half its keys are in the other part.
- A **folded reference** stays a dataset with `folded_into` and `fold_column`; its relationship to its host is written as any other.
- A **carried key** (a child carrying a snapshot's entity key) is no relationship. Its sentence is in both datasets' `role_evidence`.

Brightfield does not yet read combinations or the fold column; see *What Brightfield needs to change*.

## The sample pass

The sample is at most `SAMPLE_ROWS` = 50,000 rows of each table, drawn with DuckDB's reservoir sample at a fixed seed on one thread, so the same table always gives the same sample. A table at or under that size is read whole. Everything the pass writes says `verified: sample`.

A sample cannot answer a question about a parent's whole key. A child sampled against a sampled parent finds most of its keys missing, and every join looks orphaned. So the pass samples what is read row by row (column evidence, combinations, the child side of every candidate join) and measures against the full table anything about a key as a whole: a parent's distinct key set for overlap and orphans, a parent's uniqueness, and each table's row count. A combined parent is read through a second view of its query over its parts' full tables. Those are single aggregates DuckDB answers quickly; overlap probes the full parent column with the child's sampled values, so the parent is scanned once rather than hashed whole. The pass runs in an in-memory database holding the sampled copies as `src`, with the analyst's database attached read-only beside it for those full-table measurements.

A table's own key is still found on its sample, where a key can look unique and not be. The full pass checks it.

## The full pass

The full pass verifies the sample pass's proposal, read from `--verify`, and writes it back with:

- each relationship whose `decided_by` is `dovetail` re-measured on every row: its `evidence`, its `grade` from `relate`'s status on the full data, `verified: full`;
- each dataset whose `decided_by` is `dovetail` with its `rows` measured on every row and its `primary_key` checked: a key that is not unique, or holds an empty value, on the full data is removed from `primary_key`, and a sentence saying so is added to `role_evidence`.

It changes nothing else: roles, folds, combinations, fields, every analyst-decided element, every `excluded` flag, `rejected`, `skipped_sources`, and any keys dovetail does not know. The role is not re-scored, and no relationship appears or disappears.

A join the full data no longer bears out stays in the model, graded `possible`, with a `warning` naming each cause ("on the full data, 12 of 4,812 rows of orders point at no row of customers"). The full pass re-measures cardinality too, and any join whose grade falls carries a warning naming each cause. A dataset check that fails says so in its evidence.

It verifies dovetail's own sample proposal, not the caller's held model, and **discovers afresh** on every row, with `verified: full`, when there is nothing current to verify: no `--verify`, a proposal that cannot be read or was not written by dovetail (`generator` not `dovetail ...`), one whose datasets are not the run's datasets, one that is not a sample pass's proposal (a full pass's model is not verified again), one rejecting a join that the current `--base` no longer rejects (the join was never proposed, so it must be discovered), or one holding an analyst-decided dataset, field or join that the current `--base` no longer holds exactly as written (or any analyst-decided element when no `--base` is given), because carry would otherwise bring back what the analyst has undone. A base that holds an analyst dataset covers all that dataset's fields. That also serves an analyst running dovetail directly.

## What both passes carry through

When `--base` is given, both passes:

- write each analyst-decided dataset, field and relationship back exactly as given, in its place;
- never write a relationship matching (by `from`, `from_columns`, `to`, `to_columns`) one in `meta.rejected`;
- copy `meta.rejected` and `meta.skipped_sources` through.

"Exactly as given" is tested by round trip: an analyst element read from `--base` and written by dovetail is byte-identical in its YAML, so dovetail's types keep the keys they do not know, as Brightfield's do. The analyst's elements override the sample's elements of the same name or keys.

## Rulings

Each decision, with its reason.

- **dovetail owns its Ossie types.** The wire types live in `dovetail-core`, checked against the vendored schema and a golden file. Depending on Brightfield's crate would make a cycle between the two repos, and a shared crate is worth building once a third reader of Ossie models appears. The cost is two copies of the shapes, which Brightfield's own test of dovetail's output catches if they drift.
- **Paths are explicit.** dovetail does not derive anything from a project folder or learn Brightfield's layout; the caller resolves its database and passes each path. The same command then serves an analyst running dovetail on any DuckDB.
- **A field a package does not label is untyped, never floor-typed.** The caller leaves a label out when the column's own data contradicts it, and a floor label there would disagree with the profile the analyst has already seen. A combined dataset's columns take their parts' package labels. Its token column, which no package describes, is labelled by the combination from the token's own type: a year (`INTEGER`) is `datetime.component.year` and a date `datetime.date.iso`. That label is what the combination knows, not a floor guess; the token column takes its `datatype` from that label through FineType's map, since no package gives it a type: a date token labelled `datetime.date.iso` maps to Frictionless `date` and is written `datatype: Date`, while a year token labelled `datetime.component.year` maps to Frictionless `year`, which the map does not cover, so it has none.
- **`datatype` is written only for a typed field.** An untyped field claiming a datatype would be a guess. For `number`, the DuckDB type decides between `Decimal` and `Float`, because the package's type covers `DECIMAL`, `FLOAT` and `DOUBLE` alike.
- **Lineage columns are dropped before measuring.** `__source_row` is unique and dense in every table and would otherwise be found as every table's key.
- **Two package resources whose names collide are refused, naming both files.** DuckDB treats names case-insensitively, so the collision would otherwise land two datasets on one table.
- **Grades map from `relate`'s statuses without a new threshold.** Accepted is `likely`, suggested is `possible`, rejected is not written. A join needs a unique parent key to be written at all, since the one side of a relationship must be unique.
- **`child_type` and `parent_type` are FineType types**, not DuckDB types, because that is what the profile the analyst sees shows.
- **Relationship names are stable and made unique deterministically.** The caller matches by keys and keeps the name, so a name must not move between runs. When an analyst's join shares a name with a proposed one, the analyst's keeps it and the proposed one takes a suffix.
- **An analyst's join is kept even if it matches a rejected entry.** The analyst's own decision outranks the memory of a rejection.
- **Parts of a combination stay datasets.** Every source table is then a dataset, and a part the analyst wants kept apart is one edit.
- **A combined dataset is a query over the `raw_` tables.** It is written as the table roles design rules, and its source cannot name the sampled copies, which exist only inside a run.
- **Under sampling, every dataset's row count is the full count.** A combination's is the sum of the parts its view reads, and a duplicate combination reads one part. Evidence sentences say when a figure was measured on a sample ("in a sample of N of M rows"), so a sample figure is never read as a verified one.
- **The sample samples rows and measures keys on the full table**, because a sampled parent makes every join look orphaned.
- **The sample is a fixed reservoir at one seed on one thread**, so the same inputs give a byte-identical model.
- **Names that are DuckDB keywords or not plain identifiers are quoted**, in any keyword category but unreserved, so the emitted `source` and `expression.sql` run as written.
- **The full pass weakens and never removes.** A join the full data no longer bears out stays, graded `possible`, with a warning. Removing it would rearrange the analyst's view under them; a weakened grade with its reason lets them decide.
- **The full pass re-measures cardinality, and every fall in grade names each cause**, so the analyst sees why rather than only that.
- **A key that repeats or is empty on the full data is dropped, with a sentence.** A key found on a sample can be unique by luck; a primary key that is not one is worse than none.
- **The full pass verifies dovetail's own sample proposal, not the held model.** The held model is written by the caller, whose merge keeps its own `generator` and whose reader drops Meridian keys it does not know, so nothing in it says reliably which elements dovetail proposed. A stand-in model with every table unclear and no joins would otherwise be "verified" into a model with no joins at all. The analyst's decisions still come from `--base`.
- **`--verify` with the sample pass is ignored with a warning**, since a sample pass proposes and has nothing to verify.
- **`meta.rejected` and `meta.skipped_sources` are copied through.** The caller's merge takes `skipped_sources` from the proposal, so a proposal without them would erase the analyst's list of files it passed over.
- **The goldens are built by `run_model`, one for each pass, and none for floor typing.** `retail.sample.ossie.yaml` is a real sample pass over the retail packages fixture, and `retail.full.ossie.yaml` the full pass verifying it, so the golden holds what the command writes rather than a model assembled by hand. A floor-typed golden would change with every FineType release, and Brightfield, which always passes `--packages`, never runs that path.
- **The write is atomic**, through `<out>.writing` and a rename, so a reader never sees a half-written proposal.

## What Brightfield needs to change

The hand-off contract. Brightfield's side, not done here:

1. Read the new keys: `role_scores`, `fold_column`, `combined_into`, `combination`, and the model's provenance, so a marginal role can be shown as marginal and a combination split again.
2. **Combined datasets in silver**: build a dataset from its `source` when it is a query rather than from `raw_<name>`, give a part with `combined_into` no silver table of its own, and measure joins against the combined table. Until then a model with a combination breaks the silver build.
3. **Folds as one struct column**: `fold_column` names it. Silver folds today into flat `<reference>_<field>` columns.
4. The `Dovetail` implementation: run `dovetail model` as a child with `--db` (the Protocol database path), `--packages <protocol>/packages`, `--base <protocol>/model.ossie.yaml` when it exists, `--pass`, `--out` the pass's proposal file, and, for the full pass, `--verify <protocol>/.discovery/sample.ossie.yaml`; kill it on cancel.
5. Decide which warning stands when dovetail's full-pass warning and Brightfield's silver measurement both write `warning`. dovetail writes one only when the full data weakens a join it proposed.

## Tests

The model's tests are in `crates/dovetail-core/tests/` and `crates/dovetail/tests/`. `cargo test -p dovetail-core --no-default-features` and `cargo test -p dovetail --no-default-features` run them without the candle classifier; CI runs the whole workspace both with default features and without.

- `ossie_golden.rs`: the two goldens, `tests/ossie-fixtures/retail.sample.ossie.yaml` (a sample pass over the retail fixture with its packages) and `retail.full.ossie.yaml` (the full pass verifying that sample), each built by `run_model`, read back unchanged and validated against the vendored schema. A second test walks the JSON in each `MERIDIAN` `data` string of both goldens and checks every key Brightfield's crate defines: a key Brightfield requires is on every element with its JSON type and enum values, an optional key has its type wherever it is written, and the optional keys the retail goldens never write (`rejected`, `skipped_sources`, `source_part`, `parent_duplicates`, `warning`) are listed, so a change that starts writing one says so.
- `ossie_wire.rs`: Brightfield's own fixture reads and validates; write then read is the identity; unknown keys survive; keys are written in the wire order.
- `ossie_session.rs`: packages name and type the datasets, lineage columns are never measured, an unlabelled field is untyped, colliding resources are refused, and the sample is a repeatable copy with the full data still readable.
- `ossie_sample.rs`: a sampled session (samples of 50 to 500 rows) measures a parent's whole key and every row count on the full data, a combination parent through its parts' full tables, so its joins are accepted with no orphans; duplicates are counted; the sample repeats.
- `ossie_project.rs`: the projection over the retail fixture: datasets, combinations with their parts and a `source` that runs, time fields, datatypes, quoting, unique relationship names.
- `ossie_carry.rs`: `carry` over the sample golden: analyst elements byte-identical, the rejected join not proposed, `excluded`, `rejected` and `skipped_sources` carried.
- `ossie_passes.rs`: the full pass rearranges nothing, re-measures cardinality, warns on a repeated parent key and drops an empty key; it discovers afresh with no `--verify`, a stand-in, a stale proposal, one naming a removed table or a missing part, and an unreadable file; both passes are byte-identical run to run; and both passes, given a `--base` holding an analyst's dataset, field and join and a rejected join, write the analyst's elements byte-identical and leave the rejected join out.
- `crates/dovetail/tests/model_cli.rs`: the command: a sample then full pass, usage errors exit `2`, a missing database exits `1` and leaves neither `--out` nor `<out>.writing`, `--base` is never written, `--verify` with the sample pass warns and is ignored, the help names every flag, and `--name` defaults to the database's stem.
