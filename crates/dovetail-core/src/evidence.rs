//! evidence — what dovetail measures about the analyst's database, measured
//! inside DuckDB.
//!
//! A [`Session`] is an in-memory DuckDB that attaches the analyst's database
//! as `src`, read-only, and holds dovetail's own evidence tables in its
//! `memory` catalog. Every measurement is a query that returns rows and names
//! the tables it reads fully qualified, so dovetail's evidence tables are
//! never measured themselves. Writing an evidence table is a separate step
//! that only a session runs: the SQL builders here take the catalog to read
//! (a catalog name for the column listing, `src` for a session and
//! `current_database()` for a bare connection; a table prefix for the rest,
//! `SESSION_PREFIX` for a session and empty for a bare connection), so the
//! same measurements run on a connection dovetail must never write to.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use duckdb::Connection;

use crate::finetype_ext::{self, TypingRequest, TypingSource};
use crate::ossie::LINEAGE;
use crate::packages::SourceTable;
use crate::relate::{self, quote, type_family, ColumnRef, Edge, FIELD_SAMPLE_N};
use crate::roles::{points_at, CarriedKey, TableEvidence, TimeColumn};

/// How a session's queries name the analyst's tables.
pub(crate) const SESSION_PREFIX: &str = "src.main.";

/// How a session built by [`Session::over`] names the analyst's database
/// itself, attached beside `src` for the full data.
pub(crate) const FULL_PREFIX: &str = "\"full\".main.";

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

/// How many times as many numbers as it has distinct values an integer
/// column's values may span and still count as fixed-width keys; see
/// `parent_key_shaped`. classicmodels' customer numbers, 122 of them from 103
/// to 496, span 3.2 times their count; a unique five-digit measure of a few
/// hundred values spans hundreds of times its count.
pub const KEY_SHAPE_MAX_SPAN: i64 = 10;

/// Words that make a date describe when the thing itself began, such as an
/// employee's `hire_date` or a person's `BirthDate`, rather than date an
/// occurrence. A name counts when one of its words, split at `_`, other
/// separators and lower-to-upper case changes and compared lower-cased, is one
/// of these. Such a date is an attribute of an entity: a table of employees
/// with a hire date and a department it repeats is still employees. Words that
/// as often name a logged occurrence (`opened`, `signup`, `released`) are left
/// out, since an event log of those actions has no other time.
pub const LIFECYCLE_TIME_WORDS: &[&str] = &[
    "birth",
    "birthdate",
    "birthday",
    "dob",
    "born",
    "hire",
    "hired",
    "hiredate",
    "founded",
    "established",
];

/// Whether `name` holds one of [`LIFECYCLE_TIME_WORDS`], as that list
/// describes.
pub fn is_lifecycle_name(name: &str) -> bool {
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut prev_lower = false;
    for ch in name.chars() {
        if !ch.is_ascii_alphanumeric() {
            words.push(std::mem::take(&mut word));
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() && prev_lower {
            words.push(std::mem::take(&mut word));
        }
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        word.push(ch.to_ascii_lowercase());
    }
    words.push(word);
    words
        .iter()
        .any(|w| LIFECYCLE_TIME_WORDS.contains(&w.as_str()))
}

/// Column names that refer to another row of the same table, such as an
/// employee's manager. A name counts lower-cased, with a trailing `_id` or `id`
/// dropped, when what is left is an entry; or, when it ended in `_id`, starts
/// with an entry and `_` or ends with `_` and an entry. So `manager_id`,
/// `ParentId`, `parent_category_id` and `category_parent_id` count, and
/// `manager_rating` and `manager_paid` do not. A column needs
/// such a name to be a self-reference: a dense integer key otherwise contains
/// every small count in the table, and a name such as `order_priority` shares
/// only the table's own prefix.
pub const SELF_POINTER_NAMES: &[&str] = &[
    "parent",
    "manager",
    "reports_to",
    "supervisor",
    "predecessor",
    "successor",
    "mentor",
    "referred_by",
];

/// Whether `name` is one of [`SELF_POINTER_NAMES`], as that list describes.
pub fn is_self_pointer_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let compound = name.strip_suffix("_id");
    let stem = compound
        .or_else(|| name.strip_suffix("id"))
        .unwrap_or(&name);
    SELF_POINTER_NAMES.iter().any(|entry| {
        stem == *entry
            || (compound.is_some()
                && (stem
                    .strip_prefix(entry)
                    .is_some_and(|rest| rest.starts_with('_'))
                    || stem
                        .strip_suffix(entry)
                        .is_some_and(|rest| rest.ends_with('_'))))
    })
}

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
    /// The attached database's path, or `:memory:` for one built in memory.
    source: String,
    /// Labels already known, by [`LabelKey`]; [`column_evidence`] types only
    /// the columns missing from it.
    known_labels: BTreeMap<LabelKey, Option<String>>,
    /// For a session built by [`Session::over`]: each source table's name and
    /// where its full data is read from.
    full_relations: BTreeMap<String, String>,
    /// Each of those tables' full row counts.
    full_rows: BTreeMap<String, i64>,
    /// The sample size [`Session::over`] was asked for.
    sample: Option<u64>,
}

/// How a session built by [`Session::over`] labels its columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTyping {
    /// From the source tables' package fields; a field with no label, and a
    /// column with no field, is untyped.
    Packages,
    /// As [`Session::attach`] types.
    Request(TypingRequest),
}

