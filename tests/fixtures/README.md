# Survey detection fixture corpus

Ground-truth inputs for the survey detection eval (spec
`2026-06-20-survey-detection-and-load`, ac-01). Each fixture is a directory
holding one data file plus a `manifest.json` describing the expected detection
result. The eval harness (ac-03) scores a detector by comparing its output
against these manifests; the round-trip test (ac-07) checks that the emitted
`.sql` reproduces the manifest's row count and column set.

## Manifest schema

```jsonc
{
  "name": "csv-simple",            // fixture id (matches directory name)
  "file": "data.csv",              // data file within the directory
  "format": "csv",                 // csv | tsv | parquet | ndjson | json
  "structure": "flat-table",       // flat-table | records-array | single-object
  "row_count": 3,                  // expected loaded row count
  "columns": ["id", "name", ...],  // expected column set, in order
  "notes": "...",                  // optional: what makes this fixture interesting
  "duplicate_columns": ["id"]      // optional: columns appearing more than once
}
```

### `structure` values

- **flat-table** — already row-shaped (CSV, TSV, Parquet, NDJSON, or a JSON
  array of flat records). Loads natively.
- **records-array** — a JSON array of objects at the top level; rows are the
  array elements.
- **single-object** — a JSON file that is one object; one row.

The eval scores an exact full-structure match per fixture (recursive levels
all-or-nothing — per review-spec finding 2).
