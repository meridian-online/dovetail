# dovetail infers each table's role and proposes the minimal related set

Design for `dovetail relate` to go beyond joins: to say what each table *is* (an entity, an event, a reference, a bridge, a snapshot, or unclear), each call with its evidence and score, and to propose the minimal set of related tables an analyst can build on, by folding lookups in and combining tables that are the same kind of thing.

## Intent

An analyst who receives an extract from someone else's database has to understand a schema they did not design, then shape it into something they can enrich and analyse. Brightfield draws dovetail's proposal on arrival and lets the analyst reshape it; this design is dovetail's half: the proposal and the reasons for it.

dovetail does not fit tables to one target pattern (star schema, one big table, activity schema, Data Vault). It infers a small vocabulary of table roles from evidence it already gathers, and the familiar patterns become ways of combining roles: a star is events with their entities; an activity stream is all events unioned; customer attributes are entities with their events rolled up. Tidy data's rule is the test of *minimal*: one table per kind of thing observed, and no lookup left that only labels another.

Every proposal says why, as every join does today. A role the evidence cannot support is called **unclear**, never forced.

## Goals

1. **A role for every table**, one of `entity`, `event`, `reference`, `bridge`, `snapshot`, `unclear`, with a score for each role, the winning role, and its evidence as short sentences an analyst can read ("`customer_id` unique across 4,812 rows; pointed at by transactions and loyalty_visits").
2. **Folds.** A reference that exactly one table points at is proposed folded into that table.
3. **Combinations.** Tables that are the same kind of thing split across files (`customers_2023`, `customers_2024`; a store list per region) are proposed combined into one dataset, with the evidence shown, and classified as a partition, a snapshot, or a duplicate.
4. **One result in the core library** that the Ossie model writer (separate work) serialises, and that `dovetail relate` prints until that writer lands.
5. **Measured, not asserted.** A role corpus with answer keys, and an eval harness that scores any role scorer against it, so thresholds are tuned against data and a later classifier can be compared on the same footing.

## Non-goals

- **Writing the Ossie model**, moving to DuckDB 2.0, reading finetype's per-source data packages, and the sample-then-verify passes. Each is separate work; this design produces the result they carry. For the data packages it defines only the input slot their typing will fill (see *FineType profiles before anything is scored*).
- **Unpicking nested data into tables** (shredding JSON columns, arrays of objects, structs keyed by values). Agreed as the next piece of work once relational CSVs are right. This design keeps the door open (see *Combined and folded datasets are queries*), but scores only top-level columns of loaded tables.
- **A learned classifier.** Evaluated after the score-per-role approach has a baseline; the harness is built so one can plug in.
- **Joins out to reference data the analyst did not bring.**
- **How Brightfield draws or edits roles.**

## Anti-goals

It fails, even if it works, if:

- a role is shown without the evidence that produced it;
- a marginal call is presented with the same confidence as a clear one;
- a table that fits nothing is forced into a role rather than called unclear;
- folding or combining loses the ability to say which source row a value came from;
- thresholds are set by feel and never checked against the corpus.

## Constraints

- **Ossie `0.2.0.dev0`** (core schema `core-spec/ossie-schema.json` at `apache/ossie` commit `d13bb6a534a100e792a048dd7a14d4d37c723b2c`, 2026-10-26). Read before this design, and binding on its shapes:
  - Datasets, fields, relationships and the model each take `custom_extensions` entries of `{vendor_name, data}`, where `data` is an opaque string (JSON by convention). Meridian's vendor name is `MERIDIAN`.
  - A relationship has no cardinality field: `from` is the many side, `to` the one side. **A many-to-many cannot be a relationship; it is a bridge dataset with two relationships.**
  - `from_columns` and `to_columns` name *declared fields*, not raw source columns.
  - Datasets have `primary_key` and `unique_keys`, and no role or grain field. Roles live in the Meridian extension.
  - Fields carry `dimension.is_time`, defaulting to true for `Date`, `Time`, `DateTime`, `DateTimeTz`. It is the standard place to mark an event's time field, and `is_time: false` marks an audit timestamp.
  - A dataset's `source` is a table *or a query*.
  - Field datatypes have no struct or list type; a nested column is `Opaque`, and a field's `expression` can reach inside a struct (`category.name`).
