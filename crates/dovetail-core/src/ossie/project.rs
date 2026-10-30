//! The projection from table roles' [`RoleModel`] to an Ossie [`Model`].
//!
//! The model is a view of what the run decided, not a second analysis: every role,
//! key, fold, combination and join in it is one `roles` already settled. Nothing here
//! measures a table beyond reading back what the session holds (column types and
//! labels, row counts) and the combination views' columns.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

use super::{
    Combination, Dataset, DatasetMeta, Datatype, DecidedBy, Evidence, Expression, Field, FieldMeta,
    Grade, Model, ModelMeta, Relationship, RelationshipMeta, Role, Typing, Verified, OSSIE_VERSION,
};
use crate::combine::{self, view_column_evidence, CombinationKind};
use crate::evidence::{
    column_evidence, list_columns, ColumnEvidence, EvidenceError, Session, SESSION_PREFIX,
    VIEW_PREFIX,
};
use crate::finetype_ext::TypingSource;
use crate::packages::SourceTable;
use crate::relate::{quote, Edge, EdgeStatus};
use crate::role_key::Role as KeyRole;
use crate::roles::{RoleCall, RoleModel};

/// What produced a model, written into it as its provenance.
#[derive(Debug, Clone)]
pub struct Provenance {
    /// `Sample` or `Full`: how much of the data the evidence was measured on.
    pub pass: Verified,
    /// Where the column types came from.
    pub typing: TypingSource,
    /// The sample size the session was asked for; `None` when it read every row.
    pub sample_rows: Option<u64>,
}

/// Each dataset's columns, in order.
type Columns = BTreeMap<String, Vec<Column>>;

/// One column as the model needs it: its DuckDB type and FineType label.
struct Column {
    name: String,
    duckdb_type: String,
    label: Option<String>,
}

/// Project `model`, the roles `roles::run_session` settled over `session`, to an Ossie
/// model called `name`.
///
/// `tables` are the source tables the session was built from. `session` must be the one
/// the role model was run over: the combinations' views live in it, and it holds the
/// column types and row counts. Datasets and relationships are sorted by name.
///
/// A dataset's `source` is the table's relation as the analyst's database names it
/// (`raw_customers`, quoted only where SQL needs it), never the session's `"full".main.`
/// attachment, which exists only inside dovetail. A combination's `source` is its query
/// with each `src` name replaced by that quoted relation, so it runs against the
/// analyst's database alone.
///
/// # Errors
///
/// When the session's catalog cannot be read back.
pub fn project(
    model: &RoleModel,
    tables: &[SourceTable],
    session: &Session,
    name: &str,
    provenance: &Provenance,
) -> Result<Model, EvidenceError> {
    let (columns, counted) = read_columns(session, &model.combinations)?;
    let ctx = Context {
        model,
        tables,
        session,
        columns,
        counted,
        reserved: reserved_words(session)?,
    };

    let in_combination: BTreeMap<&str, &combine::Combination> = model
        .combinations
        .iter()
        .flat_map(|c| c.parts.iter().map(move |p| (p.as_str(), c)))
        .collect();

    let mut datasets: Vec<Dataset> = Vec::new();
    for t in tables {
        match in_combination.get(t.name.as_str()) {
            Some(c) => datasets.push(ctx.part(t, c)),
            None => datasets.push(ctx.table(t)),
        }
    }
    for c in &model.combinations {
        datasets.push(ctx.combined(c));
    }
    datasets.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Model {
        version: OSSIE_VERSION.to_string(),
        name: name.to_string(),
        description: None,
        datasets,
        relationships: ctx.relationships(provenance.pass),
        metrics: Vec::new(),
        meta: ModelMeta {
            generator: format!("dovetail {}", env!("CARGO_PKG_VERSION")),
            pass: Some(provenance.pass),
            typing: Some(typing_of(&provenance.typing)),
            sample_rows: provenance.sample_rows,
            ..ModelMeta::default()
        },
        extensions: Vec::new(),
        extras: Default::default(),
    })
}

fn typing_of(typing: &TypingSource) -> Typing {
    let (source, finetype_tag, model_hash) = match typing {
        TypingSource::Packages => ("packages", None, None),
        TypingSource::Floor { .. } => ("floor", None, None),
        TypingSource::Profiled {
            extension_version,
            model_sha256,
        } => (
            "profiled",
            // `finetype 0.6.64` is release `v0.6.64`.
            extension_version
                .strip_prefix("finetype ")
                .map(|v| format!("v{v}")),
            Some(model_sha256.clone()),
        ),
    };
    Typing {
        source: source.to_string(),
        finetype_tag,
        model_hash,
        extra: Map::new(),
    }
}

