#!/bin/sh
# Reassembles VBTRACE.COM, VBDIAG.COM and VBROWS.COM from their sources with sjasmplus
# 1.24.0 (override with $SJASMPLUS). cargo test never runs this; it uses the
# committed .COM files.
set -eu
cd "$(dirname "$0")"
"${SJASMPLUS:-sjasmplus}" --nologo --raw=VBTRACE.COM vbtrace.asm
"${SJASMPLUS:-sjasmplus}" --nologo --raw=VBDIAG.COM vbdiag.asm
"${SJASMPLUS:-sjasmplus}" --nologo --raw=VBROWS.COM vbrows.asm