- **What DuckDB can do is done in DuckDB.** Every measurement is a read query; every combination and fold is SQL.
- **Determinism.** Same input, byte-identical output: no `HashMap` iteration on an order-bearing path; tables, roles, evidence and proposals in a stable order.
- **Builds on dovetail as it is on `main`**: the `duckdb` crate resolves in `Cargo.lock` to `libduckdb-sys 1.10504.0`, DuckDB 1.5.4, with the finetype-guided feature on by default as it already is. Nothing here depends on the DuckDB 2.0 move, and nothing here should make that move harder.
- **The evidence vocabulary of joins is unchanged**, with one scoring fix. `relate` misses a join whose key is not named like an id: `transactions.sku → products.sku` has the same name on both sides, full value overlap and a unique parent, yet scores 0.44 and is rejected, because only `id`-shaped names count as key-like. Hugh chose to fix that here: an exact-name match to a unique parent with full overlap earns enough key credit to pass `CONF_HIGH`, **but only when the parent column looks like a key**: at least 20 distinct values, and FineType does not type it as a person or place name, free text, a date, a number that measures something, or a boolean (an identifier, a code, or untyped all qualify). Without that limit a small unique `stores.city`, or a unique `email` or `name` shared between tables, would become an accepted join and plant false pointers in the role evidence. Every existing `relate` test keeps passing (the coincidental-boolean and bare-`id` cases in particular); a `sku` case pins the fix, and a negative case pins the limit (`customers.city` against a small unique `stores.city` stays rejected). The second change to `relate` is its typing input, under *FineType profiles before anything is scored*. Otherwise `relate`'s edges, scores and statuses stay as they are, and roles read them.
- **Public repository.** No private planning identifiers or local paths in code, comments, commits or pull request text.

## Approaches

| | How | Gains | Costs |
|---|---|---|---|
| A | Fixed yes/no tests per role, checked in a fixed order | Simplest; each call reads as "passed these tests" | Order-dependent; a narrow miss falls through silently; no difference between a confident and a marginal call |
| **B (chosen)** | A score per role from named evidence items, as `relate` scores a join; the highest wins; unclear when the top two are close or the winner is under a floor | Each call has a strength; unclear falls out of the scores; thresholds are named constants tuned against the corpus; matches how dovetail already reasons | More numbers to tune, so it leans on the answer keys |
| C | A learned classifier trained on the corpus | Could find patterns no rule names | Cannot produce the evidence sentence each proposal must carry; needs role labels the corpus does not supply; asks for trust dovetail's design commitments refuse |

Hugh chose B, and asked that a classifier be considered once B has been evaluated. The harness's scorer trait is what makes that comparison cheap.

## What was decided

### FineType profiles before anything is scored

Roles are scored on a full FineType profile of every column, not on `relate`'s model-free floor alone. The floor types only what is provable from a value's shape (emails, ISO timestamps, UUIDs, check-digit identifiers) and declines exactly the columns roles lean on: person and place names, free-text labels and descriptions, dates in non-ISO forms. Those separate a reference from a small entity, and an event from a table with an audit date.

- **One typing per run, read by joins and roles alike**, as `relate` already promises. `relate::run_path` builds it from the best source available and passes the same typing to join discovery and to the `roles` module, which takes it as an input (`SemanticTypes` or its successor) rather than typing columns itself. This changes what joins see when a model is loaded: semantic agreement and identifier key credit can fire on columns the floor declined. Under the floor, joins see exactly what they see today, so `tests/relate_semantic_keys.rs` and the other `relate` tests, which run without a model, keep their statuses; profiled runs are pinned by new tests.
- **Where the typing comes from**, in order: finetype's per-source data packages when they are given (this design defines the input slot; reading the packages is the separate work above); otherwise the finetype-guided classifier dovetail already carries, run once per column; otherwise the model-free floor. **The floor is used whenever no model is loaded**: the feature is off (`--no-default-features`), `DOVETAIL_FINETYPE_MODEL_DIR` is unset or names a missing directory, or the model fails to load. In every case the run records which typing it used (source, and the finetype commit and model identity when profiled), and every role's evidence names it, so a floor-only call never passes for a profiled one.
- **DuckDB's own types count too.** A `DATE` or `TIMESTAMP` column is a time column whatever FineType says, so the floor-only path still finds events in typed sources such as Parquet.
- **The eval runs both ways**, profiled and floor-only, on the same answer keys, so the report shows what FineType's profile is worth to each role.

