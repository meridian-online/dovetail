# Changelog convention

dovetail keeps a `CHANGELOG.md` at the repo root. **Every release gets an
entry** — this is part of the `ship` stage, not optional.

## Format

[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) over
[SemVer](https://semver.org/). Group changes under `Added`, `Changed`,
`Deprecated`, `Removed`, `Fixed`, `Security` — only the headings that apply.

Entries are product-facing: describe the capability change a user would notice,
not the commit mechanics. Reference the spec or PR where it helps
(`(#3)`, `spec 2026-06-20-…`).

## Working rule

- While work lands, accrue lines under `## [Unreleased]`. Each merged spec that
  changes behaviour adds its line here — don't wait for release day.
- **On release** (`ship`): rename `[Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`,
  open a fresh empty `## [Unreleased]` above it, tag the commit `vX.Y.Z`, and
  update the compare-link footer.
- The version is the workspace `version` in the root `Cargo.toml`; bump it in
  the same release commit.

A release without a changelog entry is incomplete — treat a missing entry the
same as a missing test.
