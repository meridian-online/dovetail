//! The full pass's verification of the sample pass's proposal.
//!
//! The sample pass found its joins and keys on samples. [`verify_full`] measures each one
//! dovetail proposed again on every row and rewrites what the measurement says: a join's
//! evidence, cardinality, grade and warning, a dataset's row count, and a key the full
//! data does not bear out.
//! Nothing is moved, renamed, added or removed, so the analyst's view stays as it was.

use std::collections::BTreeMap;

use super::{Dataset, DecidedBy, Grade, Model, Relationship, Verified};
use crate::evidence::{
    self, edge_evidence_with, is_audit_name, ColumnEvidence, EdgeEvidence, EvidenceError, Pass,
    Session, FULL_PREFIX, VIEW_PREFIX,
};
use crate::relate::{edge_from_evidence, quote, Edge, EdgeStatus, CONF_HIGH};

/// Verify `proposal` on the full data `session` reads: an unsampled session built by
/// [`Session::over`] over the run's source tables.
///
/// For each relationship dovetail decided, the one join is measured again on every row:
/// its `evidence` and `cardinality` are rewritten, its `grade` is `likely` when `relate`
/// would accept it and `possible` otherwise, and `verified` is `full`. A join whose grade
/// falls, or that `relate` would now reject, stays, graded `possible`, with a `warning`
/// naming each cause. For each dataset dovetail decided, `rows` is the full count, and a
/// `primary_key` whose columns repeat or are empty on the full data is removed, with a
/// sentence saying so added to `role_evidence`.
///
/// A combined dataset is read through its `source` query, run over the session's tables.
/// An element that cannot be measured keeps its sample figures: a join among them is
/// graded `possible` with a warning saying why, and a dataset says why in its
/// `role_evidence`. Everything else, and every element the analyst decided, is returned
/// as given.
pub fn verify_full(mut proposal: Model, session: &Session) -> Model {
    let relations = relations(&proposal, session);
    for d in &mut proposal.datasets {
        if d.meta.decided_by != DecidedBy::Dovetail {
            continue;
        }
        let checked = match relations.get(&d.name) {
            Some(Ok(relation)) => verify_dataset(d, &relation.sql, session),
            Some(Err(why)) => Err(why.clone()),
            None => Err("it is no table of this run".to_string()),
        };
        if let Err(why) = checked {
            d.meta
                .role_evidence
                .push(format!("not checked on the full data: {why}"));
        }
    }
    // Each field's FineType label, by dataset and column: the proposal's typing.
    let labels: BTreeMap<(String, String), Option<String>> = proposal
        .datasets
        .iter()
        .flat_map(|d| {
            d.fields
                .iter()
                .map(|f| ((d.name.clone(), f.name.clone()), f.meta.finetype.clone()))
        })
        .collect();
    for r in &mut proposal.relationships {
        if r.meta.decided_by != DecidedBy::Dovetail {
            continue;
        }
        match measure(r, &labels, &relations, session) {
            Ok(measured) => rewrite(r, &measured),
            Err(why) => {
                r.meta.grade = Grade::Possible;
                r.meta.warning = Some(format!("not measured on the full data: {why}"));
            }
        }
    }
    proposal.meta.pass = Some(Verified::Full);
    proposal.meta.generator = format!("dovetail {}", env!("CARGO_PKG_VERSION"));
    proposal
}

/// Where a dataset's full data is read in the session.
struct Relation {
    sql: String,
    dataset: evidence::Dataset,
}

/// Each dataset the session can read: a source table as the session holds it, and a
/// combined dataset as a view over its `source`, created now, or why that view could not
/// be made.
fn relations(proposal: &Model, session: &Session) -> BTreeMap<String, Result<Relation, String>> {
    let mut out = BTreeMap::new();
    for d in &proposal.datasets {
        if let Some(c) = &d.meta.combination {
            let relation = combined_view(session, &d.name, &d.source, &c.parts).map(|sql| {
                let mut dataset = evidence::Dataset::view(&d.name);
                dataset.parts = c.parts.clone();
                Relation { sql, dataset }
            });
            out.insert(d.name.clone(), relation);
        } else if session.full_relation(&d.name).is_some() {
            let dataset = evidence::Dataset::source(&d.name);
            out.insert(
                d.name.clone(),
                Ok(Relation {
                    sql: dataset.relation_sql(),
                    dataset,
                }),
            );
        }
    }
    out
}

