# dovetail on DuckDB 2.0, with FineType pinned to v0.6.64

A short design for a small change. The plan is `docs/superpowers/plans/2026-10-26-duckdb-2.md`.

## Intent

Table roles types columns through FineType's DuckDB extension, and that extension is built only for DuckDB 2.0 from v0.6.63 on. dovetail is on the `duckdb` crate 1.10504 (DuckDB 1.5) and pins `finetype-core` and `finetype-model` to v0.6.60. The table-roles build cannot start until dovetail runs DuckDB 2.0 and links the FineType version whose extension it will load. Brightfield, which will link dovetail, already runs DuckDB 2.0 through `duckdb = "=1.20000.0"`, and its gate refuses a FineType version that disagrees with its own pin.

## Goals

- The `duckdb` crate is `=1.20000.0`, bundled, matching Brightfield, and `SELECT version()` on dovetail's connection reports `v2.0.0`.
- `finetype-core` and `finetype-model` are pinned to the v0.6.64 tag.
- The FineType v0.6.64 release's extension file loads on dovetail's connection and `ft_version()` reports `finetype 0.6.64`.
- `cargo test --workspace` and the no-default-features suite pass, with clippy clean on both.
- survey's and relate's outputs on the test fixtures are byte-identical to `main`'s, except where DuckDB 2.0 or FineType 0.6.64 changes them, and each such change is named in the pull request.

## Non-goals

- Loading FineType's extension inside dovetail's own code paths. That is table roles' first task.
- Bumping Brightfield's FineType pin or refreshing the community extension's entry. Both have their own issues; this design states the version they converge on.
- Reading the new `grade` and `note` fields that 0.6.64 adds to validation.

## Anti-goals

- A build where the bundled DuckDB names itself `v2.0.0-dev88614`. It fetches extensions from a path nothing is published under, so any later `INSTALL` silently fails.
- A FineType pin that disagrees with the extension dovetail will load, which breaks discovery-to-execution parity: survey's in-process classifier and the extension would type the same column differently.

## Constraints

- One cargo build at a time on the development Mac, which has 16 GB of memory.
- The `finetype` checkout beside this repo is not a build input. The tag in `Cargo.toml` and the commit `Cargo.lock` records are.
- libduckdb-sys 1.20000.0 compiles DuckDB 2.0.0 with its version as `v2.0.0-dev88614` (duckdb/duckdb-rs#917, fixed upstream but not yet released), and from 1.20000.0 it defines DuckDB's `DEBUG` whenever the profile has debug info. Brightfield found both, and the fixes for both carry over: a `CXXFLAGS` entry defining `DUCKDB_VERSION="v2.0.0"` in `.cargo/config.toml`, and `debug = false` for `libduckdb-sys` in the dev profile.

## Which FineType version: v0.6.64, in all three places

These were the versions on 2026-10-26:

| Where | Version | Source |
|---|---|---|
| dovetail | v0.6.60 | root `Cargo.toml` |
| Brightfield | v0.6.59 | `packaging/finetype-pin.env` on main |
| Community extension, DuckDB 2.0 | 0.6.63, ref `80a8bc1` | `extensions/finetype/description.yml` in duckdb/community-extensions; `INSTALL finetype FROM community` on DuckDB 2.0.0 reports `finetype 0.6.63` |
| Community extension, DuckDB 1.5 | 0.6.62, frozen at `a8ec866` | the same file's `variegata` key |
| Latest FineType release | v0.6.64 at `5c0dd55` | released 2026-10-26 |

The community channel has not picked up 0.6.64. No pull request for it is open in duckdb/community-extensions; FineType's own release work has drafting it as its next step.

The call (mine, made while Hugh was away): **pin v0.6.64 in dovetail now. Brightfield and the community entry move to v0.6.64 through their own issues, and dovetail's move does not wait for either.** The reasons:

1. dovetail will not load FineType from the community channel. The approved table-roles plan loads the extension file attached to the pinned FineType release, because `INSTALL … FROM community` cannot hold a pin. So what the community channel serves does not decide what dovetail links. I downloaded the v0.6.64 release's `aarch64-apple-darwin` extension, checked its SHA-256, and loaded it in the DuckDB 2.0.0 CLI: `ft_version()` reported `finetype 0.6.64`. It loads only under the file name `finetype.duckdb_extension`, because DuckDB derives the init symbol from the file name.
2. Brightfield has to move from v0.6.59 whichever version is chosen, and it already has an issue to take the release that carries the validation-rule fixes, which is v0.6.64. Settling on v0.6.63 would mean a second bump of all three, with a second re-baseline of survey's eval, within days.
3. 0.6.64 changes how some columns are typed: uppercase UUIDs now profile as `uuid`, and 32-hex columns are no longer `tsid`. Both feed relate's key evidence, so table roles should be built on these rules from the start.

The risk of this call is that the community channel lags v0.6.64 for a while. Only people running `INSTALL finetype FROM community` see that, and they get 0.6.63, which is consistent within itself. The plan's last task records whether the channel had picked up 0.6.64 when the work finished.

This replaces the original verification step (`INSTALL finetype FROM community; LOAD finetype; SELECT ft_version()`) with loading the release file. That command checks the community channel, not the pin, and the table-roles design rules it out.

## What 0.6.64 changes for dovetail

- Library API: `MultiBranchClassifier::from_bytes` takes a trailing snapshot argument and `TableValidationResult` gains `column_notes`. dovetail calls neither. It calls `MultiBranchClassifier::load(dir)`, whose signature is unchanged, so no compile change is expected.
- `load(dir)` now reads `validation_snapshot.json` from the model directory when present. A model directory without one reads the current published rules, and 0.6.64 changes those, so survey's model-backed results can move. The model-backed eval must therefore run against a model directory fetched for v0.6.64 that contains its snapshot.
- 13 types' validation rules change (see FineType's CHANGELOG for 0.6.64). dovetail's typing floor calls `finetype_core::validate_value_for_label` and `deterministic_fast_path`, so floor-typed fields can change too, uppercase UUIDs among them.

## Approaches considered

1. Pin v0.6.64 now and let the other two converge through their own issues. Chosen, for the reasons above.
2. Pin v0.6.63, the version the community channel serves today. This matches the issue's original wording, but it costs a second bump everywhere within days, and Brightfield does not want 0.6.63.
3. Pin v0.6.64 and hold the merge until the community channel serves it. The pin would mean the same in all three places on the day of the merge, but it waits on a third party's merge and build for a channel dovetail does not use.

## Left to the builder

- Whether a DuckDB 2.0 behaviour change shows up in dovetail's own SQL. Brightfield's list was `~` and `!~`, `//` and `%` by zero, `x -> …` lambdas, `ORDER BY true` and the PEG parser default. The plan audits for these.
- Re-recording any expected value that moves, naming each one and saying whether DuckDB 2.0 or FineType 0.6.64 moved it.