/// A column's table, name and DuckDB type: what a known label is filed under.
pub type LabelKey = (String, String, String);

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
        Ok(Self::with_typing(conn, typing, path))
    }

    /// Build `src` in memory by running `build_sql` inside it.
    pub fn from_sql(build_sql: &str, typing: TypingRequest) -> Result<Session, EvidenceError> {
        let conn = open()?;
        conn.execute_batch("ATTACH ':memory:' AS src; USE src;")?;
        conn.execute_batch(build_sql)?;
        conn.execute_batch("USE memory;")?;
        Ok(Self::with_typing(conn, typing, ":memory:"))
    }

    /// Model `tables` of the database at `db`. `db` is attached read-only as
    /// `"full"` (a keyword, so quoted), and `src` is an in-memory catalog holding each table under its
    /// name, without the lineage columns: a view over the full data when
    /// `sample` is `None`, else a copy of at most `sample` rows drawn by a
    /// reservoir sample at a fixed seed on one thread (a table of that size or
    /// less is copied whole). The full data stays readable through
    /// [`Session::full_relation`].
    pub fn over(
        db: &Path,
        tables: &[SourceTable],
        sample: Option<u64>,
        typing: SessionTyping,
    ) -> Result<Session, EvidenceError> {
        if sample == Some(0) {
            return Err(EvidenceError::Session(
                "a sample of 0 rows is not a sample".to_string(),
            ));
        }
        let path = db.to_str().ok_or_else(|| {
            EvidenceError::Session(format!(
                "the database path {} is not valid UTF-8",
                db.display()
            ))
        })?;
        let conn = open()?;
        conn.execute_batch(&format!(
            "ATTACH '{}' AS \"full\" (READ_ONLY); ATTACH ':memory:' AS src;",
            path.replace('\'', "''")
        ))?;
        let full = list_columns(&conn, "full")?;
        let mut full_relations = BTreeMap::new();
        let mut full_rows = BTreeMap::new();
        for t in tables {
            let present: Vec<String> = full
                .iter()
                .filter(|c| c.table == t.relation && LINEAGE.contains(&c.column.as_str()))
                .map(|c| quote(&c.column))
                .collect();
            let from = format!("{FULL_PREFIX}{}", quote(&t.relation));
            let select = if present.is_empty() {
                format!("SELECT * FROM {from}")
            } else {
                format!("SELECT * EXCLUDE ({}) FROM {from}", present.join(", "))
            };
            let target = format!("{SESSION_PREFIX}{}", quote(&t.name));
            let rows: i64 =
                conn.query_row(&format!("SELECT count(*) FROM {from}"), [], |r| r.get(0))?;
            match sample {
                Some(n) if rows > i64::try_from(n).unwrap_or(i64::MAX) => {
                    with_one_thread(&conn, |conn| {
                        conn.execute_batch(&format!(
                            "CREATE TABLE {target} AS {select} \
                             USING SAMPLE reservoir({n} ROWS) REPEATABLE (42)"
                        ))?;
                        Ok(())
                    })?
                }
                Some(_) => conn.execute_batch(&format!("CREATE TABLE {target} AS {select}"))?,
                None => conn.execute_batch(&format!("CREATE VIEW {target} AS {select}"))?,
            }
            full_relations.insert(t.name.clone(), from);
            full_rows.insert(t.name.clone(), rows);
        }
        let mut session = match typing {
            SessionTyping::Request(request) => Self::with_typing(conn, request, path),
            SessionTyping::Packages => Session {
                conn,
                typing: TypingSource::Packages,
                source: path.to_string(),
                known_labels: BTreeMap::new(),
                full_relations: BTreeMap::new(),
                full_rows: BTreeMap::new(),
                sample: None,
            },
        };
        if typing == SessionTyping::Packages {
            let mut labels = BTreeMap::new();
            for c in list_columns(session.conn(), "src")? {
                let label = tables
                    .iter()
                    .find(|t| t.name == c.table)
                    .and_then(|t| t.fields.iter().find(|f| f.name == c.column))
                    .and_then(|f| f.finetype.clone());
                labels.insert(label_key(&c), label);
            }
            session.known_labels = labels;
        }
        session.full_relations = full_relations;
        session.full_rows = full_rows;
        session.sample = sample;
        Ok(session)
    }

    /// Where the full data of the source table `name` is read from, as a
    /// qualified relation, for a session built by [`Session::over`].
    pub fn full_relation(&self, name: &str) -> Option<&str> {
        self.full_relations.get(name).map(String::as_str)
    }

    /// The full row count of the source table `name`, for a session built by
    /// [`Session::over`].
    pub fn full_rows(&self, name: &str) -> Option<i64> {
        self.full_rows.get(name).copied()
    }

    /// The sample size a session built by [`Session::over`] was asked for.
    pub fn sampled(&self) -> Option<u64> {
        self.sample
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// The attached database's path, which the descriptor names its
    /// resources after.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Give the session labels already worked out under its own typing, such
    /// as a cached profile, so [`column_evidence`] does not type those columns
    /// again. The caller vouches that they came from the same typing as
    /// [`Session::typing`]; a column whose table, name or DuckDB type is not
    /// among them is typed as usual.
    pub fn set_known_labels(&mut self, labels: BTreeMap<LabelKey, Option<String>>) {
        self.known_labels = labels;
    }

    fn with_typing(conn: Connection, typing: TypingRequest, source: &str) -> Session {
        let typing = finetype_ext::load(&conn, typing);
        Session {
            conn,
            typing,
            source: source.to_string(),
            known_labels: BTreeMap::new(),
            full_relations: BTreeMap::new(),
            full_rows: BTreeMap::new(),
            sample: None,
        }
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
    let mut labels = column_labels_of(s, &columns)?.into_iter();
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
    write_column_evidence(s, &evidence, true, false)?;
    Ok(evidence)
}

/// Every column of the session's `src` labelled under its typing, by
/// [`LabelKey`]: the known labels where the session has them, the rest typed
/// now. This is the typing [`column_evidence`] uses, without the counts.
pub fn column_labels(s: &Session) -> Result<BTreeMap<LabelKey, Option<String>>, EvidenceError> {
    let columns = list_columns(s.conn(), "src")?;
    let labels = column_labels_of(s, &columns)?;
    Ok(columns.iter().map(label_key).zip(labels).collect())
}

fn label_key(c: &ListedColumn) -> LabelKey {
    (c.table.clone(), c.column.clone(), c.duckdb_type.clone())
}

/// A label for each of `columns`, in order: the session's known label where
/// it has one, else typed under the session's typing.
fn column_labels_of(
    s: &Session,
    columns: &[ListedColumn],
) -> Result<Vec<Option<String>>, EvidenceError> {
    let unknown: Vec<ListedColumn> = columns
        .iter()
        .filter(|c| !s.known_labels.contains_key(&label_key(c)))
        .cloned()
        .collect();
    // Under the packages a column no field labels is untyped, never floor-typed.
    let typed = if unknown.is_empty() || matches!(s.typing, TypingSource::Packages) {
        Vec::new()
    } else if matches!(s.typing, TypingSource::Profiled { .. }) {
        with_one_thread(s.conn(), |conn| {
            label_tables(conn, SESSION_PREFIX, &unknown, profile_labels)
        })?
    } else {
        label_tables(s.conn(), SESSION_PREFIX, &unknown, floor_labels)?
    };
    let mut typed = typed.into_iter();
    Ok(columns
        .iter()
        .map(|c| match s.known_labels.get(&label_key(c)) {
            Some(known) => known.clone(),
            None => typed.next().flatten(),
        })
        .collect())
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

/// The columns of the catalog named `catalog` (`src` in a session,
/// `current_database()` on a bare connection), its `main` schema, in table and
/// column order.
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
pub(crate) fn group_by_table(columns: &[ListedColumn]) -> Vec<(&str, &[ListedColumn])> {
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
/// `LIMIT` without `ORDER BY` takes the first rows in insertion order, which
/// rests on DuckDB's default `preserve_insertion_order`.
pub(crate) fn sample_sql(prefix: &str, table: &str, column: &str, n: usize) -> String {
    format!(
        "SELECT CAST({col} AS VARCHAR) FROM {prefix}{tbl} WHERE {col} IS NOT NULL LIMIT {n}",
        col = quote(column),
        tbl = quote(table),
    )
}

/// The version of how this module asks FineType for a profile and reads its
/// answer: [`profile_sql`], [`profile_labels`] and [`with_one_thread`]. A
/// cached profile is stamped with it, so bump it whenever any of the three
/// changes what a column is labelled.
pub(crate) const PROFILE_CODE_VERSION: u32 = 1;

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
pub(crate) fn label_tables(
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
    let restored = conn.execute_batch(&format!("SET threads = {threads}"));
    let value = result?;
    restored?;
    Ok(value)
}

/// Write `rows` to `memory.column_evidence`, replacing what was there when
/// `replace`, else adding to it. `inherited` rows took their labels from
/// other columns (a view's from its parts), so their label source says so.
pub(crate) fn write_column_evidence(
    s: &Session,
    rows: &[ColumnEvidence],
    replace: bool,
    inherited: bool,
) -> Result<(), EvidenceError> {
    let label_source = match (inherited, &s.typing) {
        (true, _) => "inherited",
        (false, TypingSource::Profiled { .. }) => "profiled",
        (false, TypingSource::Floor { .. }) => "floor",
        (false, TypingSource::Packages) => "packages",
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

/// One candidate join, child column to parent column, measured.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeEvidence {
    pub child: ColumnRef,
    pub parent: ColumnRef,
    /// The child's non-null rows.
    pub child_total: i64,
    pub child_distinct: i64,
    /// The parent's non-null rows.
    pub parent_total: i64,
    pub parent_distinct: i64,
    /// Distinct child values present in the parent.
    pub overlap_distinct: i64,
    /// Non-null child rows whose value is absent from the parent.
    pub orphan_count: i64,
    pub name_similarity: f64,
    pub semantic_similarity: f64,
    pub child_label: Option<String>,
    pub parent_label: Option<String>,
    /// Whether the parent's values look like keys (at least 90% are made of
    /// letters, digits and `- _ / . :` and share one length or, among values
    /// with a letter, one pattern; see `parent_key_shaped`). Measured only for
    /// an untyped parent that could earn `exact_name_key_credit`
    /// otherwise; `false` elsewhere.
    pub parent_key_shaped: bool,
    /// Parent key values held by more than one parent row, NULLs left out.
    pub parent_duplicates: i64,
    /// The parent's non-null key rows on the full data. A sampled session
    /// measures a source table, or a combination over its parts' full tables,
    /// on its full rows, so this is `parent_total`.
    pub parent_full_total: i64,
}

/// Which run a set of edge evidence rows belongs to, kept apart in
/// `memory.edge_evidence` by its `pass` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// The analyst's own tables, as the descriptor describes them.
    Parts,
    /// The combined datasets in place of their parts, beside the untouched
    /// tables.
    Combined,
}

impl Pass {
    pub fn as_str(self) -> &'static str {
        match self {
            Pass::Parts => "parts",
            Pass::Combined => "combined",
        }
    }
}

/// Where a dataset's rows are read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Relation {
    /// A table of the analyst's database, `src.main`.
    Source,
    /// A view dovetail created in its own `memory.main`.
    View,
}

/// The key a snapshot combination is measured with: its parts' entity key
/// plus the token that tells the parts apart.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SnapshotKey {
    pub entity_key: Vec<String>,
    pub token: String,
}

