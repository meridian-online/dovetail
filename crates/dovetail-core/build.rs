//! Embeds the finetype tag the workspace pins as `DOVETAIL_FINETYPE_TAG`, so
//! the FineType extension dovetail loads is checked against the same version
//! its in-process finetype crates are built from, and the commit `Cargo.lock`
//! resolved that tag to as `DOVETAIL_FINETYPE_COMMIT` (`unknown` without a
//! lock file), which stamps the role corpus's profile cache.
//!
//! Assumes the workspace layout: this crate at `crates/dovetail-core`, with the
//! workspace `Cargo.toml` and `Cargo.lock` two directories up.

use std::path::Path;

fn main() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let text = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", manifest.display()));
    let tag = text
        .lines()
        .find(|line| line.starts_with("finetype-core"))
        .and_then(|line| line.split("tag = \"").nth(1))
        .and_then(|rest| rest.split('"').next())
        .expect("the workspace Cargo.toml pins finetype-core to a tag");
    println!("cargo:rustc-env=DOVETAIL_FINETYPE_TAG={tag}");

    let lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let commit = std::fs::read_to_string(&lock)
        .ok()
        .and_then(|text| {
            let mut lines = text
                .lines()
                .skip_while(|l| *l != "name = \"finetype-core\"");
            lines
                .find(|l| l.starts_with("source = "))
                .and_then(|l| l.split('#').nth(1))
                .map(|c| c.trim_end_matches('"').to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=DOVETAIL_FINETYPE_COMMIT={commit}");
}
