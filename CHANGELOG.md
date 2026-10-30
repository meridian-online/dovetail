# Changelog

All notable changes to dovetail are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Every release gets an entry. On release, rename `[Unreleased]` to the version
and date, then open a fresh `[Unreleased]` above it.

## [Unreleased]

### Added

- **Table roles** — `dovetail relate` calls each table an entity, event,
  reference, bridge, snapshot or unclear, with every role's score and the
  evidence behind the call; proposes folding a lookup into the one table that
  uses it; and combines tables split across files (by year, date, region or
  shard) as a partition, snapshot or duplicate, measuring joins again against
  the combined datasets. Measurement runs in DuckDB as evidence tables, typed
  by FineType's DuckDB extension from the pinned release, with the
  model-free floor as its fallback. A role corpus (a retail fixture in the
  repo, 25 CTU databases and 2,000 GitTables tables fetched locally) and an
  eval harness record the scorer's accuracy in `tests/role-corpus/RESULTS.md`.
  See `docs/table-roles.md`.
- An exact-name join to a unique, key-shaped parent (`transactions.sku` to
  `products.sku`) earns enough key credit to be accepted.

- **Self-assembling descriptor** — one flow now emits a single Frictionless
  descriptor that carries BOTH halves of the model: every field is typed with a
  FineType semantic type (not the coarse SQL family), and the discovered
  `foreignKeys` ride inside each Table Schema with their evidence and confidence.
  `dovetail relate <db> --out` samples each column and types it with FineType's
  deterministic, model-free typer, then attaches the verified relationships — an
  email becomes `string`/`email`, an ISO timestamp `datetime`, a UUID
  `string`/`uuid`, while a plain surrogate key stays `integer`.
- A model-free semantic typing floor (`typing`) built on FineType's
  deterministic value-only classifier, shared by `survey` and `relate` — so
  descriptors carry real semantic types with no neural model or runtime
  `labels/` directory.
- **survey** — detection-first, SQL-native load: detects format and structure,
  emits a standalone `.sql` load plus a Frictionless `datapackage.json`
  descriptor, and routes low-confidence detections to suggest-and-confirm (#1).
- **transform-shim** — embedded jaq passthrough for JSON reshaping (#2).
- **relate** — discovers, verifies, and renders foreign keys across DuckDB
  tables, writing `foreignKeys` into the descriptor (#3).
- CLI `--help` documentation and a size-optimised release profile.
- Frictionless `format` on Table Schema fields, derived from FineType's
  authoritative type map (e.g. `email` for an email column, `%d/%m/%Y` for a
  `dmy_slash` date).

### Changed

- The shipped `dovetail survey` now runs the FineType-guided typer by default
  (previously library-only, behind `default-features = false`): a normal survey
  assigns semantic field types via the neural classifier when a model directory
  is configured and the deterministic typing floor otherwise, and prints the
  assembled `datapackage.json`. Build `--no-default-features` for the fast
  structural-only binary.
- **relate** now types fields from the data via FineType instead of mapping the
  coarse DuckDB SQL type family; the SQL family remains the fallback for columns
  FineType cannot conclusively type.
- Field typing now reads FineType's authoritative Frictionless map
  (`frictionless_for`) instead of a hand-rolled guess; `x-dovetailSemanticType`
  is retained as the lossless carrier.
- Frictionless conformance moved from profile 1.0 to the self-contained 2.0
  profile (vendored from FineType). `resource.path` is now emitted relative to
  the descriptor, as 2.0 requires.

[Unreleased]: https://github.com/meridian-online/dovetail/commits/main
