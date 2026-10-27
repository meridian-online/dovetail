//! Embeds the finetype tag the workspace pins as `DOVETAIL_FINETYPE_TAG`, so
//! the FineType extension dovetail loads is checked against the same version
//! its in-process finetype crates are built from.

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
}
