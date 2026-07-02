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
`calcard` (.ics/.vcf). DuckDB via the `duckdb` crate, bundled and pinned by
`Cargo.lock`.

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

## finetype path dependency

dovetail consumes finetype as an **in-process library dependency** (choice
0012): `finetype-core` and `finetype-model` are path deps pointing at the
sibling checkout `../finetype/crates/*`. `finetype-core` carries the light
structure/schema primitives and the authoritative Frictionless type map
(`frictionless_for`); `finetype-model` carries the candle column classifier used
by the finetype-guided detector.

Because they are path deps, a build resolves whatever is checked out in
`../finetype` and pins it in `Cargo.lock`. When the sibling version moves,
`cargo build`/`test` re-resolves and updates the lockfile — commit that change
alongside the work that triggered it. For a reproducible build, keep
`../finetype` on a committed (clean) state.

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
