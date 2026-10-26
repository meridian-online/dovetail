# Table roles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `dovetail relate` calls each table an entity, event, reference, bridge, snapshot or unclear, with a score per role and the evidence for it, and proposes folds and combinations, measured inside DuckDB and scored in Rust, with a corpus and eval harness that score it.

**Architecture:** An in-memory DuckDB session attaches the analyst's database read-only, types every column through FineType's DuckDB extension, and writes three evidence tables (column, table, edge) with set-based SQL. Pure Rust functions score joins (today's `relate` arithmetic, unchanged) and roles over those evidence rows. Combinations are found first and measured through views; folds are proposed from the role calls. An eval harness scores any `RoleScorer` against committed answer keys.

**Tech Stack:** Rust 1.94 workspace, `duckdb` crate (bundled, DuckDB 2.0, `=1.20000.0`), FineType's DuckDB community extension and `finetype-core` at the same tag, serde_json.

**Prerequisite:** dovetail's move to DuckDB 2.0, with its FineType pin set to the community extension's version, is separate work and must be merged before Task 1 starts. Check: `grep 'duckdb = ' crates/dovetail-core/Cargo.toml` names `=1.20000.0`.

**Spec:** `docs/superpowers/specs/2026-10-26-table-roles-design.md`

## Global Constraints

- Roles are exactly `entity`, `event`, `reference`, `bridge`, `snapshot`, `unclear`. Combination kinds are exactly `partition`, `snapshot`, `duplicate`.
- Starting thresholds, as named `pub const`s in `roles.rs`: `ROLE_FLOOR = 0.5`, `ROLE_MARGIN = 0.15`, `REFERENCE_MAX_ROWS = 1000`, `REFERENCE_MAX_LABEL_COLUMNS = 4`, `EVENT_MIN_TIME_FILL = 0.9`, `SNAPSHOT_MAX_DATE_RATIO = 0.01`, `BRIDGE_MIN_POINTER_SHARE = 0.5`. In `relate.rs`: `KEY_NAME_MATCH_MIN_DISTINCT = 20`.
- "Points at" means an accepted or suggested edge whose parent is unique. Nothing else is role evidence.
- A reference folds only when exactly one table points at it. The fold is one struct column named after the singularised reference.
- The analyst's database is attached `READ_ONLY`; every dovetail write goes to the in-memory session.
- The FineType extension must report the same version as dovetail's `finetype-core` tag, as set by the DuckDB 2.0 move; a mismatch is an error. When the extension or its model is unavailable, typing falls back to the model-free floor, and the run's `TypingSource` and every role's evidence say so.
- Determinism: byte-identical output for the same input. `BTreeMap`/sorted `Vec` on every order-bearing path; no `HashMap` iteration.
- Every SQL identifier is double-quoted through `relate`'s `quote` (made `pub(crate)`).
- Public repo: no private planning identifiers or local paths in code, comments, commits, branch names or PR text. Corpus data is never committed.
- One cargo build at a time on the 16 GB Mac. Before each commit: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and the tests the task names. The first build of a fresh target dir compiles bundled DuckDB and candle: allow 20–40 minutes; incremental test runs after that take 1–3 minutes.
- Tests that need the FineType extension or its model skip with a printed reason (`eprintln!("skipping: …")` and return) when it is unavailable; tests that need `DOVETAIL_CORPUS` do likewise.

## Review Focus

1. **Awkward identifiers** (`Order ID`, `select`, `a"b`, mixed case like Chinook's `InvoiceLine`): every evidence query quotes them and runs. Pinned in Task 2.
2. **Empty tables and all-NULL columns:** no panic, no division by zero; an empty table is `unclear` with the evidence "no rows". Pinned in Tasks 2 and 5.
3. **No single-column key** (Sakila's `film_actor`, Chinook's `PlaylistTrack`): the key search finds the unique pair, and the table scores as a bridge. Pinned in Task 6.
4. **Self-references** (Chinook's `Employee.ReportsTo`): `relate` skips same-table pairs by design, so this edge is never found; the table must still score as an entity, not unclear. Pinned in Task 6.
5. **The extension available but the wrong version, or without its model, or not installable offline:** a version mismatch errors with both versions named; a missing model falls back to the floor and says so. Pinned in Task 1.

---

### Task 1: Load FineType's community extension at the pinned version

**Files:**
- Create: `crates/dovetail-core/src/finetype_ext.rs`
- Modify: `crates/dovetail-core/src/lib.rs` (add `pub mod finetype_ext;`)
- Test: `crates/dovetail-core/tests/finetype_extension.rs`

**Interfaces:**
- Produces:

```rust
pub fn finetype_tag() -> &'static str;              // the finetype tag Cargo.toml pins, embedded at build time (build.rs reads the root Cargo.toml)
pub enum TypingSource { Profiled { finetype_version: String, model: String }, Floor { reason: String } }
pub enum ExtensionError { VersionMismatch { expected: String, found: String }, Duckdb(duckdb::Error) }
pub fn load(conn: &duckdb::Connection) -> Result<TypingSource, ExtensionError>;
```

`load` runs `INSTALL finetype FROM community; LOAD finetype;`, checks `ft_version()` against `finetype_tag()` (strip a leading `v` on either side), then confirms the model loads by profiling a one-row literal. Install or load fails (offline, not published) → `Ok(Floor { reason: "finetype extension not available: <error>" })`; model missing → `Ok(Floor { reason: "finetype model not found" })`; version differs → `Err(VersionMismatch)`. `model` is whatever identity the extension reports for its model (read finetype's `crates/finetype-duckdb/src/lib.rs` for what `ft_version` and `ft_profile` return, and record their exact output shapes in a doc comment).

- [ ] **Step 1: Check the prerequisite and the route.** Confirm the DuckDB 2.0 move is merged (see *Prerequisite*). On a connection from the bundled crate, `INSTALL finetype FROM community; LOAD finetype; SELECT ft_version();` must report the pinned version. If the community build for that version is not published yet, **stop and raise it** (flag the Kata issue `needs-human`); do not fall back to a local build or the in-process classifier.
- [ ] **Step 2: Write the failing tests** in `tests/finetype_extension.rs`: `tag_matches_cargo_toml` (reads the root `Cargo.toml`, asserts both finetype tags equal `finetype_tag()`); `loads_and_reports_pinned_version` (skips if unavailable; `load` returns `Profiled` whose `finetype_version` equals the tag without the `v`); `missing_model_falls_back_to_floor` (with `FINETYPE_MODEL_DIR` pointed at an empty temp dir: `Floor` whose reason names the model); and a unit test in `finetype_ext.rs` on `fn check_version(expected: &str, found: &str) -> Result<(), ExtensionError>`: `"v0.6.64"` vs `"0.6.64"` ok, vs `"0.6.60"` → `VersionMismatch` naming both.
- [ ] **Step 3: Run** `cargo test -p dovetail-core --test finetype_extension`. Expected: FAIL, module missing.
- [ ] **Step 4: Implement** `finetype_ext.rs` as the Interfaces block says.
- [ ] **Step 5: Run** the same command, then `cargo test --workspace`. Expected: PASS; extension tests pass or print their skip reason.
- [ ] **Step 6: Commit**: "Load FineType's community extension at the pinned version, falling back to the floor".

### Task 2: The evidence session and `column_evidence`

**Files:**
- Create: `crates/dovetail-core/src/evidence.rs`
- Modify: `crates/dovetail-core/src/lib.rs`, `crates/dovetail-core/src/relate.rs` (`quote`, `type_family` and `singularise` become `pub(crate)`)
- Test: `crates/dovetail-core/tests/evidence_columns.rs`

**Interfaces:**
- Consumes: `finetype_ext::{load, TypingSource}`.
- Produces:

```rust
pub struct Session { conn: duckdb::Connection, pub typing: TypingSource }
impl Session {
    pub fn attach(db_path: &Path) -> Result<Session, EvidenceError>;   // in-memory conn; ATTACH '<path>' AS src (READ_ONLY)
    pub fn from_sql(build_sql: &str) -> Result<Session, EvidenceError>; // ATTACH ':memory:' AS src; USE src; run build_sql; USE memory
    pub fn conn(&self) -> &duckdb::Connection;
}
pub struct ColumnEvidence { pub table: String, pub column: String, pub duckdb_type: String, pub rows: i64,
    pub non_null: i64, pub distinct: i64, pub label: Option<String>, pub audit_name: bool }
pub fn column_evidence(s: &Session) -> Result<Vec<ColumnEvidence>, EvidenceError>; // also writes memory.column_evidence
```

Tables are read from `src.information_schema.columns` (schema `main`), in `table, ordinal` order. Counts come from one `SELECT count(*), count(col), count(DISTINCT col) …` per table, all columns in one statement. Labels come from `ft_profile` per column when `typing` is `Profiled`, else from `typing::deterministic_semantic_type` over a 500-value sample (today's floor, `FIELD_SAMPLE_N`). `audit_name` is true for names matching `created_at`, `updated_at`, `modified`, `modified_at`, `inserted_at`, `loaded_at`, `last_update`, `_ts`/`_at` suffixes on those stems (the list is a `pub const AUDIT_TIME_NAMES`). `memory.column_evidence` has exactly these columns plus `label_source VARCHAR` (`profiled` or `floor`).

- [ ] **Step 1: Write the failing tests**: `counts_and_types` (a 3-row fixture with one NULL: `rows 3, non_null 2, distinct 2`); `awkward_names_are_quoted` (tables/columns `"Order ID"`, `"select"`, `"a""b"`, `InvoiceLine`: evidence produced for each); `empty_table_has_zero_counts` (no panic; `rows 0`); `analyst_db_is_not_written` (attach a temp `.duckdb` file, run `column_evidence`, reopen the file: no `column_evidence` table and the same table list); `email_is_labelled` (profiled or floor, an email column's label ends in `.email`); `audit_names` (`created_at` true, `purchased_at` false).
- [ ] **Step 2: Run** `cargo test -p dovetail-core --test evidence_columns`. Expected: FAIL.
- [ ] **Step 3: Implement** `evidence.rs` per the Interfaces block.
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit**: "Measure and type every column inside DuckDB into column evidence".

### Task 3: `edge_evidence` in SQL, and `relate` scored from it

**Files:**
- Modify: `crates/dovetail-core/src/evidence.rs`, `crates/dovetail-core/src/relate.rs`
- Test: `crates/dovetail-core/tests/evidence_edges.rs`; existing `relate_discovery.rs`, `relate_semantic_keys.rs`, `descriptor_stitch.rs`, `demo_substrate.rs` must pass unchanged.

**Interfaces:**
- Consumes: `Session`, `ColumnEvidence`.
- Produces:

```rust
// evidence.rs
pub struct EdgeEvidence { pub child: ColumnRef, pub parent: ColumnRef, pub child_total: i64, pub child_distinct: i64,
    pub parent_total: i64, pub parent_distinct: i64, pub overlap_distinct: i64, pub orphan_count: i64,
    pub name_similarity: f64, pub semantic_similarity: f64, pub child_label: Option<String>, pub parent_label: Option<String> }
pub fn edge_evidence(s: &Session, cols: &[ColumnEvidence], tables: &[String]) -> Result<Vec<EdgeEvidence>, EvidenceError>;
// relate.rs
pub fn edge_from_evidence(e: &EdgeEvidence) -> Edge;     // pure: today's score_edge arithmetic, assign_status, demote_on_mismatch
pub fn run_session(s: &Session) -> Result<RelateRun, EvidenceError>;
```

Candidate pairs are pruned in Rust from `ColumnEvidence` exactly as today (different tables, `type_family` match, `max(name, semantic) ≥ 0.3`); `tables` restricts which tables take part (Task 9 passes combined views). The measurements for all surviving pairs run as set-based SQL: one query per child column computes `overlap_distinct` and `orphan_count` against each of its candidate parents (for example a `UNION ALL` of per-pair aggregates), and totals and distincts come from `ColumnEvidence`, not new queries. `memory.edge_evidence` holds every row. `edge_from_evidence` reproduces today's `Edge` field for field. `run_path` becomes `Session::attach` + `run_session`; `discover`/`discover_with_types`/`build_descriptor` keep their signatures, built on a `Session` over the given connection's database, so callers do not change.

- [ ] **Step 1: Capture the baseline.** Before changing `relate.rs`, add `descriptor_is_byte_identical_to_baseline` to `evidence_edges.rs`: build each of `tests/relate-fixtures/build.sql` and `identifier-keys.build.sql`, run today's `run_path`-equivalent, and write the descriptor JSON to `tests/relate-fixtures/*.descriptor.golden.json` (committed). The test asserts equality against those files.
- [ ] **Step 2: Write the failing tests**: `edge_evidence_matches_manifest_counts` (case a: `orphan_count 0`, `overlap_distinct` = child distinct; case b: `orphan_count 1`); `edge_from_evidence_is_pure` (a hand-built `EdgeEvidence` equal to case a's yields `Accepted` with the same confidence as today, to 2 dp).
- [ ] **Step 3: Run** `cargo test -p dovetail-core --test evidence_edges`. Expected: the golden test PASSES (baseline), the two new tests FAIL.
- [ ] **Step 4: Implement** `edge_evidence`, `edge_from_evidence` and `run_session`, and rewire `run_path`.
- [ ] **Step 5: Run** `cargo test -p dovetail-core --test evidence_edges --test relate_discovery --test relate_semantic_keys --test descriptor_stitch --test demo_substrate`. Expected: PASS, golden descriptors unchanged byte for byte under the floor typing.
- [ ] **Step 6: Commit**: "Measure candidate joins with set-based SQL and score them from evidence rows".

### Task 4: Key credit for exact-name joins to key-like parents

**Files:**
- Modify: `crates/dovetail-core/src/relate.rs`, `tests/relate-fixtures/build.sql`, `tests/relate-fixtures/manifest.json`
- Test: `crates/dovetail-core/tests/relate_discovery.rs`

**Interfaces:**
- Consumes: `EdgeEvidence` (Task 3).
- Produces: `pub const KEY_NAME_MATCH_MIN_DISTINCT: i64 = 20;` and `fn exact_name_key_credit(e: &EdgeEvidence) -> bool`, true when child and parent column names are equal (case-insensitive), the parent is unique, `overlap_distinct == child_distinct`, `parent_distinct >= 20`, and the parent label is `None` or not in an excluded family. The excluded families are a `pub const` list of FineType label prefixes: person and place names, free text, dates and times, booleans, and measured numbers (read finetype's taxonomy for the exact domain prefixes, e.g. `identity.person.`, `geography.`, `datetime.`; record the list with a comment). When it holds, the parent's key signal is raised to `SEMANTIC_KEY_FLOOR`, the same floor an identifier type earns.

- [ ] **Step 1: Add fixture cases** to `build.sql`: (e) `orders_e.sku → products_e.sku`, 25 unique SKUs like `BEV-001`, full overlap → manifest `accepted`; (f) `customers_f.city → stores_f.city`, 3 unique cities → manifest `rejected`, why "small unique attribute column, not a key".
- [ ] **Step 2: Run** `cargo test -p dovetail-core --test relate_discovery`. Expected: FAIL on case e only.
- [ ] **Step 3: Implement** the credit.
- [ ] **Step 4: Run** `cargo test -p dovetail-core --test relate_discovery --test relate_semantic_keys --test evidence_edges`. Expected: PASS. If a golden descriptor changes, the change must be the new cases only; regenerate and review the diff.
- [ ] **Step 5: Commit**: "Credit an exact-name join to a unique, key-like parent".

### Task 5: The retail fixture and the answer-key format

**Files:**
- Create: `tests/role-fixtures/retail/build.sql`, `tests/role-fixtures/retail/answer-key.json`, `tests/role-fixtures/README.md`
- Create: `crates/dovetail-core/src/role_key.rs` (answer-key types and loader); modify `lib.rs`
- Test: `crates/dovetail-core/tests/role_fixture_retail.rs`

**Interfaces:**
- Produces:

```rust
pub struct AnswerKey { pub name: String, pub tables: BTreeMap<String, ExpectedTable>,
    pub folds: Vec<ExpectedFold>, pub combinations: Vec<ExpectedCombination> }
pub struct ExpectedTable { pub role: Role, pub why: String }
pub struct ExpectedFold { pub reference: String, pub into: String }
pub struct ExpectedCombination { pub name: String, pub parts: Vec<String>, pub kind: CombinationKind }
pub fn load_answer_key(path: &Path) -> Result<AnswerKey, String>;
```

`Role` and `CombinationKind` are defined here (serde lower-case names) and re-exported by `roles` (Task 6). Table names in keys match case-insensitively.

`build.sql` is deterministic (DuckDB `range()` with arithmetic or `hash()`-based values, no `random()`) and builds, with Brightfield's stand-in column names: `customers` (400 rows: `customer_id, name, email, joined_on`), `stores` (12: `store_id, city, region`), `categories` (8: `category_id, name`), `products` (120: `sku, name, category_id, unit_price`), `transactions` (5,000: `txn_id, customer_id, store_id, sku, quantity, amount, purchased_at`), `loyalty_visits` (1,500: `visit_id, customer_id, store_id, visited_on, points`), `customer_segments` (400 customers × 6 monthly `assessed_on` dates: `customer_id, segment, assessed_on`), `promo_codes` (20: `code, discount_pct, valid_to`, nothing joins to it); plus a partition pair `suppliers_north`/`suppliers_south` (disjoint `supplier_id`s, same columns) and a snapshot pair `price_list_2024`/`price_list_2025` (same `sku`s, different `unit_price`). The answer key: customers, stores, products entities; categories reference, folded into products; transactions, loyalty_visits events; customer_segments snapshot; promo_codes unclear; `suppliers` partition of the north/south pair (role entity); `price_list` snapshot of the 2024/2025 pair (role snapshot). Each `why` is one line.

- [ ] **Step 1: Write the failing tests**: `fixture_builds_with_expected_counts` (row counts above); `answer_key_loads` (10 tables after combination, `categories` folds into `products`, two combinations with the kinds above); `relate_finds_the_stand_in_joins` (via `run_session`: accepted edges include `transactions.customer_id→customers.customer_id`, `transactions.store_id→stores.store_id`, `transactions.sku→products.sku`, `products.category_id→categories.category_id`, `loyalty_visits.customer_id→customers.customer_id`, `customer_segments.customer_id→customers.customer_id`).
- [ ] **Step 2: Run** `cargo test -p dovetail-core --test role_fixture_retail`. Expected: FAIL.
- [ ] **Step 3: Write** the fixture, the key and `role_key.rs`.
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit**: "A retail role fixture at realistic size, and the answer-key format".

### Task 6: Role scoring as a pure function

**Files:**
- Create: `crates/dovetail-core/src/roles.rs`; modify `lib.rs`
- Test: unit tests in `roles.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `Edge`, `EdgeStatus` (relate); `Role` (role_key).
- Produces:

```rust
pub struct TimeColumn { pub name: String, pub fill_rate: f64, pub distinct: i64, pub audit: bool, pub varies_within_parent: bool }
pub struct TableEvidence { pub name: String, pub rows: i64, pub columns: Vec<String>, pub key: Vec<String>,
    pub key_is_identifier: bool, pub time_columns: Vec<TimeColumn>, pub label_columns: Vec<String>,
    pub attribute_columns: Vec<String>, pub constant_date: Option<String> }
pub struct RoleCall { pub table: String, pub role: Role, pub scores: Vec<(Role, f64)>,   // all six, in the enum's order
    pub evidence: Vec<String>, pub primary_key: Vec<String>, pub time_field: Option<String>,
    pub audit_time_fields: Vec<String>, pub typing: String }                            // "profiled" | "floor: <reason>"
pub fn points_at<'a>(edges: &'a [Edge]) -> impl Iterator<Item = &'a Edge>;           // accepted|suggested && parent_unique
pub fn score_table(t: &TableEvidence, edges: &[Edge], typing: &TypingSource) -> RoleCall;
```

Each role's score is a weighted sum in `[0,1]` of the evidence-for items in the spec's table, minus the evidence-against items, clamped; the weights are named `const`s at the top of `roles.rs`, one group per role. `role` is the top score unless it is under `ROLE_FLOOR` or within `ROLE_MARGIN` of the runner-up, then `Unclear`. Evidence sentences name concrete counts and tables (`"customer_id unique across 400 rows; pointed at by transactions, loyalty_visits"`). Ties between roles break in the enum's order, so the result is deterministic. `key` is empty when no column or pair is unique.

- [ ] **Step 1: Write the failing unit tests**, each on a hand-built `TableEvidence` and edge list: `entity_with_inbound_pointers`; `entity_with_audit_timestamp_is_not_event` (`created_at`, audit, no pointer out → entity, `audit_time_fields == ["created_at"]`); `event_with_repeating_pointer` (`purchased_at`, `varies_within_parent`, pointer to customers → event, `time_field == Some("purchased_at")`); `reference_small_label_lookup`; `small_store_list_is_entity` (12 rows, `city`/`region` as attribute columns, two inbound pointers → entity); `bridge_two_pointers_unique_pair` (key is the pair, empty attributes); `event_with_two_pointers_is_not_bridge` (two pointers plus a non-audit time column → event); `snapshot_repeating_key_few_dates`; `one_date_snapshot` (`constant_date` set, key is a unique pointer → snapshot); `snapshot_signals_count_against_event` (customer_segments shape → snapshot, event score lower by ≥ `ROLE_MARGIN`); `self_reference_still_entity` (an `employees` table with a unique key, no found edges, `reports_to` column → entity, not unclear); `empty_table_is_unclear` (evidence contains "no rows"); `close_scores_are_unclear` (top two within 0.15 → `Unclear`, both scores kept); `floor_typing_is_named` (`typing == "floor: …"`); `suggested_edge_to_non_unique_parent_ignored` (`points_at` drops it).
- [ ] **Step 2: Run** `cargo test -p dovetail-core --lib roles`. Expected: FAIL.
- [ ] **Step 3: Implement** `roles.rs`.
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit**: "Score each table's role from evidence rows".

### Task 7: `table_evidence` in SQL, and roles on the retail fixture

**Files:**
- Modify: `crates/dovetail-core/src/evidence.rs`
- Test: `crates/dovetail-core/tests/role_fixture_retail.rs`

**Interfaces:**
- Consumes: `ColumnEvidence`, `Edge`, `TableEvidence`, `score_table`.
- Produces: `pub fn table_evidence(s: &Session, cols: &[ColumnEvidence], edges: &[Edge], tables: &[String]) -> Result<Vec<TableEvidence>, EvidenceError>` (also writes `memory.table_evidence`).

Key search: a single column with `distinct == rows == non_null` first (prefer one that something points at, then an identifier label, then the first by ordinal); else pairs of columns in ordinal order, tested with one `count(DISTINCT (a, b))` query per candidate pair, pointer columns first; give up after the first 20 pairs. Time columns: DuckDB `DATE`/`TIMESTAMP*` types or a `datetime.` label. `varies_within_parent`: for the table's first pointer, `count(DISTINCT (pointer, time)) > count(DISTINCT pointer)`. Label columns: text columns, not key, not pointer, labelled as a name/description/code (or, under the floor, named `name`, `label`, `title`, `description`, `desc`, `code`); other non-key, non-pointer columns are attributes. `constant_date`: a date column with one distinct value.

- [ ] **Step 1: Write the failing test** `retail_roles_match_answer_key` (profiled; skips without the extension): build the fixture, `run_session`, `table_evidence`, `score_table` for the eight stand-in tables; each role equals the answer key; on failure print the table's scores and evidence. And `retail_roles_floor_recorded` (always runs): the same under forced floor typing; asserts only that it runs and every call's `typing` starts with `floor`.
- [ ] **Step 2: Run** `cargo test -p dovetail-core --test role_fixture_retail`. Expected: FAIL.
- [ ] **Step 3: Implement** `table_evidence`. Tune the role weights only as far as the retail fixture needs, and record each change's reason in a comment beside the constant.
- [ ] **Step 4: Run** the same command and `cargo test -p dovetail-core --lib roles`. Expected: PASS.
- [ ] **Step 5: Commit**: "Measure tables in DuckDB, and call the retail fixture's roles".

### Task 8: Folds

**Files:**
- Modify: `crates/dovetail-core/src/roles.rs`
- Test: unit tests in `roles.rs`; `role_fixture_retail.rs`

**Interfaces:**
- Produces: `pub struct Fold { pub reference: String, pub host: String, pub child_column: String, pub parent_column: String, pub struct_column: String }` and `pub fn propose_folds(calls: &[RoleCall], edges: &[Edge]) -> Vec<Fold>`, sorted by `reference`.

A fold is proposed for each `Reference` call with exactly one distinct table pointing at it (`points_at`). `struct_column` is `singularise(reference)`. The fold rule is one function, `fn foldable(pointers: &[&Edge]) -> bool`, so it changes in one place.

- [ ] **Step 1: Write the failing tests**: `single_pointer_reference_folds` (`categories` into `products`, `struct_column "category"`); `shared_reference_stays` (two tables point at it → no fold); `non_reference_never_folds`; and in `role_fixture_retail.rs`, `retail_folds_match_answer_key` (profiled).
- [ ] **Step 2: Run** `cargo test -p dovetail-core --lib roles` and `--test role_fixture_retail`. Expected: FAIL.
- [ ] **Step 3: Implement** `propose_folds`.
- [ ] **Step 4: Run** both. Expected: PASS.
- [ ] **Step 5: Commit**: "Propose folding a lookup into the one table that uses it".

### Task 9: Combinations, measured through views, with joins rediscovered

**Files:**
- Create: `crates/dovetail-core/src/combine.rs`; modify `lib.rs`, `evidence.rs`
- Test: `crates/dovetail-core/tests/role_combinations.rs`; `role_fixture_retail.rs`

**Interfaces:**
- Consumes: `Session`, `ColumnEvidence`, `edge_evidence`, `table_evidence`.
- Produces:

```rust
pub struct TokenColumn { pub name: String, pub values: Vec<(String, String)> }   // (part, value), sorted by part
pub struct Combination { pub name: String, pub parts: Vec<String>, pub kind: CombinationKind,
    pub token_column: Option<TokenColumn>, pub source_sql: String, pub evidence: Vec<String> }
pub struct CarriedKey { pub child: ColumnRef, pub snapshot: String, pub key: String, pub overlap: f64 }
pub fn find_combinations(s: &Session, cols: &[ColumnEvidence]) -> Result<Vec<Combination>, EvidenceError>;
pub fn create_views(s: &Session, combos: &[Combination]) -> Result<Vec<String>, EvidenceError>; // memory views named after each combination; returns the table list to measure: views plus untouched tables
```

Grouping: tables whose column-name sets are equal (order ignored), types in the same `type_family`, labels equal where both are labelled, and names equal after removing one token (a run of digits, a date such as `2024-01`, or a word that differs between the names, e.g. `north`/`south`); `name` is the name without the token and its separator. Kind, checked in this order: **duplicate** if the parts' rows are identical (`EXCEPT ALL` both ways empty); else **snapshot** if the key (from `table_evidence` on the first part) overlaps across parts by at least `SNAPSHOT_MIN_KEY_OVERLAP = 0.5` (named const, tuned later); else **partition**. `source_sql` is `SELECT …, '<value>' AS "<token>" FROM src."<part>" UNION ALL BY NAME …` for partition and snapshot (token column typed as `INTEGER` for a year, `DATE` for a date, else `VARCHAR` named `period` for digits or after the varying word's role, defaulting to `part`); for duplicate it is the first part alone. After `create_views`, `edge_evidence` and `table_evidence` run over the returned list, so parts never appear in edges. For a snapshot combination, an edge from a child to its entity key is not a relationship: it becomes a `CarriedKey` with its overlap, and the snapshot's evidence names it.

- [ ] **Step 1: Write the failing tests** in `role_combinations.rs`: `partition_pair` (`suppliers_north`/`suppliers_south` → `suppliers`, partition, token values `north`, `south`); `snapshot_pair` (`price_list_2024`/`price_list_2025` → snapshot, token `INTEGER` 2024/2025, key `[sku, <token>]`); `duplicate_checked_first` (two identical tables → duplicate, not snapshot); `different_columns_never_combine`; `no_edges_between_parts`; `edges_point_at_combined_view` (an `orders` table pointing at both supplier parts' ids → one accepted edge to `suppliers`, no orphans); `snapshot_child_key_is_carried` (a child with `sku` against the price-list snapshot → `CarriedKey`, no edge). In `role_fixture_retail.rs`: `retail_combinations_match_answer_key` (profiled).
- [ ] **Step 2: Run** `cargo test -p dovetail-core --test role_combinations`. Expected: FAIL.
- [ ] **Step 3: Implement** `combine.rs` and the view-based measurement.
- [ ] **Step 4: Run** it, `--test role_fixture_retail`, and `--lib roles`. Expected: PASS.
- [ ] **Step 5: Commit**: "Combine same-kind tables, and measure joins against the combined dataset".

### Task 10: The `RoleModel`, and `dovetail relate` prints it

**Files:**
- Modify: `crates/dovetail-core/src/roles.rs`, `crates/dovetail/src/main.rs` (`run_relate`)
- Test: `crates/dovetail-core/tests/role_model.rs`; `crates/dovetail/tests/` CLI test if the bin has one (else in `role_model.rs` through `render_summary`)

**Interfaces:**
- Produces:

```rust
pub struct RoleModel { pub typing: TypingSource, pub tables: Vec<RoleCall>, pub folds: Vec<Fold>,
    pub combinations: Vec<Combination>, pub carried_keys: Vec<CarriedKey>, pub edges: Vec<Edge> }
pub fn run_session(s: &Session) -> Result<RoleModel, EvidenceError>;   // roles::run_session: column evidence → combinations → views → edges → tables → roles → folds
pub fn run_path(path: &Path) -> Result<RoleModel, EvidenceError>;
impl RoleModel { pub fn render_summary(&self) -> String; }
```

`render_summary` prints, after a `typing:` line, one line per table sorted by role then name: role, name, score to 2 dp, the runner-up role and score, and the first evidence sentence; then folds and combinations, one line each. `dovetail relate` keeps its descriptor output and printed join summary unchanged, and appends this section.

- [ ] **Step 1: Write the failing tests**: `retail_model_is_deterministic` (two `run_session` calls on fresh sessions give byte-identical `render_summary`); `summary_names_typing` (the first line starts `typing: profiled` or `typing: floor`); `relate_output_keeps_descriptor` (the descriptor `run_path` writes is unchanged by the roles section: compare against Task 3's golden files).
- [ ] **Step 2: Run** `cargo test -p dovetail-core --test role_model`. Expected: FAIL.
- [ ] **Step 3: Implement** `RoleModel`, `run_session`, `run_path`, `render_summary`, and the CLI call.
- [ ] **Step 4: Run** it, then `cargo build -p dovetail` and `cargo run -p dovetail -- relate <a DuckDB file built from tests/role-fixtures/retail/build.sql>`. Expected: PASS; the printed section lists the eight stand-in tables and the two combinations.
- [ ] **Step 5: Commit**: "Carry roles, folds and combinations in one model, and print it from relate".

### Task 11: The eval harness and tier 1

**Files:**
- Create: `crates/dovetail-core/src/role_eval.rs`; modify `lib.rs`
- Create: `tests/role-corpus/RESULTS.md` (generated, committed)
- Test: `crates/dovetail-core/tests/role_eval_harness.rs`

**Interfaces:**
- Consumes: `AnswerKey`, `RoleModel`, `Session`.
- Produces:

```rust
pub trait RoleScorer { fn name(&self) -> &str; fn score(&self, s: &Session) -> Result<RoleModel, EvidenceError>; }
pub struct ScorePerRole;                    // name "score-per-role"; delegates to roles::run_session
pub struct SetResult { pub set: String, pub tier: u8, pub typing: String, pub tables: usize, pub correct: usize,
    pub confusion: BTreeMap<(Role, Role), usize>, pub folds: PrecisionRecall, pub combinations: PrecisionRecall,
    pub misses: Vec<String> }
pub struct PrecisionRecall { pub true_pos: usize, pub false_pos: usize, pub false_neg: usize }
pub fn score_set(model: &RoleModel, key: &AnswerKey, tier: u8) -> SetResult;
pub fn render_report(results: &[SetResult], stamp: &str) -> String;   // Markdown; stamp names finetype version, model, dovetail commit
```

Matching is case-insensitive on table names. The report has a summary table (set, tier, typing, tables, role accuracy, fold P/R, combination P/R), a confusion matrix per tier, and the misses list, all sorted, so it diffs cleanly.

- [ ] **Step 1: Write the failing tests**: `score_set_counts` (hand-built model and key: 3 of 4 roles right, one fold missed → `correct 3`, folds `fn 1`); `report_is_deterministic`; `tier1_retail_is_exact` (profiled; skips without the extension): `correct == tables`, folds and combinations with no false positives or negatives; `tier1_retail_floor_recorded`: runs floor-only, asserts it completes and that the report labels it `floor`.
- [ ] **Step 2: Run** `cargo test -p dovetail-core --test role_eval_harness`. Expected: FAIL.
- [ ] **Step 3: Implement** `role_eval.rs`.
- [ ] **Step 4: Run** the same command. Expected: PASS. Write the tier-1 report (both typings) to `tests/role-corpus/RESULTS.md`.
- [ ] **Step 5: Add CI**: in `.github/workflows/ci.yml`, the existing test job already runs `role_eval_harness` floor-only. If finetype's model and extension can be fetched in CI the way finetype's own CI fetches them, add a step that sets `FINETYPE_MODEL_DIR` and runs `tier1_retail_is_exact`; if not, note in `tests/role-fixtures/README.md` that the profiled exact check is a local gate run before a branch is called ready.
- [ ] **Step 6: Commit**: "Score role calls against answer keys, and hold the retail fixture exact".

### Task 12: The corpus, its answer keys, and the label checkpoint

**Files:**
- Create: `scripts/fetch-role-corpus.sh`, `tests/role-corpus/README.md`, `tests/role-corpus/ctu/<database>.json` (one answer key per database), `crates/dovetail-core/src/role_corpus.rs` (profile cache), `crates/dovetail-core/tests/role_corpus_eval.rs`

**Interfaces:**
- Consumes: `RoleScorer`, `score_set`, `render_report`, `load_answer_key`.
- Produces: `pub fn load_corpus_set(dir: &Path) -> Result<Session, EvidenceError>` (an in-memory session whose `src` holds one table per Parquet file, `CREATE TABLE src."<stem>" AS FROM read_parquet(...)`) and `pub fn profile_cache_stamp() -> String` (finetype version from `ft_version()`, the model identity, and the finetype commit from `Cargo.lock`).

`fetch-role-corpus.sh` takes `DOVETAIL_CORPUS` (required, refuses to run without it) and writes `ctu/<db>/<table>.parquet` plus `ctu/<db>/declared-keys.csv` for each database in a list at the top of the script (the 25: Chinook, northwind, sakila, classicmodels, pubs, AdventureWorks2014, ErgastF1, lahman_2014, Hockey, Basketball_men, financial, imdb_small, imdb_MovieLens, Mondial, GOSales, Walmart_2014, world, university, stats_CEB, employee, TubePricing, CraftBeer, Countries, PremierLeague, ccs), copying each table through `mysql_query` from `relational.fel.cvut.cz` (guest access; DuckDB's attached MySQL catalogue confuses same-named tables across databases, so tables are copied by query), and `gittables/sample/` with 2,000 tables drawn from a local GitTables copy named by `GITTABLES_DIR`, stratified by topic directory, seed `20261026`. Each set's profile is cached in `<set>/.profile.json` with the stamp, and rebuilt when the stamp differs.

- [ ] **Step 1: Write the script** and `tests/role-corpus/README.md` (what each tier is, how to fetch, licences: the data is not committed).
- [ ] **Step 2: Draft the answer keys** for all 25 CTU databases: every table's role and a one-line why, folds, combinations, using each database's declared keys as the join evidence. Keys use the original table names.
- [ ] **Step 3: Checkpoint: Hugh reviews the labels.** Flag the Kata issue `needs-human` with "Role labels for the CTU corpus are ready for review", push the branch, and stop until he has reviewed them. Apply his corrections. **No threshold is tuned against tier 2 before this.**
- [ ] **Step 4: Write** `role_corpus_eval.rs`: `corpus_baseline` (skips without `DOVETAIL_CORPUS`) runs `ScorePerRole` over tiers 2 and 3, profiled and floor-only, and writes the full report to `tests/role-corpus/RESULTS.md`; for tier 2 it also reports join recall against `declared-keys.csv`; for tier 3, the role distribution, the unclear rate and the reference rate.
- [ ] **Step 5: Run** `DOVETAIL_CORPUS=… cargo test -p dovetail-core --test role_corpus_eval -- --nocapture`. Expected: completes; `RESULTS.md` holds tiers 1–3 with the stamp. Cost: the first run profiles every corpus table (expect tens of minutes); later runs read the cache.
- [ ] **Step 6: Commit** the script, keys, harness and `RESULTS.md`: "Fetch the role corpus, label it, and record the first baseline".

### Task 13: Fold the design into the repo and ask Brightfield for its changes

**Files:**
- Modify: `CLAUDE.md` (the `relate` command's description: roles, folds, combinations; the evidence session and FineType extension; the corpus and eval), `README.md` if it describes `relate`, `CHANGELOG.md` (`[Unreleased]`)
- Delete: `docs/superpowers/specs/2026-10-26-table-roles-design.md`, `docs/superpowers/plans/2026-10-26-table-roles.md`

- [ ] **Step 1: Write** the docs: the roles and their evidence table, the fold and combination rules, where the work runs, the typing order and its fallback, the corpus tiers and how to run the eval, and the open thresholds (tiers 2–3 targets set from the baseline).
- [ ] **Step 2: Record the Brightfield asks** as a comment on Brightfield's own-files Kata issue (not on GitHub): the six changes from the spec's *What Brightfield needs to change*, each one line, plus the retail fixture's larger data for its stand-in.
- [ ] **Step 3: Run** `cargo test --workspace` and `scripts/check-public-hygiene.sh`. Expected: PASS.
- [ ] **Step 4: Commit**: "Document table roles, and fold the spec and plan into the repo".
