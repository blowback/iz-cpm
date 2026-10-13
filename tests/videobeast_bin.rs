#![cfg(feature = "videobeast")]
// Runs the built binary, so the VideoBeast's stdout/stderr split and the
// vendored exit(1) on a missing asset can be seen from outside. Each test
// is its own process, so they may run in parallel.

use std::process::{Command, Output, Stdio};

fn run_tracer(assets: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_iz-cpm"));
    cmd.arg("tests/artifacts/vb/VBTRACE.COM").stdin(Stdio::null());
    if let Some(dir) = assets {
        cmd.env("BEASTEM_ASSETS", dir);
    }
    cmd.output().expect("run iz-cpm")
}

#[test]
fn stdout_is_the_cp_m_console_only() {
    let out = run_tracer(None);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "VRAM A5 5A\r\nF9 20\r\nLINES 0069\r\n"
    );
}

#[test]
fn missing_assets_end_the_process_with_a_message() {
    let empty = std::env::temp_dir().join(format!("izcpm-no-assets-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let out = run_tracer(Some(empty.to_str().unwrap()));
    std::fs::remove_dir(&empty).unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("video_registers.mem"));
}
