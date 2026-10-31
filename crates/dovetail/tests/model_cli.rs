//! CLI-level proof of `dovetail model`: the built binary over the retail fixture
//! held the way Brightfield holds it, and the packages that name its tables.
//! The fixture builder is the core tests' shared helper, included by path.

#[path = "../../dovetail-core/tests/common/mod.rs"]
mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use dovetail_core::ossie;

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dovetail-cli-model-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn dovetail(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dovetail"))
        .arg("model")
        .args(args)
        .output()
        .unwrap()
}

fn model(db: &Path, extra: &[&str]) -> Output {
    let mut args: Vec<&std::ffi::OsStr> = vec!["--db".as_ref(), db.as_os_str()];
    args.extend(extra.iter().map(|a| std::ffi::OsStr::new(*a)));
    dovetail(&args)
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn read_model(path: &Path) -> ossie::Model {
    ossie::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn sample_then_full() {
    let dir = tmp_dir("sample-then-full");
    let db = common::build_raw_retail_db(&dir);
    let packages = common::retail_packages_dir();
    let sample = dir.join(".discovery/sample.ossie.yaml");
    let full = dir.join(".discovery/full.ossie.yaml");
    let p = packages.to_str().unwrap();

    let o = model(
        &db,
        &[
            "--packages",
            p,
            "--pass",
            "sample",
            "--out",
            sample.to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let o = model(
        &db,
        &[
            "--packages",
            p,
            "--pass",
            "full",
            "--verify",
            sample.to_str().unwrap(),
            "--out",
            full.to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!stderr(&o).contains("discovering afresh"), "{}", stderr(&o));

    let (s, f) = (read_model(&sample), read_model(&full));
    assert!(!s.relationships.is_empty());
    let names = |m: &ossie::Model| -> Vec<String> {
        m.relationships.iter().map(|r| r.name.clone()).collect()
    };
    assert_eq!(
        names(&s),
        names(&f),
        "the full pass keeps the joins in order"
    );
    for entry in std::fs::read_dir(dir.join(".discovery")).unwrap() {
        let path = entry.unwrap().path();
        assert!(!path.to_string_lossy().ends_with(".writing"), "{path:?}");
    }
}

#[test]
fn usage_errors_exit_2() {
    let dir = tmp_dir("usage");
    let db = common::build_raw_retail_db(&dir);
    let out = dir.join("m.ossie.yaml");
    let o = model(&db, &["--pass", "sample"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        stderr(&o).contains("usage: dovetail model"),
        "{}",
        stderr(&o)
    );
    let o = model(
        &db,
        &["--pass", "sometimes", "--out", out.to_str().unwrap()],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(!out.exists());
}

#[test]
fn missing_db_exits_1() {
    let dir = tmp_dir("missing-db");
    let out = dir.join("m.ossie.yaml");
    let o = model(
        &dir.join("absent.duckdb"),
        &["--pass", "sample", "--out", out.to_str().unwrap()],
    );
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).starts_with("dovetail model: "), "{}", stderr(&o));
    assert!(!out.exists());
    assert!(!dir.join("m.ossie.yaml.writing").exists());
}

#[test]
fn base_is_never_written() {
    let dir = tmp_dir("base");
    let db = common::build_raw_retail_db(&dir);
    let packages = common::retail_packages_dir();
    let first = dir.join("first.ossie.yaml");
    let again = dir.join("again.ossie.yaml");
    let o = model(
        &db,
        &[
            "--packages",
            packages.to_str().unwrap(),
            "--pass",
            "sample",
            "--out",
            first.to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let before = std::fs::read(&first).unwrap();
    let o = model(
        &db,
        &[
            "--packages",
            packages.to_str().unwrap(),
            "--pass",
            "sample",
            "--base",
            first.to_str().unwrap(),
            "--out",
            again.to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(std::fs::read(&first).unwrap(), before);
    assert!(again.exists());
}

#[test]
fn verify_with_the_sample_pass_warns_and_is_ignored() {
    let dir = tmp_dir("verify-sample");
    let db = common::build_raw_retail_db(&dir);
    let packages = common::retail_packages_dir();
    let out = dir.join("m.ossie.yaml");
    let o = model(
        &db,
        &[
            "--packages",
            packages.to_str().unwrap(),
            "--pass",
            "sample",
            "--verify",
            dir.join("absent.yaml").to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        stderr(&o).contains("dovetail model: --verify applies to the full pass only; ignored"),
        "{}",
        stderr(&o)
    );
    assert!(out.exists());
}

#[test]
fn model_help_exits_0_and_names_every_flag() {
    let o = dovetail(&["--help".as_ref()]);
    assert!(o.status.success());
    let text = String::from_utf8_lossy(&o.stdout);
    for flag in [
        "--db",
        "--pass",
        "--out",
        "--packages",
        "--base",
        "--verify",
        "--name",
    ] {
        assert!(text.contains(flag), "help lacks {flag}: {text}");
    }
}

#[test]
fn top_level_help_lists_model() {
    let o = Command::new(env!("CARGO_BIN_EXE_dovetail"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("model --db"));
}

#[test]
fn name_defaults_to_the_db_stem() {
    let dir = tmp_dir("name");
    let db = common::build_raw_retail_db(&dir);
    let packages = common::retail_packages_dir();
    let out = dir.join("m.ossie.yaml");
    let o = model(
        &db,
        &[
            "--packages",
            packages.to_str().unwrap(),
            "--pass",
            "sample",
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(read_model(&out).name, "retail");
}
