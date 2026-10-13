#![cfg(feature = "videobeast")]
// Run control for scripted runs: stdout carries only the CP/M console, the
// exit code says how the run ended (0 normal end, 1 emulator error, 2 cycle
// limit), and a store to the firmware byte 0xFF2B leaves the BIOS jump table
// alone. Each test runs the built binary in its own process.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run_iz_cpm(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_iz-cpm"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run iz-cpm")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn program_end_exits_0_with_only_console_text() {
    let out = run_iz_cpm(&["tests/artifacts/vb/PASS.COM"]);
    assert_eq!(stdout(&out), "PASS\r\n");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn program_end_via_ccp_with_exit_on_warmboot_exits_0() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_iz-cpm"))
        .args(["-a", "tests/artifacts/vb", "--exit-on-warmboot"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start iz-cpm");
    child.stdin.take().unwrap().write_all(b"PASS\r").unwrap();
    let out = child.wait_with_output().expect("wait for iz-cpm");
    let text = stdout(&out);
    assert!(text.ends_with("PASS\r\n"), "stdout: {:?}", text);
    assert!(!text.contains("iz-cpm"), "banner on stdout: {:?}", text);
    assert!(stderr(&out).contains("CP/M 2.2 Emulation"));
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn endless_loop_stops_at_max_cycles_with_exit_2() {
    let out = run_iz_cpm(&["--max-cycles", "1000000", "tests/artifacts/vb/LOOP.COM"]);
    assert!(out.stdout.is_empty(), "stdout: {:?}", stdout(&out));
    assert!(stderr(&out).contains("--max-cycles 1000000"), "stderr: {:?}", stderr(&out));
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn firmware_byte_store_is_ignored_and_logged() {
    let out = run_iz_cpm(&["tests/artifacts/vb/FF2B.COM"]);
    assert_eq!(stdout(&out), "FF2B 8E\r\n");
    assert!(
        stderr(&out).contains("iz-cpm: ignored write of 0x24 to 0xFF2B (firmware console byte)"),
        "stderr: {:?}",
        stderr(&out)
    );
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn emulator_errors_exit_1_with_message_on_stderr() {
    let missing = run_iz_cpm(&["tests/artifacts/vb/MISSING.COM"]);
    assert!(missing.stdout.is_empty(), "stdout: {:?}", stdout(&missing));
    assert!(stderr(&missing).contains("MISSING.COM"), "stderr: {:?}", stderr(&missing));
    assert_eq!(missing.status.code(), Some(1));

    let bad_limit = run_iz_cpm(&["--max-cycles", "lots", "tests/artifacts/vb/PASS.COM"]);
    assert!(bad_limit.stdout.is_empty(), "stdout: {:?}", stdout(&bad_limit));
    assert!(stderr(&bad_limit).contains("--max-cycles"), "stderr: {:?}", stderr(&bad_limit));
    assert_eq!(bad_limit.status.code(), Some(1));
}

#[test]
fn call_trace_goes_to_stderr() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_iz-cpm"))
        .args(["-t", "-a", "tests/artifacts/vb", "--exit-on-warmboot"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start iz-cpm");
    child.stdin.take().unwrap().write_all(b"PASS\r").unwrap();
    let out = child.wait_with_output().expect("wait for iz-cpm");
    let text = stdout(&out);
    assert!(stderr(&out).contains("[[Open file"), "stderr: {:?}", stderr(&out));
    assert!(!text.contains("[["), "trace on stdout: {:?}", text);
    assert!(text.ends_with("PASS\r\n"), "stdout: {:?}", text);
    assert_eq!(out.status.code(), Some(0));
}
