//! `run_model`: one pass of `dovetail model`, from a database to an Ossie model.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{
    carry, project, read, verify_full, DecidedBy, Model, OssieError, Provenance, Verified,
};
use crate::evidence::{EvidenceError, Session, SessionTyping, SAMPLE_ROWS};
use crate::finetype_ext::{TypingRequest, TypingSource};
use crate::packages::{all_tables, read_packages, PackagesError, SourceTable};
use crate::roles::run_session;

/// What one run of `dovetail model` is asked to do.
#[derive(Debug, Clone)]
pub struct ModelRequest {
    /// The DuckDB holding the tables, attached read-only.
    pub db: PathBuf,
    /// A directory of data packages naming and typing the datasets; without it every
    /// table of `db` is a dataset, typed as `typing` asks.
    pub packages: Option<PathBuf>,
    /// `Sample` or `Full`.
    pub pass: Verified,
    /// The analyst's model, whose decisions are carried through.
    pub base: Option<PathBuf>,
    /// For the full pass, the sample pass's proposal to verify.
    pub verify: Option<PathBuf>,
    /// The model's name.
    pub name: String,
    /// The typing asked for when there are no packages.
    pub typing: TypingRequest,
}

/// Why a model could not be written.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Ossie { path: PathBuf, source: OssieError },
    #[error(transparent)]
    Packages(#[from] PackagesError),
    #[error(transparent)]
    Evidence(#[from] EvidenceError),
}

/// Run one pass and return its model with the run's warnings: a package resource with
/// no table, typing that fell back to the floor, and a `verify` proposal that could not
/// be used.
///
/// The sample pass discovers on samples of at most [`SAMPLE_ROWS`] rows of each table.
/// The full pass verifies the proposal at `verify` with [`verify_full`] when it reads,
/// was written by dovetail and names exactly the run's source tables; otherwise it
/// discovers afresh on every row. Either pass then carries the decisions of the model at
/// `base` through.
///
/// # Errors
///
/// When `base` cannot be read, the packages cannot be read, or the database cannot be
/// measured.
pub fn run_model(req: &ModelRequest) -> Result<(Model, Vec<String>), ModelError> {
    let base = req.base.as_deref().map(read_model).transpose()?;
    let (tables, mut warnings) = match &req.packages {
        Some(dir) => {
            let names: Vec<String> = all_tables(&req.db)?.into_iter().map(|t| t.name).collect();
            read_packages(dir, &names)?
        }
        None => (all_tables(&req.db)?, Vec::new()),
    };
    let typing = match req.packages {
        Some(_) => SessionTyping::Packages,
        None => SessionTyping::Request(req.typing),
    };

    let model = match req.pass {
        Verified::Full => {
            let proposal = req.verify.as_deref().map(|path| usable(path, &tables));
            match proposal {
                Some(Ok(proposal)) => {
                    // The proposal's own typing stands: the full pass types nothing,
                    // so its session loads no typing.
                    let session = Session::over(&req.db, &tables, None, SessionTyping::Packages)?;
                    verify_full(proposal, &session)
                }
                Some(Err(why)) => {
                    warnings.push(format!("discovering afresh on the full data: {why}"));
                    discover(req, &tables, typing, None, &mut warnings)?
                }
                None => discover(req, &tables, typing, None, &mut warnings)?,
            }
        }
        _ => discover(req, &tables, typing, Some(SAMPLE_ROWS), &mut warnings)?,
    };
    Ok((
        match &base {
            Some(base) => carry(model, base),
            None => model,
        },
        warnings,
    ))
}

fn read_model(path: &Path) -> Result<Model, ModelError> {
    let text = std::fs::read_to_string(path).map_err(|source| ModelError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    read(&text).map_err(|source| ModelError::Ossie {
        path: path.to_path_buf(),
        source,
    })
}

/// Run table roles over `tables`, sampled at `sample` rows or on every row, and project
/// the result.
fn discover(
    req: &ModelRequest,
    tables: &[SourceTable],
    typing: SessionTyping,
    sample: Option<u64>,
    warnings: &mut Vec<String>,
) -> Result<Model, ModelError> {
    let session = Session::over(&req.db, tables, sample, typing)?;
    if let (SessionTyping::Request(TypingRequest::Best), TypingSource::Floor { reason }) =
        (typing, &session.typing)
    {
        warnings.push(format!("typing fell back to the floor: {reason}"));
    }
    let roles = run_session(&session)?;
    let provenance = Provenance {
        pass: if sample.is_some() {
            Verified::Sample
        } else {
            Verified::Full
        },
        typing: roles.typing.clone(),
        sample_rows: sample,
    };
    Ok(project(&roles, tables, &session, &req.name, &provenance)?)
}

/// The proposal at `path` when the full pass can verify it, else why not.
///
/// It must read, say `dovetail` wrote it, and name exactly the run's source tables: each
/// table is a dataset that is not a combination, every such dataset dovetail decided is
/// a table, and every combination's parts are tables. An analyst's dataset with no
/// table of its own is carried, not compared.
fn usable(path: &Path, tables: &[SourceTable]) -> Result<Model, String> {
    let model = read_model(path).map_err(|e| e.to_string())?;
    if !model.meta.generator.starts_with("dovetail ") {
        return Err(format!(
            "{} was written by {}, not dovetail",
            path.display(),
            model.meta.generator
        ));
    }
    let run: BTreeSet<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    let plain: BTreeSet<&str> = model
        .datasets
        .iter()
        .filter(|d| d.meta.combination.is_none())
        .map(|d| d.name.as_str())
        .collect();
    let stale = |what: String| format!("{} is out of date: {what}", path.display());
    if let Some(missing) = run.difference(&plain).next() {
        return Err(stale(format!("it has no dataset {missing}")));
    }
    for d in &model.datasets {
        let decided = d.meta.decided_by == DecidedBy::Dovetail;
        match &d.meta.combination {
            None if decided && !run.contains(d.name.as_str()) => {
                return Err(stale(format!(
                    "its dataset {} is no table of this run",
                    d.name
                )));
            }
            Some(c) => {
                if let Some(p) = c.parts.iter().find(|p| !run.contains(p.as_str())) {
                    return Err(stale(format!(
                        "its dataset {} combines {p}, no table of this run",
                        d.name
                    )));
                }
            }
            None => {}
        }
    }
    Ok(model)
}
