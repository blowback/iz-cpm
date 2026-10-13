#![cfg(feature = "videobeast")]
// Line counter diagnostic: VBDIAG.COM (source in
// tests/artifacts/vb/vbdiag.asm) samples the raster line counter raw in
// modes 0 and 1, so its output here can be set beside the NanoBeast's
// (ticket 12, line counter rate differs from hardware).
//
// This file holds one test so that only one SDL instance exists per process.

mod common;
use common::*;
use izcpm::Step;

// Timing model: 125,000 ps per T-state (8 MHz); a read sees the counter as of
// the start of its instruction. The counter is beastem's currentLine: 0 at
// the frame start, +1 per line, blanking included.
//   mode 0:  800 x 40,000 ps = 32.000 us = 256.000 T per line, 525 lines
//   mode 1: 1088 x 29,767 ps = 32.386 us = 259.091 T per line, 517 lines
//
// "0xFF" fields: 0xFF reads back the byte written, at once and after the
// wrap, so both are the mode written: 00 00, then 01 01.
//
// MAX: the last reading before the first drop WAIT_WRAP sees (HL < DE).
// That is the frame's highest line only if the counter never falls inside a
// frame, which holds in this model (not on hardware, where 0xFB drops
// mid-frame). Here the drop is the wrap and the highest line is the last,
// 524 = 0x020C in mode 0 and 516 = 0x0204 in mode 1. WAIT_WRAP reads the
// low byte every 140 T (203 T when READ_LINE retries), under one line in
// either mode, so a poll always lands in the last line.
//
// Raw samples: let F be the frame start and t_L the low-byte read in
// READ_LINE that first sees the wrap. The read before it saw the last line,
// so t_L - F is in [0, 203). From t_L to the first read of the next sample:
//   rest of READ_LINE (13+4+13+4+7+10)          =  51 T
//   wrap test and RET C (4+4+4+4+11)            =  27 T
//                                               =  78 T
// Within a sample the 0xFB, 0xFA, 0xFB reads start at +0, +26, +52 T, so
// the 0xFA read of the first sample after a wrap is at F + [104, 307) T.
// From one sample to the next, start to start:
//   delay A: 78 + 10 + 512 x 26 - 5             = 13395 T
//   delay B: 78 + 10 + 1024 x 26 - 5           = 26707 T
// so the 0xFA read of the second sample is at
//   A: F + [13499, 13702) T,  B: F + [26811, 27014) T.
// Every read is before line 256, so every 0xFB byte is 00. Lines:
//              first sample      A second             B second
//   mode 0:    [0.41, 1.20)      [52.73, 53.52)       [104.73, 105.52)
//   mode 1:    [0.40, 1.18)      [52.10, 52.88)       [103.48, 104.26)
// Mode 1 delay A is fixed at 52 = 0x34. Where a range spans a line
// boundary the emulator is deterministic and always gives the same one:
// first samples 00, mode 0 A 53 = 0x35, mode 0 B 105 = 0x69 (as
// VBTRACE's LINES 0069), mode 1 B 103 = 0x67.
//
// START and END bracket the long delay (80,000,009 T = 10.0 s at 8 MHz)
// the user times with a stopwatch on hardware.
#[test]
fn diagnostic_prints_pinned_line_samples() {
    run_script_with_args(vec!(
        Step::Expect(concat!(
            "M 00 FF 00 00 MAX 020C\r\n",
            "A 00 00 00 00 35 00\r\n",
            "B 00 00 00 00 69 00\r\n",
            "M 01 FF 01 01 MAX 0204\r\n",
            "A 00 00 00 00 34 00\r\n",
            "B 00 00 00 00 67 00\r\n",
            "START\r\n",
            "END\r\n",
        )),
        ), vec!("tests/artifacts/vb/VBDIAG.COM")
    );
}
