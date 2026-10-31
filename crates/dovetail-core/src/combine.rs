//! combine — tables that are one kind of thing split across several tables,
//! combined into one dataset before any role is scored.
//!
//! [`find_combinations`] groups tables with the same columns, compatible types
//! and agreeing labels whose names differ by one token (a year, a date, a
//! number or a word). Each group is a duplicate when its parts' rows are
//! identical, a snapshot when its key largely overlaps across parts and the
//! token is a year or a date, and a partition otherwise. [`create_views`]
//! builds each as a view in dovetail's own catalog over its
//! `UNION ALL BY NAME` query, [`view_column_evidence`]
//! types the views from their parts, and the joins are measured again with the
//! views in their parts' place ([`run_combined`]). An edge to a snapshot's
//! entity key is no relationship, since the key is not unique there; it is
//! kept as a [`CarriedKey`].

use std::collections::{BTreeMap, BTreeSet};

use crate::evidence::{
    self, column_evidence, edge_evidence, edge_evidence_with, list_columns, measure_counts,
    measure_table, parent_key_shaped, table_evidence_carrying, write_column_evidence,
    ColumnEvidence, Dataset, EdgeEvidence, EvidenceError, Pass, Relation, Session, SnapshotKey,
    FULL_VIEW_PREFIX, FULL_VIEW_SCHEMA, SESSION_PREFIX, VIEW_PREFIX,
};
use crate::finetype_ext::TypingSource;
use crate::relate::{edge_from_evidence, needs_key_shape, quote, type_family, Edge, EdgeStatus};
pub use crate::role_key::CombinationKind;
use crate::roles::{CarriedKey, TableEvidence};

/// The least share of the parts' distinct key values that must appear in
/// another part for the parts to be snapshots of one thing rather than a
/// partition of it.
pub const SNAPSHOT_MIN_KEY_OVERLAP: f64 = 0.5;

/// The column that tells a combination's parts apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenColumn {
    pub name: String,
    /// `INTEGER` for a year, `DATE` for a date, else `VARCHAR`.
    pub duckdb_type: String,
    /// `(part, value)`, sorted by part.
    pub values: Vec<(String, String)>,
}

/// Tables proposed combined into one dataset.
#[derive(Debug, Clone, PartialEq)]
pub struct Combination {
    pub name: String,
    /// Sorted.
    pub parts: Vec<String>,
    pub kind: CombinationKind,
    /// `None` for a duplicate, which keeps one part.
    pub token_column: Option<TokenColumn>,
    /// The DuckDB query that builds the combined dataset from `src`.
    pub source_sql: String,
    /// The key found on the first part; empty when none is unique.
    pub key: Vec<String>,
    pub evidence: Vec<String>,
}

impl Combination {
    /// [`Combination::source_sql`] with each part's table, `src.main."<part>"`, read
    /// from the relation `relation(part)` names instead; `None` when it names none for
    /// some part. The query lists its columns, so a relation with more columns, such as
    /// a table with its lineage columns, gives the same columns.
    pub fn source_over(&self, relation: impl Fn(&str) -> Option<String>) -> Option<String> {
        let mut sql = self.source_sql.clone();
        for p in &self.parts {
            sql = sql.replace(&format!("{SESSION_PREFIX}{}", quote(p)), &relation(p)?);
        }
        Some(sql)
    }
}

/// What a token is, which decides the token column's type and name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TokenKind {
    /// Four digits between 1900 and 2100.
    Year,
    /// `YYYY-MM` or `YYYY-MM-DD`, with `-` or `_`.
    Date,
    /// Any other run of digits.
    Digits,
    /// A run of letters.
    Word,
}

/// A token of a table name: a run of letters, a run of digits, or a date.
#[derive(Debug, Clone, Copy)]
struct Token {
    start: usize,
    end: usize,
    kind: TokenKind,
}

