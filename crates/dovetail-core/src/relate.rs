//! relate — discover how DuckDB tables relate (card 0002, spec
//! 2026-06-21-relate-discover-verify-render).
//!
//! relate READS an existing DuckDB (the analyst loaded it via survey's emitted
//! `.sql`; choice 0009) and issues only read queries — schema introspection,
//! value-overlap/cardinality evidence, and referential-integrity verification
//! (choice 0014: in-process discovery reads, never pipeline execution). It scores
//! each candidate foreign-key edge, VERIFIES it against the data, and assigns a
//! status: a verified high-confidence edge auto-accepts with no analyst action;
//! a coincidence or a broken reference is rejected; a plausible-but-unprovable
//! edge is surfaced as suggested (choice 0007, refined).

use duckdb::Connection;

/// A column reference (table + column).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnRef {
    pub table: String,
    pub column: String,
}

impl ColumnRef {
    pub fn qualified(&self) -> String {
        format!("{}.{}", self.table, self.column)
    }
}

/// Evidence gathered for a candidate edge (child → parent).
#[derive(Debug, Clone)]
pub struct Evidence {
    /// Name-similarity signal in [0,1] (FK-style naming, column equality).
    pub name_similarity: f64,
    /// Fraction of the child's distinct values present in the parent, [0,1].
    pub value_overlap: f64,
    /// How key-like the parent is: id-shaped name + distinct ratio, [0,1].
    pub parent_key_likeness: f64,
    pub child_distinct: i64,
    pub parent_distinct: i64,
    pub parent_total: i64,
}

/// Verification of referential integrity against the full data.
#[derive(Debug, Clone)]
pub struct Verification {
    /// Child rows whose value is absent from the parent (NULLs excluded).
    pub orphan_count: i64,
    /// Non-null child rows considered.
    pub child_total: i64,
    /// Whether the parent column has no duplicate values (a real key).
    pub parent_unique: bool,
}

impl Verification {
    pub fn orphan_rate(&self) -> f64 {
        if self.child_total == 0 {
            1.0
        } else {
            self.orphan_count as f64 / self.child_total as f64
        }
    }
    pub fn integrity_holds(&self) -> bool {
        self.orphan_count == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeStatus {
    Accepted,
    Suggested,
    Rejected,
}

impl EdgeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeStatus::Accepted => "accepted",
            EdgeStatus::Suggested => "suggested",
            EdgeStatus::Rejected => "rejected",
        }
    }
}

/// A discovered candidate edge with its evidence, verification, score and status.
#[derive(Debug, Clone)]
pub struct Edge {
    pub child: ColumnRef,
    pub parent: ColumnRef,
    pub evidence: Evidence,
    pub verification: Verification,
    pub confidence: f64,
    pub status: EdgeStatus,
    pub reason: String,
}

// --- Tunable thresholds (ac-10/ac-04: pin the auto-accept boundary) ----------

/// Confidence at or above which an edge is "high-confidence".
pub const CONF_HIGH: f64 = 0.6;
/// Orphan rate at or below which integrity "nearly holds" (dirty-data tolerance).
pub const ORPHAN_TOLERANCE: f64 = 0.05;

// --- Public entry point ------------------------------------------------------

/// Discover, verify and score all candidate FK edges in a DuckDB.
pub fn discover(conn: &Connection) -> duckdb::Result<Vec<Edge>> {
    let columns = read_columns(conn)?;
    let mut edges = Vec::new();

    for child in &columns {
        for parent in &columns {
            if child.col.table == parent.col.table {
                continue;
            }
            if !types_compatible(&child.ty, &parent.ty) {
                continue;
            }
            // Prune: a parent must be at least plausibly key-like by name, or the
            // columns must share a name — otherwise skip the expensive queries.
            let name_similarity = name_similarity(&child.col, &parent.col);
            if name_similarity < 0.3 {
                continue;
            }
            if let Some(edge) = score_edge(conn, &child.col, &parent.col, name_similarity)? {
                edges.push(edge);
            }
        }
    }
    Ok(edges)
}

/// Open a DuckDB at `path` (read-only discovery) and discover its edges. Keeps
/// the `duckdb` dependency inside dovetail-core — the CLI stays thin.
pub fn discover_path(path: &str) -> duckdb::Result<Vec<Edge>> {
    let conn = Connection::open(path)?;
    discover(&conn)
}

/// Accepted edges only (choice 0007) — what compiles to constraint DDL.
pub fn accepted(edges: &[Edge]) -> Vec<&Edge> {
    edges.iter().filter(|e| e.status == EdgeStatus::Accepted).collect()
}

// --- Schema read (ac-02) -----------------------------------------------------

struct TypedColumn {
    col: ColumnRef,
    ty: String,
}

