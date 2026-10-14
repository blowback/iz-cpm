#![cfg(feature = "videobeast")]
// `--vb-preload LOADTXT`: VRAM and palettes from a SLIDE file set, applied
// before frame 0's captures. Each test runs the built binary in its own
// process, with fixture sets it writes under CARGO_TARGET_TMPDIR.

mod common;
use common::*;

use std::fs;
use std::path::{Path, PathBuf};

const PASS: &str = "tests/artifacts/vb/PASS.COM";

/// An empty directory under CARGO_TARGET_TMPDIR, unique to `name`.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = scratch("preload", name);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// `len` bytes of a pattern that includes 0x1A and 0x00.
fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| (i as u8).wrapping_mul(7).wrapping_add(seed)).collect()
}

/// Writes `files` (paths relative to `set`) and `load.txt` into `set`.
fn fixture(set: &Path, load_txt: &[u8], files: &[(&str, &[u8])]) -> PathBuf {
    fs::create_dir_all(set).unwrap();
    for (name, data) in files {
        let path = set.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }
    let load = set.join("load.txt");
    fs::write(&load, load_txt).unwrap();
    load
}

/// Frame 0's dumps of PASS.COM, with `--vb-preload load` when given.
struct Dumps {
    vram: Vec<u8>,
    regs: Vec<u8>,
    pal1: Vec<u8>,
    pal2: Vec<u8>,
    stderr: String,
}

fn dump_frame_0(out_dir: &Path, load: Option<&Path>) -> Dumps {
    let mut args = vec!["--frames", "0", "--dump-at", "0", "--out", out_dir.to_str().unwrap()];
    if let Some(load) = load {
        args.extend_from_slice(&["--vb-preload", load.to_str().unwrap()]);
    }
    args.push(PASS);
    let out = run_iz_cpm(&args);
    assert_eq!(out.status.code(), Some(0), "stderr: {:?}", stderr(&out));
    let read = |suffix: &str| fs::read(out_dir.join(format!("f00000-{}.bin", suffix))).unwrap();
    Dumps { vram: read("vram"), regs: read("regs"), pal1: read("pal1"), pal2: read("pal2"), stderr: stderr(&out) }
}

/// The same dumps without `--vb-preload`.
fn baseline(dir: &Path) -> Dumps {
    dump_frame_0(&dir.join("baseline"), None)
}

/// Asserts `got` equals `base` with `data` written at `at`.
fn assert_overlaid(what: &str, got: &[u8], base: &[u8], at: usize, data: &[u8]) {
    let mut want = base.to_vec();
    want[at..at + data.len()].copy_from_slice(data);
    assert_eq!(got.len(), want.len(), "{} length", what);
    if let Some(i) = (0..got.len()).find(|&i| got[i] != want[i]) {
        panic!("{} byte {:#X}: got {:#04X}, want {:#04X}", what, i, got[i], want[i]);
    }
}

/// Asserts the run was refused with exit 1 before the CPU started, wrote no
/// capture files, and printed `line` on stderr.
fn assert_refused(out_dir: &Path, load: &Path, line: &str) {
    let out = run_iz_cpm(&[
        "--frames", "0", "--dump-at", "0", "--out", out_dir.to_str().unwrap(),
        "--vb-preload", load.to_str().unwrap(), PASS,
    ]);
    let err = assert_rejected_before_start(&out);
    assert!(err.contains(line), "stderr: {:?}", err);
    let written = fs::read_dir(out_dir).map(|d| d.count()).unwrap_or(0);
    assert_eq!(written, 0, "capture files written");
}

#[test]
fn vram_file_lands_at_its_address() {
    let dir = scratch_dir("vram");
    let data = pattern(300, 0x1A);
    let load = fixture(&dir.join("set"), b"v20000.vbm\n", &[("v20000.vbm", &data)]);
    let base = baseline(&dir);
    let got = dump_frame_0(&dir.join("out"), Some(&load));
    assert!(base.vram.iter().all(|&b| b == 0));
    assert_overlaid("vram", &got.vram, &base.vram, 0x20000, &data);
    assert_eq!(got.regs, base.regs);
    assert_eq!(got.pal1, base.pal1);
    assert_eq!(got.pal2, base.pal2);
}

#[test]
fn vram_file_may_end_at_the_top() {
    let dir = scratch_dir("top");
    let data = pattern(256, 3);
    let load = fixture(&dir.join("set"), b"vFFF00.vbm\n", &[("vFFF00.vbm", &data)]);
    let base = baseline(&dir);
    let got = dump_frame_0(&dir.join("out"), Some(&load));
    assert_overlaid("vram", &got.vram, &base.vram, 0xFFF00, &data);
}

#[test]
fn vram_file_past_the_top_exits_1() {
    let dir = scratch_dir("past_top");
    let load = fixture(&dir.join("set"), b"v00000.vbm\nvFFF80.vbm\n", &[
        ("v00000.vbm", &[1; 16]),
        ("vFFF80.vbm", &[2; 256]),
    ]);
    let line = format!(
        "iz-cpm: {}:2: vFFF80.vbm: 256 bytes at 0xFFF80 run past the top of VRAM\n",
        load.display()
    );
    assert_refused(&dir.join("out"), &load, &line);
}

#[test]
fn palette_file_loads_palette_1_bank_0() {
    let dir = scratch_dir("pal_bank");
    let data = pattern(128, 0x1A);
    let load = fixture(&dir.join("set"), b"v00.vbp\n", &[("v00.vbp", &data)]);
    let base = baseline(&dir);
    let got = dump_frame_0(&dir.join("out"), Some(&load));
    assert_overlaid("pal1", &got.pal1, &base.pal1, 0, &data);
    assert_eq!(got.pal2, base.pal2);
    assert_eq!(got.vram, base.vram);
    assert_eq!(got.regs, base.regs);
}

