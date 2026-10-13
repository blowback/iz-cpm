; FF2B.COM -- firmware-byte test for iz-cpm feature `videobeast`.
;
; Stores 0x24 at 0xFF2B (firmware 1.8 screen_page) as the game does, reads it
; back and prints the value through BIOS CONOUT, called via the jump table.
; iz-cpm ignores the store: 0xFF2B is the low byte of the WRITE entry's
; target, so the read-back is 0x8E. Nothing is restored.
;
; Output:
;   FF2B 8E
;
; Rebuild with build.sh (sjasmplus 1.24.0).

BDOS            EQU 0x0005
WBOOT_PTR       EQU 0x0001      ; JP at 0x0000 targets the BIOS WBOOT entry
CONOUT_OFFSET   EQU 9           ; CONOUT entry is 3 entries after WBOOT
C_WRITESTR      EQU 9

SCREEN_PAGE     EQU 0xFF2B      ; firmware 1.8 screen_page

        ORG 0x0100

START:
        LD A,0x24
        LD (SCREEN_PAGE),A
        LD A,(SCREEN_PAGE)
        LD (READ_BACK),A

        LD DE,MSG_FF2B
        LD C,C_WRITESTR
        CALL BDOS

        LD A,(READ_BACK)
        CALL PRINT_HEX
        LD C,13
        CALL CONOUT
        LD C,10
        CALL CONOUT
        JP 0

; PRINT_HEX: print A as two hex digits through CONOUT. Destroys: all.
PRINT_HEX:
        PUSH AF
        RRCA
        RRCA
        RRCA
        RRCA
        CALL .nibble
        POP AF
.nibble:
        AND 0x0F
        ADD A,0x90
        DAA
        ADC A,0x40
        DAA
        LD C,A
        ; fall through

; CONOUT: print the character in C through the BIOS jump table.
; Destroys: DE, HL.
CONOUT:
        LD HL,(WBOOT_PTR)
        LD DE,CONOUT_OFFSET
        ADD HL,DE
        JP (HL)

MSG_FF2B:       DB "FF2B $"

READ_BACK:      DB 0