fn read_columns(conn: &Connection) -> duckdb::Result<Vec<TypedColumn>> {
    let mut stmt = conn.prepare(
        "SELECT table_name, column_name, data_type
         FROM information_schema.columns
         WHERE table_schema = 'main'
         ORDER BY table_name, ordinal_position",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(TypedColumn {
            col: ColumnRef { table: r.get(0)?, column: r.get(1)? },
            ty: r.get(2)?,
        })
    })?;
    rows.collect()
}

/// Coarse type-family compatibility — candidates must share a family.
fn types_compatible(a: &str, b: &str) -> bool {
    type_family(a) == type_family(b)
}

fn type_family(ty: &str) -> &'static str {
    let t = ty.to_ascii_uppercase();
    if t.contains("INT") || t.contains("HUGEINT") {
        "int"
    } else if t.contains("CHAR") || t.contains("STRING") || t.contains("TEXT") {
        "text"
    } else if t.contains("BOOL") {
        "bool"
    } else if t.contains("DOUBLE") || t.contains("REAL") || t.contains("DECIMAL") || t.contains("FLOAT") {
        "float"
    } else {
        "other"
    }
}

// --- Evidence + verification + scoring (ac-02/03/04) --------------------------

fn score_edge(
    conn: &Connection,
    child: &ColumnRef,
    parent: &ColumnRef,
    name_similarity: f64,
) -> duckdb::Result<Option<Edge>> {
    let ct = quote(&child.table);
    let cc = quote(&child.column);
    let pt = quote(&parent.table);
    let pc = quote(&parent.column);

    let child_total: i64 = conn.query_row(
        &format!("SELECT count(*) FROM {ct} WHERE {cc} IS NOT NULL"),
        [],
        |r| r.get(0),
    )?;
    if child_total == 0 {
        return Ok(None);
    }
    let child_distinct: i64 = conn.query_row(
        &format!("SELECT count(DISTINCT {cc}) FROM {ct} WHERE {cc} IS NOT NULL"),
        [],
        |r| r.get(0),
    )?;
    let parent_total: i64 =
        conn.query_row(&format!("SELECT count(*) FROM {pt} WHERE {pc} IS NOT NULL"), [], |r| r.get(0))?;
    let parent_distinct: i64 = conn.query_row(
        &format!("SELECT count(DISTINCT {pc}) FROM {pt} WHERE {pc} IS NOT NULL"),
        [],
        |r| r.get(0),
    )?;

    // Distinct child values present in the parent.
    let overlap_distinct: i64 = conn.query_row(
        &format!(
            "SELECT count(*) FROM (SELECT DISTINCT {cc} AS v FROM {ct} WHERE {cc} IS NOT NULL) c
             WHERE c.v IN (SELECT {pc} FROM {pt} WHERE {pc} IS NOT NULL)"
        ),
        [],
        |r| r.get(0),
    )?;

    // Orphan rows: child non-null values absent from the parent.
    let orphan_count: i64 = conn.query_row(
        &format!(
            "SELECT count(*) FROM {ct}
             WHERE {cc} IS NOT NULL
               AND {cc} NOT IN (SELECT {pc} FROM {pt} WHERE {pc} IS NOT NULL)"
        ),
        [],
        |r| r.get(0),
    )?;

    let value_overlap =
        if child_distinct == 0 { 0.0 } else { overlap_distinct as f64 / child_distinct as f64 };
    let parent_unique = parent_total > 0 && parent_total == parent_distinct;
    let parent_distinct_ratio =
        if parent_total == 0 { 0.0 } else { parent_distinct as f64 / parent_total as f64 };
    let parent_key_likeness =
        0.6 * key_name_signal(&parent.column) + 0.4 * parent_distinct_ratio;

    // Confidence: naming + overlap, GATED by how key-like the parent is. A
    // perfect name/overlap match against a non-key parent (e.g. a boolean) is
    // still low confidence — this is what stops coincidental overlaps.
    let confidence = (0.5 * name_similarity + 0.5 * value_overlap) * parent_key_likeness;

    let evidence = Evidence {
        name_similarity,
        value_overlap,
        parent_key_likeness,
        child_distinct,
        parent_distinct,
        parent_total,
    };
    let verification = Verification { orphan_count, child_total, parent_unique };

    let (status, reason) = assign_status(&verification, confidence);

    Ok(Some(Edge { child: child.clone(), parent: parent.clone(), evidence, verification, confidence, status, reason }))
}

