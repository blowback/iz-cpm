#![cfg(feature = "videobeast")]
// Dumps and PNGs at chosen frames (architecture AD-20): `--dump-at LIST`
// and `--png-at LIST` write files to `--out DIR` as each listed frame
// starts. Each test runs the built binary in its own process and writes
// under CARGO_TARGET_TMPDIR.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const JOYECHO: &str = "tests/artifacts/vb/JOYECHO.COM";
const VBPIC: &str = "tests/artifacts/vb/VBPIC.COM";
const JOY_SCRIPT: &str = "tests/artifacts/vb/joy.txt";

/// The dump files for one frame and their sizes (README "Scripted runs").
const DUMPS: [(&str, u64); 6] = [
    ("-vram.bin", 1_048_576),
    ("-regs.bin", 256),
    ("-pal1.bin", 512),
    ("-pal2.bin", 512),
    ("-z80.bin", 65_536),
    ("-ram.bin", 524_288),
];

/// The cases tests/artifacts/vb/golden.sh writes to golden/<case>/; keep the
/// two in step.
const GOLDEN_CASES: [(&str, &[&str]); 2] = [
    ("vbpic", &["--dump-at", "0,2", "--png-at", "0,2", "--frames", "3", VBPIC]),
    ("joyecho", &["--joy", JOY_SCRIPT, "--dump-at", "0,5", "--png-at", "5", JOYECHO]),
];

fn run_iz_cpm(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_iz-cpm"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run iz-cpm")
}

/// Runs iz-cpm with `--out dir` added.
fn run_with_out(dir: &Path, args: &[&str]) -> Output {
    let mut all = vec!["--out", dir.to_str().unwrap()];
    all.extend_from_slice(args);
    run_iz_cpm(&all)
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// An empty path under CARGO_TARGET_TMPDIR, unique to `name`.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dump_png").join(name);
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_file(&dir);
    fs::create_dir_all(dir.parent().unwrap()).unwrap();
    dir
}

/// File names in `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {}", dir.display(), e))
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

/// Decodes a PNG, checking it is 8-bit RGB. Returns (width, height, rgb).
fn decode_png(path: &Path) -> (u32, u32, Vec<u8>) {
    let file = std::io::BufReader::new(fs::File::open(path).unwrap());
    let mut reader = png::Decoder::new(file).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

/// beastem's colour for a packed 0RRRrrGGGggBBBbb entry: each 3-bit
/// component c becomes (int)(c * (255/7.0)).
fn beastem_rgb(packed: u16) -> [u8; 3] {
    let c = |shift: u16| (((packed >> shift) & 7) as f64 * (255.0 / 7.0)) as u8;
    [c(12), c(7), c(2)]
}

/// Asserts the run was rejected with exit 1 before the VideoBeast (and so
/// the CPU) started, with `needle` on stderr.
fn assert_rejected(out: &Output, needle: &str) {
    let err = stderr(out);
    assert!(out.stdout.is_empty(), "stdout: {:?}", stdout(out));
    assert!(err.contains(needle), "stderr: {:?}", err);
    assert!(!err.contains("Read 128 entries"), "stderr: {:?}", err);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn dumps_have_the_documented_names_and_sizes_and_leave_stdout_alone() {
    let dir = scratch("dumps");
    let out = run_with_out(&dir, &["--dump-at", "0,2", JOYECHO]);
    assert_eq!(stdout(&out), "JOY 00 00 00 00 00 00 00 00\r\n", "stderr: {:?}", stderr(&out));
    assert_eq!(out.status.code(), Some(0));
    let mut expected: Vec<String> = Vec::new();
    for frame in ["f00000", "f00002"] {
        for (suffix, size) in DUMPS {
            let name = format!("{}{}", frame, suffix);
            let len = fs::metadata(dir.join(&name)).unwrap().len();
            assert_eq!(len, size, "{}", name);
            expected.push(name);
        }
    }
    expected.sort();
    assert_eq!(names(&dir), expected);
}

#[test]
fn png_of_a_known_picture() {
    let dir = scratch("png");
    let out = run_with_out(&dir, &["--png-at", "2", "--frames", "3", VBPIC]);
    assert_eq!(out.status.code(), Some(0), "stderr: {:?}", stderr(&out));
    assert_eq!(names(&dir), ["f00002.png"]);
    let (w, h, rgb) = decode_png(&dir.join("f00002.png"));
    assert_eq!((w, h), (640, 480));

    // VBPIC sets mode 0 (640 x 480, not doubled) in frame 0, so frame 1 is
    // scanned undoubled and frame 2 starts with it on the surface, one
    // surface pixel per VideoBeast pixel. beastem draws a layer on line L
    // when 8 * top <= L < 8 * (bottom + 1) and from x = 8 * left to
    // 8 * right - 1; it renders line L into surface row L. Layer 0 has top 2,
    // bottom 3, left 2, right 3, so it covers x 16-23, y 16-31. With scroll 0
    // pixel x reads row byte x - 16 of VRAM row y - 16, which VBPIC set to
    // 1 for bytes 0-15 of rows 0-15: palette 2 entry 1, 0x03E0. Every other
    // pixel is the background, 0x7C00.
    let green = beastem_rgb(0x03E0);
    let red = beastem_rgb(0x7C00);
    assert_eq!(green, [0, 255, 0]);
    assert_eq!(red, [255, 0, 0]);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let px = &rgb[(y * w as usize + x) * 3..][..3];
            let want = if (16..24).contains(&x) && (16..32).contains(&y) { green } else { red };
            assert_eq!(px, want, "pixel ({}, {})", x, y);
        }
    }
}

