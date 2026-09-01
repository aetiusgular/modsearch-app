//! A30 cross-run identity: two fresh engine processes given the same request
//! (with an explicit dayEpoch) must produce byte-identical output, and a
//! different dayEpoch must produce different output. Each process gets its own
//! database file so state is identical-fresh and runs cannot collide.

use std::io::Write;
use std::process::{Command, Stdio};

fn oneshot(request: &str, tag: &str) -> Vec<u8> {
    let db = std::env::temp_dir().join(format!("mod-int-{}-{}.db", std::process::id(), tag));
    let _ = std::fs::remove_file(&db);
    let mut child = Command::new(env!("CARGO_BIN_EXE_modsearch-engine"))
        .arg("oneshot")
        .env("MOD_ENGINE_DB", &db)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("engine binary runs");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(request.as_bytes())
        .expect("request written");
    let out = child.wait_with_output().expect("engine exits");
    let _ = std::fs::remove_file(&db);
    assert!(out.status.success(), "engine exited non-zero");
    out.stdout
}

const REPLAY: &str = r#"{"id":1,"type":"getFeed","query":{},"hidden":[],"userId":"golden","dayEpoch":20000}"#;
const OTHER_DAY: &str = r#"{"id":1,"type":"getFeed","query":{},"hidden":[],"userId":"golden","dayEpoch":20001}"#;

#[test]
fn two_processes_are_byte_identical() {
    let a = oneshot(REPLAY, "a");
    let b = oneshot(REPLAY, "b");
    assert!(!a.is_empty());
    assert_eq!(a, b, "same request, same day, fresh state: output must be byte-identical");
}

#[test]
fn different_day_changes_the_feed() {
    let a = oneshot(REPLAY, "c");
    let b = oneshot(OTHER_DAY, "d");
    assert_ne!(a, b, "a different dayEpoch must reseed exploration");
}
