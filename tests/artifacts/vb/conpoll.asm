; CONPOLL.COM -- console polling test for iz-cpm feature `videobeast`.
;
; Polls console status with BDOS function 11 and, when a key is waiting,
; reads it with BDOS function 1, which echoes it. Never ends on its own, so
; only the end of console input (exit 1) or --max-cycles stops it. The loop
; sits past the TPA base: iz-cpm aborts a program whose PC returns to 0x0100.
;
; Rebuild with build.sh (sjasmplus 1.24.0).

BDOS            EQU 0x0005
C_READ          EQU 1
C_STAT          EQU 11

        ORG 0x0100

START:
        NOP
.poll:
        LD C,C_STAT
        CALL BDOS
        OR A
        JR Z,.poll
        LD C,C_READ
        CALL BDOS
        JR .poll