/// A table or view to measure, named with where it is read from, so a view
/// never stands in for a table of the same name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dataset {
    pub name: String,
    pub relation: Relation,
    /// Set for a snapshot combination's view: its key is fixed rather than
    /// searched for, and its token is its one non-audit time column.
    pub snapshot: Option<SnapshotKey>,
    /// For a combination's view: the source tables it unites.
    pub parts: Vec<String>,
}

impl Dataset {
    /// A table of the analyst's database.
    pub fn source(name: &str) -> Dataset {
        Dataset {
            name: name.to_string(),
            relation: Relation::Source,
            snapshot: None,
            parts: Vec::new(),
        }
    }

    /// A view in dovetail's own catalog.
    pub fn view(name: &str) -> Dataset {
        Dataset {
            name: name.to_string(),
            relation: Relation::View,
            snapshot: None,
            parts: Vec::new(),
        }
    }

    /// Each of `names` as a table of the analyst's database.
    pub fn sources(names: &[String]) -> Vec<Dataset> {
        names.iter().map(|n| Dataset::source(n)).collect()
    }

    /// The fully qualified relation a session reads.
    pub fn relation_sql(&self) -> String {
        match self.relation {
            Relation::Source => format!("{SESSION_PREFIX}{}", quote(&self.name)),
            Relation::View => format!("{VIEW_PREFIX}{}", quote(&self.name)),
        }
    }
}

/// How a session's queries name dovetail's own views.
pub(crate) const VIEW_PREFIX: &str = "memory.main.";

/// How a sampled session's queries name a combination's view over its parts' full
/// tables, beside its sampled view in [`VIEW_PREFIX`].
pub(crate) const FULL_VIEW_PREFIX: &str = "memory.full_data.";

/// The schema of [`FULL_VIEW_PREFIX`].
pub(crate) const FULL_VIEW_SCHEMA: &str = "memory.full_data";

/// The datasets' names, after checking that no name is listed twice: a name
/// is what evidence rows and edges carry, so two datasets of one name would
/// read as one.
fn dataset_names(datasets: &[Dataset]) -> Result<Vec<String>, EvidenceError> {
    let mut seen = BTreeSet::new();
    for d in datasets {
        if !seen.insert(d.name.as_str()) {
            return Err(EvidenceError::Session(format!(
                "the dataset {} is listed twice",
                d.name
            )));
        }
    }
    Ok(datasets.iter().map(|d| d.name.clone()).collect())
}