struct Context<'a> {
    model: &'a RoleModel,
    tables: &'a [SourceTable],
    session: &'a Session,
    /// Each dataset's columns, in order.
    columns: Columns,
    /// Each table's rows as the evidence counted them.
    counted: BTreeMap<String, i64>,
    /// DuckDB's keywords that cannot stand bare, lower case: a name among them is quoted in SQL.
    reserved: BTreeSet<String>,
}

impl Context<'_> {
    fn call(&self, table: &str) -> Option<&RoleCall> {
        self.model.tables.iter().find(|c| c.table == table)
    }

    /// Say on `meta` which table `name` folds into, and as which struct column, when the
    /// roles proposed folding it. A combination can be a fold's reference as a table can.
    fn fold(&self, meta: &mut DatasetMeta, name: &str) {
        if let Some(f) = self.model.folds.iter().find(|f| f.reference == name) {
            meta.folded_into = Some(f.host.clone());
            meta.fold_column = Some(f.struct_column.clone());
        }
    }

    /// An untouched source table.
    fn table(&self, t: &SourceTable) -> Dataset {
        let call = self.call(&t.name);
        let mut meta = self.meta(call, t.source_file.clone().unwrap_or_default());
        meta.source_part = t.source_part.clone();
        meta.rows = self.rows(&[t.name.as_str()]);
        self.fold(&mut meta, &t.name);
        Dataset {
            name: t.name.clone(),
            source: self.sql_name(&t.relation),
            description: None,
            primary_key: call.map(|c| c.primary_key.clone()).unwrap_or_default(),
            fields: self.fields(&t.name, &[t.name.as_str()], call),
            metrics: Vec::new(),
            meta,
            extensions: Vec::new(),
            extras: Default::default(),
        }
    }

    /// A part of a combination: a dataset of its own that says what it was combined into.
    fn part(&self, t: &SourceTable, c: &combine::Combination) -> Dataset {
        let call = self.call(&c.name);
        let mut meta = self.meta(call, t.source_file.clone().unwrap_or_default());
        meta.role_evidence = vec![format!("combined into {}", c.name)];
        meta.combined_into = Some(c.name.clone());
        meta.source_part = t.source_part.clone();
        meta.rows = self.rows(&[t.name.as_str()]);
        Dataset {
            name: t.name.clone(),
            source: self.sql_name(&t.relation),
            description: None,
            primary_key: Vec::new(),
            fields: self.fields(&t.name, &[t.name.as_str()], call),
            metrics: Vec::new(),
            meta,
            extensions: Vec::new(),
            extras: Default::default(),
        }
    }

    /// A combination: a dataset that is a query over its parts' tables.
    fn combined(&self, c: &combine::Combination) -> Dataset {
        let call = self.call(&c.name);
        let files: Vec<String> = c
            .parts
            .iter()
            .filter_map(|p| self.table_of(p).and_then(|t| t.source_file.clone()))
            .collect();
        let mut meta = self.meta(call, files.join(", "));
        meta.combination = Some(Combination {
            kind: match c.kind {
                CombinationKind::Partition => "partition",
                CombinationKind::Snapshot => "snapshot",
                CombinationKind::Duplicate => "duplicate",
            }
            .to_string(),
            parts: c.parts.clone(),
            token_column: c
                .token_column
                .as_ref()
                .map(|t| t.name.clone())
                .unwrap_or_default(),
            extra: Map::new(),
        });
        // The parts the view reads: a duplicate keeps only the first.
        let read: Vec<&str> = match c.kind {
            CombinationKind::Duplicate => c.parts.iter().take(1).map(String::as_str).collect(),
            _ => c.parts.iter().map(String::as_str).collect(),
        };
        meta.rows = self.rows(&read);
        self.fold(&mut meta, &c.name);
        Dataset {
            name: c.name.clone(),
            source: self.combined_source(c),
            description: None,
            primary_key: call.map(|c| c.primary_key.clone()).unwrap_or_default(),
            fields: self.fields(
                &c.name,
                &c.parts.iter().map(String::as_str).collect::<Vec<_>>(),
                call,
            ),
            metrics: Vec::new(),
            meta,
            extensions: Vec::new(),
            extras: Default::default(),
        }
    }

    /// The combination's query with each `src` name written as the part's relation in
    /// the analyst's database.
    fn combined_source(&self, c: &combine::Combination) -> String {
        let mut sql = c.source_sql.clone();
        for p in &c.parts {
            let relation = self.table_of(p).map_or(p.as_str(), |t| t.relation.as_str());
            sql = sql.replace(&format!("{SESSION_PREFIX}{}", quote(p)), &quote(relation));
        }
        sql
    }

    /// A name as SQL: bare when it is a plain identifier DuckDB does not reserve, else quoted.
    fn sql_name(&self, name: &str) -> String {
        if simple(name) && !self.reserved.contains(&name.to_ascii_lowercase()) {
            name.to_string()
        } else {
            quote(name)
        }
    }

    fn table_of(&self, name: &str) -> Option<&SourceTable> {
        self.tables.iter().find(|t| t.name == name)
    }

    /// The full row count of the named source tables, summed. Without one measured by the
    /// session (an attached database, not a built one), the rows the evidence counted.
    fn rows(&self, names: &[&str]) -> Option<u64> {
        let mut total = 0i64;
        for n in names {
            total += self
                .session
                .full_rows(n)
                .or_else(|| self.counted.get(*n).copied())?;
        }
        u64::try_from(total).ok()
    }

    fn meta(&self, call: Option<&RoleCall>, source_file: String) -> DatasetMeta {
        let Some(call) = call else {
            return DatasetMeta {
                decided_by: DecidedBy::Dovetail,
                source_file,
                ..DatasetMeta::default()
            };
        };
        let scores: Map<String, Value> = call
            .scores
            .iter()
            .map(|(r, s)| {
                (
                    role_name(*r).to_string(),
                    json!((s * 10_000.0).round() / 10_000.0),
                )
            })
            .collect();
        DatasetMeta {
            role: role_of(call.role),
            role_evidence: call.evidence.clone(),
            role_scores: Some(scores),
            excluded: false,
            decided_by: DecidedBy::Dovetail,
            source_file,
            ..DatasetMeta::default()
        }
    }

    /// The dataset's fields in column order. `from` are the tables whose packages type
    /// them, in order: the table itself, or a combination's parts.
    fn fields(&self, dataset: &str, from: &[&str], call: Option<&RoleCall>) -> Vec<Field> {
        let times: BTreeSet<&str> = call
            .and_then(|c| c.time_field.as_deref())
            .into_iter()
            .collect();
        let audit: BTreeSet<&str> = call
            .map(|c| c.audit_time_fields.iter().map(String::as_str).collect())
            .unwrap_or_default();
        self.columns
            .get(dataset)
            .into_iter()
            .flatten()
            .map(|c| {
                let datatype = c
                    .label
                    .as_deref()
                    .and_then(|l| self.datatype(from, &c.name, l, &c.duckdb_type));
                let mut extras = super::Extras::new();
                let is_time = if times.contains(c.name.as_str()) {
                    Some(true)
                } else if audit.contains(c.name.as_str()) {
                    Some(false)
                } else {
                    None
                };
                if let Some(is_time) = is_time {
                    extras.insert(
                        "dimension".to_string(),
                        serde_yaml::from_str(&format!("is_time: {is_time}")).expect("yaml"),
                    );
                }
                Field {
                    name: c.name.clone(),
                    expression: Expression::sql(self.sql_name(&c.name)),
                    datatype,
                    meta: FieldMeta {
                        finetype: c.label.clone(),
                        decided_by: DecidedBy::Dovetail,
                        extra: Map::new(),
                    },
                    extensions: Vec::new(),
                    extras,
                }
            })
            .collect()
    }

    /// A typed field's Ossie datatype: the package's Frictionless type when a package
    /// names the column, else the one FineType's map gives its label.
    fn datatype(
        &self,
        from: &[&str],
        column: &str,
        label: &str,
        duckdb_type: &str,
    ) -> Option<Datatype> {
        let packaged = from
            .iter()
            .filter_map(|t| self.table_of(t))
            .flat_map(|t| &t.fields)
            .find(|f| f.name == column)
            .map(|f| f.frictionless_type.clone());
        let frictionless =
            packaged.or_else(|| finetype_core::frictionless_for(label).map(|f| f.ftype))?;
        let duckdb_type = duckdb_type.to_ascii_uppercase();
        match frictionless.as_str() {
            "string" => Some(Datatype::String),
            "integer" => Some(Datatype::Integer),
            "number" if duckdb_type.starts_with("DECIMAL") => Some(Datatype::Decimal),
            "number" => Some(Datatype::Float),
            "boolean" => Some(Datatype::Boolean),
            "date" => Some(Datatype::Date),
            "time" => Some(Datatype::Time),
            "datetime" if duckdb_type == "TIMESTAMP WITH TIME ZONE" => Some(Datatype::DateTimeTz),
            "datetime" => Some(Datatype::DateTime),
            _ => None,
        }
    }

    fn label(&self, table: &str, column: &str) -> Option<&str> {
        self.column(table, column).and_then(|c| c.label.as_deref())
    }

    fn column(&self, table: &str, column: &str) -> Option<&Column> {
        self.columns.get(table)?.iter().find(|c| c.name == column)
    }

    /// The accepted and suggested joins to a unique parent, child to parent.
    fn relationships(&self, pass: Verified) -> Vec<Relationship> {
        let edges: Vec<&Edge> = self
            .model
            .edges
            .iter()
            .filter(|e| {
                matches!(e.status, EdgeStatus::Accepted | EdgeStatus::Suggested)
                    && e.verification.parent_unique
            })
            .collect();
        let mut pairs: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for e in &edges {
            *pairs
                .entry((e.child.table.as_str(), e.parent.table.as_str()))
                .or_default() += 1;
        }
        let mut out: Vec<Relationship> = edges
            .iter()
            .map(|e| {
                let (from, to) = (&e.child.table, &e.parent.table);
                let name = if pairs[&(from.as_str(), to.as_str())] > 1 {
                    format!("{from}_{to}_{}", e.child.column)
                } else {
                    format!("{from}_{to}")
                };
                let child_label = self.label(from, &e.child.column);
                let parent_label = self.label(to, &e.parent.column);
                let types_agree = match (child_label, parent_label) {
                    (Some(a), Some(b)) => a == b,
                    (None, None) => {
                        let ty = |t: &str, c: &str| self.column(t, c).map(|c| &c.duckdb_type);
                        ty(from, &e.child.column) == ty(to, &e.parent.column)
                            && ty(from, &e.child.column).is_some()
                    }
                    _ => false,
                };
                Relationship {
                    name,
                    from: from.clone(),
                    to: to.clone(),
                    from_columns: vec![e.child.column.clone()],
                    to_columns: vec![e.parent.column.clone()],
                    meta: RelationshipMeta {
                        grade: match e.status {
                            EdgeStatus::Accepted => Grade::Likely,
                            _ => Grade::Possible,
                        },
                        evidence: Evidence {
                            value_overlap: e.evidence.value_overlap,
                            orphans: count(e.verification.orphan_count),
                            child_rows: count(e.verification.child_total),
                            child_type: child_label.map(str::to_string),
                            parent_type: parent_label.map(str::to_string),
                            types_agree,
                            name_similarity: e.evidence.name_similarity,
                            parent_duplicates: count(e.evidence.parent_duplicates),
                            extra: Map::new(),
                        },
                        cardinality: if e.verification.child_total > 0
                            && e.evidence.child_distinct == e.verification.child_total
                        {
                            "1:1"
                        } else {
                            "N:1"
                        }
                        .to_string(),
                        verified: pass,
                        decided_by: DecidedBy::Dovetail,
                        warning: None,
                        extra: Map::new(),
                    },
                    extras: Default::default(),
                    extensions: Vec::new(),
                }
            })
            .collect();
        unique_names(&mut out);
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}

