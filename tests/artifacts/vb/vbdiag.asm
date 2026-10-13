; VBDIAG.COM -- line counter diagnostic for iz-cpm feature `videobeast` and
; the NanoBeast (ticket 12: line counter rate differs from hardware).
;
; Enters and exits as VBTRACE.COM does (architecture AD-1, AD-23): saves
; 0xFF2B, the page-2 mapping, the lock, the mode and bank register 0xF9,
; restores them on exit, and prints only after exit.
;
; For mode m in 0x00, 0x01, with interrupts off:
;   1. Write m to 0xFF, read 0xFF at once, wait for a wrap (the frame start,
;      where the mode takes effect), read 0xFF again.
;   2. Poll the line counter until it next drops, and keep the last reading
;      before that drop (MAX). This is the frame's highest line only if the
;      counter never falls inside a frame; if it does (hardware run 1:
;      0xFB drops mid-frame), MAX is just the reading before that drop.
;   3. Straight after that wrap: raw sample, delay A (512 passes), raw
;      sample. Wait for a wrap; raw sample, delay B (1024 passes, as in
;      VBTRACE), raw sample.
; A raw sample is three reads, 0xFB 0xFA 0xFB, with no retry.
;
; Then prints, and runs a long delay with interrupts off between START and
; END, to time with a stopwatch:
;   7 + 46 x 1703957 - 5 + 10 + 62230 x 26 - 5 = 80,000,009 T
;   = 10.0 s at 8 MHz.
;
; Output (hex):
;   M mm FF rr rr MAX hhll      mode, 0xFF read at once / after the wrap,
;                               last reading before the next drop
;   A hh ll hh hh ll hh         raw samples either side of delay A
;   B hh ll hh hh ll hh         raw samples either side of delay B
;   (the same three lines for mode 01)
;   START
;   END
;
; With no VideoBeast the counter reads 0xFFFF: each wrap wait gives up after
; 65536 polls and the program still exits.
;
; Delay loops use JR, not JP: iz80 0.5.1 charges JP cc 12/7 T instead of
; 10/10.
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

DELAY_A         EQU 512         ; delay loop passes, 26 T-states each
DELAY_B         EQU 1024
LONG_OUTER      EQU 46          ; passes of the 65536-pass inner loop
LONG_TAIL       EQU 62230       ; passes of the tail loop

BLOCK_LEN       EQU 17          ; mode, 2 x 0xFF, max (H, L), 6 x A, 6 x B

; RAW dst: three reads 0xFB, 0xFA, 0xFB stored at dst..dst+2, no retry.
; 78 T; the reads start 0, 26 and 52 T after the first instruction.
        MACRO RAW dst
        LD A,(REG_LINE_H)       ; 13
        LD (dst),A              ; 13
        LD A,(REG_LINE_L)       ; 13
        LD (dst+1),A            ; 13
        LD A,(REG_LINE_H)       ; 13
        LD (dst+2),A            ; 13
        ENDM

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

        XOR A
        CALL MODE_RUN
        LD HL,BUF
        LD DE,BLOCK0
        LD BC,BLOCK_LEN
        LDIR
        LD A,1
        CALL MODE_RUN
        LD HL,BUF
        LD DE,BLOCK1
        LD BC,BLOCK_LEN
        LDIR

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
        LD HL,BLOCK0
        CALL PRINT_BLOCK
        LD HL,BLOCK1
        CALL PRINT_BLOCK

        ; Long delay for the stopwatch, interrupts off.
        LD DE,MSG_START
        CALL PRINT_STR
        DI
        LD D,LONG_OUTER         ; 7
.outer:
        LD BC,0                 ; 10
.inner:
        DEC BC                  ; 6
        LD A,B                  ; 4
        OR C                    ; 4
        JR NZ,.inner            ; 12 taken, 7 on the last pass
        DEC D                   ; 4
        JR NZ,.outer            ; 12 taken, 7 on the last pass
        LD BC,LONG_TAIL         ; 10
.tail:
        DEC BC                  ; 6
        LD A,B                  ; 4
        OR C                    ; 4
        JR NZ,.tail             ; 12 taken, 7 on the last pass
        EI
        LD DE,MSG_END
        CALL PRINT_STR
        JP 0

