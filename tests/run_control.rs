#![cfg(feature = "videobeast")]
// Run control for scripted runs: stdout carries only the CP/M console, the
// exit code says how the run ended (0 normal end, 1 emulator error, 2 cycle
// limit), and a store to the firmware byte 0xFF2B leaves the BIOS jump table
// alone. Each test runs the built binary in its own process.

mod common;
use common::*;

use std::io::Write;
use std::process::{Command, Stdio};

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

#[test]
fn console_input_ending_before_a_warm_boot_exits_1() {
    // DIR is built into the CCP, so no program warm boots; the CCP's next
    // read finds stdin at end of file.
    let mut child = Command::new(env!("CARGO_BIN_EXE_iz-cpm"))
        .args(["-a", "tests/artifacts/vb", "--exit-on-warmboot"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start iz-cpm");
    child.stdin.take().unwrap().write_all(b"DIR\r").unwrap();
    let out = child.wait_with_output().expect("wait for iz-cpm");
    let err = stderr(&out);
    assert!(stdout(&out).contains("PASS"), "stdout: {:?}", stdout(&out));
    assert!(err.ends_with("iz-cpm: console input ended\n"), "stderr: {:?}", err);
    assert!(!err.contains("panicked"), "stderr: {:?}", err);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn console_polling_at_end_of_input_exits_1() {
    // CONPOLL.COM only polls with BDOS 11 while no key is waiting, so the
    // status check is what must find stdin at end of file.
    // No --max-cycles: that is the case covered. A deadline turns a hang
    // into a failure.
    let mut child = Command::new(env!("CARGO_BIN_EXE_iz-cpm"))
        .arg("tests/artifacts/vb/CONPOLL.COM")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start iz-cpm");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while child.try_wait().expect("poll iz-cpm").is_none() {
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("iz-cpm still running after 5 s polling at end of input");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let out = child.wait_with_output().expect("wait for iz-cpm");
    let err = stderr(&out);
    assert_eq!(stdout(&out), "");
    assert!(err.ends_with("iz-cpm: console input ended\n"), "stderr: {:?}", err);
    assert!(!err.contains("panicked"), "stderr: {:?}", err);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn console_polling_at_end_of_input_exits_1_before_the_cycle_limit() {
    let out = Command::new(env!("CARGO_BIN_EXE_iz-cpm"))
        .args(["--max-cycles", "2000000", "tests/artifacts/vb/CONPOLL.COM"])
        .stdin(Stdio::null())
        .output()
        .expect("run iz-cpm");
    let err = stderr(&out);
    assert_eq!(stdout(&out), "");
    assert!(err.ends_with("iz-cpm: console input ended\n"), "stderr: {:?}", err);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn piped_input_reaches_a_polling_program_before_input_ends() {
    // CONPOLL.COM echoes each key it reads, so stdout shows what reached it.
    let mut child = Command::new(env!("CARGO_BIN_EXE_iz-cpm"))
        .args(["--max-cycles", "2000000", "tests/artifacts/vb/CONPOLL.COM"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start iz-cpm");
    child.stdin.take().unwrap().write_all(b"abc").unwrap();
    let out = child.wait_with_output().expect("wait for iz-cpm");
    let err = stderr(&out);
    assert_eq!(stdout(&out), "abc");
    assert!(err.ends_with("iz-cpm: console input ended\n"), "stderr: {:?}", err);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn slow_polling_at_end_of_input_runs_to_the_cycle_limit() {
    // CONSLOW.COM polls with BDOS 11 about every 2,600 T-states, too far
    // apart to count as spinning on status, so stdin at end of file does not
    // end the run.
    let out = Command::new(env!("CARGO_BIN_EXE_iz-cpm"))
        .args(["--max-cycles", "2000000", "tests/artifacts/vb/CONSLOW.COM"])
        .stdin(Stdio::null())
        .output()
        .expect("run iz-cpm");
    let err = stderr(&out);
    assert!(out.stdout.is_empty(), "stdout: {:?}", stdout(&out));
    assert!(!err.contains("console input ended"), "stderr: {:?}", err);
    assert!(err.contains("--max-cycles 2000000"), "stderr: {:?}", err);
    assert_eq!(out.status.code(), Some(2));
}