/// Render a dataset's name as the relation to read, for the datasets listed.
fn relation_of(datasets: &[Dataset]) -> impl Fn(&str) -> String + '_ {
    move |table: &str| {
        datasets.iter().find(|d| d.name == table).map_or_else(
            // Only listed datasets are measured; an unlisted name is read
            // as the analyst's table it names.
            || format!("{SESSION_PREFIX}{}", quote(table)),
            Dataset::relation_sql,
        )
    }
}

/// How many rows of each table a sampled session draws.
pub const SAMPLE_ROWS: u64 = 50_000;

/// Measure every candidate join among `datasets` and write the rows to
/// `memory.edge_evidence` under `pass`, replacing that pass's rows.
pub fn edge_evidence(
    s: &Session,
    cols: &[ColumnEvidence],
    datasets: &[Dataset],
    pass: Pass,
) -> Result<Vec<EdgeEvidence>, EvidenceError> {
    edge_evidence_with(s, cols, datasets, pass, &|_, _| false, Vec::new())
}

/// [`edge_evidence`], leaving out every pair of tables `skip(child, parent)`
/// names and taking `reused` rows, measured before, in their place. The rows
/// come back, and are written, in the order [`edge_evidence`] would measure
/// them: child, then parent, in `cols` order.
pub fn edge_evidence_with(
    s: &Session,
    cols: &[ColumnEvidence],
    datasets: &[Dataset],
    pass: Pass,
    skip: &dyn Fn(&str, &str) -> bool,
    reused: Vec<EdgeEvidence>,
) -> Result<Vec<EdgeEvidence>, EvidenceError> {
    let tables = dataset_names(datasets)?;
    // A combination's views over its parts' full tables, which only a sampled
    // session makes.
    let full_views: BTreeSet<String> = match s.sampled() {
        Some(_) => {
            let mut stmt = s.conn().prepare(
                "SELECT view_name FROM duckdb_views() \
                 WHERE database_name = 'memory' AND schema_name = 'full_data'",
            )?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<duckdb::Result<_>>()?
        }
        None => BTreeSet::new(),
    };
    // A sampled session answers questions about a parent's whole key from
    // its full rows: a source table's full table, and a combination's view
    // over its parts' full tables.
    let full = |table: &str| -> Option<String> {
        s.sampled()?;
        let d = datasets.iter().find(|d| d.name == table)?;
        match d.relation {
            Relation::Source => s.full_relation(&d.name).map(str::to_string),
            Relation::View => full_views
                .contains(&d.name)
                .then(|| format!("{FULL_VIEW_PREFIX}{}", quote(&d.name))),
        }
    };
    let mut edges = measure_edges(s.conn(), &relation_of(datasets), &full, cols, &tables, skip)?;
    if !reused.is_empty() {
        let position: std::collections::BTreeMap<(&str, &str), usize> = cols
            .iter()
            .enumerate()
            .map(|(i, c)| ((c.table.as_str(), c.column.as_str()), i))
            .collect();
        let at = |c: &ColumnRef| {
            position
                .get(&(c.table.as_str(), c.column.as_str()))
                .copied()
                .unwrap_or(usize::MAX)
        };
        edges.extend(reused);
        edges.sort_by_key(|e| (at(&e.child), at(&e.parent)));
    }
    write_edge_evidence(s, pass.as_str(), &edges)?;
    Ok(edges)
}

/// The candidate joins among `tables`, measured, in child then parent column
/// order. A pair is a candidate when its columns sit in different tables,
/// share a type family, the child has a non-null value, and the pair's name or
/// semantic similarity reaches 0.3. Totals and distincts come from `cols`;
/// one query per child column measures overlap and orphans against all its
/// candidate parents. `relation` renders a table name as the relation to read;
/// a pair of tables `skip(child, parent)` names is no candidate. `full(table)`
/// is the relation holding the whole of a table the rows were sampled from; a
/// parent it names is measured there (its non-null rows, distinct values and
/// duplicated values, and which child values it holds), not in `relation`.
pub(crate) fn measure_edges(
    conn: &Connection,
    relation: &dyn Fn(&str) -> String,
    full: &dyn Fn(&str) -> Option<String>,
    cols: &[ColumnEvidence],
    tables: &[String],
    skip: &dyn Fn(&str, &str) -> bool,
) -> Result<Vec<EdgeEvidence>, EvidenceError> {
    let in_scope: Vec<&ColumnEvidence> =
        cols.iter().filter(|c| tables.contains(&c.table)).collect();
    let mut edges = Vec::new();
    // Each parent column's key shape, measured at most once.
    let mut shapes: std::collections::BTreeMap<(String, String), bool> = Default::default();
    // Each parent column's key counts, measured at most once.
    let mut keys: std::collections::BTreeMap<(String, String), ParentKey> = Default::default();
    for child in &in_scope {
        if child.non_null == 0 {
            continue;
        }
        let child_ref = column_ref(child);
        let mut candidates = Vec::new();
        for parent in &in_scope {
            if child.table == parent.table
                || skip(&child.table, &parent.table)
                || type_family(&child.duckdb_type) != type_family(&parent.duckdb_type)
            {
                continue;
            }
            let parent_ref = column_ref(parent);
            let name = relate::name_similarity(&child_ref, &parent_ref);
            let semantic = relate::semantic_similarity(
                &child_ref,
                child.label.as_deref(),
                parent.label.as_deref(),
            );
            // A pair must be at least plausibly related before it is measured.
            // Shared identifier typing is a second way past spelling, so a pair
            // whose names share nothing and whose types share everything is
            // still measured.
            if name.max(semantic) < 0.3 {
                continue;
            }
            candidates.push((*parent, parent_ref, name, semantic));
        }
        if candidates.is_empty() {
            continue;
        }
        let parents: Vec<(String, &str)> = candidates
            .iter()
            .map(|(p, ..)| {
                (
                    full(&p.table).unwrap_or_else(|| relation(&p.table)),
                    p.column.as_str(),
                )
            })
            .collect();
        let sql = overlap_sql(&relation(&child.table), &child.column, &parents);
        let mut stmt = conn.prepare(&sql)?;
        let counts = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))?
            .collect::<duckdb::Result<Vec<_>>>()?;
        for ((parent, parent_ref, name, semantic), (overlap, orphans)) in
            candidates.into_iter().zip(counts)
        {
            let key = (parent.table.clone(), parent.column.clone());
            let counts = match keys.get(&key) {
                Some(&counts) => counts,
                None => {
                    let counts = parent_key(
                        conn,
                        &relation(&parent.table),
                        full(&parent.table).as_deref(),
                        parent,
                    )?;
                    keys.insert(key, counts);
                    counts
                }
            };
            let mut edge = EdgeEvidence {
                child: child_ref.clone(),
                parent: parent_ref,
                child_total: child.non_null,
                child_distinct: child.distinct,
                parent_total: counts.total,
                parent_distinct: counts.distinct,
                overlap_distinct: overlap,
                orphan_count: orphans,
                name_similarity: name,
                semantic_similarity: semantic,
                child_label: child.label.clone(),
                parent_label: parent.label.clone(),
                parent_key_shaped: false,
                parent_duplicates: counts.duplicates,
                parent_full_total: counts.full_total,
            };
            // One SELECT, and only for a pair the cheaper conditions leave
            // undecided.
            // Only text and integer columns can be keys here: a decimal, float,
            // date or boolean column measures something.
            if relate::needs_key_shape(&edge)
                && matches!(type_family(&parent.duckdb_type), "text" | "int")
            {
                let key = (parent.table.clone(), parent.column.clone());
                edge.parent_key_shaped = match shapes.get(&key) {
                    Some(&shaped) => shaped,
                    None => {
                        let shaped = parent_key_shaped(
                            conn,
                            &relation(&parent.table),
                            &parent.column,
                            type_family(&parent.duckdb_type) == "int",
                        )?;
                        shapes.insert(key, shaped);
                        shaped
                    }
                };
            }
            edges.push(edge);
        }
    }
    Ok(edges)
}

