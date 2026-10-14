#![cfg(feature = "videobeast")]
// Tracer: page 0x40 reaches the vendored VideoBeast. VBTRACE.COM (source in
// tests/artifacts/vb/vbtrace.asm) maps the VideoBeast into CPU page 2,
// round-trips two VRAM bytes and bank register 0xF9, and times the raster
// line counter over a delay.
//
// This file holds one test so that only one SDL instance exists per process.

mod common;
use common::*;

// Expected line delta, mode 0 (640x480): one line is 800 pixels x 40,000 ps
// = 32 us = 256 T-states at 8 MHz (125,000 ps per T-state).
//   delay loop: 10 + 1024 x 26 - 5             = 26629 T
//   plus the code between the two samples, start
//   to start of the two LD A,(REG_LINE_L) reads
//   (accesses are stamped at instruction start):
//   rest of READ_LINE from the first read, LD
//   (LINE0),HL, CALL, start of the next one   =   101 T
//   total 26730 T / 256                        = 104.41 lines
// The line count advances 104 or 105 times depending on where in a line
// the first sample falls (the delay alone gives 104, +-1 for the reads).
// The emulator is deterministic, so it is always 105 = 0x69.
#[test]
fn tracer_reaches_videobeast() {
    run_script_with_args(vec!(
        Step::Expect("VRAM A5 5A\r\n"),
        Step::Expect("F9 20\r\n"),
        Step::Expect("LINES 0069\r\n"),
        ), vec!("tests/artifacts/vb/VBTRACE.COM")
    );
}