/// Make every relationship's name unique, deterministically and in the order given: a
/// name two share takes `_<to column>` after it, and one still shared takes a counter.
fn unique_names(rels: &mut [Relationship]) {
    let names: Vec<String> = rels.iter().map(|r| r.name.clone()).collect();
    for (r, name) in rels.iter_mut().zip(&names) {
        if shared_in(&names, name) > 1 {
            r.name = format!("{name}_{}", r.to_columns.join("_"));
        }
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for r in rels.iter_mut() {
        let base = r.name.clone();
        let mut n = 1;
        while !seen.insert(r.name.clone()) {
            n += 1;
            r.name = format!("{base}_{n}");
        }
    }
}

fn shared_in(names: &[String], name: &str) -> usize {
    names.iter().filter(|n| *n == name).count()
}

fn count(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

fn role_name(r: KeyRole) -> &'static str {
    match r {
        KeyRole::Entity => "entity",
        KeyRole::Event => "event",
        KeyRole::Reference => "reference",
        KeyRole::Bridge => "bridge",
        KeyRole::Snapshot => "snapshot",
        KeyRole::Unclear => "unclear",
    }
}

fn role_of(r: KeyRole) -> Role {
    match r {
        KeyRole::Entity => Role::Entity,
        KeyRole::Event => Role::Event,
        KeyRole::Reference => Role::Reference,
        KeyRole::Bridge => Role::Bridge,
        KeyRole::Snapshot => Role::Snapshot,
        KeyRole::Unclear => Role::Unclear,
    }
}

fn simple(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// DuckDB's keywords that cannot stand bare as a name (every category but `unreserved`), read from the session's own engine so the list is the one
/// the pinned DuckDB parses by.
fn reserved_words(s: &Session) -> Result<BTreeSet<String>, EvidenceError> {
    let mut stmt = s.conn().prepare(
        "SELECT keyword_name FROM duckdb_keywords() WHERE keyword_category <> 'unreserved'",
    )?;
    let words = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<duckdb::Result<BTreeSet<String>>>()?;
    Ok(words.into_iter().map(|w| w.to_ascii_lowercase()).collect())
}

/// Every dataset's columns in order, typed as the run typed them. The types and
/// labels are read back from the evidence the run wrote; a session that has not
/// written it is measured now.
fn read_columns(
    s: &Session,
    combos: &[combine::Combination],
) -> Result<(Columns, BTreeMap<String, i64>), EvidenceError> {
    let known = match read_evidence(s)? {
        Some(rows) => rows,
        None => {
            let cols = column_evidence(s)?;
            view_column_evidence(s, combos, &cols)?
        }
    };
    let mut labels: BTreeMap<(&str, &str), Option<&str>> = BTreeMap::new();
    for c in &known {
        labels.insert((c.table.as_str(), c.column.as_str()), c.label.as_deref());
    }
    let mut counted: BTreeMap<String, i64> = BTreeMap::new();
    for c in &known {
        let n = counted.entry(c.table.clone()).or_default();
        *n = (*n).max(c.rows);
    }
    let mut out: BTreeMap<String, Vec<Column>> = BTreeMap::new();
    for c in list_columns(s.conn(), "src")? {
        let label = labels
            .get(&(c.table.as_str(), c.column.as_str()))
            .copied()
            .flatten()
            .map(str::to_string);
        out.entry(c.table).or_default().push(Column {
            name: c.column,
            duckdb_type: c.duckdb_type,
            label,
        });
    }
    for c in combos {
        let mut stmt = s.conn().prepare(&format!(
            "SELECT column_name, column_type FROM (DESCRIBE {VIEW_PREFIX}{})",
            quote(&c.name)
        ))?;
        let described: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<duckdb::Result<_>>()?;
        let columns = described
            .into_iter()
            .map(|(name, duckdb_type)| {
                let label = labels
                    .get(&(c.name.as_str(), name.as_str()))
                    .copied()
                    .flatten()
                    .map(str::to_string);
                Column {
                    name,
                    duckdb_type,
                    label,
                }
            })
            .collect();
        out.insert(c.name.clone(), columns);
    }
    Ok((out, counted))
}

/// The column evidence the run wrote to the session; `None` when it wrote none.
fn read_evidence(s: &Session) -> Result<Option<Vec<ColumnEvidence>>, EvidenceError> {
    let written: i64 = s.conn().query_row(
        "SELECT count(*) FROM information_schema.tables
         WHERE table_catalog = 'memory' AND table_schema = 'main' AND table_name = 'column_evidence'",
        [],
        |r| r.get(0),
    )?;
    if written == 0 {
        return Ok(None);
    }
    let mut stmt = s.conn().prepare(
        "SELECT \"table\", \"column\", duckdb_type, \"rows\", non_null, \"distinct\", label, audit_name
         FROM memory.main.column_evidence",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ColumnEvidence {
                table: r.get(0)?,
                column: r.get(1)?,
                duckdb_type: r.get(2)?,
                rows: r.get(3)?,
                non_null: r.get(4)?,
                distinct: r.get(5)?,
                label: r.get(6)?,
                audit_name: r.get(7)?,
            })
        })?
        .collect::<duckdb::Result<Vec<_>>>()?;
    Ok(Some(rows))
}
