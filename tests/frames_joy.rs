#![cfg(feature = "videobeast")]
// Frames and the scripted joystick (architecture AD-20): frame 0 starts with
// the program's first instruction at 0x100, frame n at the nth wrap of the
// VideoBeast line counter to 0. `--frames N` ends the run as frame N starts;
// `--joy FILE` sets what port 0x9F reads in each frame. Each test runs the
// built binary in its own process.

mod common;
use common::*;

use std::io::Write;
use std::process::{Command, Stdio};

const JOYECHO: &str = "tests/artifacts/vb/JOYECHO.COM";

/// The cycle in "iz-cpm: frame N started (--frames N) at cycle C".
fn frame_start_cycle(err: &str, frames: u64) -> u64 {
    let prefix = format!("iz-cpm: frame {} started (--frames {}) at cycle ", frames, frames);
    let line = err.lines().find(|l| l.starts_with(&prefix)).unwrap_or_else(|| panic!("stderr: {:?}", err));
    line[prefix.len()..].parse().expect("cycle number")
}

#[test]
fn scripted_joystick_changes_as_each_frame_starts() {
    let out = run_iz_cpm(&["--joy", "tests/artifacts/vb/joy.txt", JOYECHO]);
    assert_eq!(stdout(&out), "JOY 00 00 05 05 81 81 81 00\r\n", "stderr: {:?}", stderr(&out));
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn without_a_script_the_joystick_reads_0() {
    let out = run_iz_cpm(&[JOYECHO]);
    assert_eq!(stdout(&out), "JOY 00 00 00 00 00 00 00 00\r\n", "stderr: {:?}", stderr(&out));
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn frames_ends_the_run_as_frame_n_starts() {
    let out = run_iz_cpm(&["--frames", "3", JOYECHO]);
    assert!(out.stdout.is_empty(), "stdout: {:?}", stdout(&out));
    // Start-up mode 0x08: a frame is 525 lines x 256 T = 134400 T. beastem's
    // first frame start comes one line (256 T) after time 0, while the line
    // counter still reads 0 from init, so it is not a wrap; wrap n, and so
    // frame n, comes at 256 + n x 134400 T. Frame 3 starts at 403456 T. The
    // run stops at the first instruction boundary at or after that, and no
    // Z80 instruction takes more than 23 T.
    let start = 256 + 3 * 525 * 256;
    let cycle = frame_start_cycle(&stderr(&out), 3);
    assert!((start..start + 23).contains(&cycle), "cycle {}", cycle);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn zero_frames_runs_no_instruction() {
    let out = run_iz_cpm(&["--frames", "0", JOYECHO]);
    assert!(out.stdout.is_empty(), "stdout: {:?}", stdout(&out));
    assert_eq!(frame_start_cycle(&stderr(&out), 0), 0);
    // JOYECHO's entry stores 0x24 to 0xFF2B a few instructions in, which
    // would be logged; nothing ran, so nothing was.
    assert!(!stderr(&out).contains("0xFF2B"), "stderr: {:?}", stderr(&out));
    assert_eq!(out.status.code(), Some(0));
}

/// Runs JOYECHO with `--joy script` and checks it is rejected with exit 1,
/// stderr starting with `message`, before the VideoBeast or CPU started.
fn assert_bad_script(script: &str, message: &str) {
    let out = run_iz_cpm(&["--joy", script, JOYECHO]);
    let err = assert_rejected_before_start(&out);
    assert!(err.starts_with(message), "stderr: {:?}", err);
}

#[test]
fn missing_script_exits_1_naming_the_file() {
    assert_bad_script("tests/artifacts/vb/MISSING.TXT", "iz-cpm: tests/artifacts/vb/MISSING.TXT: ");
}

#[test]
fn malformed_script_line_exits_1_naming_file_and_line() {
    assert_bad_script("tests/artifacts/vb/joy-malformed.txt", "iz-cpm: tests/artifacts/vb/joy-malformed.txt:2: ");
}

#[test]
fn script_frame_out_of_order_exits_1_naming_file_and_line() {
    assert_bad_script(
        "tests/artifacts/vb/joy-bad.txt",
        "iz-cpm: tests/artifacts/vb/joy-bad.txt:2: frame 1 is not after frame 2",
    );
}

#[test]
fn bad_frames_value_exits_1() {
    let out = run_iz_cpm(&["--frames", "x", JOYECHO]);
    assert!(out.stdout.is_empty(), "stdout: {:?}", stdout(&out));
    assert!(stderr(&out).contains("--frames"), "stderr: {:?}", stderr(&out));
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn ccp_does_not_start_frames_until_the_program_does() {
    // `--frames 0` ends the run as the .COM starts, so the CCP must have run
    // (printed its prompt, read the command) before frame 0.
    let mut child = Command::new(env!("CARGO_BIN_EXE_iz-cpm"))
        .args(["-a", "tests/artifacts/vb", "--exit-on-warmboot", "--frames", "0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start iz-cpm");
    child.stdin.take().unwrap().write_all(b"PASS\r").unwrap();
    let out = child.wait_with_output().expect("wait for iz-cpm");
    let text = stdout(&out);
    // The CCP echoes the command; PASS.COM would print PASS a second time.
    assert_eq!(text.matches("PASS").count(), 1, "stdout: {:?}", text);
    let cycle = frame_start_cycle(&stderr(&out), 0);
    assert!(cycle > 0, "cycle {}", cycle);
    assert_eq!(out.status.code(), Some(0));
}
