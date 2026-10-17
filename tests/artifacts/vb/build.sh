#!/bin/sh
# Reassembles VBTRACE.COM, VBDIAG.COM, VBROWS.COM, PASS.COM, LOOP.COM,
# FF2B.COM, JOYECHO.COM, VBPIC.COM, JOYMOVE.COM, CONPOLL.COM and CONSLOW.COM
# from their sources with sjasmplus 1.24.0 (override with $SJASMPLUS). cargo test
# never runs this; it uses the committed .COM files.
set -eu
cd "$(dirname "$0")"
"${SJASMPLUS:-sjasmplus}" --nologo --raw=VBTRACE.COM vbtrace.asm
"${SJASMPLUS:-sjasmplus}" --nologo --raw=VBDIAG.COM vbdiag.asm
"${SJASMPLUS:-sjasmplus}" --nologo --raw=VBROWS.COM vbrows.asm
"${SJASMPLUS:-sjasmplus}" --nologo --raw=PASS.COM pass.asm
"${SJASMPLUS:-sjasmplus}" --nologo --raw=LOOP.COM loop.asm
"${SJASMPLUS:-sjasmplus}" --nologo --raw=FF2B.COM ff2b.asm
"${SJASMPLUS:-sjasmplus}" --nologo --raw=JOYECHO.COM joyecho.asm
"${SJASMPLUS:-sjasmplus}" --nologo --raw=VBPIC.COM vbpic.asm
"${SJASMPLUS:-sjasmplus}" --nologo --raw=JOYMOVE.COM joymove.asm
"${SJASMPLUS:-sjasmplus}" --nologo --raw=CONPOLL.COM conpoll.asm
"${SJASMPLUS:-sjasmplus}" --nologo --raw=CONSLOW.COM conslow.asm
