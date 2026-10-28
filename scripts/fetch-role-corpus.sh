#!/usr/bin/env bash
# Fetch the role corpus's tiers 2 and 3 into DOVETAIL_CORPUS, one Parquet file
# per table. The data is not committed; its answer keys are, under
# tests/role-corpus/.
#
#   DOVETAIL_CORPUS=DIR [GITTABLES_DIR=DIR] scripts/fetch-role-corpus.sh [ctu] [gittables]
#
# With no arguments it fetches both tiers. Writes, under DOVETAIL_CORPUS:
#
#   ctu/<db>/<table>.parquet     every base table of each database in CTU_DBS,
#                                copied from the CTU Prague Relational Dataset
#                                Repository (guest access) with no constraints
#   ctu/<db>/declared-keys.csv   that database's declared primary and foreign
#                                keys, which score join discovery
#   gittables/sample/<topic>/<file>.parquet
#                                SAMPLE_SIZE tables drawn from a local GitTables
#                                copy at GITTABLES_DIR (one directory per topic)
#   gittables/sample/manifest.csv
#                                the drawn files, as topic,file
#
# A database already fetched is skipped; delete its directory to fetch it
# again. The GitTables sample is drawn again on every run, and replaces the
# last one.
#
# Tables are copied with mysql_query rather than through the attached
# catalogue, which confuses same-named tables across the server's databases.
#
# The sample is stratified by topic: files are ranked within their topic by the
# md5 of `<topic>/<file>` and the seed, then taken rank by rank across all
# topics, so each topic gives an equal share until it runs out. md5 of the
# relative path keeps the draw the same wherever the copy lives and under any
# DuckDB version.
#
# Needs the DuckDB 2.0 command-line tool on PATH (or named by DUCKDB), and the
# network for the CTU tier.
set -euo pipefail

CTU_DBS=(Chinook northwind sakila classicmodels pubs AdventureWorks2014 ErgastF1 lahman_2014
  Hockey Basketball_men financial imdb_small imdb_MovieLens Mondial GOSales Walmart_2014
  world university stats_CEB employee TubePricing CraftBeer Countries PremierLeague ccs)
CTU_CONN='host=relational.fel.cvut.cz port=3306 user=guest password=ctu-relational'
SAMPLE_SIZE=2000
SEED=20261026

: "${DOVETAIL_CORPUS:?set DOVETAIL_CORPUS to the directory the corpus is written to}"
DUCKDB=${DUCKDB:-duckdb}

duck() { "$DUCKDB" -no-agent "$@"; }

# A single-quoted SQL string literal.
sql_str() { printf "'%s'" "${1//\'/\'\'}"; }

fetch_ctu() {
  local out=$DOVETAIL_CORPUS/ctu db part tables t sql attach inner
  mkdir -p "$out"
  for db in "${CTU_DBS[@]}"; do
    if [[ -f $out/$db/declared-keys.csv ]]; then echo "skip ctu/$db"; continue; fi
    echo "== ctu/$db"
    part=$out/.$db.part
    rm -rf "$part" && mkdir -p "$part"
    attach="INSTALL mysql; LOAD mysql; ATTACH $(sql_str "$CTU_CONN database=$db") AS src (TYPE mysql, READ_ONLY);"
    tables=$(duck -noheader -list -c "$attach
      SELECT table_name FROM mysql_query('src', 'SELECT table_name FROM information_schema.tables WHERE table_schema = ''$db'' AND table_type = ''BASE TABLE''') ORDER BY 1;")
    if [[ -z $tables ]]; then
      echo "ctu/$db: the server lists no base tables; check the name against CTU_DBS" >&2
      exit 1
    fi
    sql=$attach
    while IFS= read -r t; do
      [[ -n $t ]] || continue
      # The MySQL query is itself a string inside mysql_query's string argument.
      inner="SELECT * FROM \`$db\`.\`$t\`"
      sql+=" COPY (SELECT * FROM mysql_query('src', $(sql_str "$inner"))) TO $(sql_str "$part/$t.parquet") (FORMAT parquet);"
    done <<<"$tables"
    sql+=" COPY (SELECT k.table_name, k.column_name, k.ordinal_position, k.referenced_table_name, k.referenced_column_name,
        CASE WHEN k.constraint_name = 'PRIMARY' THEN 'primary_key' ELSE 'foreign_key' END AS kind, k.constraint_name
      FROM mysql_query('src', 'SELECT table_name, column_name, ordinal_position, referenced_table_name, referenced_column_name, constraint_name FROM information_schema.key_column_usage WHERE table_schema = ''$db''') k
      WHERE k.constraint_name = 'PRIMARY' OR k.referenced_table_name IS NOT NULL
      ORDER BY kind, table_name, constraint_name, ordinal_position)
      TO $(sql_str "$part/declared-keys.csv") (HEADER);"
    duck -c "$sql" >/dev/null
    rm -rf "${out:?}/$db"
    mv "$part" "$out/$db"
  done
}

fetch_gittables() {
  : "${GITTABLES_DIR:?set GITTABLES_DIR to a local GitTables copy, one directory per topic}"
  local out=$DOVETAIL_CORPUS/gittables/sample topic file
  rm -rf "$out" && mkdir -p "$out"
  duck -c "
    COPY (
      WITH files AS (
        SELECT regexp_extract(file, '([^/]+)/[^/]+$', 1) AS topic, parse_filename(file) AS file
        FROM glob($(sql_str "$GITTABLES_DIR/*/*.parquet"))),
      ranked AS (
        SELECT topic, file, md5(topic || '/' || file || '$SEED') AS h,
          row_number() OVER (PARTITION BY topic ORDER BY md5(topic || '/' || file || '$SEED')) AS r
        FROM files),
      drawn AS (SELECT topic, file FROM ranked ORDER BY r, h LIMIT $SAMPLE_SIZE)
      SELECT * FROM drawn ORDER BY topic, file)
    TO $(sql_str "$out/manifest.csv") (HEADER);" >/dev/null
  # Each row is topic,file, with DuckDB's CSV quoting where a name needs it.
  duck -noheader -list -separator $'\t' -c "SELECT topic, file FROM read_csv($(sql_str "$out/manifest.csv"), all_varchar = true)" |
    while IFS=$'\t' read -r topic file; do
      mkdir -p "$out/$topic"
      cp "$GITTABLES_DIR/$topic/$file" "$out/$topic/$file"
    done
  echo "gittables/sample: $(($(wc -l <"$out/manifest.csv") - 1)) tables"
}

tiers=("$@")
[[ ${#tiers[@]} -gt 0 ]] || tiers=(ctu gittables)
for tier in "${tiers[@]}"; do
  case $tier in
    ctu) fetch_ctu ;;
    gittables) fetch_gittables ;;
    *) echo "unknown tier: $tier (expected ctu or gittables)" >&2; exit 2 ;;
  esac
done
