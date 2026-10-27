//! evidence — what dovetail measures about the analyst's database, measured
//! inside DuckDB.
//!
//! A [`Session`] is an in-memory DuckDB that attaches the analyst's database
//! as `src`, read-only, and holds dovetail's own evidence tables in its
//! `memory` catalog. Every measurement is a query that returns rows and names
//! the tables it reads fully qualified, so dovetail's evidence tables are
//! never measured themselves. Writing an evidence table is a separate step
//! that only a session runs: the SQL builders here take a catalog prefix
//! ([`SESSION_PREFIX`] for a session, empty for a bare connection), so the
//! same measurements run on a connection dovetail must never write to.

use std::path::Path;

use duckdb::Connection;

use crate::finetype_ext::{self, TypingRequest, TypingSource};
use crate::relate::{quote, FIELD_SAMPLE_N};

/// How a session's queries name the analyst's tables.
pub(crate) const SESSION_PREFIX: &str = "src.main.";

/// Column names that record when a row was written rather than when anything
/// happened. A name also counts when it is one of these with a trailing `_at`
/// dropped and `_ts` or `_at` added, such as `created_ts` or `last_update_at`.
pub const AUDIT_TIME_NAMES: &[&str] = &[
    "created_at",
    "updated_at",
    "modified",
    "modified_at",
    "inserted_at",
    "loaded_at",
    "last_update",
];

#[derive(Debug, thiserror::Error)]
pub enum EvidenceError {
    #[error("DuckDB: {0}")]
    Duckdb(#[from] duckdb::Error),
    #[error("{0}")]
    Session(String),
}

/// An in-memory DuckDB with the analyst's database attached as `src`.
pub struct Session {
    conn: Connection,
    /// Which typing labels this session's columns.
    pub typing: TypingSource,
}

impl Session {
    /// Attach the DuckDB file at `db_path` read-only.
    pub fn attach(db_path: &Path, typing: TypingRequest) -> Result<Session, EvidenceError> {
        let path = db_path.to_str().ok_or_else(|| {
            EvidenceError::Session(format!(
                "the database path {} is not valid UTF-8",
                db_path.display()
            ))
        })?;
        let conn = open()?;
        conn.execute_batch(&format!(
            "ATTACH '{}' AS src (READ_ONLY)",
            path.replace('\'', "''")
        ))?;
        Ok(Self::with_typing(conn, typing))
    }

    /// Build `src` in memory by running `build_sql` inside it.
    pub fn from_sql(build_sql: &str, typing: TypingRequest) -> Result<Session, EvidenceError> {
        let conn = open()?;
        conn.execute_batch("ATTACH ':memory:' AS src; USE src;")?;
        conn.execute_batch(build_sql)?;
        conn.execute_batch("USE memory;")?;
        Ok(Self::with_typing(conn, typing))
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    fn with_typing(conn: Connection, typing: TypingRequest) -> Session {
        let typing = finetype_ext::load(&conn, typing);
        Session { conn, typing }
    }
}

/// FineType's release extension is unsigned, and DuckDB fixes that setting
/// when the database opens.
fn open() -> Result<Connection, EvidenceError> {
    let config = duckdb::Config::default().allow_unsigned_extensions()?;
    Ok(Connection::open_in_memory_with_flags(config)?)
}

/// One column of the analyst's database, measured and typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnEvidence {
    pub table: String,
    pub column: String,
    pub duckdb_type: String,
    pub rows: i64,
    pub non_null: i64,
    pub distinct: i64,
    /// The FineType label, such as `identity.person.email`, or `None` when the
    /// typing declined.
    pub label: Option<String>,
    pub audit_name: bool,
}

/// Measure and type every column of the session's `src`, in table and column
/// order, and write the rows to `memory.column_evidence`.
pub fn column_evidence(s: &Session) -> Result<Vec<ColumnEvidence>, EvidenceError> {
    let columns = list_columns(s.conn(), "src")?;
    let profiled = matches!(s.typing, TypingSource::Profiled { .. });
    let labels = if profiled {
        with_one_thread(s.conn(), |conn| {
            label_tables(conn, SESSION_PREFIX, &columns, profile_labels)
        })?
    } else {
        label_tables(s.conn(), SESSION_PREFIX, &columns, floor_labels)?
    };
    let mut labels = labels.into_iter();
    let mut evidence = Vec::with_capacity(columns.len());
    for (table, cols) in group_by_table(&columns) {
        let names: Vec<&str> = cols.iter().map(|c| c.column.as_str()).collect();
        let counts = measure_counts(s.conn(), SESSION_PREFIX, table, &names)?;
        for (c, (rows, non_null, distinct)) in cols.iter().zip(counts) {
            evidence.push(ColumnEvidence {
                table: c.table.clone(),
                column: c.column.clone(),
                duckdb_type: c.duckdb_type.clone(),
                rows,
                non_null,
                distinct,
                label: labels.next().flatten(),
                audit_name: is_audit_name(&c.column),
            });
        }
    }
    write_column_evidence(s, &evidence, true)?;
    Ok(evidence)
}

/// Whether `name` is one of [`AUDIT_TIME_NAMES`], ignoring case, or one of
/// their stems with `_ts` or `_at`.
pub fn is_audit_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    AUDIT_TIME_NAMES.iter().any(|audit| {
        let stem = audit.strip_suffix("_at").unwrap_or(audit);
        name == *audit
            || name
                .strip_prefix(stem)
                .is_some_and(|rest| rest == "_ts" || rest == "_at")
    })
}

