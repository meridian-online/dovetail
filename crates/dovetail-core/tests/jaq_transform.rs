//! jaq passthrough shim tests:
//! - ac-02 byte-equal passthrough (gate)
//! - ac-04 NDJSON default output
//! - ac-05 reproducibility (self-parity)
//! - ac-06 reference parity vs canonical jq on the curated subset
//! - ac-08 jaq version stamp

use std::path::{Path, PathBuf};
use std::process::Command;

use dovetail_core::transform::{run_jaq, JAQ_CORE_VERSION};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    program: String,
    input: String,
    #[serde(default)]
    jq_equivalent: bool,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn corpus() -> Vec<Case> {
    let path = repo_root().join("tests/jaq-corpus/cases.json");
    let text = std::fs::read_to_string(path).expect("read corpus");
    serde_json::from_str(&text).expect("parse corpus")
}

// ac-02 — the program run is byte-equal to the program passed in.
#[test]
fn passthrough_program_is_byte_equal() {
    for case in corpus() {
        let out = run_jaq(&case.program, case.input.as_bytes())
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert_eq!(out.program, case.program, "{}: program was rewritten", case.name);
    }
}

// ac-04 — output is valid NDJSON (each line parses as one JSON value).
#[test]
fn output_is_valid_ndjson() {
    for case in corpus() {
        let out = run_jaq(&case.program, case.input.as_bytes())
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        let ndjson = out.to_ndjson();
        for line in ndjson.lines() {
            serde_json::from_str::<serde_json::Value>(line)
                .unwrap_or_else(|e| panic!("{}: line not valid JSON: {line:?}: {e}", case.name));
        }
    }
}

// ac-04 — jaq's multi-format input: a YAML document converts and runs. (jaq-json
// reads JSON; YAML is converted to JSON first, exercising the non-JSON input path.)
#[test]
fn yaml_input_converts_and_runs() {
    let yaml = "name: ada\nid: 1\n";
    let json = serde_yaml::from_str::<serde_json::Value>(yaml).unwrap();
    let json_bytes = serde_json::to_vec(&json).unwrap();
    let out = run_jaq(".name", &json_bytes).unwrap();
    assert_eq!(out.values, vec!["\"ada\""]);
}

// ac-05 — two runs produce byte-identical output (self-parity).
#[test]
fn output_is_reproducible() {
    for case in corpus() {
        let a = run_jaq(&case.program, case.input.as_bytes()).unwrap().to_ndjson();
        let b = run_jaq(&case.program, case.input.as_bytes()).unwrap().to_ndjson();
        assert_eq!(a, b, "{}: output not reproducible", case.name);
    }
}

// ac-06 — embedded jaq matches canonical jq byte-for-byte on the equivalent
// subset. Skips with a notice when jq is absent (keeps CI dependency-free).
#[test]
fn reference_parity_vs_system_jq() {
    if !jq_available() {
        eprintln!("ac-06: system jq not found — skipping reference-parity check");
        return;
    }
    let mut mismatches = Vec::new();
    for case in corpus().into_iter().filter(|c| c.jq_equivalent) {
        let ours = run_jaq(&case.program, case.input.as_bytes())
            .unwrap_or_else(|e| panic!("{}: {e}", case.name))
            .to_ndjson();
        let theirs = system_jq(&case.program, &case.input);
        if ours != theirs {
            mismatches.push(format!(
                "{}: jaq={:?} jq={:?}", case.name, ours, theirs
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "reference-parity divergence on the curated subset (move to divergence-notes.md or fix):\n{}",
        mismatches.join("\n")
    );
}

// ac-08 — the embedded jaq version is retrievable and non-empty.
#[test]
fn jaq_version_is_stamped() {
    assert!(!JAQ_CORE_VERSION.is_empty());
    assert!(JAQ_CORE_VERSION.chars().next().unwrap().is_ascii_digit());
}

fn jq_available() -> bool {
    Command::new("jq").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn system_jq(program: &str, input: &str) -> String {
    use std::io::Write;
    let mut child = Command::new("jq")
        .arg("-c")
        .arg(program)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn jq");
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().expect("jq output");
    String::from_utf8(out.stdout).expect("jq utf8")
}
