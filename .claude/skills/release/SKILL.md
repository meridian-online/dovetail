---
description: >-
  Cut a dovetail release — bump the workspace version, roll CHANGELOG.md's
  [Unreleased] section into a dated version heading, tag `vX.Y.Z`, push, and
  publish a GitHub release whose notes ARE that changelog section. The changelog
  is the spine of the release: a release without an entry is incomplete. Git
  push + GitHub publish, so run the pre-flight gates first and stop on any
  failure.
when_to_use: User says "release", "ship", "cut a release", "tag a version", or "publish vX.Y.Z". Treat as a deliberate, reviewed action — never auto-fire mid-task.
argument-hint: "[patch | minor | major]"
arguments: bump
allowed-tools: Bash, Read, Edit
---

# /release

Cut a versioned release of dovetail. One release type: a tagged GitHub release
of the workspace. The **changelog is the centrepiece** — every release rolls
`CHANGELOG.md`'s `[Unreleased]` section into a dated version heading, and that
exact section becomes the GitHub release notes.

## Changelog convention (enforced here)

dovetail keeps a `CHANGELOG.md` at the repo root, and **every release gets an
entry** — treat a missing entry the same as a missing test.

- **Format.** [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) over
  [SemVer](https://semver.org/). Group changes under `Added`, `Changed`,
  `Deprecated`, `Removed`, `Fixed`, `Security` — only the headings that apply.
- **Product-facing entries.** Describe the capability change a user would
  notice, not the commit mechanics. Reference the spec or PR where it helps
  (`(#3)`, `spec 2026-06-20-…`).
- **Accrue as work lands.** Each merged change that alters behaviour adds its
  line under `## [Unreleased]` — don't wait for release day.
- **On release.** Rename `[Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`, open a
  fresh empty `## [Unreleased]` above it, tag the commit `vX.Y.Z`, and update the
  compare-link footer. The version is the workspace `version` in the root
  `Cargo.toml`; bump it in the same release commit.

This skill operationalises the convention: it gates on a non-empty
`[Unreleased]`, rolls it into a dated version heading, and publishes that section
verbatim as the GitHub release notes — so the changelog and the release can't
drift.

## Versioning policy

dovetail is pre-1.0. **Prefer patch** (`0.1.x`) — the default. Use **minor**
(`0.x.0`) for a notable new capability (a new command, a new output surface).
Reserve **major** (`x.0.0`) for the eventual 1.0 stabilisation. All crates use
`version.workspace = true`, so the root `Cargo.toml` is the only version to bump.

## Usage

```
/release          # patch bump (default)
/release minor    # minor bump
/release major    # major bump
```

## Instructions

### 1. Pre-flight gates — stop on any failure

Run all of these; **do not proceed past a failure**. Present the summary and
wait for confirmation before tagging.

1. **On `main`, clean tree** — `git rev-parse --abbrev-ref HEAD` is `main` and
   `git status --porcelain` is empty. Release from `main` only; a release
   commit on a feature branch tags the wrong history.
2. **Up to date with origin** — `git fetch` then confirm `main` is not behind
   `origin/main`.
3. **Tests pass** — `cargo test --workspace` with zero failures. This includes
   the ac-08 Frictionless conformance test (`crates/dovetail-core/tests/conformance.rs`)
   — the emitted descriptors must still validate against the vendored profile.
4. **Zero warnings** — `cargo build --workspace 2>&1 | grep "^warning" | head -1`
   is empty.
5. **finetype path-dep is committed** — dovetail consumes `finetype-core` /
   `finetype-model` as path dependencies (`../finetype`). Confirm the sibling
   checkout is on a committed state (no dirty working tree there) so the build
   you tag is reproducible: `git -C ../finetype status --porcelain` is empty.
   Note in the summary which finetype commit dovetail builds against
   (`git -C ../finetype rev-parse --short HEAD`).
6. **CI is green** *(if configured)* — if `.github/workflows/` has a CI
   workflow, the latest run on `main` must pass (`gh run list --branch main
   --limit 1`). Skip if no workflow exists yet.
7. **The changelog gate** — `CHANGELOG.md`'s `[Unreleased]` section must have at
   least one entry. An empty `[Unreleased]` means the work since the last
   release was never written up — **treat a missing entry as a missing test**
   and stop. Reconcile it before continuing (next step).

### 2. Reconcile the changelog

Before stamping, make sure `[Unreleased]` actually covers what shipped. Compile
candidate lines from:

```bash
PREV=$(git describe --tags --abbrev=0 2>/dev/null || echo "")   # "" if no tags yet
git log ${PREV:+$PREV..}HEAD --oneline --no-merges
```

- Entries are **product-facing**: the capability a user would notice, not commit
  mechanics. Reference the spec or PR where it helps (`(#3)`, `spec 2026-06-20-…`).
- Group under `Added` / `Changed` / `Deprecated` / `Removed` / `Fixed` /
  `Security` — only the headings that apply.

Add any missing lines to `[Unreleased]` now.

### 3. Determine the version

```bash
CUR=$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')
```

Compute the next version from `CUR` and the `bump` argument (default `patch`).
Print `CUR → NEXT` and the date (`date +%Y-%m-%d`).

### 4. Roll the changelog (the centrepiece)

Edit `CHANGELOG.md`:

1. Rename `## [Unreleased]` to `## [NEXT] - YYYY-MM-DD`.
2. Open a fresh empty `## [Unreleased]` immediately above it.
3. Update the compare-link footer:
   - `[Unreleased]: https://github.com/meridian-online/dovetail/compare/vNEXT...HEAD`
   - `[NEXT]: https://github.com/meridian-online/dovetail/compare/vPREV...vNEXT`
     (for the **first** release, use
     `https://github.com/meridian-online/dovetail/releases/tag/vNEXT` instead).

Keep the format [Keep a Changelog](https://keepachangelog.com/) — do not
reorder or reword prior released sections.

### 5. Bump the version

Edit the root `Cargo.toml` `version` to `NEXT`, then refresh the lockfile and
confirm it still builds:

```bash
cargo check --workspace      # updates Cargo.lock with the new version
```

### 6. Commit, tag, push

```bash
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "Release vNEXT"
git tag -a vNEXT -m "dovetail vNEXT"
git push && git push --tags
```

End the commit message with the project's `Co-Authored-By` trailer.

### 7. Publish the GitHub release — notes ARE the changelog

Extract the new version's changelog section and publish it verbatim as the
release notes:

```bash
# Slice CHANGELOG.md from the NEXT heading to the next '## [' heading.
awk '/^## \[NEXT\]/{f=1;next} /^## \[/{f=0} f' CHANGELOG.md > /tmp/notes.md
gh release create vNEXT --title "vNEXT" --notes-file /tmp/notes.md
```

(Substitute the literal version for `NEXT`.) The GitHub release and the
changelog never drift, because one is generated from the other.

### 8. Summary

Report:

```
Released dovetail vNEXT.

  Tag:        vNEXT
  Date:       YYYY-MM-DD
  finetype:   <short-sha> (path dep dovetail built against)
  Release:    https://github.com/meridian-online/dovetail/releases/tag/vNEXT

  Changelog:
  <the section, as published>
```

## Rollback

A release is just a tag plus a GitHub release — both reversible before anyone
depends on them:

1. **Before anyone pulls** — `gh release delete vNEXT`, `git push --delete
   origin vNEXT`, `git tag -d vNEXT`, then revert the release commit. Move the
   stamped entry back under `[Unreleased]`.
2. **After it's out** — don't rewrite history. Cut the next patch (`/release`)
   with the fix, and note the regression under `Fixed`.
