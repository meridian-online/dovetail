//! dovetail — thin CLI over dovetail-core (choice 0008). The survey/relate verbs
//! land in later ACs; this entry point exists so the workspace builds and the
//! detection layer has a host binary to grow into.

fn main() -> anyhow::Result<()> {
    eprintln!("dovetail: CLI surface not yet wired (survey emission is spec 2026-06-20-survey-detection-and-load, ac-05+)");
    Ok(())
}