/// A parent column's key counts.
#[derive(Debug, Clone, Copy)]
struct ParentKey {
    total: i64,
    distinct: i64,
    duplicates: i64,
    /// `total` on the full data.
    full_total: i64,
}

/// `parent`'s non-null rows, distinct values and duplicated values (values
/// held by more than one row). From `full` when the table was sampled; else
/// the rows and distinct values are `parent`'s own and only the duplicates are
/// counted, and not at all when every non-null row is distinct. A parent that
/// already repeats in its sample is still counted in full: its share of
/// distinct values is part of a join's confidence, and its duplicates are
/// written into the model.
fn parent_key(
    conn: &Connection,
    relation: &str,
    full: Option<&str>,
    parent: &ColumnEvidence,
) -> Result<ParentKey, EvidenceError> {
    let counts = |relation: &str| -> Result<(i64, i64, i64), EvidenceError> {
        let c = quote(&parent.column);
        Ok(conn.query_row(
            &format!(
                "WITH g AS (SELECT {c} AS v, count(*) AS n FROM {relation} \
                            WHERE {c} IS NOT NULL GROUP BY {c}) \
                 SELECT CAST(coalesce(sum(n), 0) AS BIGINT), count(*), \
                        count(*) FILTER (WHERE n > 1) FROM g"
            ),
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    };
    if let Some(full) = full {
        let (total, distinct, duplicates) = counts(full)?;
        return Ok(ParentKey {
            total,
            distinct,
            duplicates,
            full_total: total,
        });
    }
    let duplicates = if parent.non_null == parent.distinct {
        0
    } else {
        counts(relation)?.2
    };
    Ok(ParentKey {
        total: parent.non_null,
        distinct: parent.distinct,
        duplicates,
        full_total: parent.non_null,
    })
}

/// Whether a text or integer column's non-null values look like keys. A value
/// is key-like when it holds only letters, digits and the separators
/// `- _ / . :`, starts with a letter or digit, and either holds a letter or is
/// all digits; so `BEV-12` and `10042` are key-like and `12.50`, `$12.50`,
/// `1,234.50`, `(12.50)` and `New York` are not. The column is key-shaped when
/// at least 90% of all its non-null values are key-like and share one length,
/// or are key-like, hold a letter and share one pattern (each letter is `A`, a
/// run of digits `9`). Only lettered values count towards the pattern share,
/// since every all-digit value has the pattern `9`; all-digit values count
/// towards the length share. A value that is not key-like counts against both
/// shares. An empty column is not key-shaped.
///
/// An `integer` column's values count towards the length share only when they
/// are dense, spanning at most [`KEY_SHAPE_MAX_SPAN`] times as many numbers as
/// it has distinct values: keys are handed out in sequence, while unique
/// measures of one width (populations from 10000 to 99999) are spread over
/// their range.
pub(crate) fn parent_key_shaped(
    conn: &Connection,
    relation: &str,
    column: &str,
    integer: bool,
) -> Result<bool, EvidenceError> {
    let c = quote(column);
    let dense = if integer {
        format!(
            "(SELECT max(CAST(s AS HUGEINT)) - min(CAST(s AS HUGEINT)) + 1
                     <= {KEY_SHAPE_MAX_SPAN} * count(DISTINCT s) FROM k)"
        )
    } else {
        "true".to_string()
    };
    let sql = format!(
        "WITH v AS (SELECT CAST({c} AS VARCHAR) AS s FROM {relation} WHERE {c} IS NOT NULL),
              k AS (SELECT s FROM v
                    WHERE regexp_full_match(s, '[A-Za-z0-9][A-Za-z0-9_/.:-]*')
                      AND (regexp_matches(s, '[A-Za-z]') OR regexp_full_match(s, '[0-9]+'))),
              p AS (SELECT regexp_replace(regexp_replace(s, '[A-Za-z]', 'A', 'g'),
                                          '[0-9]+', '9', 'g') AS pat,
                         regexp_matches(s, '[A-Za-z]') AS lettered, length(s) AS len FROM k)
         SELECT (SELECT count(*) FROM v),
                coalesce((SELECT max(n) FROM (SELECT count(*) AS n FROM p
                                              WHERE lettered OR {dense} GROUP BY len)), 0),
                coalesce((SELECT max(n) FROM (SELECT count(*) AS n FROM p WHERE lettered GROUP BY pat)), 0)"
    );
    let (total, same_len, same_pat): (i64, i64, i64) =
        conn.query_row(&sql, [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(total > 0 && same_len.max(same_pat) * 10 >= total * 9)
}

fn column_ref(c: &ColumnEvidence) -> ColumnRef {
    ColumnRef {
        table: c.table.clone(),
        column: c.column.clone(),
    }
}

/// For each of `parents` (a relation and a column), in order: how many
/// distinct non-null values of the child column it holds, and how many
/// non-null child rows it does not. One statement: the child's values are
/// grouped once and each parent is an arm of a `UNION ALL`. Each arm probes
/// the parent column with the child's values, which a sample keeps few, so
/// a parent read in full is scanned once rather than hashed whole.
pub(crate) fn overlap_sql(
    child_relation: &str,
    child_column: &str,
    parents: &[(String, &str)],
) -> String {
    let cc = quote(child_column);
    let arms: Vec<String> = parents
        .iter()
        .enumerate()
        .map(|(i, (relation, column))| {
            let pc = quote(column);
            format!(
                "SELECT {i} AS i, count(*) FILTER (WHERE hit),
                        CAST(coalesce(sum(n) FILTER (WHERE NOT hit), 0) AS BIGINT)
                 FROM (SELECT n, v IN (SELECT {pc} FROM {relation}
                                       WHERE {pc} IN (SELECT v FROM __dovetail_child_values)) AS hit
                       FROM __dovetail_child_values)"
            )
        })
        .collect();
    format!(
        "WITH __dovetail_child_values AS (SELECT {cc} AS v, count(*) AS n FROM {child_relation}
                        WHERE {cc} IS NOT NULL GROUP BY {cc})
         {} ORDER BY i",
        arms.join(" UNION ALL ")
    )
}

/// Replace `pass`'s rows of `memory.edge_evidence` with `rows`.
fn write_edge_evidence(
    s: &Session,
    pass: &str,
    rows: &[EdgeEvidence],
) -> Result<(), EvidenceError> {
    s.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory.main.edge_evidence (
             pass VARCHAR, child_table VARCHAR, child_column VARCHAR,
             parent_table VARCHAR, parent_column VARCHAR, child_total BIGINT,
             child_distinct BIGINT, parent_total BIGINT, parent_distinct BIGINT,
             overlap_distinct BIGINT, orphan_count BIGINT, name_similarity DOUBLE,
             semantic_similarity DOUBLE, child_label VARCHAR, parent_label VARCHAR,
             parent_key_shaped BOOLEAN)",
    )?;
    s.conn.execute(
        "DELETE FROM memory.main.edge_evidence WHERE pass = ?",
        [pass],
    )?;
    let mut insert = s.conn.prepare(
        "INSERT INTO memory.main.edge_evidence
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )?;
    for r in rows {
        insert.execute(duckdb::params![
            pass,
            r.child.table,
            r.child.column,
            r.parent.table,
            r.parent.column,
            r.child_total,
            r.child_distinct,
            r.parent_total,
            r.parent_distinct,
            r.overlap_distinct,
            r.orphan_count,
            r.name_similarity,
            r.semantic_similarity,
            r.child_label,
            r.parent_label,
            r.parent_key_shaped,
        ])?;
    }
    Ok(())
}

/// The most column pairs [`table_evidence`] tests for a key before it gives up.
const KEY_PAIR_LIMIT: usize = 20;

/// Column names the floor takes as labels, ignoring case.
const LABEL_NAMES: &[&str] = &["name", "label", "title", "description", "desc", "code"];

/// Whether a FineType label names, describes or codes something: a person's or
/// thing's name, free text, or a code.
fn is_label_label(label: &str) -> bool {
    let leaf = label.rsplit('.').next().unwrap_or(label);
    matches!(
        label,
        "representation.text.entity_name"
            | "representation.text.plain_text"
            | "representation.text.word"
    ) || leaf.ends_with("_name")
        || leaf.ends_with("_code")
}

fn is_time_column(c: &ColumnEvidence) -> bool {
    let ty = c.duckdb_type.to_ascii_uppercase();
    ty == "DATE"
        || ty.starts_with("TIMESTAMP")
        || c.label
            .as_deref()
            .is_some_and(|l| l.starts_with("datetime."))
}

/// Measure each of `datasets`: its rows, its key, its time columns and how
/// they vary within the first pointer out, and its label and attribute
/// columns. `edges` give the pointers, as [`crate::roles::points_at`] reads
/// them. The rows are written to `memory.table_evidence`, replacing what was
/// there.
pub fn table_evidence(
    s: &Session,
    cols: &[ColumnEvidence],
    edges: &[Edge],
    datasets: &[Dataset],
) -> Result<Vec<TableEvidence>, EvidenceError> {
    table_evidence_carrying(s, cols, edges, &[], datasets)
}

/// [`table_evidence`], with each [`CarriedKey`]'s column a pointer out of
/// its table as an edge's is.
pub fn table_evidence_carrying(
    s: &Session,
    cols: &[ColumnEvidence],
    edges: &[Edge],
    carried: &[CarriedKey],
    datasets: &[Dataset],
) -> Result<Vec<TableEvidence>, EvidenceError> {
    dataset_names(datasets)?;
    let floor = matches!(s.typing, TypingSource::Floor { .. });
    let mut out = datasets
        .iter()
        .map(|d| measure_table(s.conn(), cols, edges, carried, d, floor))
        .collect::<Result<Vec<_>, _>>()?;
    // A sampled table's row count is a question about the whole table.
    if s.sampled().is_some() {
        for (t, d) in out.iter_mut().zip(datasets) {
            let full = match d.relation {
                Relation::Source => s.full_rows(&d.name),
                // A combination holds the rows of all its parts.
                Relation::View if !d.parts.is_empty() => {
                    d.parts.iter().map(|p| s.full_rows(p)).sum()
                }
                Relation::View => None,
            };
            if let Some(rows) = full {
                t.rows = rows;
            }
        }
    }
    write_table_evidence(s, &out, edges, carried)?;
    Ok(out)
}

/// The table's pointers out: the child columns of the edges that point at
/// another table, and of the keys it carries.
fn pointer_columns<'a>(
    edges: &'a [Edge],
    carried: &'a [CarriedKey],
    table: &str,
) -> BTreeSet<&'a str> {
    points_at(edges)
        .filter(|e| e.child.table == table && e.parent.table != table)
        .map(|e| e.child.column.as_str())
        .chain(
            carried
                .iter()
                .filter(|c| c.child.table == table && c.snapshot != table)
                .map(|c| c.child.column.as_str()),
        )
        .collect()
}