/// Create a view named `name` in the session over a combined dataset's `source`, each
/// part's table read as the session holds it, without its lineage columns. The query
/// names each part's table as `FROM "<relation>"`, as the projection writes it. Returns
/// the view's relation, or why it could not be made.
fn combined_view(
    session: &Session,
    name: &str,
    source: &str,
    parts: &[String],
) -> Result<String, String> {
    let mut sql = source.to_string();
    for p in parts {
        let relation = session
            .full_relation(p)
            .and_then(|r| r.strip_prefix(FULL_PREFIX))
            .ok_or_else(|| format!("its part {p} is no table of this run"))?;
        let held = evidence::Dataset::source(p).relation_sql();
        sql = sql.replace(&format!("FROM {relation}"), &format!("FROM {held}"));
    }
    let view = format!("{VIEW_PREFIX}{}", quote(name));
    session
        .conn()
        .execute_batch(&format!("CREATE OR REPLACE VIEW {view} AS {sql}"))
        .map_err(|e| format!("its source does not run: {e}"))?;
    Ok(view)
}

/// The full row count, and the key checked: dropped when a key column is empty in any
/// row, or the key's values repeat among the rows that fill it.
fn verify_dataset(d: &mut Dataset, relation: &str, session: &Session) -> Result<(), String> {
    let conn = session.conn();
    let rows: i64 = conn
        .query_row(&format!("SELECT count(*) FROM {relation}"), [], |r| {
            r.get(0)
        })
        .map_err(|e| format!("its rows could not be counted: {e}"))?;
    d.meta.rows = u64::try_from(rows).ok();
    if d.primary_key.is_empty() {
        return Ok(());
    }
    let unchecked = |e: duckdb::Error| format!("its key could not be checked: {e}");
    let mut failed = Vec::new();
    for column in &d.primary_key {
        let empty: i64 = conn
            .query_row(
                &format!(
                    "SELECT count(*) FROM {relation} WHERE {} IS NULL",
                    quote(column)
                ),
                [],
                |r| r.get(0),
            )
            .map_err(unchecked)?;
        if empty > 0 {
            failed.push(format!(
                "{column} is empty in {} of the full table",
                rows_of(empty)
            ));
        }
    }
    let key: Vec<String> = d.primary_key.iter().map(|c| quote(c)).collect();
    let filled: Vec<String> = key.iter().map(|c| format!("{c} IS NOT NULL")).collect();
    let repeated: i64 = conn
        .query_row(
            &format!(
                "SELECT coalesce(sum(n), 0)::BIGINT FROM \
                 (SELECT count(*) AS n FROM {relation} WHERE {} GROUP BY {} HAVING count(*) > 1)",
                filled.join(" AND "),
                key.join(", ")
            ),
            [],
            |r| r.get(0),
        )
        .map_err(unchecked)?;
    if repeated > 0 {
        failed.push(format!(
            "{} repeats in {} of the full table",
            d.primary_key.join(", "),
            rows_of(repeated)
        ));
    }
    if !failed.is_empty() {
        d.meta.role_evidence.extend(failed);
        d.primary_key.clear();
    }
    Ok(())
}

/// Measure the one join `r` names on the full data.
fn measure(
    r: &Relationship,
    labels: &BTreeMap<(String, String), Option<String>>,
    relations: &BTreeMap<String, Result<Relation, String>>,
    session: &Session,
) -> Result<EdgeEvidence, String> {
    let ([from_column], [to_column]) = (r.from_columns.as_slice(), r.to_columns.as_slice()) else {
        return Err("only a join on one column is measured".to_string());
    };
    let side = |name: &str| match relations.get(name) {
        Some(Ok(relation)) => Ok(relation),
        Some(Err(why)) => Err(format!("{name}: {why}")),
        None => Err(format!("{name} is no table of this run")),
    };
    let (from, to) = (side(&r.from)?, side(&r.to)?);
    let column = |dataset: &str, relation: &Relation, column: &str| {
        let label = labels
            .get(&(dataset.to_string(), column.to_string()))
            .cloned()
            .flatten();
        column_evidence(session, dataset, &relation.sql, column, label)
            .map_err(|e| format!("{dataset}.{column}: {e}"))
    };
    let cols = [
        column(&r.from, from, from_column)?,
        column(&r.to, to, to_column)?,
    ];
    let measured = edge_evidence_with(
        session,
        &cols,
        &[from.dataset.clone(), to.dataset.clone()],
        Pass::Combined,
        &|_, _| false,
        Vec::new(),
    )
    .map_err(|e| e.to_string())?;
    measured
        .into_iter()
        .find(|e| {
            e.child.table == r.from
                && e.child.column == *from_column
                && e.parent.table == r.to
                && e.parent.column == *to_column
        })
        .ok_or_else(|| "the full data no longer makes it a candidate join".to_string())
}

