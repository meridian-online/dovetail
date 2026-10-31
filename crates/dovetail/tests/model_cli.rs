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

#[test]
fn out_naming_an_input_is_a_usage_error() {
    let dir = tmp_dir("out-is-input");
    let db = common::build_raw_retail_db(&dir);
    let packages = common::retail_packages_dir();
    let base = dir.join("base.ossie.yaml");
    let o = model(
        &db,
        &[
            "--packages",
            packages.to_str().unwrap(),
            "--pass",
            "sample",
            "--out",
            base.to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let before = std::fs::read(&base).unwrap();
    let db_before = std::fs::read(&db).unwrap();

    // The same file, spelled differently: a `..` detour and a path that is not there yet.
    let detour = dir.join("sub/../base.ossie.yaml");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    for (flag, other, out) in [
        ("--base", &base, &base),
        ("--base", &base, &detour),
        ("--verify", &base, &base),
        ("--db", &db, &db),
    ] {
        let mut extra = vec!["--pass", "full", "--out", out.to_str().unwrap()];
        if flag != "--db" {
            extra.extend([flag, other.to_str().unwrap()]);
        }
        let o = model(&db, &extra);
        assert_eq!(o.status.code(), Some(2), "{flag}: {}", stderr(&o));
        assert!(stderr(&o).starts_with("dovetail model: "), "{}", stderr(&o));
        assert_eq!(std::fs::read(&base).unwrap(), before, "{flag}");
        assert_eq!(std::fs::read(&db).unwrap(), db_before, "{flag}");
        assert!(!dir.join("base.ossie.yaml.writing").exists());
    }
}

#[test]
fn out_is_compared_after_resolving_dots_and_by_its_temporary_file() {
    let dir = tmp_dir("out-dots");
    let db = common::build_raw_retail_db(&dir);
    let base = dir.join("base.ossie.yaml");
    std::fs::write(&base, "keep me").unwrap();
    let writing = dir.join("base.ossie.yaml.writing");
    std::fs::write(&writing, "keep me too").unwrap();

    // `missing` is genuinely absent, so only a lexical resolution of `..` can match.
    let through_missing = dir.join("missing/../base.ossie.yaml");
    assert!(!dir.join("missing").exists());
    let o = model(
        &db,
        &[
            "--pass",
            "full",
            "--base",
            base.to_str().unwrap(),
            "--out",
            through_missing.to_str().unwrap(),
        ],
    );
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert!(stderr(&o).starts_with("dovetail model: "), "{}", stderr(&o));

    // The temporary file `--out` is written through is itself an input.
    let o = model(
        &db,
        &[
            "--pass",
            "full",
            "--base",
            writing.to_str().unwrap(),
            "--out",
            base.to_str().unwrap(),
        ],
    );
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert!(stderr(&o).contains(".writing"), "{}", stderr(&o));

    assert_eq!(std::fs::read_to_string(&base).unwrap(), "keep me");
    assert_eq!(std::fs::read_to_string(&writing).unwrap(), "keep me too");
    assert!(!dir.join("missing").exists());
}

#[cfg(unix)]
#[test]
fn out_is_resolved_through_a_symlinked_directory() {
    let dir = tmp_dir("out-symlink");
    let db = common::build_raw_retail_db(&dir);
    let real = dir.join("real/inner");
    std::fs::create_dir_all(&real).unwrap();
    let base = dir.join("real/base.ossie.yaml");
    std::fs::write(&base, "keep me").unwrap();
    // `work` points at `real/inner`, so `work/..` is `real`, not `dir`.
    std::os::unix::fs::symlink(&real, dir.join("work")).unwrap();

    let through_link = dir.join("work/../base.ossie.yaml");
    let o = model(
        &db,
        &[
            "--pass",
            "full",
            "--base",
            base.to_str().unwrap(),
            "--out",
            through_link.to_str().unwrap(),
        ],
    );
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert_eq!(std::fs::read_to_string(&base).unwrap(), "keep me");

    // The same text with a link that does not change what `..` means is not a clash:
    // `dir/base.ossie.yaml` is a different file from `dir/real/base.ossie.yaml`.
    let other = dir.join("base.ossie.yaml");
    let o = model(
        &db,
        &[
            "--pass",
            "sample",
            "--base",
            base.to_str().unwrap(),
            "--out",
            other.to_str().unwrap(),
        ],
    );
    assert_ne!(o.status.code(), Some(2), "{}", stderr(&o));
}
