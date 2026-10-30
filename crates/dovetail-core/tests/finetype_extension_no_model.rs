//! An extension without its model falls back to the floor. Its own test binary,
//! so no other test has loaded FineType's process-wide classifier first: the
//! extension keeps the first model it loads for the life of the process.

use dovetail_core::finetype_ext::{load, TypingRequest, TypingSource};

#[test]
fn missing_model_falls_back_to_floor() {
    // The real release file when one is configured; otherwise any existing file
    // stands in, since the model is checked before the extension is loaded.
    let scratch = std::env::temp_dir().join(format!("dovetail-no-model-{}", std::process::id()));
    let model_dir = scratch.join("model");
    std::fs::create_dir_all(&model_dir).unwrap();
    if std::env::var_os("DOVETAIL_FINETYPE_EXTENSION").is_none() {
        let stand_in = scratch.join("finetype.duckdb_extension");
        std::fs::write(&stand_in, b"not an extension").unwrap();
        std::env::set_var("DOVETAIL_FINETYPE_EXTENSION", &stand_in);
    }
    std::env::set_var("FINETYPE_MODEL_DIR", &model_dir);

    let conn = duckdb::Connection::open_in_memory().unwrap();
    let source = load(&conn, TypingRequest::Best);
    std::fs::remove_dir_all(&scratch).ok();

    match source {
        TypingSource::Floor { reason } => {
            assert!(
                reason.contains("FINETYPE_MODEL_DIR") && reason.contains("model"),
                "the reason does not name the model: {reason}"
            );
            assert!(
                reason.contains(&model_dir.display().to_string()),
                "the reason does not name the directory: {reason}"
            );
        }
        other => panic!("an empty model directory gave {other:?}"),
    }
}
