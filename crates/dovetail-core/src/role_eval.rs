//! role_eval — score role calls against answer keys, and render the record.
//!
//! As [`crate::eval`] does for structure: a [`RoleScorer`] turns a session
//! into a [`RoleModel`], [`score_set`] compares that model with a set's
//! [`AnswerKey`], and [`render_report`] writes the results as Markdown. The
//! report is sorted throughout, so a committed copy is the eval record and a
//! change to the scorer shows as a diff of it; the stamp is its only line that
//! varies from run to run.

use std::collections::{BTreeMap, BTreeSet};

use crate::evidence::{EvidenceError, Session};
use crate::finetype_ext::TypingSource;
use crate::role_key::{AnswerKey, CombinationKind, Role};
use crate::roles::{self, RoleModel};

/// Every role, in the order the confusion matrix lists them.
const ROLES: [Role; 6] = [
    Role::Entity,
    Role::Event,
    Role::Reference,
    Role::Bridge,
    Role::Snapshot,
    Role::Unclear,
];

/// Something that calls every table's role in a session.
pub trait RoleScorer {
    fn name(&self) -> &str;
    fn score(&self, s: &Session) -> Result<RoleModel, EvidenceError>;
}

/// The scorer [`roles`] implements: each role scored from evidence, the top
/// one taken.
pub struct ScorePerRole;

impl RoleScorer for ScorePerRole {
    fn name(&self) -> &str {
        "score-per-role"
    }

    fn score(&self, s: &Session) -> Result<RoleModel, EvidenceError> {
        roles::run_session(s)
    }
}

/// Matches against an answer key: called and in the key, called and not, in
/// the key and not called.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PrecisionRecall {
    pub true_pos: usize,
    pub false_pos: usize,
    pub false_neg: usize,
}

impl PrecisionRecall {
    fn of<T: Ord>(called: &BTreeSet<T>, expected: &BTreeSet<T>) -> Self {
        let true_pos = called.intersection(expected).count();
        PrecisionRecall {
            true_pos,
            false_pos: called.len() - true_pos,
            false_neg: expected.len() - true_pos,
        }
    }

    /// `P / R` to two places, `-` where nothing was called or expected.
    fn render(&self) -> String {
        let ratio = |n: usize, d: usize| {
            if d == 0 {
                "-".to_string()
            } else {
                format!("{:.2}", n as f64 / d as f64)
            }
        };
        format!(
            "{} / {}",
            ratio(self.true_pos, self.true_pos + self.false_pos),
            ratio(self.true_pos, self.true_pos + self.false_neg)
        )
    }
}

/// One set's calls scored against its answer key.
#[derive(Debug, Clone, PartialEq)]
pub struct SetResult {
    pub set: String,
    pub tier: u8,
    /// `profiled` or `floor`.
    pub typing: String,
    /// Tables in the answer key.
    pub tables: usize,
    /// Tables called the role the key gives them.
    pub correct: usize,
    /// (the key's role, the call) for every table both name.
    pub confusion: BTreeMap<(Role, Role), usize>,
    pub folds: PrecisionRecall,
    pub combinations: PrecisionRecall,
    /// One sentence per disagreement with the key, sorted.
    pub misses: Vec<String>,
}

fn role_name(r: Role) -> &'static str {
    match r {
        Role::Entity => "entity",
        Role::Event => "event",
        Role::Reference => "reference",
        Role::Bridge => "bridge",
        Role::Snapshot => "snapshot",
        Role::Unclear => "unclear",
    }
}

fn typing_label(t: &TypingSource) -> &'static str {
    match t {
        TypingSource::Profiled { .. } => "profiled",
        TypingSource::Floor { .. } => "floor",
    }
}

/// Score `model` against `key`. Table names match ignoring case. A fold
/// matches on its reference and host; a combination on its name, parts and
/// kind, so a pair combined as the wrong kind is both a false positive and a
/// false negative.
pub fn score_set(model: &RoleModel, key: &AnswerKey, tier: u8) -> SetResult {
    let mut misses = Vec::new();
    let mut confusion = BTreeMap::new();
    let mut correct = 0;

    let called: BTreeMap<String, Role> = model
        .tables
        .iter()
        .map(|c| (c.table.to_lowercase(), c.role))
        .collect();
    for (name, want) in &key.tables {
        match called.get(name) {
            Some(&got) => {
                *confusion.entry((want.role, got)).or_insert(0) += 1;
                if got == want.role {
                    correct += 1;
                } else {
                    misses.push(format!(
                        "table `{name}`: the key says {}, called {}",
                        role_name(want.role),
                        role_name(got)
                    ));
                }
            }
            None => misses.push(format!("table `{name}`: in the key, not called")),
        }
    }
    for (name, got) in &called {
        if !key.tables.contains_key(name) {
            misses.push(format!(
                "table `{name}`: called {}, not in the key",
                role_name(*got)
            ));
        }
    }

    let folds_called: BTreeSet<(String, String)> = model
        .folds
        .iter()
        .map(|f| (f.reference.to_lowercase(), f.host.to_lowercase()))
        .collect();
    let folds_expected: BTreeSet<(String, String)> = key
        .folds
        .iter()
        .map(|f| (f.reference.clone(), f.into.clone()))
        .collect();
    for (r, h) in folds_called.difference(&folds_expected) {
        misses.push(format!("fold `{r}` into `{h}`: called, not in the key"));
    }
    for (r, h) in folds_expected.difference(&folds_called) {
        misses.push(format!("fold `{r}` into `{h}`: in the key, not called"));
    }

    let combination = |name: &str, parts: &[String], kind| {
        let mut parts: Vec<String> = parts.iter().map(|p| p.to_lowercase()).collect();
        parts.sort();
        (name.to_lowercase(), parts, kind)
    };
    let combos_called: BTreeSet<_> = model
        .combinations
        .iter()
        .map(|c| combination(&c.name, &c.parts, c.kind))
        .collect();
    let combos_expected: BTreeSet<_> = key
        .combinations
        .iter()
        .map(|c| combination(&c.name, &c.parts, c.kind))
        .collect();
    let describe = |(name, parts, kind): &(String, Vec<String>, _)| {
        let kind = match kind {
            CombinationKind::Partition => "partition",
            CombinationKind::Snapshot => "snapshot",
            CombinationKind::Duplicate => "duplicate",
        };
        format!("combination `{name}` ({kind} of {})", parts.join(", "))
    };
    for c in combos_called.difference(&combos_expected) {
        misses.push(format!("{}: called, not in the key", describe(c)));
    }
    for c in combos_expected.difference(&combos_called) {
        misses.push(format!("{}: in the key, not called", describe(c)));
    }

    misses.sort();
    SetResult {
        set: key.name.clone(),
        tier,
        typing: typing_label(&model.typing).to_string(),
        tables: key.tables.len(),
        correct,
        confusion,
        folds: PrecisionRecall::of(&folds_called, &folds_expected),
        combinations: PrecisionRecall::of(&combos_called, &combos_expected),
        misses,
    }
}