The corpus is profiled once and cached beside it, so an eval run does not re-run the classifier over every table. The cache records the finetype commit from `Cargo.lock` and the model's identity it was built with, and is rebuilt when either differs; the eval report carries the same stamp, so a finetype bump shows in the committed record.

### The roles and their evidence

Each role is scored in `[0, 1]` from named evidence items measured on the table and on `relate`'s edges. **"Points at" means an accepted or suggested edge whose parent side is unique.** An edge to a non-unique column (typically two child tables sharing a foreign-key column, such as `transactions.customer_id` and `loyalty_visits.customer_id`) is not evidence for any role: Ossie's `to` must be the one side, so such an edge can never become a relationship. Rejected edges are not evidence either. The table below fixes the evidence each role reads and the starting thresholds; the weights that combine them are the builder's, tuned against the corpus.

| Role | Evidence for | Evidence against | Starting thresholds |
|---|---|---|---|
| **Entity**: one row per thing, with a real key | A unique, non-null key (single column, or a pair); another table points at it, or FineType types the key as an identifier; timestamps, if any, are audit-like | A non-audit time column together with a repeating pointer out (that is an event) | Key unique across all rows |
| **Event**: one row per timestamped occurrence, pointing at an entity | A non-audit date or time column; a pointer out to an entity whose values repeat across rows with different times; more rows than that parent | No time column; nothing it points at; the snapshot signals (key plus date unique while the date takes few distinct values relative to rows, or a constant as-of date) | Time column at least 90% filled; at least one parent value repeats |
| **Reference**: a small lookup that only labels another table | Few rows; a key plus label columns only: name-, description- or code-like text that labels the key; something points at it | Any time column; any pointer out; attribute columns that describe a thing (places, dates, amounts), which make it a small entity | At most 1,000 rows and at most 4 columns besides the key |
| **Bridge**: two pointers and little else, a many-to-many link | Two or more pointers out; pointers are most of its columns; the pointer tuple is unique | Something points at it; a non-audit time column (that is an event with several pointers) | Pointers at least half the columns |
| **Snapshot**: an entity's state as of dates | A key that repeats, while key plus a date column is unique, and the date takes few distinct values relative to rows; or the one-date case: keyed by an entity's key (a unique pointer to it) with an as-of date that is constant across rows | A date that varies per row (that is an event) | Distinct dates at most 1% of rows |
| **Unclear** | — | — | Winner under 0.5, or within 0.15 of the runner-up |

Settled in conversation:

- **One role per table.** An `orders` table is an event; that `order_lines` hang off it shows as relationships, not a second role.
- **An audit timestamp is not an event.** A customers table with `created_at` is an entity, and its timestamp gets `is_time: false`. What separates the two is the pointer: an event's pointer values repeat across rows with different times.
- **The losing roles stay visible.** Every role's score is kept, so a consumer can show "event 0.82, entity 0.41".

- **Size alone does not make a reference.** A three-row `stores` table with `city` and `region` is an entity: its columns describe stores rather than label a code. A lookup's columns only name what its key means.

Audit-likeness of a time column is the builder's to define from name (`created_at`, `updated_at`, `modified`, `inserted`, `loaded_at`, …) and from behaviour (it does not distinguish rows of the same parent), and to pin with tests.

### Folds

A reference is proposed folded into the table that points at it **only when exactly one table points at it**. A lookup two tables point at stays a shared reference, unfolded. (Hugh flagged this as the rule to watch in adversarial review; it is a named rule so it can be changed in one place.)

The fold's form in silver is **one struct column**, named after the reference (singularised: `categories` → `category`), holding the reference's label columns: `category STRUCT(name, parent)`, not prefixed columns such as `categories_name`. A wide table keeps the lookup grouped and recognisable, and the fold is easy to undo. In the Ossie model the folded reference stays a dataset, marked folded into its host; the host can expose flat fields over the struct (`category_name` with expression `category.name`), so consumers that cannot read structs still see typed fields.

