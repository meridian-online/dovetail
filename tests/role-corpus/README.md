# Role corpus

The tables dovetail's role calls are scored against, in three tiers. Only the answer keys and the eval record are committed here; the data of tiers 2 and 3 is fetched, because its licences vary and some sets are large.

| Tier | What | Where the data is | Scored |
|---|---|---|---|
| 1 | The retail stand-in fixture | `tests/role-fixtures/retail/` (built by SQL, committed) | Roles, folds, combinations; must be exact under FineType's profile |
| 2 | 25 business-shaped databases from the CTU Prague Relational Dataset Repository, with their declared keys | Fetched into `$DOVETAIL_CORPUS/ctu/` | Roles, folds and combinations against `ctu/<db>.json`, and join recall against the declared keys |
| 3 | A seeded sample of 2,000 single tables from GitTables, stratified by topic | Drawn into `$DOVETAIL_CORPUS/gittables/sample/` from a local GitTables copy | Each table alone: the spread of roles, the unclear rate, and how often a table is called a reference |

`RESULTS.md` is the eval record: the report of each run, which changes only when the scorer's calls change.

## Running the eval

Tier 1 is `role_eval_harness`, which CI runs; its part of `RESULTS.md` ends at the heading `# Corpus baseline: tiers 2 and 3`. Tiers 2 and 3 are `corpus_baseline` in `role_corpus_eval`, which skips without `DOVETAIL_CORPUS` and FineType's extension and model (the two variables `scripts/fetch-finetype.sh` prints), and rewrites the record from that heading on:

```bash
DOVETAIL_CORPUS=/path/to/corpus cargo test -p dovetail-core --test role_corpus_eval -- --nocapture
```

It scores every database and every sampled table floor-only and profiled. A table or database that fails to load or score is listed under the record's `Skipped` heading rather than failing the run. FineType's profile is cached beside the data, in `ctu/<db>/.profile.json` and `.<file>.profile.json` beside each GitTables file, stamped with the extension's version, the model's SHA-256 and the finetype commit dovetail builds against; a cache with another stamp is profiled again. The eval reads the Parquet files through DuckDB's parquet extension, which dovetail's build does not compile in, so DuckDB installs it from its extension repository on the first run; neither stamp records that extension's build. The same test file's `answer_keys_are_consistent` checks the answer keys against themselves and against the fetch script's list, with no data.

## Fetching

```bash
DOVETAIL_CORPUS=/path/to/corpus GITTABLES_DIR=/path/to/gittables scripts/fetch-role-corpus.sh
```

Pass `ctu` or `gittables` to fetch one tier. It needs the DuckDB 2.0 command-line tool, and the network for tier 2, which copies every table from `relational.fel.cvut.cz` with its guest account (about 20 minutes). `GITTABLES_DIR` is a local copy of GitTables with one directory of Parquet files per topic; the sample is drawn from it with a fixed seed, so the same copy always gives the same 2,000 tables, listed in `gittables/sample/manifest.csv`.

The corpus is laid out one Parquet file per table:

```
$DOVETAIL_CORPUS/
  ctu/<db>/<table>.parquet
  ctu/<db>/declared-keys.csv        primary and foreign keys as the source database declares them
  gittables/sample/<topic>/<file>.parquet
  gittables/sample/manifest.csv
```

## Answer keys

`ctu/<db>.json` holds one answer key per tier-2 database, in the format `tests/role-fixtures/README.md` describes: every table's role with a one-line why, the folds, and the combinations. Table names are the source database's own. The labels were drafted from each database's schema, values and declared keys, using these conventions:

- A table of lines of an order or invoice is an event, though its time sits on its parent.
- A table of two or more pointers and little else is a bridge, even with an audit timestamp. So is a link from a table to a code that has no table of its own (a country's languages, a film's genres).
- A table whose key is an entity's key plus a period (a player's season, a country's year) is a snapshot; a table of individual occurrences, each at its own time, is an event even when it also carries a season.
- A history of what was in effect between dates (an employee's department from one date to another, a price from a start date) is a snapshot, whether it hangs off one table or links two.
- An empty table is unclear: there is nothing in it to place.
- A reference folds only into a table with rows; a pointer from an empty table does not count.
- Audit timestamps (`last_update`, `ModifiedDate`) are ignored, and business dates on an entity (a hire date) do not make it an event.

GitTables has no answer keys: its tables have no relationships, and tier 3 measures the spread of calls rather than their accuracy.

## Licences

None of this data is redistributed here. The CTU repository's databases keep the licences of their original publishers, which vary by database; see the repository's page for each. GitTables is published under CC BY 4.0.
