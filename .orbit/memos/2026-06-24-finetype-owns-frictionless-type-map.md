# FineType now owns the Frictionless type map — retire dovetail's heuristic

**From the FineType session, 2026-06-24.** FineType shipped the authoritative
label → Frictionless `{type, format}` fold (finetype choice 0105, spec
`2026-06-24-frictionless-datapackage-profile-output` ac-01). This is the map
dovetail currently *guesses* at. Dovetail can now consume it instead.

## What changed on the FineType side (shipped)

- Every one of FineType's 244 taxonomy leaves carries a hand-verified
  `frictionless: {type, format}` block, surfaced on `finetype_core::Definition`
  as `pub frictionless: Option<Frictionless>` (`Frictionless { ftype, format }`).
- It is CI-guarded: `finetype check` fails if any leaf lacks a valid block, so for
  a real finetype label the lookup is always populated.
- FineType is the **upstream owner** of this fold — dovetail, arcform and FineType
  should all read one map, not three heuristics.

## The change dovetail should make

**Retire `crates/dovetail-core/src/datapackage.rs::frictionless_type()`** (the
`fn frictionless_type(semantic: Option<&str>) -> String` at ~line 121). Replace it
with a lookup against the FineType map, keyed on the semantic label dovetail
already stores in `x-dovetailSemanticType`:

```rust
// pseudocode
let fx = taxonomy.get(strip_locale(label)).and_then(|d| d.frictionless.as_ref());
let (ty, format) = match fx {
    Some(f) => (f.ftype.clone(), f.format.clone()),
    None    => ("string".to_string(), None), // no label (shape-heuristic path) / unknown
};
```

Two struct/behaviour follow-ons:

1. **Add `format` to dovetail's `Field` struct.** Today `Field { name, ty,
   semantic_type }` has *no* `format` field, so it drops the Frictionless format
   entirely. Add `#[serde(rename = "format", skip_serializing_if = "Option::is_none")]
   pub format: Option<String>` and populate it from the map. This is where the
   precision lives (see below).
2. **Strip the locale suffix before lookup.** Taxonomy keys are 3-level
   (`domain.category.type`); FineType's wire label is already 3-level, but guard
   against a 4th `.LOCALE` segment defensively.

## Why it matters — what the heuristic was throwing away

dovetail's current `frictionless_type()` only distinguishes date/datetime/
integer/number/boolean and defaults **everything else to `string` with no
format**. The authoritative map adds:

- `string` **formats**: `email`, `uri`, `uuid` (e.g. `identity.person.email` →
  `string`/`email`) — dovetail currently emits bare `string`.
- The full temporal split: `date` / `time` / `datetime` / `year` / `yearmonth` /
  `duration` — dovetail only did date-vs-datetime.
- **Exact strptime patterns** in `format` (e.g. `datetime.date.dmy_slash` →
  `date`/`"%d/%m/%Y"`) — dovetail had nowhere to put these.
- `geopoint`, `list`, `object`, `array` — all collapsed to `string` today.

## The accessor — SHIPPED (finetype-core, 2026-06-24)

The seam is built. `finetype-core` now exposes the map behind a non-default
feature `embedded-taxonomy` (it `include_str!`s the sibling labels at compile
time — workspace-only, off for the crates.io light core). Two entry points:

- `finetype_core::frictionless_for(label: &str) -> Option<Frictionless>` — the
  one you want. Cached embedded taxonomy, strips a `.LOCALE` suffix, returns
  `None` for unknown labels.
- `finetype_core::Taxonomy::embedded()` — full taxonomy handle if you need more.

`Frictionless { pub ftype: String, pub format: Option<String> }`.

**Enable the feature** in `dovetail-core/Cargo.toml`:

```toml
finetype-core = { workspace = true, features = ["embedded-taxonomy"] }
```

Then the replacement for `frictionless_type()` is just:

```rust
let (ty, format) = match finetype_core::frictionless_for(label) {
    Some(f) => (f.ftype, f.format),
    None    => ("string".to_string(), None), // no/unknown label
};
```

No need to load a `Taxonomy` yourself, and **do not** re-embed the finetype
labels inside dovetail — `frictionless_for` already owns that. (This is the
"standalone registry publish" follow-up choice 0105 deferred, now delivered.)

## Version pin — align the family on 2.0

dovetail pins `datapackage.org/profiles/**1.0**/` in two places
(`datapackage.rs:94` `DATAPACKAGE_PROFILE`, `relate.rs:204`). FineType (0105) and
arcform (decision 0017) both chose **2.0**. Recommend moving dovetail to 2.0 so the
three projects emit the same profile.

## Out of scope / unchanged

- `relate.rs::frictionless_type(ty: &str)` maps **DuckDB** type families (used when
  relate works on a DuckDB table that has no finetype label). Keep it as the
  no-label fallback — it's a different input than the semantic-label map.
- The `foreignKeys` relationship model (choice 0003) is untouched.

**One line for the author:** FineType now publishes the canonical type fold and
the accessor to read it (`finetype_core::frictionless_for`, feature
`embedded-taxonomy`); dovetail should delete its `frictionless_type()` guess, call
that, and start carrying the `format` field it currently drops.