#[test]
fn palette_file_past_bank_3_is_cut_short() {
    let dir = scratch_dir("pal_overflow");
    let data = pattern(256, 9);
    let load = fixture(&dir.join("set"), b"v13.vbp\n", &[("v13.vbp", &data)]);
    let base = baseline(&dir);
    let got = dump_frame_0(&dir.join("out"), Some(&load));
    assert_overlaid("pal2", &got.pal2, &base.pal2, 384, &data[..128]);
    assert_eq!(got.pal1, base.pal1);
    assert_eq!(got.regs, base.regs);
}

#[test]
fn mixed_case_name_in_a_subdirectory() {
    let dir = scratch_dir("subdir");
    let data = pattern(40, 1);
    let load = fixture(&dir.join("set"), b"sub/V0aBcD.VBM\n", &[("sub/V0aBcD.VBM", &data)]);
    let base = baseline(&dir);
    let got = dump_frame_0(&dir.join("out"), Some(&load));
    assert_overlaid("vram", &got.vram, &base.vram, 0x0ABCD, &data);
}

#[test]
fn other_names_are_ignored_with_a_line_each() {
    let dir = scratch_dir("ignored");
    // None of them exists: ignored names are not read.
    let load = fixture(&dir.join("set"), b"GAME.COM\nv1234.vbm\nv24.vbp\n", &[]);
    let base = baseline(&dir);
    let got = dump_frame_0(&dir.join("out"), Some(&load));
    for (n, name) in ["GAME.COM", "v1234.vbm", "v24.vbp"].iter().enumerate() {
        let line = format!(
            "iz-cpm: {}:{}: ignoring {} (not a SLIDE VRAM or palette name)\n",
            load.display(),
            n + 1,
            name
        );
        assert!(got.stderr.contains(&line), "stderr: {:?}", got.stderr);
    }
    assert_eq!(got.vram, base.vram);
    assert_eq!(got.regs, base.regs);
    assert_eq!(got.pal1, base.pal1);
    assert_eq!(got.pal2, base.pal2);
}

#[test]
fn blank_lines_and_crlf_are_accepted_and_later_files_win() {
    let dir = scratch_dir("crlf");
    let first = pattern(64, 5);
    let second = pattern(16, 200);
    let load = fixture(&dir.join("set"), b"\r\nv01000.vbm  \r\n   \r\n\nv01010.vbm\r\n", &[
        ("v01000.vbm", &first),
        ("v01010.vbm", &second),
    ]);
    let base = baseline(&dir);
    let got = dump_frame_0(&dir.join("out"), Some(&load));
    let mut want = first.clone();
    want[0x10..0x20].copy_from_slice(&second);
    assert_overlaid("vram", &got.vram, &base.vram, 0x1000, &want);
}

#[test]
fn empty_files_write_nothing() {
    let dir = scratch_dir("empty");
    let load = fixture(&dir.join("set"), b"v20000.vbm\nv00.vbp\nv13.vbp\n", &[
        ("v20000.vbm", &[]),
        ("v00.vbp", &[]),
        ("v13.vbp", &[]),
    ]);
    let base = baseline(&dir);
    let got = dump_frame_0(&dir.join("out"), Some(&load));
    assert_eq!(got.vram, base.vram);
    assert_eq!(got.regs, base.regs);
    assert_eq!(got.pal1, base.pal1);
    assert_eq!(got.pal2, base.pal2);
}

#[test]
fn missing_listed_file_exits_1_naming_line_and_path() {
    let dir = scratch_dir("missing");
    let load = fixture(&dir.join("set"), b"v00.vbp\n\nsub/v00100.vbm\n", &[("v00.vbp", &[1; 128])]);
    let line = format!("iz-cpm: {}:3: sub/v00100.vbm: ", load.display());
    assert_refused(&dir.join("out"), &load, &line);
}

#[test]
fn missing_load_txt_exits_1_naming_it() {
    let dir = scratch_dir("no_load_txt");
    let load = dir.join("set").join("load.txt");
    assert_refused(&dir.join("out"), &load, &format!("iz-cpm: {}: ", load.display()));
}

#[test]
fn a_full_set_twice_gives_identical_dumps_and_unchanged_registers() {
    let dir = scratch_dir("repeat");
    let load = fixture(&dir.join("set"), b"v40000.vbm\nv00.vbp\nv12.vbp\nv03.vbp\n", &[
        ("v40000.vbm", &pattern(4096, 11)),
        ("v00.vbp", &pattern(512, 12)),
        ("v12.vbp", &pattern(256, 13)),
        ("v03.vbp", &pattern(128, 14)),
    ]);
    let base = baseline(&dir);
    let a = dump_frame_0(&dir.join("a"), Some(&load));
    let b = dump_frame_0(&dir.join("b"), Some(&load));
    assert!(a.vram == b.vram && a.pal1 == b.pal1 && a.pal2 == b.pal2 && a.regs == b.regs);
    assert_eq!(a.regs, base.regs);
    assert_ne!(a.pal1, base.pal1);
    assert_ne!(a.pal2, base.pal2);
    // Every dump file of the two runs is byte-identical.
    for entry in fs::read_dir(dir.join("a")).unwrap() {
        let name = entry.unwrap().file_name();
        assert!(fs::read(dir.join("a").join(&name)).unwrap() == fs::read(dir.join("b").join(&name)).unwrap());
    }
}
