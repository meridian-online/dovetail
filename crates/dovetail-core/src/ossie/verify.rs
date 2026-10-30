//! The full pass's verification of the sample pass's proposal.
//!
//! The sample pass found its joins and keys on samples. [`verify_full`] measures each one
//! dovetail proposed again on every row and rewrites what the measurement says: a join's
//! evidence, grade and warning, a dataset's row count, and a key the full data repeats.
//! Nothing is moved, renamed, added or removed, so the analyst's view stays as it was.

use std::collections::BTreeMap;

use super::{Dataset, DecidedBy, Grade, Model, Relationship, Verified};
use crate::evidence::{
    self, edge_evidence_with, is_audit_name, ColumnEvidence, EdgeEvidence, EvidenceError, Pass,
    Session, FULL_PREFIX, VIEW_PREFIX,
};
use crate::relate::{edge_from_evidence, quote, EdgeStatus};

/// Verify `proposal` on the full data `session` reads: an unsampled session built by
/// [`Session::over`] over the run's source tables.
///
/// For each relationship dovetail decided, the one join is measured again on every row:
/// its `evidence` is rewritten, its `grade` is `likely` when `relate` would accept it and
/// `possible` otherwise, and `verified` is `full`. A join `relate` would now reject stays,
/// graded `possible`, with a `warning` giving the counts that failed. For each dataset
/// dovetail decided, `rows` is the full count, and a `primary_key` the full data repeats
/// is removed, with a sentence saying so added to `role_evidence`.
///
/// A combined dataset is read through its `source` query, run over the session's tables.
/// An element that cannot be measured keeps its sample figures; a join among them is
/// graded `possible` with a warning saying why. Everything else, and every element the
/// analyst decided, is returned as given.
pub fn verify_full(mut proposal: Model, session: &Session) -> Model {
    let relations = relations(&proposal, session);
    for d in &mut proposal.datasets {
        if d.meta.decided_by == DecidedBy::Dovetail {
            if let Some(relation) = relations.get(&d.name) {
                verify_dataset(d, &relation.sql, session);
            }
        }
    }
    let datasets = proposal.datasets.clone();
    for r in &mut proposal.relationships {
        if r.meta.decided_by != DecidedBy::Dovetail {
            continue;
        }
        match measure(r, &datasets, &relations, session) {
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
/// combined dataset as a view over its `source`, created now.
fn relations(proposal: &Model, session: &Session) -> BTreeMap<String, Relation> {
    let mut out = BTreeMap::new();
    for d in &proposal.datasets {
        if let Some(c) = &d.meta.combination {
            if let Some(sql) = combined_view(session, &d.name, &d.source, &c.parts) {
                let mut dataset = evidence::Dataset::view(&d.name);
                dataset.parts = c.parts.clone();
                out.insert(d.name.clone(), Relation { sql, dataset });
            }
        } else if session.full_relation(&d.name).is_some() {
            let dataset = evidence::Dataset::source(&d.name);
            out.insert(
                d.name.clone(),
                Relation {
                    sql: dataset.relation_sql(),
                    dataset,
                },
            );
        }
    }
    out
}

/// Create a view named `name` in the session over a combined dataset's `source`, each
/// part's table read as the session holds it, without its lineage columns. The query
/// names each part's table as `FROM "<relation>"`, as the projection writes it. Returns
/// the view's relation, or `None` when a part is not in the session or the view fails.
fn combined_view(session: &Session, name: &str, source: &str, parts: &[String]) -> Option<String> {
    let mut sql = source.to_string();
    for p in parts {
        let relation = session.full_relation(p)?.strip_prefix(FULL_PREFIX)?;
        let held = evidence::Dataset::source(p).relation_sql();
        sql = sql.replace(&format!("FROM {relation}"), &format!("FROM {held}"));
    }
    let view = format!("{VIEW_PREFIX}{}", quote(name));
    session
        .conn()
        .execute_batch(&format!("CREATE OR REPLACE VIEW {view} AS {sql}"))
        .ok()?;
    Some(view)
}

/// The full row count, and the key checked for uniqueness.
fn verify_dataset(d: &mut Dataset, relation: &str, session: &Session) {
    if let Ok(rows) =
        session
            .conn()
            .query_row(&format!("SELECT count(*) FROM {relation}"), [], |r| {
                r.get::<_, i64>(0)
            })
    {
        d.meta.rows = u64::try_from(rows).ok();
    }
    if d.primary_key.is_empty() {
        return;
    }
    let key: Vec<String> = d.primary_key.iter().map(|c| quote(c)).collect();
    let repeated = session.conn().query_row(
        &format!(
            "SELECT coalesce(sum(n), 0)::BIGINT FROM \
             (SELECT count(*) AS n FROM {relation} GROUP BY {} HAVING count(*) > 1)",
            key.join(", ")
        ),
        [],
        |r| r.get::<_, i64>(0),
    );
    if let Ok(n) = repeated {
        if n > 0 {
            d.meta.role_evidence.push(format!(
                "{} repeats in {} rows of the full table",
                d.primary_key.join(", "),
                grouped(n)
            ));
            d.primary_key.clear();
        }
    }
}

/// Measure the one join `r` names on the full data.
fn measure(
    r: &Relationship,
    datasets: &[Dataset],
    relations: &BTreeMap<String, Relation>,
    session: &Session,
) -> Result<EdgeEvidence, String> {
    let ([from_column], [to_column]) = (r.from_columns.as_slice(), r.to_columns.as_slice()) else {
        return Err("only a join on one column is measured".to_string());
    };
    let side = |name: &str| {
        relations
            .get(name)
            .ok_or_else(|| format!("{name} is not a table of this run"))
    };
    let (from, to) = (side(&r.from)?, side(&r.to)?);
    let column = |dataset: &str, relation: &Relation, column: &str| {
        let label = datasets
            .iter()
            .find(|d| d.name == dataset)
            .and_then(|d| d.fields.iter().find(|f| f.name == column))
            .and_then(|f| f.meta.finetype.clone());
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

/// Rewrite `r`'s measurements and grade from the join measured on the full data. Its
/// types, and whether they agree, are the proposal's typing and stay as they are.
fn rewrite(r: &mut Relationship, measured: &EdgeEvidence) {
    let edge = edge_from_evidence(measured);
    let count = |n: i64| u64::try_from(n).unwrap_or(0);
    let e = &mut r.meta.evidence;
    e.value_overlap = edge.evidence.value_overlap;
    e.orphans = count(edge.verification.orphan_count);
    e.child_rows = count(edge.verification.child_total);
    e.name_similarity = edge.evidence.name_similarity;
    e.parent_duplicates = count(edge.evidence.parent_duplicates);
    r.meta.grade = match edge.status {
        EdgeStatus::Accepted => Grade::Likely,
        _ => Grade::Possible,
    };
    r.meta.warning = (edge.status == EdgeStatus::Rejected).then(|| {
        let mut failed = Vec::new();
        if edge.verification.orphan_count > 0 {
            failed.push(format!(
                "{} of {} rows of {} point at no row of {}",
                grouped(edge.verification.orphan_count),
                grouped(edge.verification.child_total),
                r.from,
                r.to
            ));
        }
        if edge.evidence.parent_duplicates > 0 {
            failed.push(format!(
                "{} values of {}.{} are held by more than one row",
                grouped(edge.evidence.parent_duplicates),
                r.to,
                r.to_columns.join(", ")
            ));
        }
        if failed.is_empty() {
            failed.push(edge.reason.clone());
        }
        format!("on the full data, {}", failed.join("; "))
    });
    r.meta.verified = Verified::Full;
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
