#![cfg(not(feature = "videobeast"))]
// VBDIAG.COM on a stock build, where nothing answers at page 0x40: every
// register reads 0xFF, so the line counter reads 0xFFFF and never wraps.
// Each wrap wait gives up after 65536 polls (DE keeps 0xFFFF, so MAX is
// FFFF), and the program still runs its long delay and exits.

mod common;
use common::*;
use izcpm::Step;

#[test]
fn diagnostic_exits_without_videobeast() {
    run_script_with_args(vec!(
        Step::Expect(concat!(
            "M 00 FF FF FF MAX FFFF\r\n",
            "A FF FF FF FF FF FF\r\n",
            "B FF FF FF FF FF FF\r\n",
            "M 01 FF FF FF MAX FFFF\r\n",
            "A FF FF FF FF FF FF\r\n",
            "B FF FF FF FF FF FF\r\n",
            "START\r\n",
            "END\r\n",
        )),
        ), vec!("tests/artifacts/vb/VBDIAG.COM")
    );
}