/// Status from verification + confidence (ac-04). Verification is the safety
/// gate (ac-03): only an integrity-holding edge against a unique parent
/// auto-accepts. The boolean trap is caught because a non-unique parent never
/// reaches Accepted regardless of naming.
fn assign_status(v: &Verification, confidence: f64) -> (EdgeStatus, String) {
    let high = confidence >= CONF_HIGH;
    if v.integrity_holds() && v.parent_unique && high {
        (
            EdgeStatus::Accepted,
            format!(
                "verified: 0 orphans, parent is unique, confidence {:.2} ≥ {:.2}",
                confidence, CONF_HIGH
            ),
        )
    } else if high && (v.integrity_holds() || v.orphan_rate() <= ORPHAN_TOLERANCE) {
        let why = if !v.parent_unique {
            "parent not provably unique"
        } else {
            "minor orphans within tolerance"
        };
        (
            EdgeStatus::Suggested,
            format!("plausible ({why}); surfaced for review — confidence {:.2}", confidence),
        )
    } else {
        let why = if v.orphan_rate() > ORPHAN_TOLERANCE {
            format!("{} orphan row(s) ({:.0}%)", v.orphan_count, v.orphan_rate() * 100.0)
        } else {
            format!("confidence {:.2} below {:.2} (coincidental overlap)", confidence, CONF_HIGH)
        };
        (EdgeStatus::Rejected, format!("rejected: {why}"))
    }
}

// --- Name similarity ---------------------------------------------------------

/// How key-like a column NAME is: `id` / `*_id` score high.
fn key_name_signal(col: &str) -> f64 {
    let c = col.to_ascii_lowercase();
    if c == "id" {
        1.0
    } else if c.ends_with("_id") || c.ends_with("id") {
        0.85
    } else if c.ends_with("_key") || c.ends_with("code") {
        0.6
    } else {
        0.2
    }
}

/// Name-similarity signal for a candidate (child → parent), [0,1]. Rewards the
/// FK convention `child.<parent_singular>_id` and exact column-name equality.
fn name_similarity(child: &ColumnRef, parent: &ColumnRef) -> f64 {
    let cc = child.column.to_ascii_lowercase();
    // A child column named bare `id` is the table's OWN primary key, not a
    // foreign-key reference. Excluding it stops coincidental surrogate-id overlap
    // (two unrelated tables both keyed 1,2,3) from looking like a verified FK.
    if cc == "id" {
        return 0.0;
    }
    let singular = singularise(&parent.table.to_ascii_lowercase());
    let fk_named = cc == format!("{singular}_id")
        || cc == format!("{}_id", parent.table.to_ascii_lowercase())
        || cc.starts_with(&format!("{singular}_"))
        || cc.starts_with(&singular);
    if fk_named && (cc.ends_with("id") || cc.ends_with("_id")) {
        0.9
    } else if fk_named {
        0.6
    } else if cc == parent.column.to_ascii_lowercase() {
        0.7
    } else {
        0.0
    }
}

/// Naive singulariser: `categories` → `category`, `orders` → `order`.
fn singularise(s: &str) -> String {
    if let Some(stem) = s.strip_suffix("ies") {
        format!("{stem}y")
    } else if let Some(stem) = s.strip_suffix('s') {
        stem.to_string()
    } else {
        s.to_string()
    }
}

/// Double-quote a SQL identifier.
fn quote(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

// --- Renders (ac-05 / ac-06) -------------------------------------------------

use crate::datapackage::{ForeignKey, ForeignKeyReference};

impl Edge {
    /// Build the Frictionless foreignKey entry for this edge, carrying status,
    /// confidence and evidence as custom properties (ac-05, choice 0003).
    pub fn to_foreign_key(&self) -> ForeignKey {
        ForeignKey {
            fields: vec![self.child.column.clone()],
            reference: ForeignKeyReference {
                resource: self.parent.table.clone(),
                fields: vec![self.parent.column.clone()],
            },
            status: self.status.as_str().to_string(),
            confidence: round2(self.confidence),
            evidence: serde_json::json!({
                "nameSimilarity": round2(self.evidence.name_similarity),
                "valueOverlap": round2(self.evidence.value_overlap),
                "parentKeyLikeness": round2(self.evidence.parent_key_likeness),
                "parentUnique": self.verification.parent_unique,
                "orphanCount": self.verification.orphan_count,
                "reason": self.reason,
            }),
        }
    }

    /// Standard portable constraint DDL for an accepted edge (ac-06). Emitted in
    /// ALTER form — legible, and accepted by standard SQL engines (Postgres). Note
    /// DuckDB only enforces FKs declared at CREATE-table time, so this artifact is
    /// the reviewable migration, not something dovetail runs (choice 0001).
    pub fn constraint_ddl(&self) -> String {
        let cname = format!("fk_{}_{}", self.child.table, self.child.column);
        format!(
            "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({});",
            quote(&self.child.table),
            quote(&cname),
            quote(&self.child.column),
            quote(&self.parent.table),
            quote(&self.parent.column),
        )
    }
}

/// Constraint DDL for every accepted edge, in stable order (ac-06). Only accepted
/// edges compile (choice 0007).
pub fn constraint_ddl(edges: &[Edge]) -> String {
    let mut accepted: Vec<&Edge> = accepted(edges);
    accepted.sort_by(|a, b| a.child.qualified().cmp(&b.child.qualified()));
    let mut out = String::from("-- Generated by dovetail relate — accepted foreign keys\n");
    for e in accepted {
        out.push_str(&e.constraint_ddl());
        out.push('\n');
    }
    out
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}