/// One column's counts on the full data, labelled as the proposal typed it.
fn column_evidence(
    session: &Session,
    table: &str,
    relation: &str,
    column: &str,
    label: Option<String>,
) -> Result<ColumnEvidence, EvidenceError> {
    let duckdb_type: String = session.conn().query_row(
        &format!("SELECT column_type FROM (DESCRIBE {relation}) WHERE column_name = ?"),
        [column],
        |r| r.get(0),
    )?;
    let c = quote(column);
    let (rows, non_null, distinct) = session.conn().query_row(
        &format!("SELECT count(*), count({c}), count(DISTINCT {c}) FROM {relation}"),
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    Ok(ColumnEvidence {
        table: table.to_string(),
        column: column.to_string(),
        duckdb_type,
        rows,
        non_null,
        distinct,
        label,
        audit_name: is_audit_name(column),
    })
}

/// Rewrite `r`'s measurements, cardinality and grade from the join measured on the full
/// data. Its types, and whether they agree, are the proposal's typing and stay as they are.
fn rewrite(r: &mut Relationship, measured: &EdgeEvidence) {
    let edge = edge_from_evidence(measured);
    let count = |n: i64| u64::try_from(n).unwrap_or(0);
    let e = &mut r.meta.evidence;
    e.value_overlap = edge.evidence.value_overlap;
    e.orphans = count(edge.verification.orphan_count);
    e.child_rows = count(edge.verification.child_total);
    e.name_similarity = edge.evidence.name_similarity;
    e.parent_duplicates = count(edge.evidence.parent_duplicates);
    r.meta.cardinality =
        if measured.child_total > 0 && measured.child_distinct == measured.child_total {
            "1:1"
        } else {
            "N:1"
        }
        .to_string();
    let grade = match edge.status {
        EdgeStatus::Accepted => Grade::Likely,
        _ => Grade::Possible,
    };
    let fell = r.meta.grade == Grade::Likely && grade == Grade::Possible;
    r.meta.warning = (fell || edge.status == EdgeStatus::Rejected).then(|| warning(r, &edge));
    r.meta.grade = grade;
    r.meta.verified = Verified::Full;
}

/// Why the full data weakens `r`, measured as `edge`: each cause, with its counts.
fn warning(r: &Relationship, edge: &Edge) -> String {
    let mut causes = Vec::new();
    let orphans = edge.verification.orphan_count;
    if orphans > 0 {
        causes.push(format!(
            "{} of {} rows of {} {} at no row of {}",
            grouped(orphans),
            grouped(edge.verification.child_total),
            r.from,
            if orphans == 1 { "points" } else { "point" },
            r.to
        ));
    }
    let duplicates = edge.evidence.parent_duplicates;
    if duplicates > 0 || !edge.verification.parent_unique {
        let key = format!("{}.{}", r.to, r.to_columns.join(", "));
        causes.push(match duplicates {
            0 => format!("{key} is not unique"),
            1 => format!("1 value of {key} is held by more than one row"),
            n => format!(
                "{} values of {key} are held by more than one row",
                grouped(n)
            ),
        });
    }
    if edge.confidence < CONF_HIGH {
        causes.push(format!(
            "its confidence {:.2} is below {CONF_HIGH:.2}",
            edge.confidence
        ));
    }
    if edge.evidence.semantic_mismatch {
        causes.push(format!(
            "its columns' types disagree ({} and {})",
            edge.evidence
                .child_semantic_type
                .as_deref()
                .unwrap_or("untyped"),
            edge.evidence
                .parent_semantic_type
                .as_deref()
                .unwrap_or("untyped")
        ));
    }
    if causes.is_empty() {
        causes.push(edge.reason.clone());
    }
    format!("on the full data, {}", causes.join("; "))
}

/// `n` rows, grouped: `1 row`, `4,812 rows`.
fn rows_of(n: i64) -> String {
    if n == 1 {
        "1 row".to_string()
    } else {
        format!("{} rows", grouped(n))
    }
}

/// `n` with its thousands grouped by commas: `4,812`.
fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(d);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}
