//! roles — what each table is for, scored from evidence rows.
//!
//! [`score_table`] is a pure function: it reads one table's [`TableEvidence`],
//! `relate`'s edges and any snapshot keys the table carries, and gives every
//! [`Role`] a score in `[0, 1]` with the evidence sentences behind it. Each
//! score is a weighted sum of the evidence for that role minus the evidence
//! against it, clamped. The table takes the top score unless it is under
//! [`ROLE_FLOOR`] or within [`ROLE_MARGIN`] of the runner-up, when it is
//! [`Role::Unclear`]; every score is kept either way.
//!
//! "Points at" means an accepted or suggested edge whose parent is unique
//! ([`points_at`]); nothing else is role evidence. A [`CarriedKey`] from the
//! table counts as a pointer out, with its own repeat test.

use std::collections::BTreeSet;

use crate::evidence::is_audit_name;
use crate::finetype_ext::TypingSource;
use crate::relate::{singularise, ColumnRef, Edge, EdgeStatus};
pub use crate::role_key::{CombinationKind, Role};

// --- Thresholds --------------------------------------------------------------

/// The lowest winning score that names a role.
pub const ROLE_FLOOR: f64 = 0.5;
/// How far the winner must lead the runner-up to name a role.
pub const ROLE_MARGIN: f64 = 0.15;
/// The most rows a reference has.
pub const REFERENCE_MAX_ROWS: i64 = 1000;
/// The most columns a reference has besides its key.
pub const REFERENCE_MAX_LABEL_COLUMNS: usize = 4;
/// The fill rate a time column needs to time an event.
pub const EVENT_MIN_TIME_FILL: f64 = 0.9;
/// The most distinct dates, as a share of rows, a snapshot's date takes.
pub const SNAPSHOT_MAX_DATE_RATIO: f64 = 0.01;
/// The least share of a bridge's columns that are pointers.
pub const BRIDGE_MIN_POINTER_SHARE: f64 = 0.5;

/// Slack for comparing sums of weights against thresholds.
const EPS: f64 = 1e-9;

// --- Weights: entity (all zero without a key) --------------------------------

/// A unique, non-null key: one row per thing.
pub const ENTITY_KEY: f64 = 0.4;
/// Another table points at the key, or FineType types it as an identifier.
pub const ENTITY_KNOWN_KEY: f64 = 0.25;
/// Columns that describe the thing (places, dates, amounts), not just name it.
pub const ENTITY_ATTRIBUTES: f64 = 0.2;
/// Its timestamps, if any, are all audit-like.
pub const ENTITY_AUDIT_TIMES_ONLY: f64 = 0.15;
/// Against: a timed column and a repeating pointer out, the shape of an event.
pub const ENTITY_AGAINST_EVENT_SHAPE: f64 = 0.5;
/// Against: the snapshot signals; its rows are states as of a date.
pub const ENTITY_AGAINST_SNAPSHOT_SHAPE: f64 = 0.3;
/// Against: isolation. A table nothing points at, that points at nothing and
/// refers to nothing in itself fits no role the evidence can name. Added for
/// the retail fixture's promo_codes, which its answer key calls unclear.
pub const ENTITY_AGAINST_ISOLATED: f64 = 0.4;

// --- Weights: event ----------------------------------------------------------

/// A non-audit time column at least [`EVENT_MIN_TIME_FILL`] filled.
pub const EVENT_TIME: f64 = 0.4;
/// A pointer out whose values repeat, at times that differ within a parent.
pub const EVENT_REPEATING_POINTER: f64 = 0.4;
/// More rows than a table it points at.
pub const EVENT_OUTNUMBERS_PARENT: f64 = 0.2;
/// Against: the snapshot signals.
pub const EVENT_AGAINST_SNAPSHOT_SHAPE: f64 = 0.6;
/// Against: no pointer out (edge or carried key); an event happens to something.
pub const EVENT_AGAINST_NO_POINTER: f64 = 0.4;

// --- Weights: reference (all zero without a key) -----------------------------

/// At most [`REFERENCE_MAX_ROWS`] rows.
pub const REFERENCE_SMALL: f64 = 0.25;
/// A key plus at most [`REFERENCE_MAX_LABEL_COLUMNS`] columns, labels only.
pub const REFERENCE_LABELS_ONLY: f64 = 0.4;
/// Something points at it.
pub const REFERENCE_POINTED_AT: f64 = 0.35;
/// Against: a non-audit time column.
pub const REFERENCE_AGAINST_TIME: f64 = 0.4;
/// Against: a pointer out; a lookup labels, it does not refer.
pub const REFERENCE_AGAINST_POINTER_OUT: f64 = 0.4;
/// Against: attribute columns, which make it a small entity.
pub const REFERENCE_AGAINST_ATTRIBUTES: f64 = 0.4;
/// Against: isolation, as [`ENTITY_AGAINST_ISOLATED`]; a lookup nothing uses
/// is not a lookup.
pub const REFERENCE_AGAINST_ISOLATED: f64 = 0.4;

// --- Weights: bridge (all zero with fewer than two pointers out) -------------

/// Two or more pointers out.
pub const BRIDGE_POINTERS: f64 = 0.4;
/// Pointers are at least [`BRIDGE_MIN_POINTER_SHARE`] of its columns.
pub const BRIDGE_POINTER_SHARE: f64 = 0.3;
/// The pointer tuple is its unique key.
pub const BRIDGE_UNIQUE_TUPLE: f64 = 0.3;
/// Against: something points at it.
pub const BRIDGE_AGAINST_POINTED_AT: f64 = 0.5;
/// Against: a non-audit time column; that is an event with several pointers.
pub const BRIDGE_AGAINST_TIME: f64 = 0.5;

// --- Weights: snapshot (all zero without a snapshot signal) ------------------

/// A repeating key unique with a date of few distinct values, or a key that
/// is a unique pointer with a constant as-of date.
pub const SNAPSHOT_DATED_KEY: f64 = 0.7;
/// The key, less its date, points at an entity.
pub const SNAPSHOT_ENTITY_KEY: f64 = 0.3;
/// Against: another date that varies per row, the shape of an event.
pub const SNAPSHOT_AGAINST_VARYING_DATE: f64 = 0.5;