/// One dataset's evidence. `floor` takes label columns by name as well as
/// label. A snapshot's key is the dataset's key as given, and its token is the
/// one non-audit time column kept.
pub(crate) fn measure_table(
    conn: &Connection,
    cols: &[ColumnEvidence],
    edges: &[Edge],
    carried: &[CarriedKey],
    dataset: &Dataset,
    floor: bool,
) -> Result<TableEvidence, EvidenceError> {
    let table = dataset.name.as_str();
    let snapshot = dataset.snapshot.as_ref();
    let tcols: Vec<&ColumnEvidence> = cols.iter().filter(|c| c.table == table).collect();
    let rel = dataset.relation_sql();
    let rows = match tcols.first() {
        Some(c) => c.rows,
        None => conn.query_row(&format!("SELECT count(*) FROM {rel}"), [], |r| r.get(0))?,
    };
    let pointers: BTreeSet<&str> = pointer_columns(edges, carried, table);
    let pointed_at: BTreeSet<&str> = points_at(edges)
        .filter(|e| e.parent.table == table && e.child.table != table)
        .map(|e| e.parent.column.as_str())
        .collect();
    // The identifier labels relate gives key credit to, and no others.
    let identifier = |c: &ColumnEvidence| {
        c.label
            .as_deref()
            .is_some_and(crate::identifiers::is_identifier_leaf)
    };

    let key = match snapshot {
        Some(k) => k
            .entity_key
            .iter()
            .chain(std::iter::once(&k.token))
            .cloned()
            .collect(),
        None => find_key(
            conn,
            &rel,
            &tcols,
            &pointers,
            &pointed_at,
            &identifier,
            rows,
        )?,
    };
    let key_distinct: Vec<i64> = key
        .iter()
        .map(|k| {
            tcols
                .iter()
                .find(|c| &c.column == k)
                .map_or(0, |c| c.distinct)
        })
        .collect();
    let key_is_identifier = !key.is_empty()
        && key
            .iter()
            .all(|k| tcols.iter().any(|c| &c.column == k && identifier(c)));

    let self_references = match key.as_slice() {
        [k] => self_references(conn, &rel, &tcols, k, &pointers)?,
        _ => vec![],
    };

    // The first pointer out, in column order, is the parent a time varies within.
    let first_pointer = tcols.iter().find(|c| pointers.contains(c.column.as_str()));
    // A snapshot's token is its as-of time, and its only non-audit one.
    let times: Vec<&ColumnEvidence> = tcols
        .iter()
        .copied()
        .filter(|c| match snapshot {
            Some(k) => c.column == k.token || (is_time_column(c) && c.audit_name),
            None => is_time_column(c),
        })
        .collect();
    let varies = match (first_pointer, times.is_empty()) {
        (Some(p), false) => varies_within(conn, &rel, &p.column, &times)?,
        _ => vec![false; times.len()],
    };
    let time_columns: Vec<TimeColumn> = times
        .iter()
        .zip(varies)
        .map(|(c, varies)| TimeColumn {
            name: c.column.clone(),
            fill_rate: if rows > 0 {
                c.non_null as f64 / rows as f64
            } else {
                0.0
            },
            distinct: c.distinct,
            audit: c.audit_name,
            varies_within_parent: varies,
        })
        .collect();
    // One row is not an as-of date, and an audit stamp is not one either.
    let constant_date = times
        .iter()
        .filter(|_| rows > 1)
        .find(|c| c.distinct == 1 && !c.audit_name)
        .map(|c| c.column.clone());

    let mut label_columns = Vec::new();
    let mut attribute_columns = Vec::new();
    for c in &tcols {
        if key.contains(&c.column) || pointers.contains(c.column.as_str()) {
            continue;
        }
        let labelled = c.label.as_deref().is_some_and(is_label_label)
            || (floor && LABEL_NAMES.contains(&c.column.to_ascii_lowercase().as_str()));
        if type_family(&c.duckdb_type) == "text" && labelled && !is_time_column(c) {
            label_columns.push(c.column.clone());
        } else {
            attribute_columns.push(c.column.clone());
        }
    }

    Ok(TableEvidence {
        name: table.to_string(),
        rows,
        sample_rows: rows,
        columns: tcols.iter().map(|c| c.column.clone()).collect(),
        key,
        key_distinct,
        key_is_identifier,
        time_columns,
        label_columns,
        attribute_columns,
        constant_date,
        self_references,
        snapshot_token: snapshot.map(|k| k.token.clone()),
    })
}

