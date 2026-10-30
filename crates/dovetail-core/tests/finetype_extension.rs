//! dovetail loads FineType's DuckDB extension from the file attached to the
//! pinned finetype release, and falls back to the floor typing, with a reason,
//! whenever that extension or its model is not there or does not answer as the
//! pin says it should.
//!
//! The profiled test runs only when `DOVETAIL_FINETYPE_EXTENSION` and
//! `FINETYPE_MODEL_DIR` are set; `scripts/fetch-finetype.sh` prints both.

use std::sync::Mutex;

use dovetail_core::finetype_ext::{finetype_tag, load, TypingRequest, TypingSource};

/// The tests here read and set the same process environment, so they take
/// turns.
static ENV: Mutex<()> = Mutex::new(());

const EXTENSION: &str = "DOVETAIL_FINETYPE_EXTENSION";
const MODEL_DIR: &str = "FINETYPE_MODEL_DIR";

/// FineType's release extension is unsigned, and DuckDB fixes that setting when
/// the database opens.
fn connection() -> duckdb::Connection {
    let config = duckdb::Config::default()
        .allow_unsigned_extensions()
        .unwrap();
    duckdb::Connection::open_in_memory_with_flags(config).unwrap()
}

#[test]
fn tag_matches_cargo_toml() {
    let manifest = include_str!("../../../Cargo.toml");
    let line = manifest
        .lines()
        .find(|line| line.starts_with("finetype-core"))
        .expect("the workspace Cargo.toml declares finetype-core");
    assert!(
        line.contains(&format!("tag = \"{}\"", finetype_tag())),
        "finetype_tag() is {}, and the workspace pins: {line}",
        finetype_tag()
    );
}

#[test]
fn floor_request_never_loads() {
    let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let conn = connection();
    assert_eq!(
        load(&conn, TypingRequest::Floor),
        TypingSource::Floor {
            reason: "floor requested".to_string()
        }
    );
    let loaded: i64 = conn
        .query_row(
            "SELECT count(*) FROM duckdb_functions() WHERE function_name = 'ft_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(loaded, 0, "a floor request loaded the extension");
}

#[test]
fn missing_extension_falls_back() {
    let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = tempfile::tempdir().unwrap();
    let missing = scratch.path().join("absent/finetype.duckdb_extension");
    let source = with_vars(&[(EXTENSION, &missing)], || {
        load(&connection(), TypingRequest::Best)
    });

    match source {
        TypingSource::Floor { reason } => assert!(
            reason.contains(&missing.display().to_string()),
            "the reason does not name the missing path: {reason}"
        ),
        other => panic!("a missing extension gave {other:?}"),
    }
}

/// Set `vars` for the length of `body`, then restore what was there.
fn with_vars<T>(vars: &[(&str, &std::path::Path)], body: impl FnOnce() -> T) -> T {
    let saved: Vec<_> = vars
        .iter()
        .map(|(name, _)| (*name, std::env::var_os(name)))
        .collect();
    for (name, value) in vars {
        std::env::set_var(name, value);
    }
    let result = body();
    for (name, value) in saved {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
    result
}

#[test]
fn a_connection_without_unsigned_extensions_falls_back() {
    let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    // Stand-ins: the setting is checked before the extension is loaded.
    let scratch = tempfile::tempdir().unwrap();
    let extension = scratch.path().join("finetype.duckdb_extension");
    std::fs::write(&extension, b"not an extension").unwrap();
    let model = scratch.path().join("model");
    std::fs::create_dir(&model).unwrap();
    for file in ["model.safetensors", "label_map.json", "config.json"] {
        std::fs::write(model.join(file), b"").unwrap();
    }

    let conn = duckdb::Connection::open_in_memory().unwrap();
    let source = with_vars(&[(EXTENSION, &extension), (MODEL_DIR, &model)], || {
        load(&conn, TypingRequest::Best)
    });
    match source {
        TypingSource::Floor { reason } => assert!(
            reason.contains("allow_unsigned_extensions"),
            "the reason does not name the setting: {reason}"
        ),
        other => panic!("a connection refusing unsigned extensions gave {other:?}"),
    }
}

#[test]
fn loads_and_reports_pinned_version() {
    let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
    if std::env::var_os(EXTENSION).is_none() || std::env::var_os(MODEL_DIR).is_none() {
        eprintln!(
            "skipping: set {EXTENSION} and {MODEL_DIR} (scripts/fetch-finetype.sh prints both)"
        );
        return;
    }
    match load(&connection(), TypingRequest::Best) {
        TypingSource::Profiled {
            extension_version,
            model_sha256,
        } => {
            let version = finetype_tag().strip_prefix('v').unwrap_or(finetype_tag());
            assert_eq!(extension_version, format!("finetype {version}"));
            assert_eq!(model_sha256.len(), 64, "not a SHA-256: {model_sha256}");
            assert!(model_sha256.bytes().all(|b| b.is_ascii_hexdigit()));
        }
        TypingSource::Floor { reason } => panic!("fell back to the floor: {reason}"),
        TypingSource::Packages => panic!("load never answers with the packages typing"),
    }
}
