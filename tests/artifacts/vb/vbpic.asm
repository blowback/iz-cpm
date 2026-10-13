; VBPIC.COM -- known picture for iz-cpm's --png-at and --dump-at tests.
;
; Pages the VideoBeast into CPU page 2 and sets mode 0 as VBTRACE does
; (architecture AD-1), saving and restoring what it changes (AD-23). It then
; turns every layer off, sets the background to 0x7C00 (red) and makes
; layer 0 an 8bpp bitmap at VRAM 0x20000 (clear of the firmware console,
; AD-7) covering character cells 2-3 down and 2 across (pixels x 16-23,
; lines 16-31), with scroll 0. VRAM rows 0-15 (512 bytes each) start with
; 16 bytes of 1, and palette 2 entry 1 is 0x03E0 (green), so the layer shows
; a green 8 x 16 rectangle. Mode 0 is undoubled from frame 1 on, so frame 1
; is scanned with the picture and frame 2 starts with it on the surface.
;
; It waits for 3 drops of the raster line counter (frames 1, 2 and 3
; starting), restores everything and warm boots. It prints nothing.
;
; Rebuild with build.sh (sjasmplus 1.24.0).

SCREEN_PAGE     EQU 0xFF2B      ; firmware 1.8 screen_page
MBB_GET_PAGE    EQU 0xFDDC      ; C = logical page -> A = physical page
MBB_SET_PAGE    EQU 0xFDDF      ; A = logical page, E = physical page

VB_PAGE         EQU 0x40
VB_WINDOW       EQU 0x8000      ; CPU page 2
VB_REG          EQU 0xBF00      ; register r at VB_REG + r
PAL_ENTRY1      EQU VB_REG + 2  ; entry 1 of the selected palette bank
REG_LAYER0      EQU VB_REG + 0x80
LAYER_STRIDE    EQU 0x10
LAYER0_REGS     EQU 9           ; type .. bitmap base
REG_LOWER       EQU VB_REG + 0xF5
REG_BANK0       EQU VB_REG + 0xF9
REG_LINE_L      EQU VB_REG + 0xFA
REG_LINE_H      EQU VB_REG + 0xFB
REG_BACKGROUND  EQU VB_REG + 0xFC   ; low, then high at 0xFD
REG_LOCK        EQU VB_REG + 0xFE
REG_MODE        EQU VB_REG + 0xFF
UNLOCKED        EQU 0xF3

PIC_VADDR       EQU 0x20000
LOWER_PALETTE2  EQU 0x04        ; palettes, palette 2, entries 0-63
BACKGROUND      EQU 0x7C00      ; r 7, g 0, b 0
GREEN           EQU 0x03E0      ; r 0, g 7, b 0
ROWS            EQU 16
ROW_BYTES       EQU 512
PIC_WIDTH       EQU 16          ; bytes of 1 at the start of each row
DROPS           EQU 3

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

        ; Save the rest of what changes: palette bank select, background,
        ; layer 0's registers and the other layers' types.
        LD A,(REG_LOWER)
        LD (SAVED_LOWER),A
        LD HL,(REG_BACKGROUND)
        LD (SAVED_BACKGROUND),HL
        LD HL,REG_LAYER0
        LD DE,SAVED_LAYER0
        LD BC,LAYER0_REGS
        LDIR
        LD HL,REG_LAYER0 + LAYER_STRIDE
        LD DE,SAVED_TYPES
        LD B,5
.save_type:
        LD A,(HL)
        LD (DE),A
        INC DE
        LD A,L
        ADD A,LAYER_STRIDE
        LD L,A
        DJNZ .save_type

        ; Every layer off, then the background.
        LD HL,REG_LAYER0
        LD B,6
.clear_type:
        LD (HL),0
        LD A,L
        ADD A,LAYER_STRIDE
        LD L,A
        DJNZ .clear_type
        LD HL,BACKGROUND
        LD (REG_BACKGROUND),HL

        ; The bitmap: rows 0-15 at PIC_VADDR start with PIC_WIDTH bytes of 1.
        LD A,PIC_VADDR >> 12
        LD (REG_BANK0),A
        LD HL,VB_WINDOW
        LD B,ROWS
.row:
        PUSH BC
        PUSH HL
        LD B,PIC_WIDTH
.byte:
        LD (HL),1
        INC HL
        DJNZ .byte
        POP HL
        LD DE,ROW_BYTES
        ADD HL,DE
        POP BC
        DJNZ .row

        ; Palette 2 entry 1: save it, then green.
        LD A,LOWER_PALETTE2
        LD (REG_LOWER),A
        LD HL,(PAL_ENTRY1)
        LD (SAVED_PAL1),HL
        LD HL,GREEN
        LD (PAL_ENTRY1),HL

        ; Layer 0 last, so it never shows half set up.
        LD HL,LAYER0
        LD DE,REG_LAYER0
        LD BC,LAYER0_REGS
        LDIR

        ; Wait for DROPS drops of the line counter. Give up waiting for one
        ; after 65536 polls (no VideoBeast: the counter reads 0xFFFF).
        LD A,DROPS
.drop:
        LD (DROPS_LEFT),A
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
        LD A,(DROPS_LEFT)
        DEC A
        JR NZ,.drop

        ; AD-23 EXIT: registers (mode last), then the lock, page 2, 0xFF2B.
        LD HL,(SAVED_PAL1)      ; palette 2 is still selected
        LD (PAL_ENTRY1),HL
        LD A,(SAVED_LOWER)
        LD (REG_LOWER),A
        LD HL,SAVED_LAYER0
        LD DE,REG_LAYER0
        LD BC,LAYER0_REGS
        LDIR
        LD HL,REG_LAYER0 + LAYER_STRIDE
        LD DE,SAVED_TYPES
        LD B,5
.restore_type:
        LD A,(DE)
        LD (HL),A
        INC DE
        LD A,L
        ADD A,LAYER_STRIDE
        LD L,A
        DJNZ .restore_type
        LD HL,(SAVED_BACKGROUND)
        LD (REG_BACKGROUND),HL
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

; Layer 0: 8bpp bitmap, top 2, bottom 3, left 2, right 3, scroll 0,
; bitmap base 8 (x 16K = PIC_VADDR).
LAYER0:         DB 4, 2, 3, 2, 3, 0, 0, 0, PIC_VADDR >> 14

SAVED_SCREEN:   DB 0
SAVED_PAGE2:    DB 0
SAVED_LOCK:     DB 0
SAVED_MODE:     DB 0
SAVED_BANK0:    DB 0
SAVED_LOWER:    DB 0
SAVED_BACKGROUND: DW 0
SAVED_PAL1:     DW 0
SAVED_LAYER0:   DS LAYER0_REGS, 0
SAVED_TYPES:    DS 5, 0
DROPS_LEFT:     DB 0