/// The columns whose non-null values all lie in the table's own key `key`:
/// columns besides the key and its pointers out, of the key's type family,
/// with a value, not labelled as something a key is never drawn from, named
/// as [`SELF_POINTER_NAMES`] describes, and not equal to the key on every row. Sorted. One statement
/// over the table.
fn self_references(
    conn: &Connection,
    rel: &str,
    tcols: &[&ColumnEvidence],
    key: &str,
    pointers: &BTreeSet<&str>,
) -> Result<Vec<String>, EvidenceError> {
    let Some(k) = tcols.iter().find(|c| c.column == key) else {
        return Ok(vec![]);
    };
    let candidates: Vec<&ColumnEvidence> = tcols
        .iter()
        .copied()
        .filter(|c| {
            c.column != key
                && !pointers.contains(c.column.as_str())
                && c.non_null > 0
                && type_family(&c.duckdb_type) == type_family(&k.duckdb_type)
                && !c.label.as_deref().is_some_and(relate::label_excluded)
                && is_self_pointer_name(&c.column)
        })
        .collect();
    if candidates.is_empty() {
        return Ok(vec![]);
    }
    let kq = quote(key);
    let select: Vec<String> = candidates
        .iter()
        .map(|c| {
            let cq = quote(&c.column);
            format!(
                "count(*) FILTER (WHERE {cq} IS NOT NULL AND {cq} NOT IN (SELECT {kq} FROM {rel})), \
                 count(*) FILTER (WHERE {cq} IS DISTINCT FROM {kq})"
            )
        })
        .collect();
    let sql = format!("SELECT {} FROM {rel}", select.join(", "));
    let found = conn.query_row(&sql, [], |r| {
        (0..candidates.len())
            .map(|i| Ok(r.get::<_, i64>(2 * i)? == 0 && r.get::<_, i64>(2 * i + 1)? > 0))
            .collect::<duckdb::Result<Vec<_>>>()
    })?;
    let mut out: Vec<String> = candidates
        .iter()
        .zip(found)
        .filter(|(_, f)| *f)
        .map(|(c, _)| c.column.clone())
        .collect();
    out.sort();
    Ok(out)
}

