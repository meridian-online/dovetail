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