/// A column as the catalog lists it.
#[derive(Debug, Clone)]
pub(crate) struct ListedColumn {
    pub table: String,
    pub column: String,
    pub duckdb_type: String,
}

/// The columns of `catalog`'s `main` schema, in table and column order.
pub(crate) fn columns_sql(catalog: &str) -> String {
    format!(
        "SELECT table_name, column_name, data_type
         FROM information_schema.columns
         WHERE table_catalog = '{}' AND table_schema = 'main'
         ORDER BY table_name, ordinal_position",
        catalog.replace('\'', "''")
    )
}

pub(crate) fn list_columns(
    conn: &Connection,
    catalog: &str,
) -> Result<Vec<ListedColumn>, EvidenceError> {
    let mut stmt = conn.prepare(&columns_sql(catalog))?;
    let rows = stmt.query_map([], |r| {
        Ok(ListedColumn {
            table: r.get(0)?,
            column: r.get(1)?,
            duckdb_type: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Consecutive runs of one table's columns; `columns` is in table order.
fn group_by_table(columns: &[ListedColumn]) -> Vec<(&str, &[ListedColumn])> {
    let mut groups = Vec::new();
    let mut start = 0;
    for i in 1..=columns.len() {
        if i == columns.len() || columns[i].table != columns[start].table {
            groups.push((columns[start].table.as_str(), &columns[start..i]));
            start = i;
        }
    }
    groups
}

/// `count(*)`, then `count` and `count(DISTINCT …)` of each column, in one
/// statement over the table.
pub(crate) fn counts_sql(prefix: &str, table: &str, columns: &[&str]) -> String {
    let mut select = vec!["count(*)".to_string()];
    for c in columns {
        let c = quote(c);
        select.push(format!("count({c})"));
        select.push(format!("count(DISTINCT {c})"));
    }
    format!("SELECT {} FROM {prefix}{}", select.join(", "), quote(table))
}

/// `(rows, non_null, distinct)` for each of `columns`, in order.
pub(crate) fn measure_counts(
    conn: &Connection,
    prefix: &str,
    table: &str,
    columns: &[&str],
) -> Result<Vec<(i64, i64, i64)>, EvidenceError> {
    let counts = conn.query_row(&counts_sql(prefix, table, columns), [], |r| {
        let rows: i64 = r.get(0)?;
        (0..columns.len())
            .map(|i| Ok((rows, r.get(1 + 2 * i)?, r.get(2 + 2 * i)?)))
            .collect::<duckdb::Result<Vec<_>>>()
    })?;
    Ok(counts)
}

/// Up to `n` of a column's non-null values, as text: the floor's sample.
pub(crate) fn sample_sql(prefix: &str, table: &str, column: &str, n: usize) -> String {
    format!(
        "SELECT CAST({col} AS VARCHAR) FROM {prefix}{tbl} WHERE {col} IS NOT NULL LIMIT {n}",
        col = quote(column),
        tbl = quote(table),
    )
}

/// FineType's profile of each column, with its name as the header hint, in
/// one statement over the table.
pub(crate) fn profile_sql(prefix: &str, table: &str, columns: &[&str]) -> String {
    let select: Vec<String> = columns
        .iter()
        .map(|c| {
            format!(
                "struct_extract(ft_profile(CAST({} AS VARCHAR), '{}'), 'type')",
                quote(c),
                c.replace('\'', "''")
            )
        })
        .collect();
    format!("SELECT {} FROM {prefix}{}", select.join(", "), quote(table))
}

type Labeller = fn(&Connection, &str, &str, &[&str]) -> Result<Vec<Option<String>>, EvidenceError>;

/// Labels for every column, in `columns` order, a table at a time.
fn label_tables(
    conn: &Connection,
    prefix: &str,
    columns: &[ListedColumn],
    labeller: Labeller,
) -> Result<Vec<Option<String>>, EvidenceError> {
    let mut labels = Vec::with_capacity(columns.len());
    for (table, cols) in group_by_table(columns) {
        let names: Vec<&str> = cols.iter().map(|c| c.column.as_str()).collect();
        labels.extend(labeller(conn, prefix, table, &names)?);
    }
    Ok(labels)
}

/// The floor: FineType's model-free typer over a sample of each column.
pub(crate) fn floor_labels(
    conn: &Connection,
    prefix: &str,
    table: &str,
    columns: &[&str],
) -> Result<Vec<Option<String>>, EvidenceError> {
    columns
        .iter()
        .map(|c| {
            let mut stmt = conn.prepare(&sample_sql(prefix, table, c, FIELD_SAMPLE_N))?;
            let sample = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<duckdb::Result<Vec<_>>>()?;
            Ok(crate::typing::deterministic_semantic_type(&sample))
        })
        .collect()
}

/// FineType's profile; a column it cannot type (`unknown`, as an empty one
/// is) has no label. Run it under [`with_one_thread`].
pub(crate) fn profile_labels(
    conn: &Connection,
    prefix: &str,
    table: &str,
    columns: &[&str],
) -> Result<Vec<Option<String>>, EvidenceError> {
    let labels = conn.query_row(&profile_sql(prefix, table, columns), [], |r| {
        (0..columns.len())
            .map(|i| r.get::<_, Option<String>>(i))
            .collect::<duckdb::Result<Vec<_>>>()
    })?;
    Ok(labels
        .into_iter()
        .map(|l| l.filter(|l| l != "unknown"))
        .collect())
}

/// Run `body` with `SET threads = 1`, so the sample FineType's profile takes
/// does not depend on how DuckDB split the scan, and restore the thread count
/// afterwards whether or not `body` succeeded.
fn with_one_thread<T>(
    conn: &Connection,
    body: impl FnOnce(&Connection) -> Result<T, EvidenceError>,
) -> Result<T, EvidenceError> {
    let threads: i64 = conn.query_row(
        "SELECT CAST(current_setting('threads') AS BIGINT)",
        [],
        |r| r.get(0),
    )?;
    conn.execute_batch("SET threads = 1")?;
    let result = body(conn);
    conn.execute_batch(&format!("SET threads = {threads}"))?;
    result
}

/// Write `rows` to `memory.column_evidence`, replacing what was there when
/// `replace`, else adding to it.
pub(crate) fn write_column_evidence(
    s: &Session,
    rows: &[ColumnEvidence],
    replace: bool,
) -> Result<(), EvidenceError> {
    let label_source = match s.typing {
        TypingSource::Profiled { .. } => "profiled",
        TypingSource::Floor { .. } => "floor",
    };
    let create = if replace {
        "CREATE OR REPLACE TABLE"
    } else {
        "CREATE TABLE IF NOT EXISTS"
    };
    s.conn.execute_batch(&format!(
        "{create} memory.main.column_evidence (
             \"table\" VARCHAR, \"column\" VARCHAR, duckdb_type VARCHAR, \"rows\" BIGINT,
             non_null BIGINT, \"distinct\" BIGINT, label VARCHAR, audit_name BOOLEAN,
             label_source VARCHAR)"
    ))?;
    let mut insert = s
        .conn
        .prepare("INSERT INTO memory.main.column_evidence VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
    for r in rows {
        insert.execute(duckdb::params![
            r.table,
            r.column,
            r.duckdb_type,
            r.rows,
            r.non_null,
            r.distinct,
            r.label,
            r.audit_name,
            label_source,
        ])?;
    }
    Ok(())
}
