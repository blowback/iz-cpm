#![cfg(all(unix, feature = "videobeast"))]
// A VideoBeast that cannot start must end iz-cpm with exit code 1 and leave
// the terminal as it found it. Each test runs the built binary on a pty made
// by util-linux `script`, prints `stty -g` before and after, and compares.

mod common;
use common::*;

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

const VBTRACE: &str = "tests/artifacts/vb/VBTRACE.COM";
const LOOP: &str = "tests/artifacts/vb/LOOP.COM";
const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/vendor/beastem/assets");

/// What a run on a pty showed: terminal settings before and after, the exit
/// code, anything else on the pty (the program's stdout) and stderr.
struct PtyRun {
    before: String,
    after: String,
    rc: String,
    stdout: Vec<String>,
    stderr: String,
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn run_on_pty(name: &str, args: &[&str], env: &[(&str, &str)], unset: &[&str]) -> PtyRun {
    let err_file = scratch("startup_terminal", &format!("{}.stderr", name));
    let mut cmd = quote(env!("CARGO_BIN_EXE_iz-cpm"));
    for arg in args {
        cmd.push(' ');
        cmd.push_str(&quote(arg));
    }
    let shell = format!(
        "stty -g; {} 2>{}; echo rc=$?; stty -g",
        cmd,
        quote(err_file.to_str().unwrap())
    );
    let mut script = Command::new("script");
    for name in unset {
        script.env_remove(name);
    }
    let out = script
        .args(["-qec", &shell, "/dev/null"])
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .output()
        .expect("run script");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut lines: Vec<String> = text.lines().map(|l| l.trim_end_matches('\r').to_string()).collect();
    assert!(lines.len() >= 3, "script output: {:?}", text);
    let after = lines.pop().unwrap();
    let rc = lines.pop().unwrap();
    let before = lines.remove(0);
    let stderr = fs::read_to_string(&err_file).unwrap_or_default();
    PtyRun { before, after, rc, stdout: lines, stderr }
}

fn assert_failed_start(run: &PtyRun, last_stderr: &str) {
    assert_eq!(run.rc, "rc=1", "stderr: {:?}", run.stderr);
    assert!(run.stdout.is_empty(), "stdout: {:?}", run.stdout);
    let last = run.stderr.lines().last().unwrap_or("");
    assert!(last.contains(last_stderr), "last stderr line {:?}; stderr: {:?}", last, run.stderr);
    assert_eq!(run.before, run.after, "terminal settings changed");
}

/// An asset directory holding only `files`, copied from the vendored assets.
fn assets_with(name: &str, files: &[&str]) -> String {
    let dir = scratch("startup_terminal", name);
    fs::create_dir_all(&dir).unwrap();
    for file in files {
        fs::copy(Path::new(ASSETS).join(file), dir.join(file)).unwrap();
    }
    dir.to_str().unwrap().to_string()
}

#[test]
fn missing_registers_file_restores_the_terminal() {
    let dir = assets_with("no-assets", &[]);
    let run = run_on_pty("no-assets", &[VBTRACE], &[("BEASTEM_ASSETS", &dir)], &[]);
    assert_failed_start(&run, "Register file does not exist:");
    assert!(run.stderr.contains("video_registers.mem"), "stderr: {:?}", run.stderr);
}

#[test]
fn missing_palette_2_restores_the_terminal() {
    let dir = assets_with("no-palette-2", &["video_registers.mem", "palette_1.mem"]);
    let run = run_on_pty("no-palette-2", &[VBTRACE], &[("BEASTEM_ASSETS", &dir)], &[]);
    assert_failed_start(&run, "Palette file does not exist:");
    assert!(run.stderr.lines().last().unwrap().contains("palette_2.mem"), "stderr: {:?}", run.stderr);
}

#[test]
fn window_sdl_failure_restores_the_terminal() {
    let run = run_on_pty("no-such-driver", &["--window", LOOP], &[("SDL_VIDEODRIVER", "no-such-driver")], &[]);
    assert_failed_start(&run, "vbshim: SDL_Init failed:");
}

#[test]
fn window_without_a_display_restores_the_terminal() {
    // With no display to reach, SDL falls back to its offscreen driver.
    // XDG_RUNTIME_DIR goes too, or SDL may still find a Wayland socket.
    let unset = ["SDL_VIDEODRIVER", "DISPLAY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR"];
    // If SDL finds a display after all, the window opens and LOOP.COM would
    // run forever; the cycle limit ends it with exit 2, failing the test.
    let run = run_on_pty("no-display", &["--window", "--max-cycles", "8000000", LOOP], &[], &unset);
    assert_failed_start(&run, "vbshim: --window: no display to open a window on");
}
