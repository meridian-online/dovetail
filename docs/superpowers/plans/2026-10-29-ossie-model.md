# Ossie model writer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `dovetail model` writes one Apache Ossie `0.2.0.dev0` model of a DuckDB's tables, from a sample pass and then a full verification pass, typed from FineType's data packages and never changing what the analyst decided.

**Architecture:** A new `ossie` module in `dovetail-core` holds the wire types and the projection from table roles' `RoleModel`; a `packages` module reads the data packages; a new `Session` constructor builds the in-memory `src` from named source tables, dropping lineage columns and optionally sampling, with the analyst's database attached beside it as `full` for whole-table measurements. The CLI crate gains a library `cli_main` and a `model` subcommand.

**Tech Stack:** Rust, DuckDB 2.0.0 (`duckdb = "=1.20000.0"`, bundled), `serde_yaml` 0.9, `jsonschema` (dev), FineType v0.6.64.

**Branch:** `ossie-model`, cut from `table-roles`; it merges after table roles does, and is rebased onto `main` once that lands.

**Spec:** `docs/superpowers/specs/2026-10-29-ossie-model-design.md`. Read it first; it is the authority on every shape below. Also read `docs/superpowers/specs/2026-10-26-table-roles-design.md` for the `RoleModel` this serialises.

## Global Constraints

- Ossie `0.2.0.dev0`, schema at `apache/ossie` commit `d13bb6a534a100e792a048dd7a14d4d37c723b2c`, vendored as `vendor/ossie/ossie-schema.json` with its licence.
- Meridian vendor name `MERIDIAN`; `data` is a JSON string; Meridian's entry first in `custom_extensions`.
- Wire key order, model: `version, name, description, datasets, relationships, metrics, custom_extensions`; dataset: `name, source, description, primary_key, fields, metrics, custom_extensions`; field: `name, expression, datatype, custom_extensions`; relationship: `name, from, to, from_columns, to_columns, custom_extensions`. Unknown keys are kept and written back after these.
- Meridian JSON key names exactly as Brightfield's `brightfield-ossie` `model.rs` defines them (`role`, `role_evidence`, `folded_into`, `excluded`, `decided_by`, `source_file`, `source_part`, `rows`; `finetype`; `grade`, `evidence{value_overlap, orphans, child_rows, child_type, parent_type, types_agree, name_similarity, parent_duplicates}`, `cardinality`, `verified`, `warning`; `generator`, `rejected`, `skipped_sources`), plus the spec's new keys `role_scores`, `fold_column`, `combined_into`, `combination`, and model provenance `pass`, `typing`, `sample_rows`.
- Lineage columns `__source_file`, `__source_part`, `__source_row` are never measured and never fields.
- `RAW_PREFIX = "raw_"`; `SAMPLE_ROWS = 50_000`.
- Everything works under `--no-default-features`; every new test file passes with `cargo test -p dovetail-core --no-default-features`.
- Determinism: no `HashMap` iteration on an order-bearing path; byte-identical output for the same inputs.
- Public repo: no private planning identifiers or local paths in code, comments, commits or PR text.
- One cargo build at a time on Hugh's Mac; run the touched tests before each commit, `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` too.

## Review Focus

1. **Quoting.** Table and column names with spaces, quotes, dots or upper case (`"Order Lines"`, `o'brien`) must survive into `source`, the sampled copies and the combination query. Test owned by Task 2.
2. **A package resource with no `raw_` table, or a table with zero rows.** Skipped with a warning, or written with `rows: 0` and role unclear; never a crash. Tests in Tasks 2 and 4.
3. **A sampled table whose key repeats only outside the sample.** The full pass drops the key with its sentence. Test in Task 6.
4. **Base files Brightfield wrote, with keys dovetail does not know** (a field's `label`, a dataset's `unique_keys`, a non-Meridian extension). Round-trip unchanged. Test in Task 1.
5. **A `--verify` proposal from an older dovetail, or garbage YAML.** Fresh discovery, not an error. Test in Task 6.

---

### Task 1: Ossie wire types, read and write, vendored schema

