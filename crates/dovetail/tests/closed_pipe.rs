//! When the reader of dovetail's stdout goes away, as `dovetail relate … | head`
//! does, dovetail stops the way `cat` does: ended by SIGPIPE, with nothing on
//! stderr. Rust ignores SIGPIPE by default, which turns the closed pipe into a
//! `println!` panic instead.

#![cfg(unix)]

use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

#[test]
fn a_closed_stdout_pipe_ends_dovetail_by_sigpipe_without_a_panic() {
    let dir = std::env::temp_dir().join("dovetail-cli-closed-pipe");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Several pipe buffers of output, so a write fails however fast the
    // reader closes.
    let input = dir.join("rows.ndjson");
    let rows: String = (0..20_000)
        .map(|i| format!("{{\"id\":{i},\"name\":\"row number {i}\"}}\n"))
        .collect();
    std::fs::write(&input, rows).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_dovetail"))
        .arg("jaq")
        .arg(".")
        .arg(&input)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run dovetail jaq");
    drop(child.stdout.take());
    let output = child.wait_with_output().expect("wait for dovetail jaq");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "dovetail panicked on a closed pipe:\n{stderr}"
    );
    assert_eq!(
        output.status.signal(),
        Some(libc::SIGPIPE),
        "expected dovetail to end by SIGPIPE, got {:?}; stderr:\n{stderr}",
        output.status
    );
}
