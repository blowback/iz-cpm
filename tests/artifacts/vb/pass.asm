; PASS.COM -- run-control test for iz-cpm feature `videobeast`.
;
; Prints PASS and warm boots, so a scripted run ends normally (exit 0).
;
; Output:
;   PASS
;
; Rebuild with build.sh (sjasmplus 1.24.0).

BDOS            EQU 0x0005
C_WRITESTR      EQU 9

        ORG 0x0100

START:
        LD DE,MSG_PASS
        LD C,C_WRITESTR
        CALL BDOS
        JP 0

MSG_PASS:       DB "PASS",13,10,"$"