#[test]
fn frames_n_with_png_at_n_writes_frame_n() {
    let dir = scratch("frames_n");
    let out = run_with_out(&dir, &["--frames", "2", "--png-at", "2", "--dump-at", "2", VBPIC]);
    assert_eq!(out.status.code(), Some(0), "stderr: {:?}", stderr(&out));
    assert!(dir.join("f00002.png").is_file());
    assert!(dir.join("f00002-vram.bin").is_file());
    assert!(!stderr(&out).contains("not reached"), "stderr: {:?}", stderr(&out));
}

#[test]
fn frame_0_holds_the_program_and_the_start_up_registers() {
    let dir = scratch("frame0");
    let out = run_with_out(&dir, &["--dump-at", "0", "--png-at", "0", JOYECHO]);
    assert_eq!(out.status.code(), Some(0), "stderr: {:?}", stderr(&out));

    let com = fs::read(JOYECHO).unwrap();
    let z80 = fs::read(dir.join("f00000-z80.bin")).unwrap();
    assert_eq!(&z80[0x100..0x100 + com.len()], &com[..]);

    // beastem loads video_registers.mem from register 0xFF downwards,
    // skipping blank lines and lines starting '-'; the rest start at 0.
    let mem = fs::read_to_string("vendor/beastem/assets/video_registers.mem").unwrap();
    let mut startup = [0u8; 256];
    let mut r = 0xFF;
    for line in mem.lines().filter(|l| !l.is_empty() && !l.starts_with('-')) {
        let hex = line.split_whitespace().next().unwrap();
        startup[r] = u8::from_str_radix(hex, 16).unwrap();
        r -= 1;
    }
    let regs = fs::read(dir.join("f00000-regs.bin")).unwrap();
    assert_eq!(regs[0xFF], 0x08);
    assert_eq!(&regs[..], &startup[..]);

    // The start-up background, register 0xFD/0xFC, fills the surface.
    let background = beastem_rgb(u16::from_be_bytes([startup[0xFD], startup[0xFC]]));
    let (w, h, rgb) = decode_png(&dir.join("f00000.png"));
    assert_eq!((w, h), (640, 480));
    assert!(rgb.chunks(3).all(|px| px == background));
}

#[test]
fn the_same_run_twice_gives_identical_files() {
    let args = ["--joy", JOY_SCRIPT, "--dump-at", "0,3,5", "--png-at", "3,5", JOYECHO];
    let a = scratch("repeat_a");
    let b = scratch("repeat_b");
    assert_eq!(run_with_out(&a, &args).status.code(), Some(0));
    assert_eq!(run_with_out(&b, &args).status.code(), Some(0));
    let files = names(&a);
    assert_eq!(files.len(), 3 * DUMPS.len() + 2);
    assert_eq!(names(&b), files);
    for name in files {
        assert!(fs::read(a.join(&name)).unwrap() == fs::read(b.join(&name)).unwrap(), "{} differs", name);
    }
}

#[test]
fn a_frame_never_reached_writes_nothing_and_says_so() {
    // JOYECHO warm boots in frame 7.
    let dir = scratch("unreached");
    let out = run_with_out(&dir, &["--png-at", "99", JOYECHO]);
    let err = stderr(&out);
    assert!(err.contains("iz-cpm: frame 99 not reached; nothing written for it\n"), "stderr: {:?}", err);
    assert_eq!(out.status.code(), run_iz_cpm(&[JOYECHO]).status.code());
    assert_eq!(out.status.code(), Some(0));
    assert!(names(&dir).is_empty());
}

#[test]
fn a_bad_list_exits_1_naming_flag_and_value() {
    let dir = scratch("bad_list");
    let out = run_with_out(&dir, &["--dump-at", "1,,x", JOYECHO]);
    assert_rejected(&out, "--dump-at value \"1,,x\"");
    assert!(!dir.exists(), "--out was created");
    let out = run_with_out(&dir, &["--png-at", "2,x", JOYECHO]);
    assert_rejected(&out, "--png-at value \"2,x\"");
}

#[test]
fn a_list_without_out_exits_1() {
    let out = run_iz_cpm(&["--png-at", "1", JOYECHO]);
    assert_rejected(&out, "--out");
}

#[test]
fn an_out_that_is_a_file_exits_1_naming_it() {
    let path = scratch("out_is_file");
    fs::write(&path, b"not a directory").unwrap();
    let out = run_with_out(&path, &["--png-at", "1", JOYECHO]);
    assert_rejected(&out, path.to_str().unwrap());
}

#[test]
fn a_missing_out_is_created() {
    let dir = scratch("created").join("a").join("b");
    let out = run_with_out(&dir, &["--png-at", "1", JOYECHO]);
    assert_eq!(out.status.code(), Some(0), "stderr: {:?}", stderr(&out));
    assert_eq!(names(&dir), ["f00001.png"]);
}

#[test]
fn outputs_match_the_committed_goldens() {
    for (case, args) in GOLDEN_CASES {
        let golden = Path::new("tests/artifacts/vb/golden").join(case);
        let dir = scratch(&format!("golden_{}", case));
        let out = run_with_out(&dir, args);
        assert_eq!(out.status.code(), Some(0), "{}: stderr: {:?}", case, stderr(&out));
        let files = names(&dir);
        assert_eq!(files, names(&golden), "{}: file names (rerun golden.sh?)", case);
        for name in files {
            assert!(
                fs::read(dir.join(&name)).unwrap() == fs::read(golden.join(&name)).unwrap(),
                "{}/{} differs from the golden (rerun tests/artifacts/vb/golden.sh and review)",
                case,
                name
            );
        }
    }
}
