#![cfg(feature = "videobeast")]
// `--window` and JOYMOVE.COM. JOYMOVE moves a 4 x 4 marker one pixel per
// frame per held direction and colours it by fire button; headless, a
// `--joy` script drives it and dumps show where it went. The window runs use
// SDL's dummy driver (SDL_VIDEODRIVER=dummy), so no display is needed: they
// check pacing, exit codes and that `--joy` wins over the keys. Each test
// runs the built binary in its own process.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const JOYMOVE: &str = "tests/artifacts/vb/JOYMOVE.COM";
const JOYMOVE_SCRIPT: &str = "tests/artifacts/vb/joymove.txt";
const JOYECHO: &str = "tests/artifacts/vb/JOYECHO.COM";
const LOOP: &str = "tests/artifacts/vb/LOOP.COM";

// JOYMOVE's field: row r at VRAM 0x20000 + 512 r, bytes 0-127, 128 rows.
const FIELD_VADDR: usize = 0x20000;
const FIELD_SIZE: usize = 128;
const ROW_BYTES: usize = 512;

const GREEN: u16 = 0x03E0;
const YELLOW: u16 = 0x7FE0;
const CYAN: u16 = 0x03FF;
const WHITE: u16 = 0x7FFF;

fn run(args: &[&str], window: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_iz-cpm"));
    command.args(args).stdin(Stdio::null());
    if window {
        command.env("SDL_VIDEODRIVER", "dummy");
    }
    command.output().expect("run iz-cpm")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// An empty path under CARGO_TARGET_TMPDIR, unique to `name`.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("window").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.parent().unwrap()).unwrap();
    dir
}

fn read(dir: &Path, frame: u64, suffix: &str) -> Vec<u8> {
    let path = dir.join(format!("f{:05}{}", frame, suffix));
    fs::read(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// The marker at the start of `frame`: top-left (x, y) of the 1s in the
/// field, which must form one 4 x 4 square, and palette 2 entry 1.
fn marker(dir: &Path, frame: u64) -> ((usize, usize), u16) {
    let vram = read(dir, frame, "-vram.bin");
    let mut ones = Vec::new();
    for y in 0..FIELD_SIZE {
        for x in 0..FIELD_SIZE {
            match vram[FIELD_VADDR + y * ROW_BYTES + x] {
                0 => {}
                1 => ones.push((x, y)),
                other => panic!("frame {}: byte {} at ({}, {})", frame, other, x, y),
            }
        }
    }
    let (x, y) = *ones.first().unwrap_or_else(|| panic!("frame {}: no marker", frame));
    let square: Vec<(usize, usize)> = (y..y + 4).flat_map(|r| (x..x + 4).map(move |c| (c, r))).collect();
    assert_eq!(ones, square, "frame {}: marker is not one 4 x 4 square", frame);
    let pal2 = read(dir, frame, "-pal2.bin");
    ((x, y), u16::from_le_bytes([pal2[2], pal2[3]]))
}

const DUMP_FRAMES: &str = "5,20,26,28,30,35";

fn run_joymove(dir: &Path) {
    let out = run(
        &["--joy", JOYMOVE_SCRIPT, "--dump-at", DUMP_FRAMES, "--frames", "36", "--out", dir.to_str().unwrap(), JOYMOVE],
        false,
    );
    assert!(out.stdout.is_empty(), "stdout: {:?}", stdout(&out));
    assert_eq!(out.status.code(), Some(0), "stderr: {:?}", stderr(&out));
}

#[test]
fn joymove_marker_follows_the_script() {
    // joymove.txt: right from frame 5, down + Z from 15, X at 25, C at 27,
    // nothing at 29, left + up from 30, nothing from 34. JOYMOVE reads the
    // port once per frame from frame 3 or so, after clearing the field; a
    // dump at frame F shows frames up to F - 1.
    let dir = scratch("joymove");
    run_joymove(&dir);
    assert_eq!(marker(&dir, 5), ((62, 62), GREEN));
    assert_eq!(marker(&dir, 20), ((72, 67), YELLOW));
    assert_eq!(marker(&dir, 26), ((72, 72), CYAN));
    assert_eq!(marker(&dir, 28), ((72, 72), WHITE));
    assert_eq!(marker(&dir, 30), ((72, 72), GREEN));
    assert_eq!(marker(&dir, 35), ((68, 68), GREEN));
}

#[test]
fn joymove_runs_give_identical_dumps() {
    let a = scratch("joymove-a");
    let b = scratch("joymove-b");
    run_joymove(&a);
    run_joymove(&b);
    for frame in DUMP_FRAMES.split(',').map(|f| f.parse::<u64>().unwrap()) {
        for suffix in ["-vram.bin", "-regs.bin", "-pal1.bin", "-pal2.bin", "-z80.bin", "-ram.bin"] {
            assert!(read(&a, frame, suffix) == read(&b, frame, suffix), "frame {} {} differs", frame, suffix);
        }
    }
}

#[test]
fn window_runs_at_real_speed() {
    // Frame 6 starts 256 + 6 x 134400 T-states in, about 101 ms at 8 MHz;
    // at least 5 frames of wall time (83 ms) must pass.
    let start = Instant::now();
    let out = run(&["--window", "--frames", "6", LOOP], true);
    let elapsed = start.elapsed();
    assert!(out.stdout.is_empty(), "stdout: {:?}", stdout(&out));
    assert!(stderr(&out).contains("iz-cpm: frame 6 started (--frames 6)"), "stderr: {:?}", stderr(&out));
    assert_eq!(out.status.code(), Some(0));
    assert!(elapsed >= Duration::from_millis(83), "took {:?}", elapsed);
}

#[test]
fn window_with_captures_writes_them() {
    let dir = scratch("window-png");
    let out = run(&["--window", "--frames", "3", "--png-at", "2", "--out", dir.to_str().unwrap(), JOYMOVE], true);
    assert_eq!(out.status.code(), Some(0), "stderr: {:?}", stderr(&out));
    assert!(dir.join("f00002.png").is_file());
}

#[test]
fn window_that_cannot_open_exits_1() {
    // An unknown driver makes SDL fail to start, on any host.
    let out = Command::new(env!("CARGO_BIN_EXE_iz-cpm"))
        .args(["--window", LOOP])
        .stdin(Stdio::null())
        .env("SDL_VIDEODRIVER", "no-such-driver")
        .output()
        .expect("run iz-cpm");
    assert!(out.stdout.is_empty(), "stdout: {:?}", stdout(&out));
    assert!(stderr(&out).contains("vbshim: SDL_Init failed"), "stderr: {:?}", stderr(&out));
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn window_without_keys_reads_0() {
    // The dummy driver has no keyboard, so no key is ever down.
    let out = run(&["--window", JOYECHO], true);
    assert_eq!(stdout(&out), "JOY 00 00 00 00 00 00 00 00\r\n", "stderr: {:?}", stderr(&out));
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn window_with_joy_follows_the_script() {
    let out = run(&["--window", "--joy", "tests/artifacts/vb/joy.txt", JOYECHO], true);
    assert_eq!(stdout(&out), "JOY 00 00 05 05 81 81 81 00\r\n", "stderr: {:?}", stderr(&out));
    assert_eq!(out.status.code(), Some(0));
}