dovetail records the fold (the reference, the host, the relationship it folds along, and the struct column's name). It does not rewrite the host's `source`; the silver step that materialises the model compiles the fold. That keeps unfolding a metadata edit.

### Combinations

Tables are proposed combined when the evidence says they are the same kind of thing:

- the same column names (allowing for order) and compatible types;
- the same FineType type per column where both are typed;
- names that differ only by a token: a year, a date, a period, a region, a number.

The key decides what kind of combination it is:

| Kind | Evidence | Combined dataset |
|---|---|---|
| **Partition** | Key values do not overlap across the parts | Plain union of the parts |
| **Snapshot** | Key values largely overlap (the same customers in 2023 and 2024) | Union with the distinguishing token as a column; key is the entity key plus that column; role snapshot |
| **Duplicate** | Identical rows | Keep one; the others are named as duplicates |

The checks run in that table's reverse order: **duplicate first** (identical rows, so identical copies are kept once rather than unioned as a snapshot with a made-up token column), then the key-overlap test that separates snapshot from partition. The overlap threshold between partition and snapshot is the builder's, tuned on the corpus. The token column's name and type come from what the token is (a year or a date is typed through FineType; otherwise it is text named after what varies, e.g. `region`).

**Combinations come first, roles second.** The `roles` module finds combinations before it scores any role, then scores each combined dataset as one table, measured through a temporary view over its `UNION ALL BY NAME` query (a read-only connection still allows temporary views; if it does not, the measurements run over the query as a subquery). **Joins are discovered again against the combined dataset**, not carried over from its parts: an edge checked against one part is the wrong measurement (`transactions.customer_id` against `customers_2023` alone is half orphans and is rejected), so after combinations are formed, `relate`'s discovery and verification run over the combined views in place of their parts. Edges between the parts of one combination are not candidates. For a **snapshot** combination the entity key is no longer unique, so a child table that carries that key has no valid Ossie relationship to it (`to` must be the one side): dovetail proposes no relationship, and records on both tables that the child carries the snapshot's entity key, with that edge's overlap as evidence. Deriving an entity dataset from a snapshot (its distinct keys and latest state) is not in scope. Answer keys label the combined dataset, and list its parts.

### Combined and folded datasets are queries

A combined dataset has no single physical table, so its Ossie `source` is the DuckDB query that builds it (`UNION ALL BY NAME` over the parts, with the token column). Its parts are listed in the Meridian extension, so a consumer can draw them and split them again. Row lineage survives: each part's rows keep the lineage columns they were loaded with.

This is the same mechanism unpicking nested data will use: a table found inside a JSON column becomes a dataset whose source is a DuckDB query. The role rules read tables and columns, never file formats, so a table unpicked from JSON will be scored by the same rules as one loaded from CSV.

### Nesting, when dovetail proposes it

Agreed in conversation, and binding on folds now and on nested work later:

- **Structs** for named groups: a folded lookup; columns sharing a prefix (`billing_*`, `shipping_*`) may be proposed as a struct with the prefix as evidence.
- **Numbered repeats** (`phone1`–`phone3`, `jan_sales`–`dec_sales`) are a hidden child table: proposed pivoted long into their own table, not into an array.
- **Arrays** only for short lists of plain values with no identity of their own (tags). Anything with a key is its own table.

In this design only the fold's struct is built. Prefix groups and numbered repeats are recorded here so the later nested work follows the same rules; detecting them is not in scope.

### The result in the core library

A new `roles` module in `dovetail-core`, beside `relate`. It takes an open DuckDB connection and the run's edges and its one typing (as `relate::run_path` computes them under *FineType profiles before anything is scored*) and returns one deterministic result carrying, at least:

- per table: the winning role, every role's score, the evidence sentences, the primary key it found, the event time field and any audit time fields;
- the folds: reference, host, the edge folded along, the struct column's name;
- the combinations: name, parts, kind, the token column, the source query, the evidence.

The exact Rust shapes are the builder's. The Meridian extension's fields in the Ossie model are fixed by Brightfield's model crate and extended as listed under *What Brightfield needs to change*; the result must carry everything those fields need.

`dovetail relate` prints the roles, folds and combinations after its join summary, in a stable order. Writing them into the Ossie model belongs to the Ossie writer.

### The role corpus and the eval harness

**Answer keys live in the repo; data does not.** The corpus data is fetched by a script committed under `scripts/`, into a directory named by an environment variable (`DOVETAIL_CORPUS`). The data's licences vary and some sets are large, so none of it is committed. Answer keys are dovetail's own labels and are committed.

**Storage is one Parquet file per table**, so the corpus reads the same under DuckDB 1.4 and 2.0 and resembles what an analyst brings.

The corpus has three tiers:

1. **Retail stand-in** (in the repo, always run, CI). The eight tables of Brightfield's stand-in model, with the same names and columns: `customers`, `stores`, `products`, `categories`, `transactions`, `loyalty_visits`, `customer_segments`, `promo_codes`. Built as a deterministic SQL fixture like `tests/relate-fixtures/`, **at realistic row counts** (hundreds of customers, thousands of transactions, monthly segment assessments), because no threshold means anything on Brightfield's 2–8 rows per table. **All eight of Brightfield's labels bind**: customers, stores and products entities; categories a reference folded into products; transactions and loyalty_visits events; customer_segments a snapshot; promo_codes unclear. It is extended with two combination cases (a partition pair and a snapshot pair). Brightfield's stand-in can adopt the larger data; its own tests only need its labels to agree.
2. **Relational databases with declared keys** (fetched, run locally). About 25 business-shaped databases from the CTU Prague Relational Dataset Repository: among them Chinook, Northwind, Sakila, classicmodels, pubs, AdventureWorks2014, ErgastF1, lahman_2014, Hockey, financial, imdb_MovieLens, Mondial, Walmart_2014, stats_CEB and employee. Each is fetched with its declared keys stripped from the data and saved as its key answer key, so join discovery is scored for free. Role, fold and combination labels are written per table, each with a one-line why.
3. **GitTables** (fetched, run locally). A fixed, seeded sample of about 2,000 single tables stratified by topic. No relationships, so it scores only what lone tables can show: the distribution of roles, the unclear rate, and how often the reference rule fires on tables that are not lookups.

**The harness** follows `eval.rs`: a `RoleScorer` trait (as `Detector` is for structure), the score-per-role scorer as its first implementation, and a report rendered as a Markdown table: per-database role accuracy, a confusion matrix by role, fold and combination precision and recall, and the GitTables distribution. The report is deterministic, so a committed copy of it is the eval record, and a change to thresholds shows as a diff of that record.

**Labels are reviewed before thresholds are tuned.** The builder drafts the labels for tiers 1 and 2; Hugh reviews the label set (a checkpoint in the plan) before any threshold is tuned against it, so the scorer is not tuned to the builder's own guesses.

**Targets.** Tier 1 must be exact under the profiled typing; its floor-only run is recorded beside it and is not required to be exact. CI runs tier 1 floor-only against the committed record, and also profiled if the builder can fetch finetype's model in CI the way finetype's own tests do; if not, the profiled exact check is a local gate run before a branch is called ready. Exact means every role, fold and combination as its answer key says. Tiers 2 and 3 start as a recorded baseline; their targets are set after the first baseline is read, not now.

### What Brightfield needs to change

Brightfield's model crate fixes the Meridian extension's shapes and has not been built yet, so these are cheapest now. They are asks of Brightfield's work, carried there separately:

1. **Combinations:** a field on a dataset listing the parts it was combined from, and the kind (partition, snapshot, duplicate).
2. **Folds as a struct column** in the silver step, named after the reference, in place of prefixed columns.
3. **Role scores** beside `role_evidence`, so a marginal call can be shown as marginal.
4. **The event time field** marked with the standard `dimension.is_time`, and audit timestamps with `is_time: false`, rather than in the extension.
5. **Cardinality** named with Ossie's draft relational vocabulary (`MANY_TO_ONE`, `ONE_TO_ONE`), since every core relationship is already many-to-one by direction.
6. **The schema file's name**: `core-spec/ossie-schema.json` at the commit above, not `osi-schema.json`.

## Left to the builder

- The weights inside each role's score, and the exact thresholds, within the evidence and starting points above, tuned against the corpus once labels are reviewed.
- What counts as an audit timestamp, pinned with tests.
- How a table's key is found when no single column is unique (which pairs are tried, and in what order).
- The token grammar for combination names, and the overlap threshold between partition and snapshot.
- The Rust shapes of the result and the printed format of `dovetail relate`'s new section.
- The fetch script's details, the GitTables sample's seed and stratification, and the corpus directory layout.
- Which CTU databases make the final list, keeping the mix of events, snapshots, bridges and shared lookups.

## References

- Apache Ossie core specification and schema, `0.2.0.dev0`, at the commit above; its draft relational query interface (section 11.3) for the cardinality vocabulary; its ontology specification for entity and value types.
- DuckDB's "From JSON Strings to Typed Columns" and the DuckDB 2.0 announcement, on structs, `unnest` and `VARIANT`.
- Hadley Wickham, "Tidy Data".
- The CTU Prague Relational Dataset Repository (Motl and Schulte, 2015).
- GitTables (Hulsebos et al., 2023).
