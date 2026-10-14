#![cfg(feature = "videobeast")]
// Line counter row diagnostic: VBROWS.COM (source in
// tests/artifacts/vb/vbrows.asm) samples the raster line counter 160 times,
// exactly 1,024 T apart, in modes 0x00, 0x08 and 0x01, so its output here
// can be set beside the NanoBeast's (ticket 12, after VBDIAG hardware run 1).
//
// This file holds one test so that only one SDL instance exists per process.

mod common;
use common::*;

// Timing model: 125,000 ps per T-state (8 MHz); a read sees the counter as of
// the start of its instruction. The VideoBeast starts at time 0 with its
// first frame start one line later. The counter is beastem's currentLine:
// 0 at the frame start, +1 per line, blanking included; with 0x08
// (doubling) it is displayLine / 2, rounded down. A mode written to 0xFF
// takes effect at the next frame start.
//   mode 0:  800 x 40,000 ps = 256 T per line exactly, 525 lines
//            = 134,400 T per frame; frame n starts at 256 + 134,400 n T
//   mode 1: 1088 x 29,767 ps = 32,386,496 ps = 259.0920 T per line, 517
//            lines; the first line of the first mode-1 frame keeps mode 0's
//            256 T (beastem adds the old line length before switching)
//
// T of each mode write (W), from the program start:
//   entry code to the first ROWS (256 T, plus the two firmware calls, which
//   iz-cpm runs in Rust and charges only their RET, 10 T each)
//                                                W(00) =     276
//   one ROWS: write 13 + settle 300,020 (iz80 charges 8 T for the last,
//   untaken JR) + LD E 7 + 160 x 1,024 - 4 (the last JR NZ is untaken,
//   8 not 12), then RET 10, LD A,(nn) 13, CALL 17   = 463,916 T
//                                                W(08) = 464,192
//                                                W(01) = 928,108
// Sample k reads 0xFB at W + 300,040 + 1,024 k and 0xFA 26 T later; the
// output is the 0xFB byte of the first read and the 0xFA byte of the
// second.
//
// M 00 (start-up mode 0x08 is doubled; undoubled from frame 1, at
// 134,656 T):
//   first 0xFA read at 300,342 T; frame 2 starts at 269,056 T;
//   31,286 / 256 = 122.2 -> line 122 = 0x7A. 1,024 T = 4 lines exactly, so
//   sample k is line (122 + 4 k) mod 525: 0x20A at k = 100, then 0x0001.
// M 08 (doubled from frame 4, at 537,856 T):
//   first 0xFA read at 764,258 T; frame 5 starts at 672,256 T;
//   92,002 / 256 = 359.4 -> displayLine 359 -> line 179 = 0xB3. Each sample
//   is 4 display lines = 2 counts: 0x0105 (displayLine 523) at k = 41, then
//   displayLine 2 -> 0x0001.
// M 01 (mode 1 from frame 7, at 941,056 T):
//   frame 8 starts at 941,056 + 256 + 516 x 259.0920 = 1,075,003.5 T,
//   frame 9 at + 517 x 259.0920 = 1,208,954.0 T.
//   first 0xFA read at 1,228,174 T: 19,220.0 / 259.0920 = 74.2 -> 0x4A.
//   1,024 T = 3.952 lines, so the step is 4 except every 21 samples, from
//   k = 4, where it is 3 (0x0056 -> 0x0059); line 516 = 0x0204 at
//   k = 112, then 0x0003.
// Every 0xFB read here falls in the same line-256 half as its 0xFA read, so
// each hhll is one line number. The whole table was checked against these
// formulas evaluated in integer picoseconds.
#[test]
fn rows_diagnostic_prints_pinned_samples() {
    run_script_with_args(vec!(
        Step::Expect(concat!(
            "M 00\r\n",
            "007A 007E 0082 0086 008A 008E 0092 0096\r\n",
            "009A 009E 00A2 00A6 00AA 00AE 00B2 00B6\r\n",
            "00BA 00BE 00C2 00C6 00CA 00CE 00D2 00D6\r\n",
            "00DA 00DE 00E2 00E6 00EA 00EE 00F2 00F6\r\n",
            "00FA 00FE 0102 0106 010A 010E 0112 0116\r\n",
            "011A 011E 0122 0126 012A 012E 0132 0136\r\n",
            "013A 013E 0142 0146 014A 014E 0152 0156\r\n",
            "015A 015E 0162 0166 016A 016E 0172 0176\r\n",
            "017A 017E 0182 0186 018A 018E 0192 0196\r\n",
            "019A 019E 01A2 01A6 01AA 01AE 01B2 01B6\r\n",
            "01BA 01BE 01C2 01C6 01CA 01CE 01D2 01D6\r\n",
            "01DA 01DE 01E2 01E6 01EA 01EE 01F2 01F6\r\n",
            "01FA 01FE 0202 0206 020A 0001 0005 0009\r\n",
            "000D 0011 0015 0019 001D 0021 0025 0029\r\n",
            "002D 0031 0035 0039 003D 0041 0045 0049\r\n",
            "004D 0051 0055 0059 005D 0061 0065 0069\r\n",
            "006D 0071 0075 0079 007D 0081 0085 0089\r\n",
            "008D 0091 0095 0099 009D 00A1 00A5 00A9\r\n",
            "00AD 00B1 00B5 00B9 00BD 00C1 00C5 00C9\r\n",
            "00CD 00D1 00D5 00D9 00DD 00E1 00E5 00E9\r\n",
            "M 08\r\n",
            "00B3 00B5 00B7 00B9 00BB 00BD 00BF 00C1\r\n",
            "00C3 00C5 00C7 00C9 00CB 00CD 00CF 00D1\r\n",
            "00D3 00D5 00D7 00D9 00DB 00DD 00DF 00E1\r\n",
            "00E3 00E5 00E7 00E9 00EB 00ED 00EF 00F1\r\n",
            "00F3 00F5 00F7 00F9 00FB 00FD 00FF 0101\r\n",
            "0103 0105 0001 0003 0005 0007 0009 000B\r\n",
            "000D 000F 0011 0013 0015 0017 0019 001B\r\n",
            "001D 001F 0021 0023 0025 0027 0029 002B\r\n",
            "002D 002F 0031 0033 0035 0037 0039 003B\r\n",
            "003D 003F 0041 0043 0045 0047 0049 004B\r\n",
            "004D 004F 0051 0053 0055 0057 0059 005B\r\n",
            "005D 005F 0061 0063 0065 0067 0069 006B\r\n",
            "006D 006F 0071 0073 0075 0077 0079 007B\r\n",
            "007D 007F 0081 0083 0085 0087 0089 008B\r\n",
            "008D 008F 0091 0093 0095 0097 0099 009B\r\n",
            "009D 009F 00A1 00A3 00A5 00A7 00A9 00AB\r\n",
            "00AD 00AF 00B1 00B3 00B5 00B7 00B9 00BB\r\n",
            "00BD 00BF 00C1 00C3 00C5 00C7 00C9 00CB\r\n",
            "00CD 00CF 00D1 00D3 00D5 00D7 00D9 00DB\r\n",
            "00DD 00DF 00E1 00E3 00E5 00E7 00E9 00EB\r\n",
            "M 01\r\n",
            "004A 004E 0052 0056 0059 005D 0061 0065\r\n",
            "0069 006D 0071 0075 0079 007D 0081 0085\r\n",
            "0089 008D 0091 0095 0099 009D 00A1 00A5\r\n",
            "00A9 00AC 00B0 00B4 00B8 00BC 00C0 00C4\r\n",
            "00C8 00CC 00D0 00D4 00D8 00DC 00E0 00E4\r\n",
            "00E8 00EC 00F0 00F4 00F8 00FC 00FF 0103\r\n",
            "0107 010B 010F 0113 0117 011B 011F 0123\r\n",
            "0127 012B 012F 0133 0137 013B 013F 0143\r\n",
            "0147 014B 014F 0152 0156 015A 015E 0162\r\n",
            "0166 016A 016E 0172 0176 017A 017E 0182\r\n",
            "0186 018A 018E 0192 0196 019A 019E 01A2\r\n",
            "01A5 01A9 01AD 01B1 01B5 01B9 01BD 01C1\r\n",
            "01C5 01C9 01CD 01D1 01D5 01D9 01DD 01E1\r\n",
            "01E5 01E9 01ED 01F1 01F5 01F8 01FC 0200\r\n",
            "0204 0003 0007 000B 000F 0013 0017 001B\r\n",
            "001F 0023 0027 002B 002F 0033 0037 003B\r\n",
            "003F 0043 0046 004A 004E 0052 0056 005A\r\n",
            "005E 0062 0066 006A 006E 0072 0076 007A\r\n",
            "007E 0082 0086 008A 008E 0092 0096 0099\r\n",
            "009D 00A1 00A5 00A9 00AD 00B1 00B5 00B9\r\n",
        )),
        ), vec!("tests/artifacts/vb/VBROWS.COM")
    );
}
