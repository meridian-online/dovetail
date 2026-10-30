# Table roles

`dovetail relate` says what each table *is* as well as how the tables join. Every table gets one of six roles, a score for each role, and the evidence behind the call, and dovetail proposes the minimal set of related tables an analyst can build on: lookups folded into the one table that uses them, and tables that are one kind of thing split across files combined into one dataset. The code is `crates/dovetail-core/src/roles.rs` (scoring, folds, the role model), `combine.rs` (combinations) and `evidence.rs` (measurement).

dovetail does not fit tables to one target pattern (star schema, one big table, activity schema, Data Vault). The familiar patterns are ways of combining roles: a star is events with their entities, an activity stream is all events unioned, and customer attributes are entities with their events rolled up. Tidy data's rule is the test of minimal: one table per kind of thing observed, and no lookup left that only labels another. A table the evidence cannot place is called unclear, never forced.

## Rulings behind the design

Hugh made these calls when the design was agreed, and they bind later work:

- **A score per role, not fixed tests and not a learned classifier.** Each role is a weighted sum of named evidence for it minus evidence against it, as `relate` scores a join. Fixed tests checked in order cannot tell a confident call from a marginal one; a classifier cannot produce the evidence sentence every call must carry. A classifier is to be compared once this approach has a baseline, through the eval harness's `RoleScorer` trait.
- **The work runs in DuckDB.** dovetail attaches the analyst's database read-only to an in-memory DuckDB and writes its measurements there as evidence tables (`column_evidence`, `table_evidence`, `edge_evidence`). Only those rows leave the database, for a pure Rust scoring function that needs no database and is unit-tested on hand-built rows. A later dovetail DuckDB extension would wrap the same queries and the same function.
- **FineType profiles before anything is scored.** Columns are typed by FineType's DuckDB extension, loaded from the file attached to the release dovetail pins (`DOVETAIL_FINETYPE_EXTENSION`), with its model named by `FINETYPE_MODEL_DIR`. The community repository is not used, because it serves one build per DuckDB version and cannot honour a pin. The extension's version must equal the pin and a known-answer probe must pass; otherwise the run falls back to the model-free floor and says why. A run asks for `best` or `floor`, and every call names the typing it used. Typing runs single-threaded so the profile's sample is the same on every run. One typing serves joins and roles alike.
- **One role per table.** An `orders` table is an event; its lines show as relationships, not as a second role.
- **An audit timestamp is not an event.** A customers table with `created_at` is an entity.
- **Size alone does not make a reference.** A three-row stores table with city and region describes stores, so it is an entity.
- **The losing scores stay visible**, so a consumer can show "event 0.82, entity 0.41".
- **A reference folds only when exactly one table points at it**, as one struct column named after the reference (`category STRUCT(name, ...)`), not as prefixed columns. A shared lookup stays a dataset of its own. dovetail records the fold; the silver step that materialises the model compiles it, so unfolding is a metadata edit.
- **Combinations come first, roles second**, and joins are measured again against the combined datasets rather than carried over from their parts.
- **Nesting, when dovetail proposes it:** structs for named groups (a folded lookup; later, columns sharing a prefix); numbered repeats (`phone1` to `phone3`) pivoted long into their own table; arrays only for short lists of plain values with no identity. Only the fold's struct is built so far.
- **Answer keys are reviewed before thresholds are tuned**, so the scorer is not tuned to its builder's own labels.

## The roles and their evidence

"Points at" means an accepted or suggested edge whose parent is unique. An edge to a column that repeats can never be a relationship (the one side must be unique), so it is no evidence for any role, and neither is a rejected edge. A snapshot's entity key, carried by another table, counts as a pointer out of that table.

