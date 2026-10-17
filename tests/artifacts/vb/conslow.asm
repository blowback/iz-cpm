; CONSLOW.COM -- console polling test for iz-cpm feature `videobeast`.
;
; Polls console status with BDOS function 11, then waits about 2,600
; T-states before the next poll, and ignores the result. Polls that far
; apart never end console input at end of file, so only --max-cycles stops
; it (exit 2). The loop sits past the TPA base: iz-cpm aborts a program
; whose PC returns to 0x0100.
;
; Rebuild with build.sh (sjasmplus 1.24.0).

BDOS            EQU 0x0005
C_STAT          EQU 11

        ORG 0x0100

START:
        NOP
.poll:
        LD C,C_STAT
        CALL BDOS
        LD B,200
.delay:
        DJNZ .delay
        JR .poll
