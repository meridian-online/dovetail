//! The DuckDB engine dovetail links is the one it is pinned to. relate and the
//! extensions it loads depend on the engine's exact version string: DuckDB
//! fetches extensions from a path named after it, so a build that names itself
//! anything else cannot install them.

#[test]
fn the_bundled_duckdb_is_2_0_0() {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    let version: String = conn
        .query_row("SELECT version()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        version, "v2.0.0",
        "the bundled DuckDB reports {version}; .cargo/config.toml names the build v2.0.0, \
         and a CXXFLAGS set in the shell replaces it"
    );
}

/// The finetype tag dovetail pins, read from the workspace manifest, without its
/// leading `v`.
fn pinned_finetype_version() -> &'static str {
    let manifest = include_str!("../../../Cargo.toml");
    let line = manifest
        .lines()
        .find(|line| line.starts_with("finetype-core"))
        .expect("the workspace Cargo.toml declares finetype-core");
    let tag = line
        .split("tag = \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("finetype-core is pinned to a tag");
    tag.strip_prefix('v').unwrap_or(tag)
}

/// The FineType DuckDB extension that dovetail loads is the release of the
/// version dovetail links. Point `DOVETAIL_FINETYPE_EXTENSION` at that release's
/// `.duckdb_extension` file to run it.
#[test]
fn the_release_extension_reports_the_pinned_version() {
    let Some(extension) = std::env::var_os("DOVETAIL_FINETYPE_EXTENSION")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
    else {
        eprintln!(
            "skipped: set DOVETAIL_FINETYPE_EXTENSION to a FineType release's .duckdb_extension file"
        );
        return;
    };

    // DuckDB derives the extension's entry point from its file name, so the
    // release's own name (finetype-vX.Y.Z-<target>.duckdb_extension) does not load.
    let dir = std::env::temp_dir().join(format!("dovetail-ft-extension-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let copy = dir.join("finetype.duckdb_extension");
    std::fs::copy(&extension, &copy).unwrap();

    let config = duckdb::Config::default()
        .allow_unsigned_extensions()
        .unwrap();
    let conn = duckdb::Connection::open_in_memory_with_flags(config).unwrap();
    conn.execute_batch(&format!("LOAD '{}'", copy.display()))
        .unwrap();
    let version: String = conn
        .query_row("SELECT ft_version()", [], |row| row.get(0))
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(version, format!("finetype {}", pinned_finetype_version()));
}