// --- Types -------------------------------------------------------------------

/// A date or time column, as measured.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeColumn {
    pub name: String,
    pub fill_rate: f64,
    pub distinct: i64,
    /// Records when a row was written rather than when anything happened.
    pub audit: bool,
    /// Takes more than one value within one value of the table's pointer.
    pub varies_within_parent: bool,
}

/// What dovetail measured about one table.
#[derive(Debug, Clone, PartialEq)]
pub struct TableEvidence {
    pub name: String,
    pub rows: i64,
    pub columns: Vec<String>,
    /// The unique column or pair; empty when none is unique.
    pub key: Vec<String>,
    pub key_is_identifier: bool,
    pub time_columns: Vec<TimeColumn>,
    pub label_columns: Vec<String>,
    pub attribute_columns: Vec<String>,
    /// A date column with one distinct value.
    pub constant_date: Option<String>,
    /// Columns whose values all lie in the table's own single-column key,
    /// such as `reports_to`; sorted.
    pub self_references: Vec<String>,
}

/// One table's role, every role's score, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleCall {
    pub table: String,
    pub role: Role,
    /// All six roles, in the enum's order. [`Role::Unclear`] scores zero: it
    /// is what a table gets when no other role wins.
    pub scores: Vec<(Role, f64)>,
    pub evidence: Vec<String>,
    pub primary_key: Vec<String>,
    /// The event's time or the snapshot's date; `None` for other roles.
    pub time_field: Option<String>,
    pub audit_time_fields: Vec<String>,
    /// `profiled` or `floor: <reason>`.
    pub typing: String,
}

/// A child column carrying a snapshot's entity key: no relationship, since
/// the key is not unique in the snapshot, but a pointer out for scoring.
#[derive(Debug, Clone, PartialEq)]
pub struct CarriedKey {
    pub child: ColumnRef,
    pub snapshot: String,
    pub key: String,
    pub overlap: f64,
    /// Whether the child's key values repeat across its rows.
    pub child_repeats: bool,
}

/// The edges that count as role evidence: accepted or suggested, to a unique
/// parent.
pub fn points_at(edges: &[Edge]) -> impl Iterator<Item = &Edge> {
    edges.iter().filter(|e| {
        matches!(e.status, EdgeStatus::Accepted | EdgeStatus::Suggested)
            && e.verification.parent_unique
    })
}

/// A reference proposed folded into the one table that uses it. dovetail
/// records the fold; the silver step that materialises the model compiles it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fold {
    pub reference: String,
    pub host: String,
    /// The host's column that carries the reference's key.
    pub child_column: String,
    /// The reference's key column.
    pub parent_column: String,
    /// The one struct column the reference's labels fold into.
    pub struct_column: String,
}

/// The fold rule: a reference folds when there is one pointing column of one
/// table. Two tables pointing at it make it shared; two columns of one table
/// make it a role-playing lookup (origin and destination); one column reaching
/// two of its keys is ambiguous. Either way it stays a dataset of its own.
fn foldable(pointers: &[&Edge]) -> bool {
    let mut columns: Vec<(&str, &str, &str)> = pointers
        .iter()
        .map(|e| {
            (
                e.child.table.as_str(),
                e.child.column.as_str(),
                e.parent.column.as_str(),
            )
        })
        .collect();
    columns.sort_unstable();
    columns.dedup();
    columns.len() == 1
}

/// The folds to propose, one per foldable reference, sorted by reference.
pub fn propose_folds(calls: &[RoleCall], edges: &[Edge]) -> Vec<Fold> {
    let mut folds: Vec<Fold> = Vec::new();
    for call in calls.iter().filter(|c| c.role == Role::Reference) {
        let pointers: Vec<&Edge> = points_at(edges)
            .filter(|e| e.parent.table == call.table && e.child.table != call.table)
            .collect();
        if !foldable(&pointers) {
            continue;
        }
        let e = pointers[0];
        folds.push(Fold {
            reference: call.table.clone(),
            host: e.child.table.clone(),
            child_column: e.child.column.clone(),
            parent_column: e.parent.column.clone(),
            struct_column: singularise(&call.table),
        });
    }
    folds.sort_by(|a, b| a.reference.cmp(&b.reference));
    folds
}

/// A pointer out of the table, from an edge or a carried key.
struct Pointer {
    column: String,
    repeats: bool,
    /// The parent's rows, when known (an edge's, not a carried key's).
    parent_rows: Option<i64>,
    sentence: String,
}

/// Why a table reads as a snapshot.
enum SnapshotSignal {
    /// The key is a repeating column plus `date`, which takes few values.
    DatedKey { date: String, distinct: i64 },
    /// The key is a unique pointer, and `date` is the same on every row.
    AsOf { date: String },
}

impl SnapshotSignal {
    fn date(&self) -> &str {
        match self {
            SnapshotSignal::DatedKey { date, .. } | SnapshotSignal::AsOf { date } => date,
        }
    }
}

const ROLES: [Role; 5] = [
    Role::Entity,
    Role::Event,
    Role::Reference,
    Role::Bridge,
    Role::Snapshot,
];

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Entity => "entity",
        Role::Event => "event",
        Role::Reference => "reference",
        Role::Bridge => "bridge",
        Role::Snapshot => "snapshot",
        Role::Unclear => "unclear",
    }
}

fn list(items: &[String]) -> String {
    items.join(", ")
}