; MODE_RUN: steps 1-3 for mode A, results in BUF. Destroys: all.
MODE_RUN:
        LD (BUF),A
        LD (REG_MODE),A
        LD A,(REG_MODE)
        LD (BUF+1),A
        CALL WAIT_WRAP
        LD A,(REG_MODE)
        LD (BUF+2),A
        CALL WAIT_WRAP          ; DE = last reading before the drop
        RAW BUF+5
        LD BC,DELAY_A           ; 10
.delay_a:
        DEC BC                  ; 6
        LD A,B                  ; 4
        OR C                    ; 4
        JR NZ,.delay_a          ; 12 taken, 7 on the last pass
        RAW BUF+8
        LD A,D
        LD (BUF+3),A
        LD A,E
        LD (BUF+4),A
        CALL WAIT_WRAP
        RAW BUF+11
        LD BC,DELAY_B           ; 10
.delay_b:
        DEC BC                  ; 6
        LD A,B                  ; 4
        OR C                    ; 4
        JR NZ,.delay_b          ; 12 taken, 7 on the last pass
        RAW BUF+14
        RET

; WAIT_WRAP: poll READ_LINE until the line counter goes down. That is the
; frame wrap only if the counter never falls inside a frame.
; Gives up after 65536 polls (no VideoBeast: the counter reads 0xFFFF).
; Out: DE = last reading before the drop, HL = first reading after it.
; Destroys: A, BC.
WAIT_WRAP:
        LD BC,0
        CALL READ_LINE
.loop:
        LD D,H                  ; 4
        LD E,L                  ; 4
        CALL READ_LINE          ; 17 + 68 with no retry
        LD A,L                  ; 4   carry if HL < DE: the counter wrapped
        SUB E                   ; 4
        LD A,H                  ; 4
        SBC A,D                 ; 4
        RET C                   ; 11 taken, 5 not
        DEC BC                  ; 6
        LD A,B                  ; 4
        OR C                    ; 4
        JR NZ,.loop             ; 12
        RET

; READ_LINE: current raster line, read high, low, high and retried if the
; high byte changed. Out: HL. Destroys: A.
READ_LINE:
        LD A,(REG_LINE_H)       ; 13
        LD H,A                  ; 4
        LD A,(REG_LINE_L)       ; 13
        LD L,A                  ; 4
        LD A,(REG_LINE_H)       ; 13
        CP H                    ; 4
        JR NZ,READ_LINE         ; 12 taken, 7 not
        RET                     ; 10

; PRINT_BLOCK: print the three lines for the block at HL. Destroys: all.
PRINT_BLOCK:
        LD (PTR),HL
        LD DE,MSG_M
        CALL PRINT_STR
        CALL PRINT_NEXT         ; mode
        LD DE,MSG_FF
        CALL PRINT_STR
        CALL PRINT_NEXT         ; 0xFF at once
        CALL PRINT_SPACE
        CALL PRINT_NEXT         ; 0xFF after the wrap
        LD DE,MSG_MAX
        CALL PRINT_STR
        CALL PRINT_NEXT         ; max, high
        CALL PRINT_NEXT         ; max, low
        LD DE,MSG_A
        CALL PRINT_STR
        CALL PRINT_SIX
        LD DE,MSG_B
        CALL PRINT_STR
        CALL PRINT_SIX
        LD DE,MSG_CRLF
        JP PRINT_STR

; PRINT_SIX: print the next six bytes, space separated. Destroys: all.
PRINT_SIX:
        LD A,6
        LD (COUNT),A
.loop:
        CALL PRINT_SPACE
        CALL PRINT_NEXT
        LD HL,COUNT
        DEC (HL)
        JR NZ,.loop
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

MSG_M:          DB "M $"
MSG_FF:         DB " FF $"
MSG_MAX:        DB " MAX $"
MSG_A:          DB 13,10,"A$"
MSG_B:          DB 13,10,"B$"
MSG_START:      DB "START",13,10,"$"
MSG_END:        DB "END",13,10,"$"
MSG_CRLF:       DB 13,10,"$"

SAVED_SCREEN:   DB 0
SAVED_PAGE2:    DB 0
SAVED_LOCK:     DB 0
SAVED_MODE:     DB 0
SAVED_BANK0:    DB 0
PTR:            DW 0
COUNT:          DB 0
BUF:            DS BLOCK_LEN
BLOCK0:         DS BLOCK_LEN
BLOCK1:         DS BLOCK_LEN