/// The table's minimal key: one column unique and non-null on every row, not
/// a time and not labelled as something a key is never drawn from, the one
/// something points at first, then one typed as an identifier, then the
/// first; else the first unique pair among the first [`KEY_PAIR_LIMIT`] pairs
/// of non-null columns, pointer columns first. Empty when neither is found.
fn find_key(
    conn: &Connection,
    rel: &str,
    tcols: &[&ColumnEvidence],
    pointers: &BTreeSet<&str>,
    pointed_at: &BTreeSet<&str>,
    identifier: &dyn Fn(&ColumnEvidence) -> bool,
    rows: i64,
) -> Result<Vec<String>, EvidenceError> {
    if rows <= 0 {
        return Ok(vec![]);
    }
    let full: Vec<&ColumnEvidence> = tcols
        .iter()
        .copied()
        .filter(|c| c.non_null == rows)
        .collect();
    let unique: Vec<&ColumnEvidence> = full
        .iter()
        .copied()
        .filter(|c| {
            c.distinct == rows
                && !is_time_column(c)
                && !c.label.as_deref().is_some_and(relate::label_excluded)
        })
        .collect();
    let single = unique
        .iter()
        .find(|c| pointed_at.contains(c.column.as_str()))
        .or_else(|| unique.iter().find(|c| identifier(c)))
        .or_else(|| unique.first());
    if let Some(c) = single {
        return Ok(vec![c.column.clone()]);
    }
    let mut ordered: Vec<&ColumnEvidence> = full
        .iter()
        .copied()
        .filter(|c| pointers.contains(c.column.as_str()))
        .collect();
    ordered.extend(
        full.iter()
            .copied()
            .filter(|c| !pointers.contains(c.column.as_str())),
    );
    let pairs = (0..ordered.len())
        .flat_map(|i| (i + 1..ordered.len()).map(move |j| (i, j)))
        .take(KEY_PAIR_LIMIT);
    for (i, j) in pairs {
        let (a, b) = (&ordered[i].column, &ordered[j].column);
        let distinct: i64 = conn.query_row(
            &format!(
                "SELECT count(DISTINCT ({}, {})) FROM {rel}",
                quote(a),
                quote(b)
            ),
            [],
            |r| r.get(0),
        )?;
        if distinct == rows {
            return Ok(vec![a.clone(), b.clone()]);
        }
    }
    Ok(vec![])
}

/// For each of `times`, whether it takes more than one value within one value
/// of `pointer`: `count(DISTINCT (pointer, time)) > count(DISTINCT pointer)`,
/// over the rows where both are present.
/// One statement over the table.
fn varies_within(
    conn: &Connection,
    rel: &str,
    pointer: &str,
    times: &[&ColumnEvidence],
) -> Result<Vec<bool>, EvidenceError> {
    let p = quote(pointer);
    // Only rows with both a parent and a time count: a row constructor holding
    // a NULL is itself not NULL, and would read as one more pair.
    let select: Vec<String> = times
        .iter()
        .map(|t| {
            let t = quote(&t.column);
            format!(
                "count(DISTINCT ({p}, {t})) FILTER (WHERE {p} IS NOT NULL AND {t} IS NOT NULL), \
                 count(DISTINCT {p}) FILTER (WHERE {t} IS NOT NULL)"
            )
        })
        .collect();
    let sql = format!("SELECT {} FROM {rel}", select.join(", "));
    Ok(conn.query_row(&sql, [], |r| {
        (0..times.len())
            .map(|i| Ok(r.get::<_, i64>(2 * i)? > r.get::<_, i64>(2 * i + 1)?))
            .collect::<duckdb::Result<Vec<_>>>()
    })?)
}

/// Replace `memory.table_evidence` with `rows`, one per table, with the pointer
/// columns `edges` and `carried` give each. Every value is a bound parameter;
/// each list is a `list_value` of parameters built to its length, so no
/// extension is needed.
fn write_table_evidence(
    s: &Session,
    rows: &[TableEvidence],
    edges: &[Edge],
    carried: &[CarriedKey],
) -> Result<(), EvidenceError> {
    use duckdb::types::Value;
    const TIME: &str = "STRUCT(name VARCHAR, fill_rate DOUBLE, \"distinct\" BIGINT, \
                        audit BOOLEAN, varies_within_parent BOOLEAN)";
    s.conn.execute_batch(&format!(
        "CREATE OR REPLACE TABLE memory.main.table_evidence (
             \"table\" VARCHAR, \"rows\" BIGINT, \"columns\" VARCHAR[], \"key\" VARCHAR[],
             key_is_identifier BOOLEAN, pointer_columns VARCHAR[], time_columns {TIME}[],
             label_columns VARCHAR[], attribute_columns VARCHAR[], constant_date VARCHAR,
             self_references VARCHAR[], snapshot_token VARCHAR)"
    ))?;
    // A list of `n` items, each rendered by `item`, cast to `ty`.
    let list = |n: usize, item: &str, ty: &str| {
        if n == 0 {
            format!("CAST([] AS {ty})")
        } else {
            format!("CAST(list_value({}) AS {ty})", vec![item; n].join(", "))
        }
    };
    let strings = |items: &[String], params: &mut Vec<Value>| {
        params.extend(items.iter().map(|i| Value::Text(i.clone())));
        list(items.len(), "?", "VARCHAR[]")
    };
    for t in rows {
        let pointing = pointer_columns(edges, carried, &t.name);
        let pointers: Vec<String> = t
            .columns
            .iter()
            .filter(|c| pointing.contains(c.as_str()))
            .cloned()
            .collect();
        let mut params = vec![Value::Text(t.name.clone()), Value::BigInt(t.rows)];
        let columns = strings(&t.columns, &mut params);
        let key = strings(&t.key, &mut params);
        params.push(Value::Boolean(t.key_is_identifier));
        let pointer_columns = strings(&pointers, &mut params);
        for c in &t.time_columns {
            params.extend([
                Value::Text(c.name.clone()),
                Value::Double(c.fill_rate),
                Value::BigInt(c.distinct),
                Value::Boolean(c.audit),
                Value::Boolean(c.varies_within_parent),
            ]);
        }
        let times = list(
            t.time_columns.len(),
            "struct_pack(name := ?, fill_rate := ?, \"distinct\" := ?, audit := ?, \
             varies_within_parent := ?)",
            &format!("{TIME}[]"),
        );
        let labels = strings(&t.label_columns, &mut params);
        let attributes = strings(&t.attribute_columns, &mut params);
        params.push(t.constant_date.clone().map_or(Value::Null, Value::Text));
        let self_references = strings(&t.self_references, &mut params);
        params.push(t.snapshot_token.clone().map_or(Value::Null, Value::Text));
        s.conn.execute(
            &format!(
                "INSERT INTO memory.main.table_evidence VALUES (?, ?, {columns}, {key}, ?, \
                 {pointer_columns}, {times}, {labels}, {attributes}, CAST(? AS VARCHAR), \
                 {self_references}, CAST(? AS VARCHAR))"
            ),
            duckdb::params_from_iter(params),
        )?;
    }
    Ok(())
}