fn percent(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

/// Score one table's role from its evidence.
pub fn score_table(
    t: &TableEvidence,
    edges: &[Edge],
    carried: &[CarriedKey],
    typing: &TypingSource,
) -> RoleCall {
    let typing_str = match typing {
        TypingSource::Profiled { .. } => "profiled".to_string(),
        TypingSource::Floor { reason } => format!("floor: {reason}"),
    };
    let mut evidence = vec![match typing {
        TypingSource::Profiled {
            extension_version, ..
        } => format!("typing: profiled by {extension_version}"),
        TypingSource::Floor { reason } => format!("typing: floor ({reason})"),
    }];
    let audit_time_fields: Vec<String> = t
        .time_columns
        .iter()
        .filter(|c| is_audit(c))
        .map(|c| c.name.clone())
        .collect();

    if t.rows <= 0 {
        evidence.push(format!("{} has no rows", t.name));
        return RoleCall {
            table: t.name.clone(),
            role: Role::Unclear,
            scores: ROLES
                .iter()
                .chain(std::iter::once(&Role::Unclear))
                .map(|r| (*r, 0.0))
                .collect(),
            evidence,
            primary_key: t.key.clone(),
            time_field: None,
            audit_time_fields,
            typing: typing_str,
        };
    }

    // --- Measure ------------------------------------------------------------
    let rows = t.rows;
    let has_key = !t.key.is_empty();
    let inbound: Vec<String> = points_at(edges)
        .filter(|e| e.parent.table == t.name && e.child.table != t.name)
        .map(|e| e.child.table.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let pointers = pointers_out(t, edges, carried);
    let pointer_columns: BTreeSet<&str> = pointers.iter().map(|p| p.column.as_str()).collect();
    let non_audit: Vec<&TimeColumn> = t.time_columns.iter().filter(|c| !is_audit(c)).collect();
    let event_times: Vec<&TimeColumn> = non_audit
        .iter()
        .copied()
        .filter(|c| c.fill_rate + EPS >= EVENT_MIN_TIME_FILL)
        .collect();
    // Audit timestamps label nothing and describe nothing.
    let describing: Vec<String> = t
        .attribute_columns
        .iter()
        .filter(|a| !audit_time_fields.contains(a))
        .cloned()
        .collect();
    let repeating = pointers.iter().any(|p| p.repeats);
    let varies = event_times.iter().any(|c| c.varies_within_parent);
    let outnumbers = pointers
        .iter()
        .any(|p| p.parent_rows.is_some_and(|n| rows > n));
    let snapshot = snapshot_signal(t, &pointer_columns);
    let snapshot_date = snapshot.as_ref().map(|s| s.date().to_string());
    let snapshot_by_entity = snapshot.is_some() && {
        let rest: Vec<&String> = t
            .key
            .iter()
            .filter(|k| Some(k.as_str()) != snapshot_date.as_deref())
            .collect();
        !rest.is_empty() && rest.iter().all(|k| pointer_columns.contains(k.as_str()))
    };
    let varying_other_date = non_audit.iter().any(|c| {
        Some(c.name.as_str()) != snapshot_date.as_deref()
            && c.distinct as f64 / rows as f64 > SNAPSHOT_MAX_DATE_RATIO
    });
    let non_key_columns = t.columns.len().saturating_sub(t.key.len());
    // A self-reference only exempts a table from isolation; it earns nothing.
    let isolated = inbound.is_empty() && pointers.is_empty() && t.self_references.is_empty();

    // --- Evidence -----------------------------------------------------------
    if has_key {
        let mut s = format!("{} unique across {rows} rows", list(&t.key));
        if !inbound.is_empty() {
            s.push_str(&format!("; pointed at by {}", list(&inbound)));
        }
        evidence.push(s);
        if t.key_is_identifier {
            evidence.push(format!("{} typed as an identifier", list(&t.key)));
        }
    } else {
        evidence.push(format!(
            "no column or pair of columns is unique across {rows} rows"
        ));
    }
    evidence.extend(pointers.iter().map(|p| p.sentence.clone()));
    evidence.extend(
        carried
            .iter()
            .filter(|c| c.snapshot == t.name && c.child.table != t.name)
            .map(|c| {
                format!(
                    "{} carries the key {} of this snapshot, {} overlap",
                    c.child.qualified(),
                    c.key,
                    percent(c.overlap)
                )
            }),
    );
    if let [key] = t.key.as_slice() {
        evidence.extend(
            t.self_references
                .iter()
                .map(|c| format!("{c} refers to {key} in the same table")),
        );
    }
    if isolated {
        evidence.push(format!(
            "nothing points at {}, and it points at nothing",
            t.name
        ));
    }
    for c in &t.time_columns {
        if is_audit(c) {
            evidence.push(format!("{} is an audit timestamp", c.name));
        } else {
            let mut s = format!(
                "{} is a time column, {} filled, {} distinct",
                c.name,
                percent(c.fill_rate),
                c.distinct
            );
            if c.varies_within_parent {
                s.push_str(", varies within a parent");
            }
            evidence.push(s);
        }
    }
    if !t.label_columns.is_empty() {
        evidence.push(format!("label columns: {}", list(&t.label_columns)));
    }
    if !describing.is_empty() {
        evidence.push(format!("attribute columns: {}", list(&describing)));
    }
    match &snapshot {
        Some(SnapshotSignal::DatedKey { date, distinct }) => evidence.push(format!(
            "{} unique while {date} takes {distinct} distinct values across {rows} rows",
            list(&t.key)
        )),
        Some(SnapshotSignal::AsOf { date }) => evidence.push(format!(
            "{date} is one date on every row, and the key {} is a pointer",
            list(&t.key)
        )),
        None => {}
    }

    // --- Score --------------------------------------------------------------
    let credit = |cond: bool, w: f64| if cond { w } else { 0.0 };

    let entity = if has_key {
        ENTITY_KEY
            + credit(!inbound.is_empty() || t.key_is_identifier, ENTITY_KNOWN_KEY)
            + credit(!describing.is_empty(), ENTITY_ATTRIBUTES)
            + credit(non_audit.is_empty(), ENTITY_AUDIT_TIMES_ONLY)
            - credit(
                !event_times.is_empty() && repeating,
                ENTITY_AGAINST_EVENT_SHAPE,
            )
            - credit(snapshot.is_some(), ENTITY_AGAINST_SNAPSHOT_SHAPE)
            - credit(isolated, ENTITY_AGAINST_ISOLATED)
    } else {
        0.0
    };
    let event = credit(!event_times.is_empty(), EVENT_TIME)
        + credit(repeating && varies, EVENT_REPEATING_POINTER)
        + credit(outnumbers, EVENT_OUTNUMBERS_PARENT)
        - credit(snapshot.is_some(), EVENT_AGAINST_SNAPSHOT_SHAPE)
        - credit(pointers.is_empty(), EVENT_AGAINST_NO_POINTER);
    let reference = if has_key {
        let labels_only = !t.label_columns.is_empty()
            && describing.is_empty()
            && non_key_columns <= REFERENCE_MAX_LABEL_COLUMNS;
        credit(rows <= REFERENCE_MAX_ROWS, REFERENCE_SMALL)
            + credit(labels_only, REFERENCE_LABELS_ONLY)
            + credit(!inbound.is_empty(), REFERENCE_POINTED_AT)
            - credit(!non_audit.is_empty(), REFERENCE_AGAINST_TIME)
            - credit(!pointers.is_empty(), REFERENCE_AGAINST_POINTER_OUT)
            - credit(!describing.is_empty(), REFERENCE_AGAINST_ATTRIBUTES)
            - credit(isolated, REFERENCE_AGAINST_ISOLATED)
    } else {
        0.0
    };
    let bridge = if pointer_columns.len() >= 2 {
        let share = pointer_columns.len() as f64 / t.columns.len().max(1) as f64;
        let tuple = t.key.len() >= 2 && t.key.iter().all(|k| pointer_columns.contains(k.as_str()));
        BRIDGE_POINTERS
            + credit(
                share + EPS >= BRIDGE_MIN_POINTER_SHARE,
                BRIDGE_POINTER_SHARE,
            )
            + credit(tuple, BRIDGE_UNIQUE_TUPLE)
            - credit(!inbound.is_empty(), BRIDGE_AGAINST_POINTED_AT)
            - credit(!non_audit.is_empty(), BRIDGE_AGAINST_TIME)
    } else {
        0.0
    };
    let snapshot_score = if snapshot.is_some() {
        SNAPSHOT_DATED_KEY + credit(snapshot_by_entity, SNAPSHOT_ENTITY_KEY)
            - credit(varying_other_date, SNAPSHOT_AGAINST_VARYING_DATE)
    } else {
        0.0
    };

    let raw = [entity, event, reference, bridge, snapshot_score];
    let mut scores: Vec<(Role, f64)> = ROLES.iter().zip(raw).map(|(r, s)| (*r, clamp(s))).collect();

    // --- Decide -------------------------------------------------------------
    // The first highest in the enum's order wins a tie.
    let mut top = 0;
    for i in 1..scores.len() {
        if scores[i].1 > scores[top].1 + EPS {
            top = i;
        }
    }
    let (top_role, top_score) = scores[top];
    let (second_role, second_score) = scores
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != top)
        .map(|(_, s)| *s)
        .fold((Role::Unclear, f64::NEG_INFINITY), |best, s| {
            if s.1 > best.1 + EPS {
                s
            } else {
                best
            }
        });
    // A lead of exactly ROLE_MARGIN counts as clear; EPS absorbs float noise.
    let role = if top_score + EPS < ROLE_FLOOR {
        evidence.push(format!(
            "unclear: the top score, {} {top_score:.2}, is under {ROLE_FLOOR}",
            role_name(top_role)
        ));
        Role::Unclear
    } else if top_score - second_score + EPS < ROLE_MARGIN {
        evidence.push(format!(
            "unclear: {} {top_score:.2} and {} {second_score:.2} are within {ROLE_MARGIN}",
            role_name(top_role),
            role_name(second_role)
        ));
        Role::Unclear
    } else {
        evidence.push(format!(
            "{} {top_score:.2}, ahead of {} {second_score:.2}",
            role_name(top_role),
            role_name(second_role)
        ));
        top_role
    };
    scores.push((Role::Unclear, 0.0));

    let time_field = match role {
        Role::Event => event_times
            .iter()
            .find(|c| c.varies_within_parent)
            .or(event_times.first())
            .map(|c| c.name.clone()),
        Role::Snapshot => snapshot_date,
        _ => None,
    };

    RoleCall {
        table: t.name.clone(),
        role,
        scores,
        evidence,
        primary_key: t.key.clone(),
        time_field,
        audit_time_fields,
        typing: typing_str,
    }
}

fn is_audit(c: &TimeColumn) -> bool {
    c.audit || is_audit_name(&c.name)
}

/// Clamp to `[0, 1]`, rounding off the float noise of summing weights.
fn clamp(s: f64) -> f64 {
    ((s * 1e6).round() / 1e6).clamp(0.0, 1.0)
}

/// The table's pointers out, sorted by column and then target.
fn pointers_out(t: &TableEvidence, edges: &[Edge], carried: &[CarriedKey]) -> Vec<Pointer> {
    let mut out: Vec<(String, String, Pointer)> = Vec::new();
    for e in points_at(edges).filter(|e| e.child.table == t.name && e.parent.table != t.name) {
        let repeats = e.evidence.child_distinct < e.verification.child_total;
        let mut sentence = format!(
            "{} points at {}; {} distinct values across {} rows",
            e.child.column,
            e.parent.qualified(),
            e.evidence.child_distinct,
            e.verification.child_total
        );
        if repeats {
            sentence.push_str(", values repeat");
        }
        out.push((
            e.child.column.clone(),
            e.parent.qualified(),
            Pointer {
                column: e.child.column.clone(),
                repeats,
                parent_rows: Some(e.evidence.parent_total),
                sentence,
            },
        ));
    }
    for c in carried.iter().filter(|c| c.child.table == t.name) {
        let mut sentence = format!(
            "{} carries the key {} of snapshot {}, {} overlap",
            c.child.column,
            c.key,
            c.snapshot,
            percent(c.overlap)
        );
        if c.child_repeats {
            sentence.push_str(", values repeat");
        }
        out.push((
            c.child.column.clone(),
            format!("{}.{}", c.snapshot, c.key),
            Pointer {
                column: c.child.column.clone(),
                repeats: c.child_repeats,
                parent_rows: None,
                sentence,
            },
        ));
    }
    out.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    out.into_iter().map(|(_, _, p)| p).collect()
}

/// The snapshot signals: a pair key holding a date of few distinct values
/// (the other part repeats, or it would be the key alone), or a key that is a
/// unique pointer with a constant as-of date.
fn snapshot_signal(t: &TableEvidence, pointer_columns: &BTreeSet<&str>) -> Option<SnapshotSignal> {
    if t.key.len() >= 2 {
        for k in &t.key {
            if let Some(c) = t.time_columns.iter().find(|c| &c.name == k && !is_audit(c)) {
                if c.distinct as f64 / t.rows as f64 <= SNAPSHOT_MAX_DATE_RATIO + EPS {
                    return Some(SnapshotSignal::DatedKey {
                        date: c.name.clone(),
                        distinct: c.distinct,
                    });
                }
            }
        }
    }
    if let (Some(date), [key]) = (&t.constant_date, t.key.as_slice()) {
        if pointer_columns.contains(key.as_str()) {
            return Some(SnapshotSignal::AsOf { date: date.clone() });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relate::{Evidence, Verification};

    const ALL: [Role; 6] = [
        Role::Entity,
        Role::Event,
        Role::Reference,
        Role::Bridge,
        Role::Snapshot,
        Role::Unclear,
    ];

    fn profiled() -> TypingSource {
        TypingSource::Profiled {
            extension_version: "finetype 0.0.0".into(),
            model_sha256: "abc".into(),
        }
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn table(name: &str, rows: i64, columns: &[&str], key: &[&str]) -> TableEvidence {
        TableEvidence {
            name: name.into(),
            rows,
            columns: strings(columns),
            key: strings(key),
            key_is_identifier: false,
            time_columns: vec![],
            label_columns: vec![],
            attribute_columns: vec![],
            constant_date: None,
            self_references: vec![],
        }
    }

    fn time(name: &str, distinct: i64, audit: bool, varies: bool) -> TimeColumn {
        TimeColumn {
            name: name.into(),
            fill_rate: 1.0,
            distinct,
            audit,
            varies_within_parent: varies,
        }
    }

    /// An edge `child.cc → parent.pc`. `child_distinct` of `child_total`
    /// non-null child values; the parent has `parent_total` rows.
    #[allow(clippy::too_many_arguments)]
    fn edge(
        child: (&str, &str),
        parent: (&str, &str),
        status: EdgeStatus,
        parent_unique: bool,
        child_distinct: i64,
        child_total: i64,
        parent_total: i64,
    ) -> Edge {
        Edge {
            child: ColumnRef {
                table: child.0.into(),
                column: child.1.into(),
            },
            parent: ColumnRef {
                table: parent.0.into(),
                column: parent.1.into(),
            },
            evidence: Evidence {
                name_similarity: 1.0,
                semantic_similarity: 0.0,
                value_overlap: 1.0,
                parent_key_likeness: 1.0,
                child_semantic_type: None,
                parent_semantic_type: None,
                semantic_mismatch: false,
                child_distinct,
                parent_distinct: parent_total,
                parent_total,
            },
            verification: Verification {
                orphan_count: 0,
                child_total,
                parent_unique,
            },
            confidence: 0.9,
            status,
            reason: String::new(),
        }
    }

    fn score(call: &RoleCall, role: Role) -> f64 {
        call.scores.iter().find(|(r, _)| *r == role).unwrap().1
    }

    fn check_shape(call: &RoleCall) {
        let roles: Vec<Role> = call.scores.iter().map(|(r, _)| *r).collect();
        assert_eq!(roles, ALL, "all six roles, in the enum's order");
        for (r, s) in &call.scores {
            assert!((0.0..=1.0).contains(s), "{r:?} score {s} out of [0,1]");
        }
    }

    fn expect(call: &RoleCall, role: Role) {
        check_shape(call);
        assert_eq!(
            call.role, role,
            "{}: scores {:?}, evidence {:#?}",
            call.table, call.scores, call.evidence
        );
    }

    fn customers_with_inbound() -> (TableEvidence, Vec<Edge>) {
        let mut t = table(
            "customers",
            400,
            &["customer_id", "name", "email", "city"],
            &["customer_id"],
        );
        t.key_is_identifier = true;
        t.label_columns = strings(&["name"]);
        t.attribute_columns = strings(&["email", "city"]);
        let edges = vec![
            edge(
                ("transactions", "customer_id"),
                ("customers", "customer_id"),
                EdgeStatus::Accepted,
                true,
                380,
                5000,
                400,
            ),
            edge(
                ("loyalty_visits", "customer_id"),
                ("customers", "customer_id"),
                EdgeStatus::Suggested,
                true,
                200,
                900,
                400,
            ),
        ];
        (t, edges)
    }

    #[test]
    fn entity_with_inbound_pointers() {
        let (t, edges) = customers_with_inbound();
        let call = score_table(&t, &edges, &[], &profiled());
        expect(&call, Role::Entity);
        assert_eq!(call.primary_key, vec!["customer_id"]);
        assert!(
            call.evidence.iter().any(|e| {
                e
                == "customer_id unique across 400 rows; pointed at by loyalty_visits, transactions"
            }),
            "{:#?}",
            call.evidence
        );
        assert_eq!(call.typing, "profiled");
        assert_eq!(call.time_field, None);
    }

    #[test]
    fn entity_with_audit_timestamp_is_not_event() {
        let mut t = table(
            "customers",
            400,
            &["customer_id", "name", "city", "home_store_id", "created_at"],
            &["customer_id"],
        );
        t.key_is_identifier = true;
        t.label_columns = strings(&["name"]);
        t.attribute_columns = strings(&["city", "created_at"]);
        t.time_columns = vec![time("created_at", 400, true, false)];
        // A repeating pointer out: what makes an event is a timed row per
        // parent, and an audit timestamp does not time anything.
        let edges = vec![edge(
            ("customers", "home_store_id"),
            ("stores", "store_id"),
            EdgeStatus::Accepted,
            true,
            12,
            400,
            12,
        )];
        let call = score_table(&t, &edges, &[], &profiled());
        expect(&call, Role::Entity);
        assert_eq!(call.audit_time_fields, vec!["created_at"]);
        assert_eq!(call.time_field, None);
        assert!(score(&call, Role::Event) < ROLE_FLOOR, "{:?}", call.scores);
    }

    #[test]
    fn audit_name_alone_marks_audit() {
        let mut t = table(
            "customers",
            400,
            &["customer_id", "city", "updated_at"],
            &["customer_id"],
        );
        t.key_is_identifier = true;
        t.attribute_columns = strings(&["city", "updated_at"]);
        t.time_columns = vec![time("updated_at", 400, false, false)];
        let edges = vec![edge(
            ("transactions", "customer_id"),
            ("customers", "customer_id"),
            EdgeStatus::Accepted,
            true,
            380,
            5000,
            400,
        )];
        let call = score_table(&t, &edges, &[], &profiled());
        expect(&call, Role::Entity);
        assert_eq!(call.audit_time_fields, vec!["updated_at"]);
        assert_eq!(score(&call, Role::Entity), 1.0, "{:?}", call.scores);
    }

    #[test]
    fn event_with_repeating_pointer() {
        let mut t = table(
            "transactions",
            5000,
            &["transaction_id", "customer_id", "purchased_at", "amount"],
            &["transaction_id"],
        );
        t.key_is_identifier = true;
        t.attribute_columns = strings(&["purchased_at", "amount"]);
        t.time_columns = vec![time("purchased_at", 4900, false, true)];
        let edges = vec![edge(
            ("transactions", "customer_id"),
            ("customers", "customer_id"),
            EdgeStatus::Accepted,
            true,
            380,
            5000,
            400,
        )];
        let call = score_table(&t, &edges, &[], &profiled());
        expect(&call, Role::Event);
        assert_eq!(call.time_field.as_deref(), Some("purchased_at"));
        assert!(score(&call, Role::Event) - score(&call, Role::Entity) >= ROLE_MARGIN);
    }

    #[test]
    fn reference_small_label_lookup() {
        let mut t = table("categories", 8, &["category_id", "name"], &["category_id"]);
        t.key_is_identifier = true;
        t.label_columns = strings(&["name"]);
        let edges = vec![edge(
            ("products", "category_id"),
            ("categories", "category_id"),
            EdgeStatus::Accepted,
            true,
            8,
            120,
            8,
        )];
        let call = score_table(&t, &edges, &[], &profiled());
        expect(&call, Role::Reference);
    }

    #[test]
    fn small_store_list_is_entity() {
        let mut t = table(
            "stores",
            12,
            &["store_id", "name", "city", "region"],
            &["store_id"],
        );
        t.key_is_identifier = true;
        t.label_columns = strings(&["name"]);
        t.attribute_columns = strings(&["city", "region"]);
        let edges = vec![
            edge(
                ("transactions", "store_id"),
                ("stores", "store_id"),
                EdgeStatus::Accepted,
                true,
                12,
                5000,
                12,
            ),
            edge(
                ("loyalty_visits", "store_id"),
                ("stores", "store_id"),
                EdgeStatus::Accepted,
                true,
                12,
                900,
                12,
            ),
        ];
        let call = score_table(&t, &edges, &[], &profiled());
        expect(&call, Role::Entity);
        assert!(score(&call, Role::Reference) < ROLE_FLOOR);
    }

    fn product_tags_edges(child: &str) -> Vec<Edge> {
        vec![
            edge(
                (child, "product_id"),
                ("products", "product_id"),
                EdgeStatus::Accepted,
                true,
                100,
                300,
                120,
            ),
            edge(
                (child, "tag_id"),
                ("tags", "tag_id"),
                EdgeStatus::Accepted,
                true,
                20,
                300,
                25,
            ),
        ]
    }

    #[test]
    fn bridge_two_pointers_unique_pair() {
        let t = table(
            "product_tags",
            300,
            &["product_id", "tag_id"],
            &["product_id", "tag_id"],
        );
        let call = score_table(&t, &product_tags_edges("product_tags"), &[], &profiled());
        expect(&call, Role::Bridge);
        assert_eq!(call.primary_key, vec!["product_id", "tag_id"]);
    }

    #[test]
    fn event_with_two_pointers_is_not_bridge() {
        let mut t = table(
            "product_views",
            300,
            &["product_id", "tag_id", "viewed_at"],
            &[],
        );
        t.attribute_columns = strings(&["viewed_at"]);
        t.time_columns = vec![time("viewed_at", 295, false, true)];
        let call = score_table(&t, &product_tags_edges("product_views"), &[], &profiled());
        expect(&call, Role::Event);
        assert!(call.primary_key.is_empty());
        assert!(score(&call, Role::Bridge) < ROLE_FLOOR);
    }

    #[test]
    fn snapshot_repeating_key_few_dates() {
        let mut t = table(
            "price_history",
            3000,
            &["sku", "as_of", "price"],
            &["sku", "as_of"],
        );
        t.attribute_columns = strings(&["as_of", "price"]);
        t.time_columns = vec![time("as_of", 3, false, true)];
        let call = score_table(&t, &[], &[], &profiled());
        expect(&call, Role::Snapshot);
        assert_eq!(call.time_field.as_deref(), Some("as_of"));
    }

    #[test]
    fn one_date_snapshot() {
        let mut t = table(
            "customer_status",
            400,
            &["customer_id", "status", "as_of"],
            &["customer_id"],
        );
        t.key_is_identifier = true;
        t.attribute_columns = strings(&["status", "as_of"]);
        t.time_columns = vec![time("as_of", 1, false, false)];
        t.constant_date = Some("as_of".into());
        let edges = vec![edge(
            ("customer_status", "customer_id"),
            ("customers", "customer_id"),
            EdgeStatus::Accepted,
            true,
            400,
            400,
            400,
        )];
        let call = score_table(&t, &edges, &[], &profiled());
        expect(&call, Role::Snapshot);
        assert_eq!(call.time_field.as_deref(), Some("as_of"));
    }

    #[test]
    fn snapshot_signals_count_against_event() {
        // customer_segments: one row per customer per quarter-end.
        let mut t = table(
            "customer_segments",
            1200,
            &["customer_id", "as_of", "segment"],
            &["customer_id", "as_of"],
        );
        t.attribute_columns = strings(&["as_of", "segment"]);
        t.time_columns = vec![time("as_of", 3, false, true)];
        let edges = vec![edge(
            ("customer_segments", "customer_id"),
            ("customers", "customer_id"),
            EdgeStatus::Accepted,
            true,
            400,
            1200,
            400,
        )];
        let call = score_table(&t, &edges, &[], &profiled());
        expect(&call, Role::Snapshot);
        assert!(
            score(&call, Role::Snapshot) - score(&call, Role::Event) >= ROLE_MARGIN,
            "{:?}",
            call.scores
        );
    }

    fn employees() -> TableEvidence {
        let mut t = table(
            "employees",
            8,
            &[
                "employee_id",
                "last_name",
                "first_name",
                "title",
                "reports_to",
                "hire_date",
            ],
            &["employee_id"],
        );
        t.attribute_columns = strings(&[
            "last_name",
            "first_name",
            "title",
            "reports_to",
            "hire_date",
        ]);
        t.time_columns = vec![time("hire_date", 8, false, false)];
        t.self_references = strings(&["reports_to"]);
        t
    }

    #[test]
    fn self_reference_still_entity() {
        let floor = TypingSource::Floor {
            reason: "floor requested".into(),
        };
        let call = score_table(&employees(), &[], &[], &floor);
        expect(&call, Role::Entity);
        assert!(score(&call, Role::Event) < ROLE_FLOOR, "{:?}", call.scores);
    }

    #[test]
    fn self_edge_is_not_a_pointer() {
        let floor = TypingSource::Floor {
            reason: "floor requested".into(),
        };
        let edges = vec![edge(
            ("employees", "reports_to"),
            ("employees", "employee_id"),
            EdgeStatus::Suggested,
            true,
            3,
            7,
            8,
        )];
        let call = score_table(&employees(), &edges, &[], &floor);
        expect(&call, Role::Entity);
        assert!(score(&call, Role::Event) < ROLE_FLOOR, "{:?}", call.scores);
        assert!(
            !call
                .evidence
                .iter()
                .any(|e| e.contains("pointed at") || e.contains("points at")),
            "{:#?}",
            call.evidence
        );
    }

    #[test]
    fn timed_table_with_no_pointer_scores_no_event() {
        // A keyless log with a well-filled time and nothing it points at: the
        // time earns EVENT_TIME and the missing pointer takes it back.
        let mut t = table("audit_log", 500, &["message", "logged_on"], &[]);
        t.attribute_columns = strings(&["message", "logged_on"]);
        t.time_columns = vec![time("logged_on", 480, false, false)];
        let call = score_table(&t, &[], &[], &profiled());
        check_shape(&call);
        assert_eq!(score(&call, Role::Event), 0.0, "{:?}", call.scores);

        // One repeating pointer out lifts the penalty; the time does not vary
        // within it, so only the time and the outnumbered parent count.
        let edges = vec![edge(
            ("audit_log", "user_id"),
            ("users", "user_id"),
            EdgeStatus::Accepted,
            true,
            40,
            500,
            50,
        )];
        t.columns.push("user_id".into());
        let call = score_table(&t, &edges, &[], &profiled());
        assert!(
            (score(&call, Role::Event) - (EVENT_TIME + EVENT_OUTNUMBERS_PARENT)).abs() < 1e-6,
            "{:?}",
            call.scores
        );
    }

    #[test]
    fn empty_table_is_unclear() {
        let t = table("archive", 0, &["id", "note"], &[]);
        let call = score_table(&t, &[], &[], &profiled());
        expect(&call, Role::Unclear);
        assert!(
            call.evidence.iter().any(|e| e.contains("no rows")),
            "{:#?}",
            call.evidence
        );
        assert!(call.scores.iter().all(|(_, s)| *s == 0.0));
    }

    #[test]
    fn close_scores_are_unclear() {
        // A large code list something points at, its key untyped: past the
        // reference's row limit it reads as a lookup and as an entity about
        // equally.
        let mut t = table("codes", 2000, &["code_id", "description"], &["code_id"]);
        t.label_columns = strings(&["description"]);
        let edges = vec![edge(
            ("claims", "code_id"),
            ("codes", "code_id"),
            EdgeStatus::Accepted,
            true,
            1500,
            9000,
            2000,
        )];
        let call = score_table(&t, &edges, &[], &profiled());
        expect(&call, Role::Unclear);
        let mut s: Vec<f64> = call.scores.iter().map(|(_, s)| *s).collect();
        s.sort_by(|a, b| b.partial_cmp(a).unwrap());
        assert!(s[0] >= ROLE_FLOOR, "{:?}", call.scores);
        assert!(s[0] - s[1] < ROLE_MARGIN, "{:?}", call.scores);
        assert!(score(&call, Role::Entity) > 0.0 && score(&call, Role::Reference) > 0.0);
    }

    #[test]
    fn isolated_table_is_unclear() {
        // promo_codes: a keyed code list with a discount and an expiry date,
        // and nothing points at it or from it.
        let mut t = table(
            "promo_codes",
            20,
            &["code", "discount_pct", "valid_to"],
            &["code"],
        );
        t.key_is_identifier = true;
        t.attribute_columns = strings(&["discount_pct", "valid_to"]);
        t.time_columns = vec![time("valid_to", 20, false, false)];
        let call = score_table(&t, &[], &[], &profiled());
        expect(&call, Role::Unclear);
        assert!(
            call.evidence
                .iter()
                .any(|e| e == "nothing points at promo_codes, and it points at nothing"),
            "{:#?}",
            call.evidence
        );
    }

    #[test]
    fn self_reference_exempts_from_isolation() {
        let floor = TypingSource::Floor {
            reason: "floor requested".into(),
        };
        let with = score_table(&employees(), &[], &[], &floor);
        let mut alone = employees();
        alone.self_references.clear();
        let without = score_table(&alone, &[], &[], &floor);
        expect(&with, Role::Entity);
        expect(&without, Role::Unclear);
        assert!(
            (score(&with, Role::Entity) - score(&without, Role::Entity) - ENTITY_AGAINST_ISOLATED)
                .abs()
                < 1e-6,
            "{:?} against {:?}",
            with.scores,
            without.scores
        );
        assert!(
            with.evidence
                .iter()
                .any(|e| e == "reports_to refers to employee_id in the same table"),
            "{:#?}",
            with.evidence
        );
    }

    #[test]
    fn floor_typing_is_named() {
        let (t, edges) = customers_with_inbound();
        let call = score_table(
            &t,
            &edges,
            &[],
            &TypingSource::Floor {
                reason: "extension missing".into(),
            },
        );
        assert_eq!(call.typing, "floor: extension missing");
        assert!(
            call.evidence.iter().any(|e| e.contains("floor")),
            "{:#?}",
            call.evidence
        );
    }

    #[test]
    fn suggested_edge_to_non_unique_parent_ignored() {
        let edges = vec![
            edge(
                ("transactions", "customer_id"),
                ("loyalty_visits", "customer_id"),
                EdgeStatus::Suggested,
                false,
                380,
                5000,
                900,
            ),
            edge(
                ("transactions", "store_id"),
                ("stores", "store_id"),
                EdgeStatus::Rejected,
                true,
                12,
                5000,
                12,
            ),
            edge(
                ("transactions", "customer_id"),
                ("customers", "customer_id"),
                EdgeStatus::Suggested,
                true,
                380,
                5000,
                400,
            ),
        ];
        let kept: Vec<String> = points_at(&edges).map(|e| e.parent.qualified()).collect();
        assert_eq!(kept, vec!["customers.customer_id"]);

        // The non-unique parent gains no inbound pointer from it.
        let mut lv = table(
            "loyalty_visits",
            900,
            &["visit_id", "customer_id"],
            &["visit_id"],
        );
        lv.key_is_identifier = true;
        let call = score_table(&lv, &edges, &[], &profiled());
        assert!(
            !call.evidence.iter().any(|e| e.contains("pointed at")),
            "{:#?}",
            call.evidence
        );

        // The child side points only along the kept edge.
        let tx = table(
            "transactions",
            5000,
            &["transaction_id", "customer_id", "store_id"],
            &["transaction_id"],
        );
        let call = score_table(&tx, &edges, &[], &profiled());
        let pointing: Vec<&String> = call
            .evidence
            .iter()
            .filter(|e| e.contains("points at"))
            .collect();
        assert_eq!(pointing.len(), 1, "{:#?}", call.evidence);
        assert!(
            pointing[0].contains("customers.customer_id"),
            "{pointing:?}"
        );
    }

    #[test]
    fn carried_key_counts_as_pointer() {
        let mut t = table("member_visits", 2000, &["member_id", "visited_at"], &[]);
        t.attribute_columns = strings(&["visited_at"]);
        t.time_columns = vec![time("visited_at", 1990, false, true)];
        let carried = vec![CarriedKey {
            child: ColumnRef {
                table: "member_visits".into(),
                column: "member_id".into(),
            },
            snapshot: "members".into(),
            key: "member_id".into(),
            overlap: 0.98,
            child_repeats: true,
        }];
        let call = score_table(&t, &[], &carried, &profiled());
        expect(&call, Role::Event);
        assert_eq!(call.time_field.as_deref(), Some("visited_at"));
        assert!(
            call.evidence.iter().any(|e| e.contains("members")),
            "{:#?}",
            call.evidence
        );
    }

    fn call(table: &str, role: Role) -> RoleCall {
        RoleCall {
            table: table.into(),
            role,
            scores: vec![],
            evidence: vec![],
            primary_key: vec![],
            time_field: None,
            audit_time_fields: vec![],
            typing: "profiled".into(),
        }
    }

    #[test]
    fn single_pointer_reference_folds() {
        let edges = [edge(
            ("products", "category_id"),
            ("categories", "category_id"),
            EdgeStatus::Accepted,
            true,
            8,
            120,
            8,
        )];
        let calls = [
            call("categories", Role::Reference),
            call("products", Role::Entity),
        ];
        assert_eq!(
            propose_folds(&calls, &edges),
            vec![Fold {
                reference: "categories".into(),
                host: "products".into(),
                child_column: "category_id".into(),
                parent_column: "category_id".into(),
                struct_column: "category".into(),
            }]
        );
    }

    #[test]
    fn shared_reference_stays() {
        let e = |t: &str| {
            edge(
                (t, "category_id"),
                ("categories", "category_id"),
                EdgeStatus::Accepted,
                true,
                8,
                100,
                8,
            )
        };
        let edges = [e("products"), e("promotions")];
        let calls = [call("categories", Role::Reference)];
        assert!(propose_folds(&calls, &edges).is_empty());
    }

    #[test]
    fn two_columns_of_one_table_stay_a_reference() {
        let e = |c: &str| {
            edge(
                ("products", c),
                ("categories", "category_id"),
                EdgeStatus::Suggested,
                true,
                8,
                100,
                8,
            )
        };
        let edges = [e("main_id"), e("alt_id")];
        let calls = [call("categories", Role::Reference)];
        assert!(propose_folds(&calls, &edges).is_empty());
    }

    #[test]
    fn one_column_reaching_two_parent_keys_stays_a_reference() {
        let e = |p: &str| {
            edge(
                ("products", "category_id"),
                ("categories", p),
                EdgeStatus::Suggested,
                true,
                8,
                100,
                8,
            )
        };
        let edges = [e("id"), e("sort_order")];
        let calls = [call("categories", Role::Reference)];
        assert!(propose_folds(&calls, &edges).is_empty());
    }

    #[test]
    fn non_reference_never_folds() {
        let edges = [edge(
            ("orders", "customer_id"),
            ("customers", "customer_id"),
            EdgeStatus::Accepted,
            true,
            50,
            500,
            50,
        )];
        for role in [
            Role::Entity,
            Role::Event,
            Role::Bridge,
            Role::Snapshot,
            Role::Unclear,
        ] {
            assert!(propose_folds(&[call("customers", role)], &edges).is_empty());
        }
    }

    #[test]
    fn unpointed_or_rejected_reference_does_not_fold() {
        let rejected = [edge(
            ("products", "category_id"),
            ("categories", "category_id"),
            EdgeStatus::Rejected,
            true,
            8,
            120,
            8,
        )];
        let calls = [call("categories", Role::Reference)];
        assert!(propose_folds(&calls, &rejected).is_empty());
        assert!(propose_folds(&calls, &[]).is_empty());
    }
}