/// The name's tokens, in order. Anything that is not a letter or a digit
/// separates them, and a change between letters and digits ends one.
fn tokens(name: &str) -> Vec<Token> {
    let mut runs: Vec<(usize, usize, bool)> = Vec::new();
    for (i, ch) in name.char_indices() {
        let digit = ch.is_ascii_digit();
        if !(digit || ch.is_alphabetic()) {
            continue;
        }
        let end = i + ch.len_utf8();
        match runs.last_mut() {
            Some((_, e, d)) if *e == i && *d == digit => *e = end,
            _ => runs.push((i, end, digit)),
        }
    }
    let bytes = name.as_bytes();
    let digits = |r: &(usize, usize, bool), len: usize| r.2 && r.1 - r.0 == len;
    // A date's runs are joined by one `-` or `_`.
    let joined = |a: &(usize, usize, bool), b: &(usize, usize, bool)| {
        b.0 == a.1 + 1 && matches!(bytes[a.1], b'-' | b'_')
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < runs.len() {
        let r = runs[i];
        let number = |r: &(usize, usize, bool)| name[r.0..r.1].parse::<u32>().ok();
        let month = (digits(&r, 4)
            && i + 1 < runs.len()
            && digits(&runs[i + 1], 2)
            && joined(&r, &runs[i + 1]))
        .then(|| (number(&r), number(&runs[i + 1])))
        .and_then(|(y, m)| Some((y?, m?)))
        .filter(|(_, m)| (1..=12).contains(m));
        // A third two-digit run joined on is the day, and must be one of
        // that month's days, or the runs are no date at all.
        let day = month.and_then(|_| {
            (i + 2 < runs.len() && digits(&runs[i + 2], 2) && joined(&runs[i + 1], &runs[i + 2]))
                .then(|| number(&runs[i + 2]))
                .flatten()
        });
        let valid = match (month, day) {
            (Some((y, m)), Some(d)) => (1..=days_in_month(y, m)).contains(&d),
            (Some(_), None) => true,
            (None, _) => false,
        };
        if valid {
            let (end, used) = match day {
                Some(_) => (runs[i + 2].1, 3),
                None => (runs[i + 1].1, 2),
            };
            out.push(Token {
                start: r.0,
                end,
                kind: TokenKind::Date,
            });
            i += used;
            continue;
        }
        let kind = if !r.2 {
            TokenKind::Word
        } else if compact_date(&name[r.0..r.1]) {
            TokenKind::Date
        } else if digits(&r, 4) && in_range(name, &r, 1900, 2100) {
            TokenKind::Year
        } else {
            TokenKind::Digits
        };
        out.push(Token {
            start: r.0,
            end: r.1,
            kind,
        });
        i += 1;
    }
    out
}

/// Whether an all-digit run is a date written without separators: `YYYYMM`
/// or `YYYYMMDD`, its year between 1900 and 2100 and its month and day real.
fn compact_date(run: &str) -> bool {
    if run.len() != 6 && run.len() != 8 {
        return false;
    }
    let part = |a: usize, b: usize| run[a..b].parse::<u32>().ok();
    let (Some(y), Some(m)) = (part(0, 4), part(4, 6)) else {
        return false;
    };
    if !(1900..=2100).contains(&y) || !(1..=12).contains(&m) {
        return false;
    }
    match run.len() {
        8 => part(6, 8).is_some_and(|d| (1..=days_in_month(y, m)).contains(&d)),
        _ => true,
    }
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) => {
            29
        }
        2 => 28,
        _ => 31,
    }
}

fn in_range(name: &str, r: &(usize, usize, bool), lo: u32, hi: u32) -> bool {
    name[r.0..r.1]
        .parse::<u32>()
        .is_ok_and(|n| (lo..=hi).contains(&n))
}

/// The name without the token and one separator beside it; `None` when
/// nothing named is left.
fn name_without(name: &str, t: &Token) -> Option<String> {
    let is_sep = |b: u8| !b.is_ascii_alphanumeric() && b.is_ascii();
    let bytes = name.as_bytes();
    let (start, end) = if t.start > 0 && is_sep(bytes[t.start - 1]) {
        (t.start - 1, t.end)
    } else if t.end < bytes.len() && is_sep(bytes[t.end]) {
        (t.start, t.end + 1)
    } else {
        (t.start, t.end)
    };
    let rest = format!("{}{}", &name[..start], &name[end..]);
    rest.chars().any(|c| c.is_alphanumeric()).then_some(rest)
}

/// Words that name a region, so a token of them is a column named `region`.
const REGION_WORDS: &[&str] = &[
    "north",
    "south",
    "east",
    "west",
    "central",
    "northeast",
    "northwest",
    "southeast",
    "southwest",
    "midwest",
];

