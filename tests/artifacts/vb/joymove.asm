; JOYMOVE.COM -- moves a marker from the joystick, for iz-cpm's --window
; (and, headless, for --joy).
;
; Pages the VideoBeast into CPU page 2 and unlocks it as VBPIC does
; (architecture AD-1), then sets mode 0 doubled (320 x 240), turns every
; layer off, sets a grey background and makes layer 0 an 8bpp bitmap at
; VRAM 0x20000 (clear of the firmware console, AD-7) covering character
; cells 7-22 down and 12-27 across: a 128 x 128 field at (96,56), centred,
; with scroll 0. The field's VRAM (rows 0-127, bytes 0-127 of each 512-byte
; row) is cleared to 0, palette 2 entry 0 is black and entry 1 the marker
; colour.
;
; The marker is a 4 x 4 square of 1s with its top-left corner at (X,Y)
; within the field, starting at (62,62). At each frame start, found by a
; drop of the raster line counter, it reads port 0x9F once and moves the
; marker one pixel per held direction (bit 0 right, 1 left, 2 down, 3 up),
; keeping it inside the field, and colours it by fire button: none green,
; bit 4 (Z, JUMP) yellow, bit 5 (X, FIRE) cyan, bit 7 (C, DASH) white; with
; more than one held, the later in that list wins.
;
; It never exits and prints nothing. It saves nothing: there is no exit to
; restore on.
;
; Rebuild with build.sh (sjasmplus 1.24.0).

SCREEN_PAGE     EQU 0xFF2B      ; firmware 1.8 screen_page
MBB_SET_PAGE    EQU 0xFDDF      ; A = logical page, E = physical page

VB_PAGE         EQU 0x40
VB_WINDOW       EQU 0x8000      ; CPU page 2
VB_REG          EQU 0xBF00      ; register r at VB_REG + r
PAL_ENTRY0      EQU VB_REG + 0  ; entry 0 of the selected palette bank
PAL_ENTRY1      EQU VB_REG + 2
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
MODE_0_DOUBLED  EQU 0x08

PORT_JOY        EQU 0x9F
JOY_RIGHT       EQU 0           ; bit numbers
JOY_LEFT        EQU 1
JOY_DOWN        EQU 2
JOY_UP          EQU 3
JOY_JUMP        EQU 4
JOY_FIRE        EQU 5
JOY_DASH        EQU 7

FIELD_VADDR     EQU 0x20000
FIELD_BANK      EQU FIELD_VADDR >> 12   ; 4K units; 4 per 16K window
LOWER_PALETTE2  EQU 0x04        ; palettes, palette 2, entries 0-63
FIELD_SIZE      EQU 128         ; pixels across and down
MARKER          EQU 4           ; marker side
MAX_POS         EQU FIELD_SIZE - MARKER
START_POS       EQU 62

BACKGROUND      EQU 0x2108      ; r 2, g 2, b 2
BLACK           EQU 0x0000
GREEN           EQU 0x03E0      ; r 0, g 7, b 0
YELLOW          EQU 0x7FE0      ; r 7, g 7, b 0
CYAN            EQU 0x03FF      ; r 0, g 7, b 7
WHITE           EQU 0x7FFF      ; r 7, g 7, b 7

        ORG 0x0100

START:
        ; AD-1 steps 1-3.
        LD A,0x24
        LD (SCREEN_PAGE),A
        DI
        LD A,2
        LD E,VB_PAGE
        CALL MBB_SET_PAGE

        ; Unlock, mode 0 doubled.
        LD A,UNLOCKED
        LD (REG_LOCK),A
        LD A,MODE_0_DOUBLED
        LD (REG_MODE),A

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

        ; Clear the field: for each row, 128 bytes of 0.
        LD C,0                  ; row
