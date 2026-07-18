# Vendored Frictionless profiles

`datapackage-profile.json` is the self-contained **Frictionless Data Package
2.0** profile — a ref-free JSON Schema (draft-07) covering the Data Package,
Data Resource, and Table Schema specs in one file.

Source: vendored verbatim from FineType (`finetype/vendor/frictionless/
datapackage-profile.json`). dovetail, FineType, and arcform share this one
profile so conformance is checked against an identical schema across the
meridian crates — re-vendor from the same upstream file, never hand-roll.

Why vendored rather than `$ref`'d: upstream ships the 2.0 profiles as `$ref`
pointers into a generated `dictionary.json`, which is not resolvable standalone.
FineType inlines those definitions into one ref-free schema the `jsonschema`
crate validates against directly.

Conformance scope (ac-08 — every emitted `datapackage.json` must satisfy it):

- **Data Package** — `resources` required; an array of ≥1 resource.
- **Data Resource** — `name` required, plus one of `path` / `data`.
- **Table Schema** — `fields` required; each field carries a `name`, a
  Frictionless `type`, and an optional `format`.
- Custom properties (`x-dovetailLoadRecipe`, `x-dovetailSemanticType`, …) are
  permitted — the profile does not set `additionalProperties: false`.

To update: copy FineType's vendored file over this one, bump the
`DATAPACKAGE_PROFILE` pins (`datapackage.rs`, `relate.rs`), and re-run
`cargo test -p dovetail-core`.
