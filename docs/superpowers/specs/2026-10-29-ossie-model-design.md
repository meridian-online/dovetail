# dovetail writes an Apache Ossie model of an analyst's tables, from a sample first and then verified on the full data

Design for a new `dovetail model` command: it reads a loaded DuckDB and, optionally, FineType's per-source data packages, and writes one Apache Ossie model across the tables, carrying each table's role, each join's grade and evidence, and the folds and combinations that table roles propose. A quick pass over a sample writes the model first; a full pass then re-measures it on every row, rewriting only grades and evidence. Nothing an analyst decided is changed.

## Intent

Brightfield opens an analyst's own files as a Protocol, loads each table, profiles it with FineType, and draws a model the analyst reshapes. Until now its discovery is a stand-in that lists every table as unclear with no joins. dovetail is the real discoverer: Brightfield links dovetail into its binary and runs it as a child process, the way it runs arc, and the two talk through files. Data packages go in, an Ossie model comes out. This design is dovetail's half of that hand-off: the command, the model it writes, and the two passes.

The sample pass exists so the analyst sees a first model within seconds of opening their files, whatever their size. The full pass exists so that what the analyst sees is then settled on every row, without the model jumping about under them: it confirms or weakens what the sample proposed and never rearranges it.

## Goals

1. **`dovetail model`**, a command that writes one Ossie `0.2.0.dev0` model across a DuckDB's tables: datasets with primary keys and fields, relationships with their keys, and Meridian's vendor extension on each.
2. **Typing from FineType's data packages** when they are given, so dovetail built with default features off types every field the way Brightfield's profile did, with no classifier and no FineType extension.
3. **Two passes.** `--pass sample` proposes the whole model from a bounded sample of each table, quickly. `--pass full` re-measures the sample's proposal on every row and rewrites only the relationships' grades and evidence and the datasets' measured facts.
4. **The analyst wins.** An element whose `decided_by` is `analyst` is written back exactly as it was given, in both passes, and a join the analyst rejected is never proposed again.
5. **A library entry point** so Brightfield can hand its child process to dovetail's CLI, as it does to arc's.
6. **Deterministic.** Same inputs, byte-identical model.

## Non-goals

- **Brightfield's side**: the `Dovetail` implementation of its discovery trait, the runner hand-off in its `main`, and unignoring its test. That is Brightfield's work, and it waits on this.
- **Re-scoring roles on the full data.** The full pass does not change a role, a fold, a combination, or which relationships exist (see *The full pass*).
- **New discovery.** Roles, folds, combinations and edges are table roles' result as it stands; this design serialises it. Tuning the scorer is table roles' work.
- **Merging into the analyst's model.** Brightfield merges a proposal into `model.ossie.yaml` and is that file's only writer. dovetail writes a proposal file and nothing else.
- **A dovetail DuckDB extension.** Separate work; it would not write models.
- **Writing the data packages.** Brightfield writes them today, and FineType is to own them later. dovetail reads them.

## Anti-goals

It fails, even if it works, if:

- the full pass moves, renames, adds or removes anything the analyst is looking at, beyond a join's grade, evidence and warning;
- an analyst's decision is changed, or a join they rejected comes back;
- a sample-only measurement is presented as verified (`verified` must say `sample` until the full pass has run);
- Brightfield reads a half-written proposal;
- a field's FineType type in the model disagrees with the one Brightfield's profile gave it.

## Constraints

