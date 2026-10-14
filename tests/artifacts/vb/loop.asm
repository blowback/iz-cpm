; LOOP.COM -- run-control test for iz-cpm feature `videobeast`.
;
; Never ends and prints nothing, so only --max-cycles stops it (exit 2).
; The loop sits past the TPA base: iz-cpm aborts a program whose PC
; returns to 0x0100.
;
; Rebuild with build.sh (sjasmplus 1.24.0).

        ORG 0x0100

START:
        NOP
.loop:
        JR .loop