**Files:**
- Create: `crates/dovetail-core/src/ossie/mod.rs`, `crates/dovetail-core/src/ossie/wire.rs`, `crates/dovetail-core/src/ossie/meridian.rs`
- Create: `vendor/ossie/ossie-schema.json`, `vendor/ossie/LICENSE` (copy from `apache/ossie` at the pinned commit; Brightfield vendors the same file as `vendor/ossie/osi-schema.json`)
- Create: `tests/ossie-fixtures/brightfield-retail.ossie.yaml` (Brightfield's `crates/brightfield-ossie/fixtures/retail_extract/model.ossie.yaml`, MIT, with a header comment naming its origin)
- Modify: `crates/dovetail-core/Cargo.toml` (`serde_yaml = "0.9.34"` to `[dependencies]`), `crates/dovetail-core/src/lib.rs`
- Test: `crates/dovetail-core/tests/ossie_wire.rs`

**Interfaces:**
- Produces: `pub struct Model { version, name, description: Option<String>, datasets: Vec<Dataset>, relationships: Vec<Relationship>, metrics: Vec<serde_yaml::Value>, meta: ModelMeta, extensions: Vec<Extension>, extras: BTreeMap<String, serde_yaml::Value> }`, and `Dataset`, `Field`, `Relationship` likewise (core keys typed, `meta` decoded from the MERIDIAN entry, other extensions and unknown keys kept). `DatasetMeta`, `FieldMeta`, `RelationshipMeta`, `Evidence`, `ModelMeta` with Brightfield's fields plus the new keys, each new one `Option`/empty-skipped. Each meta keeps unknown JSON keys in a `#[serde(flatten)] extra: serde_json::Map` so an analyst element round-trips. Enums `Role`, `Grade`, `DecidedBy`, `Verified` lowercase. `pub fn read(yaml: &str) -> Result<Model, OssieError>`, `pub fn write(model: &Model) -> String`, `pub const OSSIE_VERSION: &str = "0.2.0.dev0"`, `pub const VENDOR: &str = "MERIDIAN"`, `pub const LINEAGE: [&str; 3]`.

- [ ] **Step 1: Write the failing tests** in `ossie_wire.rs`:
  - `brightfield_fixture_reads_and_validates`: read the fixture; 8 datasets, 7 relationships; `categories` has `role == Reference`, `folded_into == Some("products")`; the fixture validates against `vendor/ossie/ossie-schema.json` (`jsonschema::validator_for`, YAML → JSON value).
  - `write_then_read_is_identity`: `read(write(read(fixture))) == read(fixture)`, and `write(read(write(m))) == write(m)` byte for byte.
  - `unknown_keys_survive`: a model with a field `label: x`, a dataset `unique_keys: [[a]]`, a non-MERIDIAN extension and a MERIDIAN JSON key `"note":"y"` writes all four back.
  - `key_order_is_the_wire_order`: the written dataset's keys appear in `name, source, description, primary_key, fields, metrics, custom_extensions` order.
- [ ] **Step 2: Run** `cargo test -p dovetail-core --no-default-features --test ossie_wire`. Expected: FAIL, module `ossie` not found.
- [ ] **Step 3: Implement** the module. Write through private wire structs with serde field order matching the global constraint and `#[serde(flatten)] extras` last, as Brightfield's `wire.rs` does; Meridian `data` is `serde_json::to_string` of the meta (with `preserve_order`, so key order is the struct's).
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit** `Add Ossie model types that read and write Brightfield's shapes`.

### Task 2: Source tables, packages typing and the session built from them

**Files:**
- Create: `crates/dovetail-core/src/packages.rs`
- Modify: `crates/dovetail-core/src/evidence.rs` (new constructor; `Session` gains the source-table map), `crates/dovetail-core/src/finetype_ext.rs` (`TypingSource::Packages`), wherever columns are labelled from the typing (follow `column_evidence`), `crates/dovetail-core/src/roles.rs` (evidence names the packages typing as it names the other two)
- Create: `tests/ossie-fixtures/retail-packages/<table>/datapackage.json` for the retail role fixture's tables (labels as the profiled retail run gives them; leave one column per table unlabelled)
- Test: `crates/dovetail-core/tests/ossie_session.rs`

