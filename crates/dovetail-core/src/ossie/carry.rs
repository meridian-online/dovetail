//! Carrying the analyst's decisions through a proposal.
//!
//! A discovery pass proposes a model; the analyst's earlier model (`base`) says what it
//! has already settled. [`carry`] puts those decisions back: an element with
//! `decided_by: analyst` is written as the base had it, and what the analyst rejected or
//! excluded stays out.

use std::collections::HashSet;

use super::{DecidedBy, Model, Relationship};

/// A relationship's identity: the name is not part of it.
type JoinKey<'a> = (&'a str, &'a [String], &'a str, &'a [String]);

fn join_key(r: &Relationship) -> JoinKey<'_> {
    (&r.from, &r.from_columns, &r.to, &r.to_columns)
}

/// Lay `base`'s analyst-decided datasets, fields and relationships over `proposal`.
///
/// - An analyst dataset replaces the proposal's of the same name, or is appended.
/// - An analyst field in a dovetail-decided dataset replaces the proposal's field of the
///   same name in place, or is appended to it.
/// - An analyst relationship replaces the proposal's with the same `(from, from_columns,
///   to, to_columns)`, or is appended.
/// - Every dataset takes `excluded` from the base dataset of the same name.
/// - Proposed relationships matching `base.meta.rejected` are dropped.
/// - `meta.rejected` and `meta.skipped_sources` are the base's.
///
/// The proposal's order stands; elements only the base has are appended in base order.
pub fn carry(mut proposal: Model, base: &Model) -> Model {
    for theirs in &base.datasets {
        let at = proposal.datasets.iter().position(|d| d.name == theirs.name);
        if theirs.meta.decided_by == DecidedBy::Analyst {
            match at {
                Some(i) => proposal.datasets[i] = theirs.clone(),
                None => proposal.datasets.push(theirs.clone()),
            }
            continue;
        }
        let Some(i) = at else { continue };
        let ours = &mut proposal.datasets[i];
        ours.meta.excluded = theirs.meta.excluded;
        for field in &theirs.fields {
            if field.meta.decided_by != DecidedBy::Analyst {
                continue;
            }
            match ours.fields.iter().position(|f| f.name == field.name) {
                Some(j) => ours.fields[j] = field.clone(),
                None => ours.fields.push(field.clone()),
            }
        }
    }

    // Drop what the analyst rejected from the proposal first, so that an analyst join
    // that matches a rejected entry is still kept below: the analyst decided it.
    let rejected: HashSet<JoinKey<'_>> = base.meta.rejected.iter().map(join_key).collect();
    proposal
        .relationships
        .retain(|r| !rejected.contains(&join_key(r)));
    for theirs in &base.relationships {
        if theirs.meta.decided_by != DecidedBy::Analyst {
            continue;
        }
        let at = proposal
            .relationships
            .iter()
            .position(|r| join_key(r) == join_key(theirs));
        match at {
            Some(i) => proposal.relationships[i] = theirs.clone(),
            None => proposal.relationships.push(theirs.clone()),
        }
    }

    proposal.meta.rejected = base.meta.rejected.clone();
    proposal.meta.skipped_sources = base.meta.skipped_sources.clone();
    proposal
}
