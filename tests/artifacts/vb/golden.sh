#!/bin/sh
# Regenerates the golden --dump-at/--png-at outputs under golden/ (AD-14).
# Run after re-vendoring beastem (or changing the capture code), then review
# what changed with `git diff --stat tests/artifacts/vb/golden` and commit.
# tests/dump_png.rs checks that a fresh run matches these files byte for byte.
#
# Cases (keep in step with GOLDEN_CASES in tests/dump_png.rs):
#   vbpic    --dump-at 0,2 --png-at 0,2 --frames 3 VBPIC.COM
#   joyecho  --joy tests/artifacts/vb/joy.txt --dump-at 0,5 --png-at 5 JOYECHO.COM
set -eu
cd "$(dirname "$0")/../../.."
cargo build -q --features videobeast
BIN="${CARGO_TARGET_DIR:-target}/debug/iz-cpm"
GOLDEN=tests/artifacts/vb/golden
rm -rf "$GOLDEN"
"$BIN" --dump-at 0,2 --png-at 0,2 --frames 3 --out "$GOLDEN/vbpic" \
    tests/artifacts/vb/VBPIC.COM >/dev/null
"$BIN" --joy tests/artifacts/vb/joy.txt --dump-at 0,5 --png-at 5 --out "$GOLDEN/joyecho" \
    tests/artifacts/vb/JOYECHO.COM >/dev/null