/// A group's key: the name with the token cut out, what the token is, and
/// the columns with their type families, sorted.
type GroupKey = (String, TokenKind, Vec<(String, &'static str)>);

/// Find the tables of `cols` that combine. Groups are proposed largest first,
/// then a year or date token ahead of a number and a number ahead of a word,
/// then by name; a table joins at most one.
pub fn find_combinations(
    s: &Session,
    cols: &[ColumnEvidence],
) -> Result<Vec<Combination>, EvidenceError> {
    let mut by_table: BTreeMap<&str, Vec<&ColumnEvidence>> = BTreeMap::new();
    for c in cols {
        by_table.entry(c.table.as_str()).or_default().push(c);
    }
    // Every way a table's name can lose one token, keyed with its columns.
    let mut groups: BTreeMap<GroupKey, Vec<(&str, Token)>> = BTreeMap::new();
    for (&table, tcols) in &by_table {
        let mut signature: Vec<(String, &'static str)> = tcols
            .iter()
            .map(|x| (x.column.clone(), type_family(&x.duckdb_type)))
            .collect();
        signature.sort();
        for t in tokens(table) {
            let template = format!("{}\u{0}{}", &table[..t.start], &table[t.end..]);
            groups
                .entry((template, t.kind, signature.clone()))
                .or_default()
                .push((table, t));
        }
    }
    let mut proposed: Vec<(&GroupKey, &Vec<(&str, Token)>)> =
        groups.iter().filter(|(_, m)| m.len() >= 2).collect();
    proposed.sort_by(|a, b| {
        b.1.len()
            .cmp(&a.1.len())
            .then(a.0 .1.cmp(&b.0 .1))
            .then(a.0.cmp(b.0))
    });

    let existing: BTreeSet<&str> = by_table.keys().copied().collect();
    let mut used: BTreeSet<&str> = BTreeSet::new();
    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    for ((_, kind, _), members) in proposed {
        // The members agreeing on every label both sides have, in name order.
        // A column of one value is typed from too little to disagree with,
        // so only its type family counts.
        let mut labels: BTreeMap<&str, &str> = BTreeMap::new();
        let mut parts: Vec<(&str, Token)> = Vec::new();
        for &(table, token) in members {
            if used.contains(table) {
                continue;
            }
            let cols = &by_table[table];
            let agrees = cols
                .iter()
                .all(|c| match (&c.label, labels.get(c.column.as_str())) {
                    (Some(l), Some(m)) => c.distinct == 1 || l == m,
                    _ => true,
                });
            if !agrees {
                continue;
            }
            for c in cols.iter().filter(|c| c.distinct != 1) {
                if let Some(l) = &c.label {
                    labels.entry(c.column.as_str()).or_insert(l.as_str());
                }
            }
            parts.push((table, token));
        }
        if parts.len() < 2 {
            continue;
        }
        let Some(base) = name_without(parts[0].0, &parts[0].1) else {
            continue;
        };
        let name = unique_name(&base, &existing, &names);
        for (t, _) in &parts {
            used.insert(t);
        }
        names.insert(name.clone());
        let values: Vec<(String, String)> = parts
            .iter()
            .map(|(t, tok)| (t.to_string(), t[tok.start..tok.end].to_string()))
            .collect();
        let columns: Vec<String> = by_table[parts[0].0]
            .iter()
            .map(|c| c.column.clone())
            .collect();
        let labelled = labels.len();
        out.push(classify(s, cols, name, *kind, values, columns, labelled)?);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// `base`, or `base_combined` (then with a number) when a table, another
/// combination or one of dovetail's evidence tables already has that name.
fn unique_name(base: &str, existing: &BTreeSet<&str>, taken: &BTreeSet<String>) -> String {
    const RESERVED: &[&str] = &["column_evidence", "edge_evidence", "table_evidence"];
    let free = |n: &str| !existing.contains(n) && !taken.contains(n) && !RESERVED.contains(&n);
    if free(base) {
        return base.to_string();
    }
    let mut n = format!("{base}_combined");
    let mut i = 2;
    while !free(&n) {
        n = format!("{base}_combined_{i}");
        i += 1;
    }
    n
}

/// Decide a group's kind and build its query: duplicate first, then the key
/// overlap that tells a snapshot from a partition.
fn classify(
    s: &Session,
    cols: &[ColumnEvidence],
    name: String,
    kind: TokenKind,
    values: Vec<(String, String)>,
    columns: Vec<String>,
    labelled: usize,
) -> Result<Combination, EvidenceError> {
    let parts: Vec<String> = values.iter().map(|(p, _)| p.clone()).collect();
    let tokens: Vec<&str> = values.iter().map(|(_, v)| v.as_str()).collect();
    let mut evidence = vec![
        format!(
            "{} have the same {} columns, of the same type families",
            parts.join(", "),
            columns.len()
        ),
        format!("their names differ only by {}", tokens.join(", ")),
    ];
    if labelled > 0 {
        evidence.push(format!(
            "their labels agree on the {labelled} labelled columns"
        ));
    }
    let list = columns
        .iter()
        .map(|c| quote(c))
        .collect::<Vec<_>>()
        .join(", ");
    let select = |part: &str| format!("SELECT {list} FROM {SESSION_PREFIX}{}", quote(part));

    // A sampled session's tables hold only the rows drawn, which two tables that differ
    // elsewhere can share: the duplicate test reads the parts' full tables instead.
    let judged = |part: &str| match s.sampled().and(s.full_relation(part)) {
        Some(full) => format!("SELECT {list} FROM {full}"),
        None => select(part),
    };
    // Parts with different row counts cannot be duplicates, so the comparison is skipped.
    let same_size = |p: &String| s.sampled().is_none() || s.full_rows(&parts[0]) == s.full_rows(p);
    let duplicate = parts[1..].iter().try_fold(true, |all, p| {
        if !all || !same_size(p) {
            return Ok(false);
        }
        identical(s, &judged(&parts[0]), &judged(p))
    })?;
    let floor = matches!(s.typing, TypingSource::Floor { .. });
    let key = measure_table(s.conn(), cols, &[], &[], &Dataset::source(&parts[0]), floor)?.key;
    if duplicate {
        evidence.push(format!(
            "their rows are identical: EXCEPT ALL is empty both ways; {} is kept",
            parts[0]
        ));
        return Ok(Combination {
            name,
            source_sql: select(&parts[0]),
            parts,
            kind: CombinationKind::Duplicate,
            token_column: None,
            key,
            evidence,
        });
    }

    let (token_name, ty) = token_name_and_type(kind, &tokens);
    let mut token_name = token_name.to_string();
    while columns.contains(&token_name) {
        token_name = format!("source_{token_name}");
    }
    let values: Vec<(String, String)> = values
        .into_iter()
        .map(|(p, v)| (p, token_value(kind, &v)))
        .collect();
    let arms: Vec<String> = values
        .iter()
        .map(|(p, v)| {
            format!(
                "SELECT {list}, CAST('{}' AS {ty}) AS {} FROM {SESSION_PREFIX}{}",
                v.replace('\'', "''"),
                quote(&token_name),
                quote(p)
            )
        })
        .collect();
    let source_sql = arms.join(" UNION ALL BY NAME ");

    let kind_of = if key.is_empty() {
        evidence.push(format!(
            "{} has no unique key, so the parts are taken as a partition",
            parts[0]
        ));
        CombinationKind::Partition
    } else {
        let overlap = key_overlap(s, &parts, &key)?;
        let shared = format!(
            "the key {} overlaps across parts by {:.0}%",
            key.join(", "),
            overlap * 100.0
        );
        if overlap + 1e-9 < SNAPSHOT_MIN_KEY_OVERLAP {
            evidence.push(format!(
                "{shared}, under {:.0}%",
                SNAPSHOT_MIN_KEY_OVERLAP * 100.0
            ));
            CombinationKind::Partition
        } else if matches!(kind, TokenKind::Word | TokenKind::Digits) {
            // A word such as a region names where the rows are, not when, and
            // a bare number (customers_1, customers_2) numbers shards: the same
            // values in two parts are a key local to each part.
            evidence.push(format!(
                "{shared}, but {token_name} names a part rather than a time, \
                 so the key is local to each part and the parts are a partition"
            ));
            CombinationKind::Partition
        } else if !unique_over(s, &source_sql, &key, &token_name)? {
            evidence.push(format!(
                "{shared}, but {}, {token_name} is not unique and non-null over the combined rows, \
                 so the parts are taken as a partition",
                key.join(", ")
            ));
            CombinationKind::Partition
        } else {
            evidence.push(format!(
                "{shared}, at least {:.0}%; {}, {token_name} is unique over the combined rows",
                SNAPSHOT_MIN_KEY_OVERLAP * 100.0,
                key.join(", ")
            ));
            CombinationKind::Snapshot
        }
    };

    Ok(Combination {
        name,
        parts,
        kind: kind_of,
        token_column: Some(TokenColumn {
            name: token_name,
            duckdb_type: ty.to_string(),
            values,
        }),
        source_sql,
        key,
        evidence,
    })
}

/// Whether `key` plus `token` is non-null and unique on every row of the
/// combined query `source_sql`. One statement.
fn unique_over(
    s: &Session,
    source_sql: &str,
    key: &[String],
    token: &str,
) -> Result<bool, EvidenceError> {
    let cols: Vec<String> = key
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(token))
        .map(quote)
        .collect();
    let non_null = cols
        .iter()
        .map(|c| format!("{c} IS NOT NULL"))
        .collect::<Vec<_>>()
        .join(" AND ");
    let sql = format!(
        "SELECT count(*), count(DISTINCT ({})), count(*) FILTER (WHERE {non_null}) FROM ({source_sql})",
        cols.join(", ")
    );
    let (rows, distinct, filled): (i64, i64, i64) = s
        .conn()
        .query_row(&sql, [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(distinct == rows && filled == rows)
}

fn token_name_and_type(kind: TokenKind, tokens: &[&str]) -> (&'static str, &'static str) {
    match kind {
        TokenKind::Year => ("year", "INTEGER"),
        TokenKind::Date => ("as_of", "DATE"),
        TokenKind::Digits => ("part", "VARCHAR"),
        TokenKind::Word => {
            let region = tokens
                .iter()
                .all(|t| REGION_WORDS.contains(&t.to_ascii_lowercase().as_str()));
            (if region { "region" } else { "part" }, "VARCHAR")
        }
    }
}

/// A date token as an ISO date, `-` joined, the first of the month when it
/// names only a month; any other token as written.
fn token_value(kind: TokenKind, v: &str) -> String {
    if kind != TokenKind::Date {
        return v.to_string();
    }
    let v = if v.bytes().all(|b| b.is_ascii_digit()) {
        // A compact date, YYYYMM or YYYYMMDD.
        let mut iso = format!("{}-{}", &v[..4], &v[4..6]);
        if v.len() == 8 {
            iso.push('-');
            iso.push_str(&v[6..]);
        }
        iso
    } else {
        v.replace('_', "-")
    };
    if v.len() == 7 {
        format!("{v}-01")
    } else {
        v
    }
}

/// The label a token column takes from what its token is.
fn token_label(ty: &str) -> Option<String> {
    match ty {
        "INTEGER" => Some("datetime.component.year".into()),
        "DATE" => Some("datetime.date.iso".into()),
        _ => None,
    }
}

/// Whether two queries return the same rows, counting repeats.
fn identical(s: &Session, a: &str, b: &str) -> Result<bool, EvidenceError> {
    let sql = format!(
        "SELECT (SELECT count(*) FROM ({a} EXCEPT ALL {b})) + (SELECT count(*) FROM ({b} EXCEPT ALL {a}))"
    );
    let n: i64 = s.conn().query_row(&sql, [], |r| r.get(0))?;
    Ok(n == 0)
}

/// The share of each part's distinct key values that some other part also
/// holds, over all the parts' distinct key values. One statement.
fn key_overlap(s: &Session, parts: &[String], key: &[String]) -> Result<f64, EvidenceError> {
    let k = if let [one] = key {
        quote(one)
    } else {
        format!(
            "row({})",
            key.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ")
        )
    };
    let arms: Vec<String> = parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                "SELECT DISTINCT {i} AS i, {k} AS k FROM {SESSION_PREFIX}{}",
                quote(p)
            )
        })
        .collect();
    let sql = format!(
        "SELECT count(*) FILTER (WHERE n > 1), count(*) FROM (
             SELECT count(*) OVER (PARTITION BY k) AS n FROM ({}))",
        arms.join(" UNION ALL ")
    );
    let (shared, total): (i64, i64) = s
        .conn()
        .query_row(&sql, [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(if total == 0 {
        0.0
    } else {
        shared as f64 / total as f64
    })
}

/// Create a view in `memory.main` for each combination, named after it, and in a
/// sampled session a second over its parts' full tables, and return what to measure: the views and every table of `src` that is not a
/// part, sorted by name. A view whose name a kept table already has is an
/// error, so it can never stand in for that table.
pub fn create_views(s: &Session, combos: &[Combination]) -> Result<Vec<Dataset>, EvidenceError> {
    let parts: BTreeSet<&str> = combos
        .iter()
        .flat_map(|c| c.parts.iter().map(String::as_str))
        .collect();
    let mut tables: Vec<String> = list_columns(s.conn(), "src")?
        .into_iter()
        .map(|c| c.table)
        .collect();
    tables.dedup();
    let mut out: Vec<Dataset> = tables
        .iter()
        .filter(|t| !parts.contains(t.as_str()))
        .map(|t| Dataset::source(t))
        .collect();
    for c in combos {
        if out.iter().any(|d| d.name == c.name) {
            return Err(EvidenceError::Session(format!(
                "the combination {} has the name of a table it would hide",
                c.name
            )));
        }
        s.conn().execute_batch(&format!(
            "CREATE OR REPLACE VIEW {VIEW_PREFIX}{} AS {}",
            quote(&c.name),
            c.source_sql
        ))?;
        // A sampled session's view holds sampled rows; this one, over the parts'
        // full tables, is where a join to the combination reads its whole key.
        if s.sampled().is_some() {
            if let Some(sql) = c.source_over(|p| s.full_relation(p).map(str::to_string)) {
                s.conn().execute_batch(&format!(
                    "CREATE SCHEMA IF NOT EXISTS {FULL_VIEW_SCHEMA}; \
                     CREATE OR REPLACE VIEW {FULL_VIEW_PREFIX}{} AS {sql}",
                    quote(&c.name),
                ))?;
            }
        }
        let mut d = Dataset::view(&c.name);
        // The parts the view reads: a duplicate keeps only the first.
        d.parts = match c.kind {
            CombinationKind::Duplicate => c.parts.iter().take(1).cloned().collect(),
            _ => c.parts.clone(),
        };
        if c.kind == CombinationKind::Snapshot {
            if let Some(token) = &c.token_column {
                d.snapshot = Some(SnapshotKey {
                    entity_key: c.key.clone(),
                    token: token.name.clone(),
                });
            }
        }
        out.push(d);
    }
    out.sort();
    Ok(out)
}

/// `cols` and, after them, each combination's view measured: counts over the
/// view, each column labelled as its parts agree, and the token column
/// labelled from its type. The views' rows are added to
/// `memory.column_evidence`, their label source `inherited`. FineType's profile never runs over a view.
pub fn view_column_evidence(
    s: &Session,
    combos: &[Combination],
    cols: &[ColumnEvidence],
) -> Result<Vec<ColumnEvidence>, EvidenceError> {
    let mut added = Vec::new();
    for c in combos {
        let mut stmt = s.conn().prepare(&format!(
            "SELECT column_name, column_type FROM (DESCRIBE {VIEW_PREFIX}{})",
            quote(&c.name)
        ))?;
        let described: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<duckdb::Result<_>>()?;
        let names: Vec<&str> = described.iter().map(|(n, _)| n.as_str()).collect();
        let counts = measure_counts(s.conn(), VIEW_PREFIX, &c.name, &names)?;
        let token = c.token_column.as_ref();
        for ((column, ty), (rows, non_null, distinct)) in described.iter().zip(counts) {
            let label = match token {
                Some(t) if &t.name == column => token_label(&t.duckdb_type),
                _ => agreed_label(cols, &c.parts, column),
            };
            added.push(ColumnEvidence {
                table: c.name.clone(),
                column: column.clone(),
                duckdb_type: ty.clone(),
                rows,
                non_null,
                distinct,
                label,
                audit_name: evidence::is_audit_name(column),
            });
        }
    }
    write_column_evidence(s, &added, false, true)?;
    let mut out = cols.to_vec();
    out.extend(added);
    Ok(out)
}

/// The label a view's column takes from its parts: the first part's label
/// among the parts where the column holds more than one value, which the
/// grouping made agree; else the first part's label at all.
pub fn agreed_label(cols: &[ColumnEvidence], parts: &[String], column: &str) -> Option<String> {
    let of_parts = || {
        parts
            .iter()
            .filter_map(|p| cols.iter().find(|x| &x.table == p && x.column == column))
    };
    of_parts()
        .filter(|x| x.distinct != 1)
        .find_map(|x| x.label.clone())
        .or_else(|| of_parts().find_map(|x| x.label.clone()))
}

/// Score `measured` into edges, taking out each edge to a snapshot's entity
/// key. That edge is no relationship, but when it would hold against the
/// snapshot's distinct key values it is a [`CarriedKey`], and the snapshot's
/// combination names it. An edge that would not hold stays as scored.
pub fn carry_snapshot_keys(
    s: &Session,
    combos: &mut [Combination],
    datasets: &[Dataset],
    cols: &[ColumnEvidence],
    measured: &[EdgeEvidence],
) -> Result<(Vec<Edge>, Vec<CarriedKey>), EvidenceError> {
    let mut edges = Vec::new();
    let mut carried = Vec::new();
    for e in measured {
        let snapshot = datasets.iter().find(|d| {
            d.name == e.parent.table
                && d.snapshot
                    .as_ref()
                    .is_some_and(|k| k.entity_key == [e.parent.column.clone()])
        });
        let Some(d) = snapshot else {
            edges.push(edge_from_evidence(e));
            continue;
        };
        // Against the entity's distinct keys, the parent is unique.
        let mut distinct = e.clone();
        distinct.parent_total = e.parent_distinct;
        let family = cols
            .iter()
            .find(|c| c.table == e.parent.table && c.column == e.parent.column)
            .map(|c| type_family(&c.duckdb_type));
        if needs_key_shape(&distinct) && matches!(family, Some("text" | "int")) {
            distinct.parent_key_shaped = parent_key_shaped(
                s.conn(),
                &d.relation_sql(),
                &e.parent.column,
                family == Some("int"),
            )?;
        }
        let would = edge_from_evidence(&distinct);
        if would.status == EdgeStatus::Rejected {
            edges.push(edge_from_evidence(e));
            continue;
        }
        let key = CarriedKey {
            child: e.child.clone(),
            snapshot: d.name.clone(),
            key: e.parent.column.clone(),
            overlap: would.evidence.value_overlap,
            child_repeats: e.child_distinct < e.child_total,
        };
        if let Some(c) = combos.iter_mut().find(|c| c.name == d.name) {
            c.evidence.push(format!(
                "{} carries the entity key {}, {:.0}% overlap; no relationship, since {} repeats in the snapshot",
                key.child.qualified(),
                key.key,
                key.overlap * 100.0,
                key.key
            ));
        }
        carried.push(key);
    }
    Ok((edges, carried))
}

/// A run measured with combinations in their parts' place.
#[derive(Debug, Clone)]
pub struct CombinedRun {
    pub combinations: Vec<Combination>,
    /// The parts' and untouched tables' column evidence, then the views'.
    pub columns: Vec<ColumnEvidence>,
    /// What was measured: the views and the untouched tables.
    pub datasets: Vec<Dataset>,
    pub edges: Vec<Edge>,
    pub carried: Vec<CarriedKey>,
    /// One per dataset, in `datasets` order.
    pub tables: Vec<TableEvidence>,
}

/// Measure and type the session's columns, combine what combines, and measure
/// joins and tables over the combined datasets in place of their parts. Edge
/// rows are written under [`Pass::Combined`].
pub fn run_combined(s: &Session) -> Result<CombinedRun, EvidenceError> {
    let cols = column_evidence(s)?;
    let combinations = find_combinations(s, &cols)?;
    run_combined_with(s, &cols, combinations, None)
}

/// [`run_combined`] from column evidence and combinations the caller already
/// has, so no column is typed twice. With `parts`, the parts pass's edge
/// rows, a pair of untouched tables is taken from them rather than measured
/// again: only pairs that involve a combination's view are measured.
pub fn run_combined_with(
    s: &Session,
    cols: &[ColumnEvidence],
    mut combinations: Vec<Combination>,
    parts: Option<&[EdgeEvidence]>,
) -> Result<CombinedRun, EvidenceError> {
    let datasets = create_views(s, &combinations)?;
    let columns = view_column_evidence(s, &combinations, cols)?;
    let measured = match parts {
        None => edge_evidence(s, &columns, &datasets, Pass::Combined)?,
        Some(parts) => {
            let kept: BTreeSet<&str> = datasets
                .iter()
                .filter(|d| d.relation == Relation::Source)
                .map(|d| d.name.as_str())
                .collect();
            let untouched =
                |child: &str, parent: &str| kept.contains(child) && kept.contains(parent);
            let reused = parts
                .iter()
                .filter(|e| untouched(&e.child.table, &e.parent.table))
                .cloned()
                .collect();
            edge_evidence_with(s, &columns, &datasets, Pass::Combined, &untouched, reused)?
        }
    };
    let (edges, carried) =
        carry_snapshot_keys(s, &mut combinations, &datasets, &columns, &measured)?;
    let tables = table_evidence_carrying(s, &columns, &edges, &carried, &datasets)?;
    Ok(CombinedRun {
        combinations,
        columns,
        datasets,
        edges,
        carried,
        tables,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cut(name: &str) -> Vec<(String, TokenKind)> {
        tokens(name)
            .iter()
            .filter_map(|t| name_without(name, t).map(|n| (n, t.kind)))
            .collect()
    }

    #[test]
    fn names_lose_one_token() {
        assert_eq!(
            cut("price_list_2024"),
            vec![
                ("list_2024".into(), TokenKind::Word),
                ("price_2024".into(), TokenKind::Word),
                ("price_list".into(), TokenKind::Year),
            ]
        );
        assert_eq!(
            cut("sales_2024-01"),
            vec![
                ("2024-01".into(), TokenKind::Word),
                ("sales".into(), TokenKind::Date),
            ]
        );
        assert_eq!(
            cut("sales2024"),
            vec![
                ("2024".into(), TokenKind::Word),
                ("sales".into(), TokenKind::Year)
            ]
        );
        assert_eq!(cut("orders_7")[1], ("orders".into(), TokenKind::Digits));
        assert_eq!(
            cut("accounts_202401")[1],
            ("accounts".into(), TokenKind::Date)
        );
        assert_eq!(cut("snap_20240229")[1], ("snap".into(), TokenKind::Date));
        assert_eq!(cut("snap_20230229")[1], ("snap".into(), TokenKind::Digits));
        assert_eq!(
            cut("orders_202413")[1],
            ("orders".into(), TokenKind::Digits)
        );
        // A name of one token leaves nothing to call the combination.
        assert!(cut("customers").is_empty());
    }

    #[test]
    fn only_real_dates_are_dates() {
        let kinds = |n: &str| tokens(n).iter().map(|t| t.kind).collect::<Vec<_>>();
        assert_eq!(
            kinds("sales_2024_02_29"),
            vec![TokenKind::Word, TokenKind::Date]
        );
        assert_eq!(
            kinds("sales_2023_02_29"),
            vec![
                TokenKind::Word,
                TokenKind::Year,
                TokenKind::Digits,
                TokenKind::Digits
            ]
        );
        assert_eq!(
            kinds("sales_2024_13"),
            vec![TokenKind::Word, TokenKind::Year, TokenKind::Digits]
        );
    }

    #[test]
    fn dates_and_regions_name_the_token() {
        assert_eq!(token_value(TokenKind::Date, "2024_01"), "2024-01-01");
        assert_eq!(token_value(TokenKind::Date, "2024-01-15"), "2024-01-15");
        assert_eq!(
            token_name_and_type(TokenKind::Word, &["North", "south"]),
            ("region", "VARCHAR")
        );
        assert_eq!(
            token_name_and_type(TokenKind::Word, &["archive", "current"]),
            ("part", "VARCHAR")
        );
        assert_eq!(
            token_name_and_type(TokenKind::Year, &["2024"]),
            ("year", "INTEGER")
        );
    }

    #[test]
    fn taken_names_get_a_suffix() {
        let existing: BTreeSet<&str> = ["sales"].into_iter().collect();
        let taken: BTreeSet<String> = ["sales_combined".to_string()].into_iter().collect();
        assert_eq!(unique_name("sales", &existing, &taken), "sales_combined_2");
        assert_eq!(
            unique_name("table_evidence", &BTreeSet::new(), &BTreeSet::new()),
            "table_evidence_combined"
        );
    }
}
