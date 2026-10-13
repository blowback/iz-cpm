; JOYECHO.COM -- joystick port echo for iz-cpm feature `videobeast`.
;
; Pages the VideoBeast into CPU page 2 and unlocks it as VBTRACE does
; (architecture AD-1), saving and restoring what it changes (AD-23); it never
; writes the mode register. Reads port 0x9F once in frame 0, then once at the
; start of each of the next 7 frames, found by waiting for the raster line
; counter to drop back towards 0. The start-up mode 0x08 has frames of
; 525 lines x 256 T-states = 134400 T-states, so each read lands well inside
; its frame. It prints only after EXIT has restored 0xFF2B, because under
; iz-cpm that byte lies inside the fake BIOS jump table (WRITE entry).
;
; Output:
;   JOY hh hh hh hh hh hh hh hh     (port 0x9F in frames 0-7)
;
; Rebuild with build.sh (sjasmplus 1.24.0).

BDOS            EQU 0x0005
C_WRITE         EQU 2
C_WRITESTR      EQU 9

SCREEN_PAGE     EQU 0xFF2B      ; firmware 1.8 screen_page
MBB_GET_PAGE    EQU 0xFDDC      ; C = logical page -> A = physical page
MBB_SET_PAGE    EQU 0xFDDF      ; A = logical page, E = physical page

VB_PAGE         EQU 0x40
VB_REG          EQU 0xBF00      ; register r at VB_REG + r
REG_LINE_L      EQU VB_REG + 0xFA
REG_LINE_H      EQU VB_REG + 0xFB
REG_LOCK        EQU VB_REG + 0xFE
UNLOCKED        EQU 0xF3

PORT_JOY        EQU 0x9F
FRAMES          EQU 8

        ORG 0x0100

START:
        ; Frame 0: read the port at once.
        IN A,(PORT_JOY)
        LD (SLOTS),A

        ; AD-23: save the firmware byte and the page-2 mapping.
        LD A,(SCREEN_PAGE)
        LD (SAVED_SCREEN),A
        LD C,2
        CALL MBB_GET_PAGE
        LD (SAVED_PAGE2),A

        ; AD-1 steps 1-3.
        LD A,0x24
        LD (SCREEN_PAGE),A
        DI
        LD A,2
        LD E,VB_PAGE
        CALL MBB_SET_PAGE

        ; Save the lock, then unlock.
        LD A,(REG_LOCK)
        LD (SAVED_LOCK),A
        LD A,UNLOCKED
        LD (REG_LOCK),A

        ; Frames 1-7: wait for the counter to drop, then read the port.
        ; Give up waiting after 65536 polls (no VideoBeast: the counter
        ; reads 0xFFFF).
        LD IX,SLOTS + 1
        LD A,FRAMES - 1
.frame:
        LD (FRAMES_LEFT),A
        LD BC,0
        CALL READ_LINE
.wait:
        LD D,H
        LD E,L
        CALL READ_LINE
        LD A,L                  ; carry if HL < DE: the counter dropped
        SUB E
        LD A,H
        SBC A,D
        JR C,.dropped
        DEC BC
        LD A,B
        OR C
        JR NZ,.wait
.dropped:
        IN A,(PORT_JOY)
        LD (IX+0),A
        INC IX
        LD A,(FRAMES_LEFT)
        DEC A
        JR NZ,.frame

        ; AD-23 EXIT: the lock, then page 2, then 0xFF2B.
        LD A,(SAVED_LOCK)
        LD (REG_LOCK),A
        LD A,(SAVED_PAGE2)
        LD E,A
        LD A,2
        CALL MBB_SET_PAGE
        LD A,(SAVED_SCREEN)
        LD (SCREEN_PAGE),A
        EI

        ; Report.
        LD DE,MSG_JOY
        CALL PRINT_STR
        LD HL,SLOTS
        LD B,FRAMES
.print:
        PUSH BC
        PUSH HL
        CALL PRINT_SPACE
        POP HL
        PUSH HL
        LD A,(HL)
        CALL PRINT_HEX
        POP HL
        POP BC
        INC HL
        DJNZ .print
        LD DE,MSG_CRLF
        CALL PRINT_STR
        JP 0

; READ_LINE: current raster line, read high, low, high and retried if the
; high byte changed. Out: HL. Destroys: A.
READ_LINE:
        LD A,(REG_LINE_H)
        LD H,A
        LD A,(REG_LINE_L)
        LD L,A
        LD A,(REG_LINE_H)
        CP H
        JR NZ,READ_LINE
        RET

; PRINT_STR: print the '$'-terminated string at DE. Destroys: all.
PRINT_STR:
        LD C,C_WRITESTR
        JP BDOS

; PRINT_SPACE: print one space. Destroys: all.
PRINT_SPACE:
        LD E,' '
        LD C,C_WRITE
        JP BDOS

; PRINT_HEX: print A as two hex digits. Destroys: all.
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
        LD E,A
        LD C,C_WRITE
        JP BDOS

MSG_JOY:        DB "JOY$"
MSG_CRLF:       DB 13,10,"$"

SAVED_SCREEN:   DB 0
SAVED_PAGE2:    DB 0
SAVED_LOCK:     DB 0
FRAMES_LEFT:    DB 0
SLOTS:          DS FRAMES, 0
