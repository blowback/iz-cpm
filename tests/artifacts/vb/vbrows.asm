; VBROWS.COM -- second line counter diagnostic for iz-cpm feature
; `videobeast` and the NanoBeast (ticket 12, after VBDIAG hardware run 1).
;
; Enters and exits as VBDIAG.COM and VBTRACE.COM do (architecture AD-1,
; AD-23), and prints only after exit. Interrupts stay off from entry to
; exit. There is no wrap detection: it would depend on 0xFB.
;
; For each mode in order 0x00, 0x08, 0x01:
;   1. Write the mode to 0xFF, then a settle delay with no register reads,
;      so the mode is in effect (it changes only at a frame start):
;        10 + 11539 x 26 - 5 = 300,019 T (over two frames in either mode).
;   2. 160 samples exactly 1,024 T apart (163,840 T, over one frame in
;      either mode). A sample reads 0xFB then 0xFA, 26 T apart, and stores
;      both bytes; nothing else reads the VideoBeast during the series.
; One sample period, start to start:
;   reads and stores (13+7+6+13+7+6)        =   52 T
;   LD B 7 + DJNZ 71 x 13 + 8               =  938 T
;   LD A,0 7 + LD A,0 7 + NOP 4             =   18 T
;   DEC E 4 + JR NZ taken 12                =   16 T
;                                           = 1024 T
; The series uses no untaken conditional jumps: iz80 0.5.1 charges 8 T for
; an untaken JR (the Z80 takes 7), which would make the period differ
; between iz-cpm and the hardware. In the settle delay it does (iz-cpm
; counts 300,020 T), which only moves where the series starts.
;
; Output (hex), per mode:
;   M mm
;   then the 160 samples as hhll (0xFB byte, 0xFA byte), 8 per line
;   separated by spaces (20 lines).
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
REG_BANK0       EQU VB_REG + 0xF9
REG_LINE_L      EQU VB_REG + 0xFA
REG_LINE_H      EQU VB_REG + 0xFB
REG_LOCK        EQU VB_REG + 0xFE
REG_MODE        EQU VB_REG + 0xFF
UNLOCKED        EQU 0xF3

SETTLE          EQU 11539       ; settle loop passes, 26 T-states each
SAMPLES         EQU 160
PAD             EQU 72          ; DJNZ passes in one sample period
MODES           EQU 3
PER_LINE        EQU 8

        ORG 0x0100

START:
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

        ; Save lock and mode (always visible), unlock, then save the bank
        ; register (only visible while unlocked).
        LD A,(REG_LOCK)
        LD (SAVED_LOCK),A
        LD A,(REG_MODE)
        LD (SAVED_MODE),A
        LD A,UNLOCKED
        LD (REG_LOCK),A
        LD A,(REG_BANK0)
        LD (SAVED_BANK0),A

        LD HL,BUF
        LD A,(MODE_LIST)
        CALL ROWS
        LD A,(MODE_LIST+1)
        CALL ROWS
        LD A,(MODE_LIST+2)
        CALL ROWS

        ; AD-23 EXIT: registers (mode last), then the lock, page 2, 0xFF2B.
        LD A,(SAVED_BANK0)
        LD (REG_BANK0),A
        LD A,(SAVED_MODE)
        LD (REG_MODE),A
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
        LD HL,BUF
        LD (PTR),HL
        LD HL,MODE_LIST
        LD (MODE_PTR),HL
        LD A,MODES
        LD (MODES_LEFT),A
.mode:
        LD DE,MSG_M
        CALL PRINT_STR
        LD HL,(MODE_PTR)
        LD A,(HL)
        INC HL
        LD (MODE_PTR),HL
        CALL PRINT_HEX
        LD DE,MSG_CRLF
        CALL PRINT_STR
        LD A,SAMPLES
        LD (SAMPLES_LEFT),A
        LD A,PER_LINE
        LD (ON_LINE),A
.sample:
        CALL PRINT_NEXT         ; 0xFB byte
        CALL PRINT_NEXT         ; 0xFA byte
        LD HL,SAMPLES_LEFT
        DEC (HL)
        JR Z,.mode_done
        LD HL,ON_LINE
        DEC (HL)
        JR Z,.line_done
        CALL PRINT_SPACE
        JR .sample
.line_done:
        LD (HL),PER_LINE
        LD DE,MSG_CRLF
        CALL PRINT_STR
        JR .sample
.mode_done:
        LD DE,MSG_CRLF
        CALL PRINT_STR
        LD HL,MODES_LEFT
        DEC (HL)
        JR NZ,.mode
        JP 0

; ROWS: steps 1-2 for mode A, storing 2 bytes per sample at HL onwards.
; Out: HL past the samples. Destroys: A, BC, E.
ROWS:
        LD (REG_MODE),A         ; 13
        LD BC,SETTLE            ; 10
.settle:
        DEC BC                  ; 6
        LD A,B                  ; 4
        OR C                    ; 4
        JR NZ,.settle           ; 12 taken, 7 on the last pass (iz80: 8)
        LD E,SAMPLES            ; 7
.sample:
        LD A,(REG_LINE_H)       ; 13
        LD (HL),A               ; 7
        INC HL                  ; 6
        LD A,(REG_LINE_L)       ; 13
        LD (HL),A               ; 7
        INC HL                  ; 6
        LD B,PAD                ; 7
.pad:
        DJNZ .pad               ; 13 taken, 8 on the last pass
        LD A,0                  ; 7
        LD A,0                  ; 7
        NOP                     ; 4
        DEC E                   ; 4
        JR NZ,.sample           ; 12 taken
        RET

; PRINT_NEXT: print the byte at (PTR) as hex and advance PTR.
; Destroys: all.
PRINT_NEXT:
        LD HL,(PTR)
        LD A,(HL)
        INC HL
        LD (PTR),HL
        JP PRINT_HEX

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

MODE_LIST:      DB 0x00, 0x08, 0x01
MSG_M:          DB "M $"
MSG_CRLF:       DB 13,10,"$"

SAVED_SCREEN:   DB 0
SAVED_PAGE2:    DB 0
SAVED_LOCK:     DB 0
SAVED_MODE:     DB 0
SAVED_BANK0:    DB 0
PTR:            DW 0
MODE_PTR:       DW 0
MODES_LEFT:     DB 0
SAMPLES_LEFT:   DB 0
ON_LINE:        DB 0
BUF:            DS MODES * SAMPLES * 2
