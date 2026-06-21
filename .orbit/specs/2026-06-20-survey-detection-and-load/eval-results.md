# Detection eval — 2026-06-21

Corpus: tests/fixtures (7 SQL-native fixtures)

| detector | mode | hit-rate | hits |
|---|---|---|---|
| shape-heuristic | structural | 100.0% | 7/7 |
| finetype-guided | degraded (no model dir) | 100.0% | 7/7 |

## Per-fixture

| fixture | shape-heuristic | finetype-guided |
|---|---|---|
| csv-dup-cols | ✓ | ✓ |
| csv-simple | ✓ | ✓ |
| json-array | ✓ | ✓ |
| json-object | ✓ | ✓ |
| ndjson-simple | ✓ | ✓ |
| parquet-simple | ✓ | ✓ |
| tsv-simple | ✓ | ✓ |
