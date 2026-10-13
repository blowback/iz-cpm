#!/bin/sh
# Reassembles VBTRACE.COM from vbtrace.asm with sjasmplus 1.24.0
# (override with $SJASMPLUS). cargo test never runs this; it uses the
# committed VBTRACE.COM.
set -eu
cd "$(dirname "$0")"
"${SJASMPLUS:-sjasmplus}" --nologo --raw=VBTRACE.COM vbtrace.asm
