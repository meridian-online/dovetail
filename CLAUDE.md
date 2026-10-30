# dovetail

The modelling layer of meridian. dovetail discovers *how to load* unfamiliar
data and *how it relates*, then compiles those decisions into runnable,
auditable artifacts. It never executes a pipeline itself — it is a planner, or
more precisely a compiler: it builds a *load model* and a *relationship model*
from a pile of unfamiliar inputs and emits them to runnable or viewable targets.

meridian has four packages, each owning one job from raw data to insight:
finetype (understanding — profile data, infer column types), **dovetail**
(modelling — this repo), arcform (execution — run transformation pipelines),
brightfield (insight — interactive visualisation).

## Commands (the CLI surface)

- `dovetail survey <paths...>` — discover how to land files into DuckDB. Detects
  format and row-level structure, flags problems (oversized objects, hidden
  arrays, duplicate keys, non-loadable formats), and emits a Frictionless
  `datapackage.json` descriptor plus the executable load: a standalone `.sql`
  where DuckDB can do it natively, or an arcform `.yaml` where a non-SQL step
  (jaq conversion, .ics parse, dedupe) is unavoidable. SQL is preferred; the
  fallback ladder escalates only when needed.
- `dovetail relate` — run relationship discovery across DuckDB tables to find
  candidate joins. Each edge carries evidence (value overlap, cardinality,
  finetype semantic-type agreement, name similarity), a confidence score, and a
  status (`suggested` / `accepted` / `rejected`). The canonical output is
  `foreignKeys` in the descriptor; DDL, Mermaid `erDiagram`, and join SQL are
  projections of that. Only `accepted` edges compile to constraints or SQL.
  It also says what each table is (entity, event, reference, bridge, snapshot
  or unclear, every role scored, with its evidence), proposes folding a lookup
  into the one table that uses it, and combines tables that are one kind of
  thing split across files. Columns are typed first by FineType's DuckDB
  extension from the pinned release, or by the model-free floor when it is
  unavailable. The design, its rulings and the role corpus are in
  `docs/table-roles.md`.
- Transform shim — pinned access to bundled tools so emitted scripts call
  dovetail's own vendored binaries: `dovetail jaq <program> <file>` (true
  passthrough to embedded jaq, NDJSON out), `dovetail calamine` (Excel),
  `dovetail calcard` (.ics/.vcf).

## Design commitments

1. **Discovery-to-execution parity.** The transform tools used during discovery
   are byte-identical to the ones the emitted scripts call later.
2. **The plan is the artifact.** A conversion or relationship is a recorded,
   pinned, hand-runnable program in version control — not a one-off REPL action.
3. **dovetail decides; arcform and DuckDB execute.** The moment work becomes
   analytical transformation rather than ingestion, it belongs to arcform.
4. **Trust lives in the transform spec, not the executor** — a reviewer audits
   the jq program, not which binary ran it.
5. **Provenance is stamped, not implied** — every emitted artifact records what
   produced it and under which tool versions.

## Repo layout

Pure Rust, mirroring finetype: a reusable core library under a thin CLI.

- `crates/dovetail-core` (lib) — `detect`, `convert`/`transform`, `relate`, the
  plan model, `emit`, and `eval`. Unit-testable; leaves room for a DuckDB
  extension or MCP server later.
- `crates/dovetail` (bin) — thin CLI over the core.

Bundled transform crates: `jaq` (JSON/YAML/TOML/XML), `calamine` (Excel),
`calcard` (.ics/.vcf).

DuckDB is 2.0.0, through the `duckdb` crate at `=1.20000.0` with `bundled`, the same spec as Brightfield's. libduckdb-sys 1.20000.0 compiles DuckDB 2.0.0 as `v2.0.0-dev88614` (duckdb/duckdb-rs#917), and that build fetches extensions from a path nothing is published under, so `.cargo/config.toml` sets `CXXFLAGS` to name the build `v2.0.0`. Cargo applies it only to builds run inside this workspace; a `CXXFLAGS` set in the shell replaces it, and `the_bundled_duckdb_is_2_0_0` in `tests/duckdb_engine.rs` fails when that happens. Remove the entry once a libduckdb-sys release carries the fix. The dev profile turns debug info off for `libduckdb-sys`, because from 1.20000.0 it switches on DuckDB's `DEBUG` verification paths.

## Build and test

```bash
cargo build --workspace           # full build
cargo test  --workspace           # full test suite (bundles DuckDB — first build is slow)
cargo test -p dovetail-core --no-default-features   # structure layer only, skips the finetype-model ML stack
cargo build --release             # size-optimised release profile (strip + thin LTO)
```

The `finetype-guided` feature (on by default) pulls the candle-backed column
classifier via `finetype-model`; `--no-default-features` drops it for fast
iteration on the structure layer.

## finetype dependency

dovetail consumes finetype as an **in-process library dependency**:
`finetype-core` and `finetype-model` are git dependencies on the public
finetype repo, pinned to a tag under `[workspace.dependencies]` in the root
`Cargo.toml`. `finetype-core` carries the light structure/schema primitives and
the authoritative Frictionless type map (`frictionless_for`); `finetype-model`
carries the candle column classifier used by the finetype-guided detector.

Cargo fetches that tag from GitHub, and `Cargo.lock` records the commit it
resolved to. **That recorded commit is what dovetail builds against** — it is
what makes the build reproducible, and it is the version to quote when you need
to say which finetype a build used:

```bash
awk '/^name = "finetype-core"/{f=1} f && /^source = /{print; exit}' Cargo.lock
```

A checkout at `../finetype` is not an input to the build, so editing it changes
nothing here. Moving to a new finetype version is a source edit: change the
`tag` on both crates in the root `Cargo.toml`, re-resolve without building
(`cargo metadata --format-version 1 >/dev/null`), and commit `Cargo.toml` and
`Cargo.lock` together with the work that triggered it.

The tag is the FineType release whose attached DuckDB extension dovetail loads, so survey's in-process classifier and the extension type a column the same way. It moves together with Brightfield's `FINETYPE_TAG` (`packaging/finetype-pin.env`), whose gate refuses a mismatch once it links dovetail. The release's extension loads only under the file name `finetype.duckdb_extension`, since DuckDB derives its entry point from the file name. To check a release file against the pin, run `DOVETAIL_FINETYPE_EXTENSION=<file> cargo test -p dovetail-core --no-default-features --test duckdb_engine`. A model-backed eval (`DOVETAIL_FINETYPE_MODEL_DIR`) needs a model directory holding `validation_snapshot.json`; without it the model reads the current published validation rules rather than the frozen ones, and its results move for a reason that is not the pin. FineType's `download-model.sh` may not fetch it, so copy it from the tag's `models/<model>/` when it is missing.

## Style — how to write for the author

The author reads fast and decides faster; write so they can act, not parse.

- **Lead with the answer** or the recommendation. Don't restate the question,
  don't apologise, don't park the conclusion behind context.
- **One action, imperative voice.** *"Run X on Y"*, not *"it might be worth
  considering perhaps X."* If a call rests on an assumption, name it inline
  rather than sanding the recommendation into mush. One action per response, not
  a menu — if two paths genuinely matter, lay them out *and* pick one.
- **Keep it short.** Three reasons is a ceiling, not a target. Detail belongs
  available on request, not dumped up front.
- **Plain words.** Words a peer outside the project would understand; define a
  term of art the first time you use it. No corporate hedging, no jargon.
- **Tone.** British English. Direct, warm, never chatty — no performative
  formality, no peppy enthusiasm, no clinical cold.
