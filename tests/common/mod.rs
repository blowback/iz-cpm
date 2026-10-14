use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use izcpm::ConsoleTest;
pub use izcpm::Step as Step;

#[allow(dead_code)]
pub fn run_script(script: Vec<Step>) {
    let mut console = ConsoleTest::new(script);
    izcpm::run(None, &mut console);
    assert!(console.finished(), "the run ended before the script was met");
}

#[allow(dead_code)]
pub fn run_script_with_args(script: Vec<Step>, args: Vec<&str>) {
    let mut console = ConsoleTest::new(script);
    izcpm::run(Some(args), &mut console);
    assert!(console.finished(), "the run ended before the script was met");
}

/// Runs the built binary with `args`, stdin empty.
#[allow(dead_code)]
pub fn run_iz_cpm(args: &[&str]) -> Output {
    run_iz_cpm_env(args, &[])
}

/// As `run_iz_cpm`, with `env` added to the environment.
#[allow(dead_code)]
pub fn run_iz_cpm_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_iz-cpm"))
        .args(args)
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .output()
        .expect("run iz-cpm")
}

#[allow(dead_code)]
pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[allow(dead_code)]
pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// An empty path under CARGO_TARGET_TMPDIR/`group`, unique to `name`: any
/// directory or file there is removed and the parent created; the path
/// itself is not.
#[allow(dead_code)]
pub fn scratch(group: &str, name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(group).join(name);
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_file(&dir);
    fs::create_dir_all(dir.parent().unwrap()).unwrap();
    dir
}

/// Asserts the run was rejected with exit 1 before the VideoBeast (and so
/// the CPU) started: stdout empty, no VideoBeast start-up line on stderr.
/// Returns stderr for the caller's own checks.
#[allow(dead_code)]
pub fn assert_rejected_before_start(out: &Output) -> String {
    let err = stderr(out);
    assert!(out.stdout.is_empty(), "stdout: {:?}", stdout(out));
    assert!(!err.contains("Read 128 entries"), "VideoBeast started; stderr: {:?}", err);
    assert_eq!(out.status.code(), Some(1), "stderr: {:?}", err);
    err
}