| Role | Evidence for | Evidence against |
|---|---|---|
| **Entity**: one row per thing, with a real key | A unique key; something points at it or FineType types it as an identifier; describing columns; no times but audit or lifecycle dates | A time column with a repeating pointer out (an event's shape); the snapshot signals; isolation |
| **Event**: one row per timed occurrence, pointing at an entity | A time column at least 90% filled; a pointer out whose values repeat at times that differ within a parent; more rows than a parent | The snapshot signals; no pointer out |
| **Reference**: a small lookup that only labels another table | At most 1,000 rows; a key and at most 4 label columns; something points at it | A time column; a pointer out; describing columns; isolation |
| **Bridge**: two pointers and little else | Two or more pointers out; pointers at least half the columns; the pointer tuple is the key | Something points at it; a time column |
| **Snapshot**: an entity's state as of dates | A repeating key unique with a date that takes few values, or a unique pointer with one as-of date on every row; the key less its date points at an entity | Another date that varies per row |
| **Unclear** | The winner scores under 0.5, or leads the runner-up by less than 0.15 | |

The weights are the named constants at the top of `roles.rs`. Some evidence needs a definition:

- **Audit timestamps** are named so (`created_at`, `updated_at`, `modified`, `last_update`, and those stems with `_ts` or `_at`). They count for nothing.
- **Lifecycle dates** say when the thing itself began: names holding the word birth, birthdate, birthday, dob, born, hire, hired, hiredate, founded or established. They describe an entity rather than time an occurrence, so a table of employees with a hire date and a repeating department stays employees. Words that as often name a logged action (opened, signup, released) are left out, since an event log of those actions has no other time.
- **Few dates** means distinct dates at most 1% of rows, or rows filling at least half the grid of the key's other values by its dates (12 stores over 24 months is a full grid of 288 rows, though its 24 dates are 8% of them). An event's dates rarely repeat across its parents, so its grid is sparse.
- **Isolation** is a table nothing points at, that points at nothing and refers to nothing in itself. Self-references are measured, gated by self-pointer names (`parent`, `manager`, `reports_to`, ...), and a self-reference only exempts a table from isolation.
- **Exact-name key credit.** A join whose column has the same name on both sides, to a unique parent with full overlap and at least 20 distinct values, earns key credit when the parent looks like a key: FineType types it as something keys are drawn from (not a person or place name, contact detail, free text, date, boolean or measured number), or, untyped, at least 90% of its values share one length or one pattern of letters, digits and separators. An integer column's single width counts only when its values span at most ten times as many numbers as it has distinct values, since keys are handed out in sequence while a unique measure of one width is spread over its range.

## Folds

A reference folds into its host when exactly one column of one table points at it. Two tables pointing at it make it shared; two columns of one table make it a role-playing lookup (origin and destination); either way it stays a dataset. A reference can fold into a bridge.

## Combinations

Tables combine when they have the same columns with compatible types and agreeing labels, and their names differ by one token: a year, a date (`2024-01`, `2024_01_31`, `202401`, `20240131`), a number, or a word. The checks run in this order:

| Kind | Evidence | Combined dataset |
|---|---|---|
| **Duplicate** | Identical rows both ways | One part kept |
| **Snapshot** | A year or date token; the first part's key overlaps across parts by at least half; key plus token is unique over the combined rows | Union with the token as a column; key is the entity key plus the token; role snapshot |
| **Partition** | Anything else | Union with the token as a column |

A word token (a region) or a bare number (`customers_1`, `customers_2`) names a part rather than a time, so its parts are a partition however much their keys overlap. A combined dataset's source is the DuckDB query that builds it, measured through a view in dovetail's own in-memory catalog; its parts are listed so a consumer can split them again. Joins between parts of one combination are dropped. A table carrying a snapshot's entity key has no relationship to it, since that key is not unique there, but the carried key counts as a pointer out when it is scored.

## The corpus and the eval

`tests/role-corpus/README.md` describes the three tiers, how to fetch tiers 2 and 3, and how to run the eval; `tests/role-corpus/RESULTS.md` is the record of the last run.

| Tier | Data | Target |
|---|---|---|
| 1 | The retail stand-in fixture, in the repo | Exact under FineType's profile; CI runs it floor-only and profiled |
| 2 | 25 CTU databases with declared keys, 321 tables | Profiled role accuracy at least 60% (193/321), with join recall no lower than the baseline's 273/366; the floor no lower than its baseline on either measure |
| 3 | 2,000 GitTables tables, each scored alone | Profiled unclear rate under 90%, with the reference rate staying at 0%, since a table scored alone has nothing pointing at it |

The baseline the targets were set from, after the tuning on Hugh's four questions (the record has every miss):

| Tier | Typing | Role accuracy | Join recall | Unclear rate |
|---|---|---|---|---|
| 2 | floor | 147/321 (45.8%) | 269/366 (73.5%) | |
| 2 | profiled | 157/321 (48.9%) | 273/366 (74.6%) | |
| 3 | floor | | | 99.0% |
| 3 | profiled | | | 98.4% (21 snapshots, 10 entities) |

The tuning moved tier 2 profiled from 151 to 157 correct and floor from 143 to 147, with join recall unchanged. Entities with a hire or birth date (`pubs.employee`, `financial.client`, `Mondial.organization`) stopped scoring event, and small monthly snapshots (`Walmart_2014.features`, `Hockey.teamssc`) began to score snapshot. Tier 3 profiled calls 21 tables snapshots, up from 3.

## Open

- The thresholds beyond those the corpus tuning addressed (listed in the record's history) are starting values. The largest misses in tier 2 are tables whose audit column is not named in the audit list (`ModifiedDate`), snapshots keyed on three columns or on an integer year (the key search tries one column or a pair, and an integer year is a time column only when FineType types it as one), and links to a code with no table of its own, which have one pointer and so never score bridge.
- An entity whose only date names an action that is also logged as an event (`customers.joined_on`, with a repeating home-store pointer) still has an event's shape and scores event. Counting inbound pointers against event would separate it, but would also miscall `orders`, an event its lines point at.
- A GitTables table with more than 1,000 columns is skipped; distinct-count planning on one with 1,726 ran past ten minutes.