/// The report's stamp: the finetype the typing came from, the model's hash,
/// and the dovetail commit the caller names.
pub fn stamp(typing: &TypingSource, dovetail_commit: &str) -> String {
    let finetype = match typing {
        TypingSource::Profiled {
            extension_version,
            model_sha256,
        } => format!("{extension_version}, model sha256 {model_sha256}"),
        TypingSource::Floor { reason } => format!("floor typing ({reason})"),
    };
    format!("{finetype}, dovetail {dovetail_commit}")
}

/// Render `results` as the Markdown eval record: a summary table, then for
/// each tier and typing a confusion matrix and its misses. Everything is
/// sorted, by tier, then typing, then set, whatever order `results` is in.
pub fn render_report(results: &[SetResult], stamp: &str) -> String {
    let mut sorted: Vec<&SetResult> = results.iter().collect();
    sorted.sort_by(|a, b| {
        (a.tier, &a.typing, &a.set)
            .cmp(&(b.tier, &b.typing, &b.set))
            .then_with(|| a.misses.cmp(&b.misses))
    });

    let mut s = String::new();
    s.push_str("# Role eval results\n\n");
    s.push_str("Each set's role calls scored against its answer key. Written by the `role_eval_harness` test with `DOVETAIL_WRITE_ROLE_RESULTS=1`; everything but the stamp is the same on every run of the same code.\n\n");
    s.push_str(&format!("Stamp: {stamp}\n\n"));

    s.push_str("## Summary\n\n");
    s.push_str(
        "| set | tier | typing | tables | role accuracy | fold P / R | combination P / R |\n",
    );
    s.push_str("|---|---|---|---|---|---|---|\n");
    for r in &sorted {
        let pct = if r.tables == 0 {
            "-".to_string()
        } else {
            format!("{:.1}%", r.correct as f64 * 100.0 / r.tables as f64)
        };
        s.push_str(&format!(
            "| {} | {} | {} | {} | {}/{} ({pct}) | {} | {} |\n",
            r.set,
            r.tier,
            r.typing,
            r.tables,
            r.correct,
            r.tables,
            r.folds.render(),
            r.combinations.render(),
        ));
    }

    let mut groups: BTreeMap<(u8, &str), Vec<&SetResult>> = BTreeMap::new();
    for r in &sorted {
        groups
            .entry((r.tier, r.typing.as_str()))
            .or_default()
            .push(r);
    }
    for ((tier, typing), group) in groups {
        s.push_str(&format!("\n## Tier {tier}, {typing}\n\n"));
        s.push_str("Rows are the answer key's role, columns the call.\n\n");
        let mut confusion: BTreeMap<(Role, Role), usize> = BTreeMap::new();
        for r in &group {
            for (pair, n) in &r.confusion {
                *confusion.entry(*pair).or_insert(0) += n;
            }
        }
        s.push_str("| key \\ call |");
        for r in ROLES {
            s.push_str(&format!(" {} |", role_name(r)));
        }
        s.push_str("\n|---|");
        for _ in ROLES {
            s.push_str("---|");
        }
        s.push('\n');
        for want in ROLES {
            s.push_str(&format!("| {} |", role_name(want)));
            for got in ROLES {
                let n = confusion.get(&(want, got)).copied().unwrap_or(0);
                s.push_str(&format!(" {n} |"));
            }
            s.push('\n');
        }
        let misses: Vec<String> = group
            .iter()
            .flat_map(|r| r.misses.iter().map(move |m| format!("- {}: {m}", r.set)))
            .collect();
        s.push_str("\nMisses:");
        if misses.is_empty() {
            s.push_str(" none.\n");
        } else {
            s.push_str("\n\n");
            for m in misses {
                s.push_str(&m);
                s.push('\n');
            }
        }
    }
    s
}