**Interfaces:**
- Produces:
  - `pub struct SourceTable { pub name: String, pub relation: String, pub source_file: Option<String>, pub source_part: Option<String>, pub fields: Vec<PackageField> }` and `pub struct PackageField { pub name: String, pub frictionless_type: String, pub finetype: Option<String> }`.
  - `pub fn read_packages(dir: &Path, db_tables: &[String]) -> Result<(Vec<SourceTable>, Vec<String>), PackagesError>`: resources sorted by name, `relation = RAW_PREFIX + name`; the second value is the warnings (a resource with no table). `source_file` resolved per the spec.
  - `pub fn all_tables(db: &Path) -> Result<Vec<SourceTable>, EvidenceError>`: every table, named and related as itself, no fields (typing by request).
  - `pub enum SessionTyping { Packages, Request(TypingRequest) }`.
  - `Session::over(db: &Path, tables: &[SourceTable], sample: Option<u64>, typing: SessionTyping) -> Result<Session, EvidenceError>`: opens in-memory, attaches `db` read-only as `full`, attaches `:memory:` as `src`, and for each table creates `src.main."<name>"` as `SELECT * EXCLUDE (<lineage present>) FROM full.main."<relation>"`: a view when `sample` is `None`; a table with `USING SAMPLE reservoir(<n> ROWS) REPEATABLE (42)` under `SET threads = 1` when `Some(n)` and the table has more than `n` rows, else a full copy. `pub fn full_relation(&self, name: &str) -> Option<&str>` and `pub fn sampled(&self) -> Option<u64>`.
  - `TypingSource::Packages`: labels come from each `PackageField.finetype`; a field with none is untyped.

- [ ] **Step 1: Write the failing tests**:
  - `packages_name_the_datasets`: build a DuckDB file in a temp dir from the retail `build.sql` with each table renamed `raw_<t>` and the three lineage columns added; `read_packages` over the fixture returns every retail table, sorted, with `relation == "raw_<t>"`; a resource added with no table comes back as one warning.
  - `lineage_is_never_measured`: `column_evidence` over `Session::over(.., None, Packages)` lists no `__source_*` column, and no table's key is `__source_row`.
  - `unlabelled_field_is_untyped`: the unlabelled column has `label == None` under `Packages`, though the floor would type it (pick an email or ISO-date column for it).
  - `quoted_names_survive`: a table `raw_Order Lines` with a column `o'brien` is measured as `Order Lines`.
  - `other_tables_are_ignored`: a `silver_customers` table in the file is not in the session.
- [ ] **Step 2: Run** `cargo test -p dovetail-core --no-default-features --test ossie_session`. Expected: FAIL.
- [ ] **Step 3: Implement.** Existing callers (`Session::attach`, `Session::from_sql`, every `src.main.` query) must keep working unchanged; `over` builds the same `src` catalog they read.
- [ ] **Step 4: Run** it, then `cargo test -p dovetail-core --no-default-features` (every existing relate and roles test still passes). Expected: PASS.
- [ ] **Step 5: Commit** `Build a session from named source tables, typed by FineType's data packages`.

### Task 3: The sample measures a parent's whole key on the full table

**Files:**
- Modify: `crates/dovetail-core/src/evidence.rs` (`edge_evidence_with`, and `EdgeEvidence` gains `parent_duplicates: i64`), `crates/dovetail-core/src/relate.rs` (carry `parent_duplicates` into `Evidence`), table row counts in `table_evidence`
- Test: `crates/dovetail-core/tests/ossie_sample.rs`

