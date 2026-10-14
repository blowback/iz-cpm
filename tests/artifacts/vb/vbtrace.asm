; VBTRACE.COM -- tracer for iz-cpm feature `videobeast`.
;
; Pages the VideoBeast into CPU page 2 as the game does (architecture AD-1),
; saving and restoring what it changes (AD-23), and checks that:
;   - VRAM at vaddr 0x20000/0x20001 (clear of the firmware console, AD-7)
;     holds what was written;
;   - bank register 0xF9 reads back what was written;
;   - the raster line counter advances by delay / 256 lines over a timed
;     delay (mode 0: 800 pixels x 40 ns = 32 us = 256 T-states at 8 MHz).
; It prints only after EXIT has restored 0xFF2B, because under iz-cpm that
; byte lies inside the fake BIOS jump table (WRITE entry).
;
; Output:
;   VRAM A5 5A
;   F9 20
;   LINES xxxx      (hex)
;
; Rebuild with build.sh (sjasmplus 1.24.0).

BDOS            EQU 0x0005
C_WRITE         EQU 2
C_WRITESTR      EQU 9

SCREEN_PAGE     EQU 0xFF2B      ; firmware 1.8 screen_page
MBB_GET_PAGE    EQU 0xFDDC      ; C = logical page -> A = physical page
MBB_SET_PAGE    EQU 0xFDDF      ; A = logical page, E = physical page

VB_PAGE         EQU 0x40
VB_WINDOW       EQU 0x8000      ; CPU page 2
VB_REG          EQU 0xBF00      ; register r at VB_REG + r
REG_BANK0       EQU VB_REG + 0xF9
REG_LINE_L      EQU VB_REG + 0xFA
REG_LINE_H      EQU VB_REG + 0xFB
REG_LOCK        EQU VB_REG + 0xFE
REG_MODE        EQU VB_REG + 0xFF
UNLOCKED        EQU 0xF3

TEST_VADDR      EQU 0x20000
DELAY_COUNT     EQU 1024        ; delay loop passes, 26 T-states each

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
        ; register (only visible while unlocked), then mode 0.
        LD A,(REG_LOCK)
        LD (SAVED_LOCK),A
        LD A,(REG_MODE)
        LD (SAVED_MODE),A
        LD A,UNLOCKED
        LD (REG_LOCK),A
        LD A,(REG_BANK0)
        LD (SAVED_BANK0),A
        XOR A
        LD (REG_MODE),A

        ; Bank register: vaddr >> 12, written and read back.
        LD A,TEST_VADDR >> 12
        LD (REG_BANK0),A
        LD A,(REG_BANK0)
        LD (RB_BANK0),A

        ; VRAM round trip at TEST_VADDR.
        LD A,0xA5
        LD (VB_WINDOW + (TEST_VADDR & 0x0FFF)),A
        LD A,0x5A
        LD (VB_WINDOW + (TEST_VADDR & 0x0FFF) + 1),A
        LD A,(VB_WINDOW + (TEST_VADDR & 0x0FFF))
        LD (RB_VRAM0),A
        LD A,(VB_WINDOW + (TEST_VADDR & 0x0FFF) + 1)
        LD (RB_VRAM1),A

        ; Pixel doubling (start-up mode 0x08) only changes at a frame start,
        ; and while doubled the line counter steps every other line. Wait for
        ; the counter to wrap so the measurement runs in an undoubled frame.
        ; Give up after 65536 polls (no VideoBeast: the counter reads 0xFFFF).
        LD BC,0
        CALL READ_LINE
.wait:
        LD D,H
        LD E,L
        CALL READ_LINE
        LD A,L                  ; carry if HL < DE: the counter wrapped
        SUB E
        LD A,H
        SBC A,D
        JR C,.wrapped
        DEC BC
        LD A,B
        OR C
        JR NZ,.wait
.wrapped:

        ; Timed delay between two reads of the line counter:
        ; 10 + DELAY_COUNT * 26 - 5 = 26629 T-states = 104.02 lines.
        ; JR, not JP: iz80 0.5.1 charges JP cc 12/7 T instead of 10/10.
        CALL READ_LINE
        LD (LINE0),HL
        LD BC,DELAY_COUNT       ; 10
.delay:
        DEC BC                  ; 6
        LD A,B                  ; 4
        OR C                    ; 4
        JR NZ,.delay            ; 12 taken, 7 on the last pass
        CALL READ_LINE
        LD DE,(LINE0)
        OR A
        SBC HL,DE
        LD (LINE_DELTA),HL

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
        LD DE,MSG_VRAM
        CALL PRINT_STR
        LD A,(RB_VRAM0)
        CALL PRINT_HEX
        CALL PRINT_SPACE
        LD A,(RB_VRAM1)
        CALL PRINT_HEX
        LD DE,MSG_BANK0
        CALL PRINT_STR
        LD A,(RB_BANK0)
        CALL PRINT_HEX
        LD DE,MSG_LINES
        CALL PRINT_STR
        LD A,(LINE_DELTA + 1)
        CALL PRINT_HEX
        LD A,(LINE_DELTA)
        CALL PRINT_HEX
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

MSG_VRAM:       DB "VRAM $"
MSG_BANK0:      DB 13,10,"F9 $"
MSG_LINES:      DB 13,10,"LINES $"
MSG_CRLF:       DB 13,10,"$"

SAVED_SCREEN:   DB 0
SAVED_PAGE2:    DB 0
SAVED_LOCK:     DB 0
SAVED_MODE:     DB 0
SAVED_BANK0:    DB 0
RB_VRAM0:       DB 0
RB_VRAM1:       DB 0
RB_BANK0:       DB 0
LINE0:          DW 0
LINE_DELTA:     DW 0