.clear_row:
        LD A,C
        LD B,0                  ; x
        CALL ROW_ADDR
        LD (HL),0
        LD D,H
        LD E,L
        INC DE
        PUSH BC
        LD BC,FIELD_SIZE - 1
        LDIR
        POP BC
        INC C
        LD A,C
        CP FIELD_SIZE
        JR NZ,.clear_row

        ; Palette 2: entry 0 black, entry 1 green.
        LD A,LOWER_PALETTE2
        LD (REG_LOWER),A
        LD HL,BLACK
        LD (PAL_ENTRY0),HL
        LD HL,GREEN
        LD (PAL_ENTRY1),HL

        ; The marker, then layer 0, so it never shows half set up.
        LD A,1
        CALL DRAW
        LD HL,LAYER0
        LD DE,REG_LAYER0
        LD BC,LAYER0_REGS
        LDIR

; Forever: wait for a frame start, read the joystick, move and colour.
FRAME:
        CALL WAIT_FRAME
        IN A,(PORT_JOY)
        LD (JOY),A

        XOR A                   ; erase at the old position
        CALL DRAW

        LD A,(JOY)
        LD C,A
        LD A,(MX)
        BIT JOY_RIGHT,C
        JR Z,.not_right
        CP MAX_POS
        JR NC,.not_right
        INC A
.not_right:
        BIT JOY_LEFT,C
        JR Z,.not_left
        OR A
        JR Z,.not_left
        DEC A
.not_left:
        LD (MX),A
        LD A,(MY)
        BIT JOY_DOWN,C
        JR Z,.not_down
        CP MAX_POS
        JR NC,.not_down
        INC A
.not_down:
        BIT JOY_UP,C
        JR Z,.not_up
        OR A
        JR Z,.not_up
        DEC A
.not_up:
        LD (MY),A

        LD A,1                  ; draw at the new position
        CALL DRAW

        ; Colour: the last of Z, X, C held wins.
        LD HL,GREEN
        BIT JOY_JUMP,C
        JR Z,.not_jump
        LD HL,YELLOW
.not_jump:
        BIT JOY_FIRE,C
        JR Z,.not_fire
        LD HL,CYAN
.not_fire:
        BIT JOY_DASH,C
        JR Z,.not_dash
        LD HL,WHITE
.not_dash:
        LD (PAL_ENTRY1),HL      ; palette 2 is still selected
        JR FRAME

; DRAW: fill the marker at (MX,MY) with A. Destroys: A, B, DE, HL.
; Keeps C.
DRAW:
        LD (FILL),A
        LD A,(MY)
        LD D,A                  ; row
        LD E,MARKER             ; rows left
.row:
        LD A,(MX)
        LD B,A
        LD A,D
        PUSH BC
        CALL ROW_ADDR
        POP BC
        LD A,(FILL)
        LD B,MARKER
.pixel:
        LD (HL),A
        INC HL
        DJNZ .pixel
        INC D
        DEC E
        JR NZ,.row
        RET

; ROW_ADDR: map field row A into the window and point HL at column B of it.
; Row r is VRAM FIELD_VADDR + 512 r; each 16K window holds 32 rows.
; Destroys: A.
ROW_ADDR:
        PUSH AF
        RRCA                    ; (r >> 3) & 0x1C: 4K units of whole windows
        RRCA
        RRCA
        AND 0x1C
        ADD A,FIELD_BANK
        LD (REG_BANK0),A
        POP AF
        AND 0x1F                ; H = 0x80 + 2 (r & 31), L = column
        ADD A,A
        ADD A,VB_WINDOW >> 8
        LD H,A
        LD L,B
        RET

; WAIT_FRAME: return once the raster line counter drops (a frame start).
; Destroys: A, DE, HL.
WAIT_FRAME:
        CALL READ_LINE
.wait:
        LD D,H
        LD E,L
        CALL READ_LINE
        LD A,L                  ; carry if HL < DE: the counter dropped
        SUB E
        LD A,H
        SBC A,D
        JR NC,.wait
        RET

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

; Layer 0: 8bpp bitmap, top 7, bottom 22, left 12, right 28, scroll 0,
; bitmap base 8 (x 16K = FIELD_VADDR).
LAYER0:         DB 4, 7, 22, 12, 28, 0, 0, 0, FIELD_VADDR >> 14

MX:             DB START_POS
MY:             DB START_POS
JOY:            DB 0
FILL:           DB 0