- **Ossie `0.2.0.dev0`**, core schema at `apache/ossie` commit `d13bb6a534a100e792a048dd7a14d4d37c723b2c`, as table roles' design records: `from` is the many side; `from_columns`/`to_columns` name declared fields; roles and everything else Meridian-specific go in `custom_extensions` entries of `{vendor_name: MERIDIAN, data: <JSON string>}`.
- **Brightfield's `brightfield-ossie` crate fixes the shapes.** Its `DatasetMeta`, `FieldMeta`, `RelationshipMeta`, `Evidence` and `ModelMeta` are the Meridian extension's fields and their JSON names; its wire order (model: `version, name, description, datasets, relationships, metrics, custom_extensions`; dataset: `name, source, description, primary_key, fields, metrics, custom_extensions`; field: `name, expression, datatype, custom_extensions`; relationship: `name, from, to, from_columns, to_columns, custom_extensions`) is the order dovetail writes. Its reader keeps unknown top-level keys of an element (such as a field's `dimension`) and ignores unknown keys inside the Meridian JSON, which is what lets dovetail write more than it reads today (see *What Brightfield needs to change*).
- **Brightfield's Protocol conventions**, as its own-files work built them: each source table is loaded as `raw_<name>` with three lineage columns, `__source_file`, `__source_part` and `__source_row`; the data packages sit at `packages/<source>/datapackage.json`, one resource per table, each field `{name, type, x-finetype-label?}` with lineage columns left out, and `x-part` naming a sheet or database table; a proposal goes to `.discovery/sample.ossie.yaml` or `.discovery/full.ossie.yaml`; the child is killed when its pass is superseded.
- **DuckDB 2.0.0, FineType v0.6.64**, as dovetail already is.
- **Built with `default-features = false`** inside Brightfield: no candle classifier. Everything `dovetail model` does must work without the `finetype-guided` feature.
- **Public repository.** No private planning identifiers or local paths in code, comments, commits or pull request text.

## Approaches

| | How | Gains | Costs |
|---|---|---|---|
| **A (chosen)** | dovetail owns an `ossie` module in `dovetail-core`: its own serde types for the wire shape, written from the run result, checked against the vendored Ossie schema and a golden file | dovetail stays a library with no dependency on Brightfield; the shapes are pinned by tests on both sides (dovetail's golden, Brightfield's ignored test) | Two copies of the shapes, which can drift; the golden and Brightfield's test are what catch it |
| B | Depend on Brightfield's `brightfield-ossie` crate by git | One copy of the shapes | A dependency cycle between the two repos (Brightfield depends on dovetail, which would depend on Brightfield), a lockstep bump on every change, and Brightfield's design-system crate pulled into dovetail |
| C | Move the Ossie types into a small shared crate both depend on | One copy, no cycle | A new crate and repo to own and release before either side can move; worth it once a third consumer appears, not now |

My call, with Hugh away: A, and C noted as the move if a third reader of Ossie models appears. The shapes are small, and Brightfield's `dovetail_child_writes_valid_ossie` test, once unignored, validates dovetail's actual output with Brightfield's reader and validator, so drift fails a test rather than a user.

## What was decided

Each item marked *call* is mine, made while Hugh was away, for his review.

### The command

```
dovetail model --db <duckdb> --pass sample|full --out <file>
               [--packages <dir>] [--base <model.ossie.yaml>]
               [--verify <sample proposal>] [--name <model name>]
```

- `--db`: the DuckDB holding the tables, attached read-only as everywhere else.
- `--pass`: `sample` or `full`, as below.
- `--out`: where the proposal goes. dovetail writes `<out>.writing` beside it and renames it into place, so a reader finds the old file or the new one and never part of either; a child killed mid-write leaves no `--out` at all.
- `--packages`: a directory of data packages (`<dir>/*/datapackage.json`). When given, the packages name the datasets and type the fields (*Datasets and typing*).
- `--base`: the model Brightfield holds. Both passes carry its analyst decisions through; nothing else is read from it.
- `--verify`: for the full pass, the sample pass's own proposal (`.discovery/sample.ossie.yaml`), which it verifies.
- `--name`: the model's name; defaults to the `--db` file's stem.

*Call:* paths are explicit, not derived from a Protocol folder. dovetail does not learn Brightfield's folder layout or arc's manifest; Brightfield's `Dovetail` resolves the Protocol database through arc and passes each path. The same command then serves an analyst running dovetail on any DuckDB.

Exit codes follow the existing commands: `0` when the model is written, `1` when it could not be (the reason on stderr, prefixed `dovetail model:`), `2` for a usage error. Warnings about the run (typing fell back to the floor, a package resource with no table) go to stderr and do not change the exit code.

**The library entry point.** *Call:* the `dovetail` crate gains a `[lib]` exposing `pub fn cli_main() -> ExitCode`, which is today's `main`; the binary calls it. Brightfield's runner variable sets the child off into `dovetail::cli_main()`, as `BRIGHTFIELD_RUN_AS_ARC` does into `arc::cli_main()`. Brightfield depends on the `dovetail` crate with `default-features = false`.

### Datasets and typing

**With `--packages`** the packages are the list of tables. Each resource is one dataset:

- `name`: the resource's `name`;
- `source`: `raw_<name>`, which must be a table in `--db`; a resource with no table is skipped with a warning (*call*: one named constant, `RAW_PREFIX`, since it is Brightfield's convention and not dovetail's);
- `source_file`: the resource's `path` resolved against its package directory, made relative to the directory holding `packages/` when it lies inside it (Brightfield's rule for `source_file`); `source_part`: the resource's `x-part`;
- the other tables in the database (Brightfield's silver and failed tables, anything else) are not datasets and are not measured.

Typing comes from the packages: a field's FineType type is its `x-finetype-label`. *Call:* **a field with no label is untyped**, not typed by the floor. Brightfield leaves a label out when the column's own data contradicts it, and a floor label there would disagree with the profile the analyst has already seen (the anti-goal). The run's typing source is recorded as `packages`, a third `TypingSource` beside profiled and floor, and every role's evidence names it as it names the other two. This fills the input slot table roles' design left for the packages.

**Without `--packages`** every table in the database is a dataset named and sourced as itself, typed as `dovetail relate` types it today (FineType's extension when the pin, the model and the probe all hold, else the floor).

**Lineage columns are never measured.** *Call:* the three lineage columns are dropped before any evidence is taken, in both modes, and never become fields. `__source_row` is unique and dense in every table and would otherwise be found as every table's key.

**Fields.** One field per remaining column, in table order: `expression.sql` the column name; `custom_extensions` the Meridian `{finetype?, decided_by: dovetail}`. *Call:* `datatype` is written only for a field with a FineType type, from the package's `type` (`string` → `String`, `integer` → `Integer`, `number` → `Decimal` when the DuckDB column is `DECIMAL` and `Float` otherwise, `boolean` → `Boolean`, `date` → `Date`, `time` → `Time`, `datetime` → `DateTime`, or `DateTimeTz` when the DuckDB column is `TIMESTAMP WITH TIME ZONE`; any other type writes none). The package's `number` covers `DECIMAL`, `FLOAT` and `DOUBLE` alike, so the DuckDB type decides, as Brightfield's fixture has `Decimal` on its prices. Omitting `datatype` on an untyped field is also what the fixture does: an untyped field claiming a datatype would be a guess. Without packages, the same map runs over FineType's label's Frictionless type, as `frictionless_for` gives it.

**Time fields.** A role's event time or snapshot date gets `dimension: {is_time: true}`; each audit time field gets `dimension: {is_time: false}`. Other fields write no `dimension`, leaving Ossie's default. This is the standard place for it, as table roles' design asked.

### The Meridian extension on each element

dovetail writes every key Brightfield's crate defines, with the same names, and the keys below marked *new* beside them. Brightfield ignores the new keys until it reads them.

**Dataset**: `role`; `role_evidence` (the role call's evidence sentences); *new* `role_scores` (all six roles, `{"entity": 0.82, ...}` in the role enum's order); `folded_into` (the host, for a folded reference); *new* `fold_column` (the struct column it folds into); *new* `combined_into` (the combined dataset, for a part); *new* `combination` (`{kind, parts, token_column}`, on a combined dataset); `excluded: false`; `decided_by: dovetail`; `source_file`; `source_part`; `rows` (on the data the pass read: the full row count, which is cheap, in both passes).

`primary_key` (core Ossie, not the extension) is the role call's key.

**Relationship**: `grade`; `evidence` `{value_overlap, orphans, child_rows, child_type?, parent_type?, types_agree, name_similarity, parent_duplicates}`; `cardinality`; `verified` (`sample` or `full`); `decided_by: dovetail`; `warning`.

- *Call:* **grade** is `likely` for an edge `relate` accepts and `possible` for one it suggests. Rejected edges are not written. This is Brightfield's meaning of the two grades (likely is verified and accepted; possible is a proposal), mapped onto `relate`'s statuses without a new threshold.
- *Call:* **`child_type` and `parent_type` are the FineType types** of the two columns, omitted when a side is untyped, and **`types_agree`** is true when both are typed with the same type, or both untyped with the same DuckDB type. The issue asked for FineType types; Brightfield's hand-written fixture shows DuckDB types (`BIGINT`), which I read as the stand-in's placeholder, not a ruling.
- **`cardinality`** is `N:1`, or `1:1` when the child column is also unique, in the crate's vocabulary.
- **`name_similarity`** is `relate`'s; **`parent_duplicates`** counts parent key values held by more than one row, as Brightfield's own measuring does, so the two measure the same thing.
- **Name.** `<from>_<to>`, as Brightfield's fixture names joins; when one pair has more than one join, each takes `_<from column>` after it. Stable, so Brightfield's match by keys and its kept name agree.

**Combinations and folds**, as table roles' result gives them:

- A **combined dataset** is a dataset of its own: its `source` is the combination's `UNION ALL BY NAME` query written over the parts' `raw_` tables (not over dovetail's sampled copies), its fields the combined columns plus the token column, its role and key the combined call's, and its `source_file` the parts' files joined by `, ` in part order (Brightfield requires one string). Its **parts are datasets too**, each with `combined_into` set, its own fields, and the combined dataset's role with the evidence sentence "combined into <name>". *Call:* parts stay datasets because Brightfield's own test expects every source table to be a dataset, and because a part the analyst wants kept apart is then one edit. Relationships point at the combined dataset, never at a part, as table roles' design measures them: a join to one part alone is the wrong measurement (half its keys are in the other part).
  This follows Hugh's table-roles ruling that a combined dataset is a query, and **Brightfield does not yet read it**: its silver step builds every dataset from `raw_<name>`, ignores `source`, and does not know `combined_into`, so a combination would break its silver build. *Call:* dovetail writes combinations in this shape now, and Brightfield's hand-off must honour them before it ships (item 2 of *What Brightfield needs to change*); a model with no combination is unaffected.
- A **folded reference** stays a dataset with `folded_into` and `fold_column`; its relationship to its host is written as any other. Brightfield's silver step folds today into flat `<reference>_<field>` columns, not the one struct column Hugh chose in table roles' design; `fold_column` is written so that ask can be met, and Brightfield ignores it until then.
- A **carried key** (a child carrying a snapshot's entity key) is no relationship, as table roles' design rules; its sentence is in both datasets' `role_evidence`.

**Model**: Brightfield's `ModelMeta` (`generator`, `rejected`, `skipped_sources`) plus, *new*, the provenance design commitment 5 asks for: `pass`, `typing` (source, and the finetype tag and model hash when profiled), `sample_rows`, and the dovetail version. *Call:* `generator` is `dovetail <version>`.

### The sample pass

*Call:* the sample is **at most `SAMPLE_ROWS` = 50,000 rows of each table**, drawn with DuckDB's reservoir sample at a fixed seed on one thread, so the same table always gives the same sample. A table at or under that size is read whole.

**A sample cannot answer a question about a parent's whole key.** A child sampled against a sampled parent finds most of its keys missing, and every join looks orphaned. *Call:* so the sample pass samples what is read row by row (column evidence, combinations, the child side of every candidate join), and measures **against the full table** anything about a key as a whole: a parent's distinct key set for overlap and orphans, a parent's uniqueness, and each table's row count. Those are single aggregates DuckDB answers quickly. The sample pass runs in an in-memory database holding the sampled copies as `src`, with the analyst's database attached read-only beside it for those full-table measurements.

A table's own key is still found on its sample, where a key can look unique and not be; the full pass checks it (below).

Everything the sample pass writes says `verified: sample`.

### The full pass

The full pass verifies the sample pass's proposal, read from `--verify`, and writes it back with:

- each relationship whose `decided_by` is `dovetail` re-measured on every row: its `evidence`, its `grade` from `relate`'s status on the full data, `verified: full`. *Call:* a join the full data no longer bears out (`relate` would now reject it) stays in the model, graded `possible`, with a `warning` naming what failed ("12 of 4,812 child rows point at no customer"). Removing it would rearrange the analyst's view under them; a weakened grade with its reason lets them decide.
- each dataset whose `decided_by` is `dovetail` with its `rows` measured on every row, and its `primary_key` checked: *call* — a key that is not unique on the full data is removed from `primary_key`, and a sentence saying so ("customer_id repeats in 3 rows of the full table") is added to `role_evidence`. The role is not re-scored.
- everything else exactly as given: roles, folds, combinations, fields, every analyst-decided element, every `excluded` flag, `rejected`, `skipped_sources`, and any keys dovetail does not know.

*Call:* the full pass verifies dovetail's own sample proposal, not Brightfield's held model. The held model is written by Brightfield, whose merge keeps its own `generator` and whose typed reader drops Meridian keys it does not know, so nothing in it says reliably which elements dovetail proposed; Brightfield's stand-in model, every table unclear with no joins, would otherwise be "verified" into a model with no joins at all. The analyst's decisions still come from `--base` (*What both passes carry through*), and they override the sample's elements of the same name or keys.

The full pass **discovers afresh**, on every row, with `verified: full`, when there is nothing current to verify: no `--verify`, a proposal that cannot be read or was not written by dovetail (`generator` not `dovetail ...`), or one whose datasets are not the run's datasets (a file added or removed since the sample). That also serves an analyst running dovetail directly.

### What both passes carry through

When `--base` is given, both passes:

- write each analyst-decided dataset, field and relationship back exactly as given, in its place;
- never write a relationship matching (by `from`, `from_columns`, `to`, `to_columns`) one in `meta.rejected`;
- copy `meta.rejected` and `meta.skipped_sources` through. *Call:* Brightfield's merge takes `skipped_sources` from the proposal, so a proposal without them would erase the analyst's list of files Brightfield passed over.

"Exactly as given" is tested by round trip: an analyst element read from `--base` and written by dovetail is byte-identical in its YAML, which means dovetail's types keep unknown keys, as Brightfield's do.

### Where the code sits

- `dovetail-core::ossie`: the wire types, `read`, `write`, the projection from table roles' `RoleModel` to a model, and the full pass's verification. YAML through `serde_yaml` 0.9, which moves from a dev-dependency to a dependency.
- `dovetail-core::packages`: reading the data packages into the dataset list and the typing.
- The sample: a `Session` constructor that builds sampled copies and attaches the full database, and the edge measurements reading a parent from the full database when the session is sampled. The evidence queries otherwise stay as they are.
- `dovetail::cli_main` and the `model` subcommand in the CLI crate.
- The Ossie schema vendored at the pinned commit as `ossie-schema.json`, with its licence, for the conformance test.

The Ossie model is a projection of the run's result, as the Frictionless descriptor `dovetail relate --out` writes is. Neither is derived from the other.

### Tests

- **Golden model**: the retail role fixture, as a `--packages` run with a packages fixture typed from its answer key's columns, writes a model checked in as a golden file and validated against the vendored schema. Both passes, floor and packages typing.
- **Brightfield's shapes**: a test reads the golden through dovetail's own reader and checks every Meridian key Brightfield's crate defines is present with its type; the cross-check against Brightfield's actual reader is its `dovetail_child_writes_valid_ossie`, once unignored.
- **The analyst wins**: a base with an analyst-decided dataset, field and relationship, an excluded dataset, a rejected join and skipped sources comes back byte-identical in those elements from both passes, and the rejected join is not proposed.
- **The full pass rearranges nothing**: a fixture whose sample finds a join and a key that the full data breaks writes the same datasets and relationships in the same order, with the join `possible` and warned and the key dropped with its sentence.
- **The full pass discovers afresh** with no `--verify`, with a proposal not written by dovetail (Brightfield's stand-in shape), and with one whose datasets differ from the run's, and finds the joins (the case Brightfield's own test exercises).
- **The sample**: the same database gives a byte-identical sample model on two runs; a fixture larger than `SAMPLE_ROWS` with a parent larger than it still scores its join accepted (the full-table parent rule).
- **Atomic write**: a run's `--out` never exists in a partial state (the write goes through `.writing` and a rename; tested by checking the temporary name is gone and the file parses).
- **No default features**: the model tests run under `cargo test -p dovetail-core --no-default-features`.

## What Brightfield needs to change

Carried to Brightfield's hand-off work, not done here:

1. Read the new keys: `role_scores`, `fold_column`, `combined_into`, `combination`, and the model's provenance, so a marginal role can be shown as marginal and a combination split again.
2. **Combined datasets in silver**: build a dataset from its `source` when it is a query rather than from `raw_<name>`, give a part with `combined_into` no silver table of its own, and measure joins against the combined table. Until then a model with a combination breaks the silver build.
3. **Folds as one struct column**, which table roles' design already asked of Brightfield; `fold_column` names it.
4. The `Dovetail` implementation: run `dovetail model` as a child with `--db` (from arc's Protocol database path), `--packages <protocol>/packages`, `--base <protocol>/model.ossie.yaml` when it exists, `--pass`, `--out` the pass's proposal file, and, for the full pass, `--verify <protocol>/.discovery/sample.ossie.yaml`; kill it on cancel.
5. Decide which warning stands when dovetail's full-pass warning and Brightfield's silver measurement both write `warning`. dovetail writes one only when the full data weakens a join it proposed.

## Left to the builder

- The exact Rust shapes of the wire types and the projection.
- How the sampled session is built (copied tables or views) and how the edge queries are told which side reads the full database.
- The wording of the full pass's warning and key sentences.
- The packages fixture's layout under `tests/`.
- Whether `dovetail relate` also gains `--model <file>` as a shortcut for `model --pass full` without packages.

## References

- Apache Ossie core specification and schema, `0.2.0.dev0`, at the commit above.
- Brightfield's `brightfield-ossie` crate (model, wire and merge modules) and its retail stand-in fixture; its discovery module and own-files loading, as merged on its main branch.
- dovetail's table roles design, `docs/superpowers/specs/2026-10-26-table-roles-design.md`.
