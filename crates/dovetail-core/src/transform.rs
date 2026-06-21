//! Embedded jaq transform (card 0003, spec 2026-06-21-jaq-passthrough-shim).
//!
//! `run_jaq` executes a jq program over input bytes through embedded, vendored
//! jaq — no system jq on this path (choice 0005). It is **true passthrough**:
//! the program string is handed to the engine verbatim and echoed back in
//! [`JaqOutput::program`], so a reviewer reads exactly what ran (the legibility
//! value, brief principle 4). The engine never rewrites it.

use jaq_core::load::{Arena, File, Loader};
use jaq_core::{data, unwrap_valr, Compiler, Ctx, Vars};
use jaq_json::Val;

/// The embedded jaq-core version (ac-08). Tracks the `jaq-core` pin in this
/// crate's Cargo.toml / Cargo.lock; kept retrievable so emitted artifacts can
/// stamp what produced them. Auto-deriving this from Cargo.lock at build time is
/// a choice-0013 hardening follow-up.
pub const JAQ_CORE_VERSION: &str = "3.1.0";

#[derive(Debug, thiserror::Error)]
pub enum JaqError {
    #[error("jaq: failed to load/parse program: {0}")]
    Load(String),
    #[error("jaq: failed to compile program: {0}")]
    Compile(String),
    #[error("jaq: failed to parse input as JSON: {0}")]
    Input(String),
    #[error("jaq: runtime error: {0}")]
    Run(String),
}

/// The result of running a jq program.
pub struct JaqOutput {
    /// The exact program that was run — byte-equal to the input program
    /// (passthrough, never rewritten). This is ac-02's assertable artifact.
    pub program: String,
    /// Output values, each rendered as compact JSON — one per NDJSON line.
    pub values: Vec<String>,
}

impl JaqOutput {
    /// Render the output as NDJSON: one compact JSON value per line, trailing
    /// newline. Greppable and DuckDB-loadable (ac-04).
    pub fn to_ndjson(&self) -> String {
        let mut s = String::new();
        for v in &self.values {
            s.push_str(v);
            s.push('\n');
        }
        s
    }
}

/// Run a jq `program` over `input` bytes. The program is passed to embedded jaq
/// verbatim; `input` may hold one or many top-level JSON values (the filter runs
/// on each, jq-style).
pub fn run_jaq(program: &str, input: &[u8]) -> Result<JaqOutput, JaqError> {
    // Named filters from core + std + json (keys, map, select, ...).
    let defs = jaq_core::defs().chain(jaq_std::defs()).chain(jaq_json::defs());
    let funs = jaq_core::funs::<data::JustLut<Val>>()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());

    let loader = Loader::new(defs);
    let arena = Arena::default();

    // PASSTHROUGH: the program string is the File code, unmodified.
    let file = File { code: program, path: () };
    let modules = loader
        .load(&arena, file)
        .map_err(|e| JaqError::Load(format!("{e:?}")))?;

    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|e| JaqError::Compile(format!("{e:?}")))?;

    let mut values = Vec::new();
    for input_val in jaq_json::read::parse_many(input) {
        let input_val = input_val.map_err(|e| JaqError::Input(format!("{e:?}")))?;
        let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
        for out in filter.id.run((ctx, input_val)).map(unwrap_valr) {
            let v = out.map_err(|e| JaqError::Run(format!("{e:?}")))?;
            values.push(v.to_string());
        }
    }

    Ok(JaqOutput { program: program.to_string(), values })
}
