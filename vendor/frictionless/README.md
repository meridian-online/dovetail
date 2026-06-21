# Vendored Frictionless profiles

`datapackage-profile.json` is a self-contained JSON Schema (draft-07) encoding
the load-bearing conformance rules of the Frictionless **Data Package**, **Data
Resource**, and **Table Schema** specs, derived directly from the upstream
dictionary at `frictionlessdata/datapackage` (`profiles/dictionary/*.yaml`).

Why derived rather than copied: upstream ships the profiles as `$ref` pointers
into a `dictionary.json` that is *generated* from the YAML by their
`scripts/generate.ts` (a node/astro build). Rather than vendor that toolchain,
this file inlines the relevant definitions into one ref-free schema the
`jsonschema` crate can validate against directly.

Scope captured (the rules dovetail's emitted descriptors must satisfy):

- **Data Package** — `resources` required; an array of ≥1 resource.
- **Data Resource** — `name` required, plus one of `path` / `data`.
- **Table Schema** — `fields` required; each field is an object with a `name`
  and a Frictionless `type`.
- Custom properties (e.g. `x-dovetailLoadRecipe`) are permitted — the upstream
  profiles do not set `additionalProperties: false`.

A full-profile validation against the upstream-generated `dictionary.json` is a
follow-up hardening (see the eval/conformance memo). For the MVP this focused
schema is what every emitted `datapackage.json` is checked against (ac-08).
