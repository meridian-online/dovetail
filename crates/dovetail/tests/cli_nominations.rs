//! CLI-level proof of AC5: `dovetail survey --nominations` exits non-zero on
//! a nominated label the taxonomy does not carry, naming the column and the
//! label — the shape a script or CI job actually checks, not just the library
//! call underneath it.

use std::path::PathBuf;
use std::process::Command;

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dovetail-cli-nominations-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn survey_with_an_unknown_nominated_label_exits_non_zero_naming_column_and_label() {
    let dir = tmp_dir("ac5");
    let csv = dir.join("edgar.csv");
    std::fs::write(&csv, "corpus\nhello\n").unwrap();
    let nominations = dir.join("nominations.finetype.json");
    std::fs::write(
        &nominations,
        r#"{"resources":{"edgar":{"corpus":{"label":"not.a.real.label"}}}}"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_dovetail"))
        .arg("survey")
        .arg(&csv)
        .arg("--nominations")
        .arg(&nominations)
        .output()
        .expect("run dovetail survey");

    assert!(
        !output.status.success(),
        "expected a non-zero exit, got {:?}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("corpus"), "column not named: {stderr}");
    assert!(
        stderr.contains("not.a.real.label"),
        "label not named: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn survey_with_a_nominated_column_prints_the_marker_and_constraints() {
    let dir = tmp_dir("ac1-cli");
    let csv = dir.join("edgar.csv");
    std::fs::write(&csv, "corpus,id\nhello world,1\nmore text,2\n").unwrap();
    let nominations = dir.join("nominations.finetype.json");
    std::fs::write(
        &nominations,
        r#"{"resources":{"edgar":{"corpus":{"label":"representation.text.plain_text"}}}}"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_dovetail"))
        .arg("survey")
        .arg(&csv)
        .arg("--nominations")
        .arg(&nominations)
        .output()
        .expect("run dovetail survey");

    assert!(
        output.status.success(),
        "expected success, got {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(r#""x-dovetailSemanticType": "representation.text.plain_text""#),
        "{stdout}"
    );
    assert!(
        stdout.contains(r#""x-finetype-nominated": true"#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""minLength": 1"#), "{stdout}");
    assert!(stdout.contains(r#""maxLength": 65536"#), "{stdout}");

    let _ = std::fs::remove_dir_all(&dir);
}