**Interfaces:**
- Consumes: `Session::over`, `Session::full_relation`, `Session::sampled` (Task 2).
- Produces: `pub const SAMPLE_ROWS: u64 = 50_000;` in `evidence.rs`; `Evidence.parent_duplicates: i64` (parent key values held by more than one row, NULLs excluded, as Brightfield's `evidence_sql` counts them).

- [ ] **Step 1: Write the failing tests** (use a small `n` through `Session::over(.., Some(n), ..)` so the fixture stays small: parent 2,000 rows, child 20,000 rows, `n = 500`):
  - `sampled_parent_key_still_joins`: the child→parent edge is `Accepted`, `orphan_count == 0`, `parent_unique`, `value_overlap == 1.0`.
  - `sampled_rows_are_full_counts`: each table's `rows` is its full count.
  - `sample_is_repeatable`: two sessions give identical `column_evidence` and edges.
  - `parent_duplicates_counted`: a parent with 3 repeated key values reports `parent_duplicates == 3`, sampled or not.
- [ ] **Step 2: Run** `cargo test -p dovetail-core --no-default-features --test ossie_sample`. Expected: FAIL.
- [ ] **Step 3: Implement**: when the session is sampled, the parent side of overlap, orphans, uniqueness and duplicates reads `full.main."<full_relation>"` (lineage excluded), and row counts read the full relation; everything else reads `src` as now.
- [ ] **Step 4: Run** it and the whole `--no-default-features` suite. Expected: PASS, with every existing test unchanged.
- [ ] **Step 5: Commit** `Measure a parent's whole key on the full table when the session is sampled`.

### Task 4: Project a role model to an Ossie model

**Files:**
- Create: `crates/dovetail-core/src/ossie/project.rs`
- Create: `tests/ossie-fixtures/retail.sample.ossie.yaml` (golden, written by the test under `DOVETAIL_WRITE_OSSIE_GOLDEN=1`)
- Test: `crates/dovetail-core/tests/ossie_project.rs`

**Interfaces:**
- Consumes: `RoleModel` (`roles.rs`), `SourceTable`, `Session`, Task 1's types.
- Produces: `pub struct Provenance { pub pass: Verified, pub typing: TypingSource, pub sample_rows: Option<u64> }` and `pub fn project(model: &RoleModel, tables: &[SourceTable], session: &Session, name: &str, provenance: &Provenance) -> Model`.

Rules, all from the spec's *Datasets and typing* and *The Meridian extension on each element*: dataset per source table (and per combination, with parts carrying `combined_into`); `source` is `full_relation` (combinations: `source_sql` rewritten from `src` names to quoted full relations); fields in column order; `datatype` only when typed, with `DECIMAL` → `Decimal`, `TIMESTAMP WITH TIME ZONE` → `DateTimeTz`; `dimension.is_time` true on `time_field`, false on `audit_time_fields`; `role_scores` in enum order; relationships from `RoleModel.edges` filtered to accepted (`likely`) and suggested (`possible`) with a unique parent, child dataset as `from`; name `<from>_<to>`, `_<from column>` appended when a pair has more than one; `cardinality` `1:1` when the child column is unique else `N:1`; `child_type`/`parent_type` the FineType labels; `types_agree` per the spec; `verified` from `provenance.pass`; `generator` `dovetail <CARGO_PKG_VERSION>`; datasets and relationships sorted by name.

- [ ] **Step 1: Write the failing tests** over the retail fixture as a packages session (Task 2's file builder, `sample: None`):
  - `every_source_table_is_a_dataset`; `categories` has `role == Reference`, `folded_into == Some("products")`, `fold_column == Some("category")`.
  - `relationships_point_many_to_one`: `transactions_customers` has `from == "transactions"`, `to == "customers"`, `grade == Likely`, `cardinality == "N:1"`, `evidence.child_type == evidence.parent_type`, `types_agree`.
  - `combinations_are_datasets_with_parts`: the partition and snapshot combinations are datasets whose `source` starts `SELECT` and names `"raw_` relations; each part has `combined_into`; no relationship has a part as `from` or `to`.
  - `time_fields_are_marked`: the transactions event's time field has `dimension.is_time == true`; an audit column `is_time == false`.
  - `no_lineage_fields`; `empty_table_is_unclear_with_zero_rows` (add one empty table to the fixture DB).
  - `golden_matches`: `write(project(..))` equals `tests/ossie-fixtures/retail.sample.ossie.yaml` and validates against the vendored schema; `project` twice is byte-identical.
- [ ] **Step 2: Run** `cargo test -p dovetail-core --no-default-features --test ossie_project`. Expected: FAIL.
- [ ] **Step 3: Implement** `project`.
- [ ] **Step 4: Write the golden** with `DOVETAIL_WRITE_OSSIE_GOLDEN=1`, read it through, check every retail role and fold matches `tests/role-fixtures/retail/answer-key.json`, then run without the variable. Expected: PASS.
- [ ] **Step 5: Commit** `Project the role model to an Ossie model`.

### Task 5: Carry the analyst's decisions through

**Files:**
- Create: `crates/dovetail-core/src/ossie/carry.rs`
- Test: `crates/dovetail-core/tests/ossie_carry.rs`

**Interfaces:**
- Produces: `pub fn carry(proposal: Model, base: &Model) -> Model`: analyst-decided datasets, fields and relationships from `base` replace the proposal's of the same name (datasets, fields) or same `(from, from_columns, to, to_columns)` (relationships), or are added where the proposal lacks them; `excluded` from base kept on any dataset; relationships matching `base.meta.rejected` removed; `meta.rejected` and `meta.skipped_sources` copied from base. Order stays the proposal's, base-only elements appended in base order.

- [ ] **Step 1: Write the failing tests** with a base built from the golden: one dataset and one field marked `analyst` with a changed role and a MERIDIAN `"note"` key; one analyst relationship absent from the proposal; one dataset `excluded: true`; one proposed join listed in `rejected`; two `skipped_sources`.
  - `analyst_elements_are_byte_identical`: each analyst element's YAML (written alone) is identical before and after.
  - `rejected_join_is_not_proposed`; `skipped_sources_and_rejected_carry`; `excluded_survives`.
- [ ] **Step 2: Run** `cargo test -p dovetail-core --no-default-features --test ossie_carry`. Expected: FAIL.
- [ ] **Step 3: Implement** `carry`.
- [ ] **Step 4: Run** it. Expected: PASS.
- [ ] **Step 5: Commit** `Carry the analyst's decisions through every proposal`.

### Task 6: The full pass verifies the sample's proposal

**Files:**
- Create: `crates/dovetail-core/src/ossie/verify.rs`, `crates/dovetail-core/src/ossie/run.rs`
- Test: `crates/dovetail-core/tests/ossie_passes.rs`

**Interfaces:**
- Consumes: Tasks 2–5.
- Produces:
  - `pub struct ModelRequest { pub db: PathBuf, pub packages: Option<PathBuf>, pub pass: Verified, pub base: Option<PathBuf>, pub verify: Option<PathBuf>, pub name: String, pub typing: TypingRequest }`.
  - `pub fn run_model(req: &ModelRequest) -> Result<(Model, Vec<String>), ModelError>`: the model and the run's warnings. Sample: `Session::over(.., Some(SAMPLE_ROWS), ..)`, `roles::run_session`, `project`, then `carry` when `base`. Full: verify when the proposal at `verify` reads, has `generator` starting `dovetail `, and names exactly the run's source-table datasets (parts and combinations included as projected); otherwise discover afresh with `sample: None`. Then `carry`.
  - `pub fn verify_full(proposal: Model, session: &Session) -> Model`: for each dovetail relationship, re-measure that one edge on the full data (`edge_evidence_with` over the pair) and rewrite `evidence`, `grade` (accepted → likely; suggested → possible; rejected → possible with `warning`), `verified: full`; for each dovetail dataset, `rows` from the full table and its `primary_key` checked for uniqueness, dropped with a `role_evidence` sentence when not unique. Nothing else changes.

- [ ] **Step 1: Write the failing tests**:
  - `full_pass_rearranges_nothing`: a DB whose 500-row sample shows `orders.customer_id → customers.customer_id` clean and `customers.customer_id` unique, but whose full data has orphans and one duplicate key; the full model has the same datasets and relationships in the same order as the sample's, the join `possible` with a `warning` naming the orphans, `customer_id` gone from `customers.primary_key`, a sentence in its `role_evidence`, and every relationship `verified: full`.
  - `full_pass_without_verify_discovers`; `full_pass_on_standin_discovers` (a `--verify` file shaped like Brightfield's stand-in: `generator: brightfield stand-in`, every table unclear, no joins) finds the retail joins; `full_pass_on_stale_proposal_discovers` (a dataset missing); `full_pass_on_garbage_discovers` (`verify` file holds `not: [yaml`).
  - `sample_pass_says_sample`: every relationship `verified: sample`; two runs byte-identical.
- [ ] **Step 2: Run** `cargo test -p dovetail-core --no-default-features --test ossie_passes`. Expected: FAIL.
- [ ] **Step 3: Implement.** Warning wording is the builder's; it names the counts.
- [ ] **Step 4: Run** it. Expected: PASS.
- [ ] **Step 5: Commit** `Verify the sample's proposal on the full data, rewriting only grades and evidence`.

### Task 7: `dovetail model` and the library entry point

**Files:**
- Create: `crates/dovetail/src/lib.rs` (today's `main.rs` body as `pub fn cli_main() -> ExitCode`, plus `run_model_cmd`)
- Modify: `crates/dovetail/src/main.rs` (`fn main() -> ExitCode { dovetail::cli_main() }`), `crates/dovetail/Cargo.toml` (`[lib] name = "dovetail"`)
- Test: `crates/dovetail/tests/model_cli.rs`

**Interfaces:**
- Consumes: `run_model`, `write` (Tasks 1, 6).
- Produces: the command line in the spec's *The command*: `--db`, `--pass`, `--out` required; `--packages`, `--base`, `--verify`, `--name` optional (`--name` defaults to the `--db` file's stem; typing is `TypingRequest::Best` when no packages are given). Writes `<out>.writing` then renames; warnings to stderr prefixed `dovetail model:`; exit 0 written, 1 failed, 2 usage. `model --help` text in the style of `RELATE_HELP`, and a `model` line in the top-level usage.

- [ ] **Step 1: Write the failing tests** (run the built binary via `env!("CARGO_BIN_EXE_dovetail")` over Task 2's retail DB file and packages fixture):
  - `sample_then_full`: both exit 0; `.discovery/sample.ossie.yaml` and `full.ossie.yaml` exist, read, validate; no `*.writing` left; `full` passes `--verify` the sample's file and keeps its relationships in order.
  - `usage_errors_exit_2`: missing `--out`; `--pass sometimes`.
  - `missing_db_exits_1` with a `dovetail model:` message and no `--out` file.
  - `base_is_never_written`: `--base` file bytes unchanged.
- [ ] **Step 2: Run** `cargo test -p dovetail --no-default-features --test model_cli`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** it, then `cargo test --workspace` once (the full gate, with default features, including the existing CLI tests). Expected: PASS.
- [ ] **Step 5: Commit** `Add dovetail model, and a library entry point Brightfield can hand its child process to`.

### Task 8: Document the command

**Files:**
- Modify: `CLAUDE.md` (Commands: a `dovetail model` entry), `README.md` if it lists commands, `CHANGELOG.md` `[Unreleased]`, `docs/` architecture doc that describes `relate`'s outputs (find it with `rg -l 'foreignKeys' docs`)

- [ ] **Step 1:** Add the command, its two passes, its flags and the Brightfield contract (paths explicit, proposal file only, analyst wins) in the repo's own words, without planning identifiers.
- [ ] **Step 2: Run** `cargo test -p dovetail-core --no-default-features --doc` (doc links). Expected: PASS.
- [ ] **Step 3: Commit** `Document dovetail model`.

## Dependencies and verification costs

Tasks run in order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8. Task 5 needs only Task 1 and the golden from Task 4; everything else needs its predecessor.

| Command | Covers | Cost on Hugh's Mac |
|---|---|---|
| `cargo test -p dovetail-core --no-default-features --test <file>` | one task's tests | the first run builds bundled DuckDB (several minutes); after that, under a minute |
| `cargo test -p dovetail-core --no-default-features` | every structure-layer test, run at Tasks 2 and 3 to show existing tests unchanged | a few minutes |
| `cargo test --workspace` | the full gate with `finetype-guided`, once at Task 7 | the slowest: builds the candle stack, 10+ minutes cold |
| `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` | before each commit | about a minute warm |
