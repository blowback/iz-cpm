# iz-cpm — Microbeast fork

This fork extends [iz-cpm](https://github.com/ivanizag/iz-cpm) with hardware features from the [Microbeast](https://github.com/) Z80 SBC so that Microbeast software can be developed and exercised on a host machine while still benefiting from iz-cpm's CP/M 2.2 emulation.

## Memory banking

The CPU's 64 KiB address space is divided into four 16 KiB *logical* banks. Each physical bank can be remapped to a *physical* page: one of 32 Flash banks, 32 RAM banks (1 MiB in all), or the VideoBeast. Page values decode as on the hardware and in beastem, by `page & 0xE0`:

| Physical bank range | Storage              |
| ------------------ | -------------------- |
| `0x00`–`0x1F`      | 512 KiB Flash ROM    |
| `0x20`–`0x3F`      | 512 KiB RAM          |
| `0x40`–`0x5F`      | VideoBeast with feature `videobeast`; otherwise reads `0xFF`, writes ignored |
| `0x60`–`0xFF`      | Flash ROM, bank `page & 0x1F` |

All 8 bits of a page value are kept, so writing `0xFF` selects Flash bank `0x1F`.

### The blessed way: BIOS page-mapping routines

**If you want to touch memory banking, use the BIOS routines.** They are the only supported, hardware-faithful way to read and change the mapping, and iz-cpm now emulates them exactly as the real MicroBeast firmware exposes them:

| Routine        | Address  | Inputs                              | Output                                  |
| -------------- | -------- | ----------------------------------- | --------------------------------------- |
| `MBB_SET_PAGE` | `0xFDDF` | `A` = logical page (0–2), `E` = physical page (`0x00`–`0xFF`, see above) | — |
| `MBB_GET_PAGE` | `0xFDDC` | `C` = logical page (0–2)            | `A` = physical page, or `0xFF` on error |


```asm
    ; Map the CPU's third 16 KiB window (0x8000–0xBFFF) to RAM bank 0x2A
    LD   A, 2          ; logical page 2
    LD   E, 2Ah        ; physical (virtual) bank 0x2A
    CALL 0FDDFh        ; MBB_SET_PAGE

    ; Read back which physical bank logical page 2 is on
    LD   C, 2
    CALL 0FDDCh        ; MBB_GET_PAGE -> A = 2Ah
```

`MBB_SET_PAGE` updates both the hardware register and the BIOS's authoritative shadow of the mapping; `MBB_GET_PAGE` returns that shadow. Because the BIOS owns the shadow (and may re-page during disk operations), going through these routines is the only way to keep your view of the mapping in sync with reality.

### Banking I/O ports (here be dragons)

> ⚠️ **The bank-control registers (ports `0x70`–`0x73`) are WRITE-ONLY.** On real Microbeast hardware you cannot read back the latched bank — a read returns open bus, not the current mapping. iz-cpm faithfully emulates this: reading one of these ports yields a floating bus value (approximated by the port number), **not** the value you last wrote. Earlier iz-cpm versions incorrectly let you read the mapping back; code that relied on that is broken on hardware.
>
> Worse, a direct `OUT` to these ports changes the hardware register but **does not** update the BIOS shadow, so a subsequent `MBB_GET_PAGE` will read back stale data and silently desync. Any sane person should use the BIOS routines above and steer clear of these ports entirely.

| Port    | Access    | Purpose                                                                       |
| ------- | --------- | ----------------------------------------------------------------------------- |
| `0x70`  | write-only | Set the virtual bank that physical bank 0 (`0x0000`–`0x3FFF`) maps to        |
| `0x71`  | write-only | Same for physical bank 1 (`0x4000`–`0x7FFF`)                                 |
| `0x72`  | write-only | Same for physical bank 2 (`0x8000`–`0xBFFF`)                                 |
| `0x73`  | write-only | Same for physical bank 3 (`0xC000`–`0xFFFF`)                                 |
| `0x74`  | read/write | `0` = mapping disabled (CPU sees Flash banks 0–3), `1` = mapping enabled     |

When mapping is enabled, writes to a physical bank that points at a Flash virtual bank (`0x00`–`0x1F`) are silently ignored. RAM banks (`0x20`–`0x3F`) are read/write.

### Reset state

On reset, iz-cpm enables mapping with physical banks 0–3 pointing at RAM virtual banks 32–35 (the first 64 KiB of RAM). This deliberately deviates from the Microbeast hardware reset (which leaves mapping disabled and points at Flash) so that iz-cpm's existing CP/M emulation, which writes to low memory during boot, continues to work without modification. A program that wants the hardware-faithful reset behaviour can simply `OUT (74h), 0`.

### Preloading Flash

Use `--flash <path>` to load up to 512 KiB of binary data into Flash ROM virtual banks 0–31 at startup. This is useful for testing Microbeast firmware images.

```
iz-cpm --flash microbeast-firmware.bin
```

## Memory dumping

iz-cpm can dump memory pages to disk on demand, triggered by the running program writing to I/O port `0x80`. Pass `--dump <basename>` to enable; the basename is used for the first dump, with subsequent dumps getting a `.001`, `.002`, … numeric suffix inserted before any extension to avoid overwriting earlier files.

The value written to port `0x80` selects what is dumped:

| Value          | What is written                                                       |
| -------------- | --------------------------------------------------------------------- |
| `0x00`–`0x1F`  | The single Flash virtual bank `n` (16 KiB)                            |
| `0x20`–`0x3F`  | The single RAM virtual bank `n` (16 KiB)                              |
| `0x81`         | All Flash banks `0x00`–`0x1F` as one contiguous 512 KiB blob          |
| `0x82`         | All RAM banks `0x20`–`0x3F` as one contiguous 512 KiB blob            |
| `0x83`         | All banks: 512 KiB Flash followed by 512 KiB RAM (1 MiB total)        |
| anything else  | No-op                                                                 |

If `--dump` is not supplied, writes to port `0x80` are silently ignored.

Example Z80 snippet to dump everything:

```asm
    LD  A, 83h
    OUT (80h), A
```

Run with:

```
iz-cpm --dump snapshot.bin myprogram.com
```

Successive triggers produce `snapshot.bin`, `snapshot.001.bin`, `snapshot.002.bin`, …

## VideoBeast (feature `videobeast`)

Building with `--features videobeast` attaches beastem's VideoBeast, vendored unmodified in `vendor/beastem/` (check with `cd vendor/beastem && sha256sum -c UPSTREAM`), to pages `0x40`–`0x5F`. It needs a C++ compiler, `pkg-config` and the SDL2 development files (`libsdl2-dev` on Debian/Ubuntu). SDL runs with its `dummy` video driver, so no display is needed; the VideoBeast's log lines go to stderr.

On Debian/Ubuntu:

```
sudo apt-get update
sudo apt-get install -y libsdl2-dev pkg-config g++
cargo build --features videobeast
cargo test --features videobeast
```

CI runs this as the `videobeast` job in `.github/workflows/rust.yml`, beside the stock build.

Emulated time is the Z80 cycle count at 8 MHz (125,000 ps per T-state). Start-up registers and palettes come from `$BEASTEM_ASSETS`, or from `vendor/beastem/assets` when it is unset. Only one VideoBeast exists per process at a time.

### Scripted runs

With the feature, stdout carries only the CP/M console. Every iz-cpm message (the banner, `-t`/`-T` call traces, `HALT instruction`, errors) and the VideoBeast's log lines go to stderr. The exception is `-z`/`--cpu-trace`: those lines come from the iz80 crate and stay on stdout.

Extra flags:

- `--exit-on-warmboot`: when running the CCP (no program given), end iz-cpm when a program warm boots instead of reloading the CCP. A program given on the command line already ends iz-cpm when it warm boots.
- `--max-cycles N`: end iz-cpm once the CPU has run N T-states (checked after each instruction), with a message on stderr.
- `--frames N`: end iz-cpm as frame N starts, before any instruction of that frame runs, with `iz-cpm: frame N started (--frames N) at cycle C` on stderr. `--frames 0` runs no instruction of the program.
- `--joy FILE`: a joystick script giving the value port `0x9F` reads in each frame. Without one the port reads `0x00`.
- `--dump-at LIST`: as each listed frame starts, write the VideoBeast's VRAM, registers and both palettes, the Z80's 64K view and the RAM banks to `--out`.
- `--png-at LIST`: as each listed frame starts, write a PNG of the VideoBeast window to `--out`.
- `--out DIR`: where `--dump-at` and `--png-at` write; created if missing. Needed whenever either list is given.
- `--vb-preload LOADTXT`: before the program starts, load a SLIDE file set into VRAM and the palettes, as `SLIDE RV` does on hardware. See "Preloading VRAM and palettes" below.

Frames are numbered from the program: frame 0 starts as the first instruction at `0x100` is about to run, and frame n starts at the nth wrap of the VideoBeast line counter to 0 after that. In the start-up mode (`0x08`) a frame is 525 lines of 256 T-states, 134,400 T-states; the first wrap comes 256 T-states later than that, since the VideoBeast starts on its last line with the counter at 0. When running the CCP, frame 0 starts the first time a program starts at `0x100`; later programs do not restart the count. Until then `--frames` cannot end the run and port `0x9F` reads `0x00`.

A joystick script has one line per change: a decimal frame number, one or more spaces, then `0x` and two hex digits (either case), the raw port bits. Blank lines are ignored and frame numbers must strictly increase. Each value takes effect as its frame starts and holds until the next line; before the first line the port reads `0x00`. An `IN` sees the frame in effect when the instruction starts. For example:

```
2 0x05
4 0x81
7 0x00
```

A script that cannot be read, or has a bad line, ends iz-cpm with exit code 1 before the CPU starts, naming the file and line on stderr, e.g. `iz-cpm: joy.txt:2: frame 1 is not after frame 2`.

The process exit code says how the run ended:

| Code | End |
|---|---|
| 0 | Warm or cold boot, `--frames` reached, Ctrl-C Ctrl-C Y, `--help`, `--version` |
| 1 | Emulator error: bad arguments, missing file, bad joystick script, bad `--dump-at`/`--png-at`/`--out`, bad `--vb-preload` set, capture file not written, program re-entered the TPA base, `HALT`, unimplemented BIOS or BDOS call |
| 2 | `--max-cycles` reached |

The stock build keeps its output on stdout and always exits 0.

The MicroBeast firmware console byte `0xFF2B` (screen_page) lies inside iz-cpm's BIOS jump table, so once the BIOS is set up, writes that reach that RAM byte, from the program or from iz-cpm itself, are ignored, and each one is logged on stderr. A program reads back the jump table's value (`0x8E`). Writes reach it normally when page 3 is mapped elsewhere.

Test programs under `tests/artifacts/vb/` are committed as `.COM` files next to their source; `cargo test` never assembles them. After editing a source, rebuild with sjasmplus 1.24.0 (or set `$SJASMPLUS`) and commit both:

```
tests/artifacts/vb/build.sh
```

#### Dumps and PNGs

A `LIST` is comma-separated decimal frame numbers with no spaces, e.g. `0,2,10`; repeats are allowed, empty items are not. A bad `LIST`, a list without `--out`, or an `--out` that is not a directory and cannot be created ends iz-cpm with exit code 1 before the CPU starts.

Captures for a frame happen as it starts, before its first instruction and before the `--frames` check, so `--frames N --png-at N` writes frame N. Frame 0's are taken before the program's first instruction runs. A listed frame the run never reaches writes nothing, logs `iz-cpm: frame N not reached; nothing written for it` on stderr, and leaves the exit code as it would be without the flag. A file that cannot be written ends iz-cpm with exit code 1. Captures read VideoBeast state without side effects, so they never change what the program sees, and the same binary, assets, program and joystick script give byte-identical files.

Files, with `NNNNN` the frame in decimal, at least 5 digits:

| File | Bytes | Content |
|---|---|---|
| `fNNNNN-vram.bin` | 1,048,576 | VRAM by address |
| `fNNNNN-regs.bin` | 256 | register r at offset r, as stored: `0xFA`/`0xFB` (line counter) and `0xE4`-`0xE7` (multiply product) hold their values as of the program's last unlocked read of a register `0x80`-`0xFD`, not live values |
| `fNNNNN-pal1.bin`, `fNNNNN-pal2.bin` | 512 | palette 1 or 2: entry i at 2i (low byte) and 2i+1 (high byte) |
| `fNNNNN-z80.bin` | 65,536 | what the CPU sees through the current page registers; addresses on a VideoBeast page read `0xFF` |
| `fNNNNN-ram.bin` | 524,288 | RAM pages `0x20`-`0x3F` in order |
| `fNNNNN.png` | varies | the VideoBeast window surface, 8-bit RGB, no chunks but IHDR, IDAT and IEND |

The window surface at frame n's start holds the picture scanned during frame n-1; frame 0's is the start-up background. The PNG is the size of the surface, 640x480 in mode 0, pixel-doubled modes included.

VRAM, registers `0x00`-`0x7F` and the palette entries the asset files do not fill start at 0.

`tests/artifacts/vb/golden/` holds committed outputs of two cases (listed in `tests/artifacts/vb/golden.sh`), and `cargo test --features videobeast` checks a fresh run matches them byte for byte. After re-vendoring beastem, or changing the capture code, regenerate them, review `git diff --stat tests/artifacts/vb/golden` and commit:

```
tests/artifacts/vb/golden.sh
```

#### Preloading VRAM and palettes

`--vb-preload LOADTXT` names a text file listing one path per line, relative to the directory holding it. Blank lines are skipped and trailing whitespace (including `\r`) is trimmed. The file name of each path decides what happens, by SLIDE's rules (case does not matter):

- `vXXXXX.vbm`, `v` and exactly five hex digits: the file's bytes go to VRAM from address `0xXXXXX`.
- `vPB.vbp`, `P` in `0`-`1` and `B` in `0`-`3`: the file loads palette `P` (`0` is palette 1, `1` palette 2) from bank `B` upward, 128 bytes (64 entries) per bank, even byte low. Loading stops after bank 3; the rest of the file is dropped.
- Any other name is ignored, with a line on stderr naming the `LOADTXT` path as given, the line and the listed path, e.g. `iz-cpm: set/load.txt:3: ignoring tools/GAME.COM (not a SLIDE VRAM or palette name)`. Ignored files are not read.

For example:

```
v20000.vbm
sprites/v40000.vbm
v00.vbp
v10.vbp
```

Files apply in listed order, so where two overlap the later one wins. Bytes are written as they are, with no `0x1A` stripping and no padding. The preload is applied before frame 0's captures, so `--dump-at 0` shows it. It changes VRAM and palette contents only: every register is as it would be without `--vb-preload`. On hardware SLIDE leaves register `0xF5` at its last palette select; iz-cpm does not.

A `load.txt` that cannot be read, a listed file that cannot be read, or a `.vbm` whose address plus length is past `0x100000` ends iz-cpm with exit code 1 before the CPU starts, naming `load.txt`, the line and the listed path, e.g. `iz-cpm: set/load.txt:2: vFFF80.vbm: 256 bytes at 0xFFF80 run past the top of VRAM`. (SLIDE on hardware writes the earlier files before it gives up on an overrunning one; iz-cpm refuses the whole set first.)

---

# iz-cpm -- CP/M 2.2 environment

## What is this?

This is a CP/M 2.2 execution environment. It provides everything needed to run a standard CP/M for Z80 or 8080 binary.

Uses my [iz80](https://github.com/ivanizag/iz80) library for Zilog Z80 and Intel 8080 emulation.

Made with Rust

Note that iz-cpm is a very basic implementation, mostly for educational purposes. I recommend MockbaTheBorg's [RunCPM](https://github.com/MockbaTheBorg/RunCPM) or Udo Munk's [Z80pack](https://github.com/udo-munk/z80pack/blob/master/doc/README-cpm.txt) for a much more complete CP/M emulation. 

## Installation
Extract the [latest zip](https://github.com/ivanizag/iz-cpm/releases) for Linux, MacOS or Windows. Optionally run `download.sh` or `download.bat`  to download the CP/M 2.2 system disk, Microsoft Basic, Turbo Pascal, Lisp and some games.

## Build from source
To build from source, install the latest Rust compiler, clone the repo and run `cargo build --release`. To cross compile to Windows, install the target with `rustup` and run `cargo build --release --target x86_64-pc-windows-gnu`.

## Usage examples

Execute `iz-cpm` to open the CP/M command prompt (the CCP) on the current directory:
```console
casa@servidor:~/software/cpm22$ ls
ASM.COM      CCSINIT.COM   DISKDEF.LIB  iz-cpm        STAT.COM
CBIOS.ASM    CCSYSGEN.COM  DUMP.ASM     LOAD.COM      STDBIOS.ASM
CCBIOS.ASM   CPM24CCS.COM  DUMP.COM     MOVCPM.COM    SUBMIT.COM
CCBOOT.ASM   DDT.COM       ED.COM       PIP.COM       SYSGEN.COM
-CCSCPM.251  DEBLOCK.ASM   GENMOD.COM   RLOCBIOS.COM
casa@servidor:~/software/cpm22$ ../../iz-cpm 
iz-cpm https://github.com/ivanizag/iz-cpm
CP/M 2.2 Copyright (c) 1979 by Digital Research
Press ctrl-c ctrl-c to return to host

A>dir
A: CCSINIT  COM : MOVCPM   COM : CPM24CCS COM : STAT     COM
A: SYSGEN   COM : STDBIOS  ASM : IZ-CPM       : LOAD     COM
A: DISKDEF  LIB : ASM      COM : RLOCBIOS COM : DUMP     COM
A: -CCSCPM  251 : CCBIOS   ASM : DUMP     ASM : CCSYSGEN COM
A: DEBLOCK  ASM : ED       COM : SUBMIT   COM : DDT      COM
A: PIP      COM : CBIOS    ASM : CCBOOT   ASM : GENMOD   COM
A>
```

Execute `iz-cpm` with a file to execute a CP/M binary directly, bypassing the CPP:
```console
casa@servidor:~$ ./iz-cpm software/OBASIC.COM 
44531 Bytes free
BASIC Rev. 4.51
[CP/M Version]
Copyright 1977 (C) by Microsoft
Ok
print "hello"
hello
Ok

```

Map up to 16 directories as CP/M drives:
```console
casa@servidor:~$ ./iz-cpm --disk-a software/cpm22 --disk-b software/zork --disk-e .
iz-cpm https://github.com/ivanizag/iz-cpm
CP/M 2.2 Copyright (c) 1979 by Digital Research
Press ctrl-c ctrl-c to return to host

A>b:
B>dir
B: ZORK2    DAT : ZORK1    COM : ZORK3    DAT : ZORK3    COM
B: FILE_ID  DIZ : ZORK1    DAT : ZORK1    SAV : ZORK2    COM
B>zork1
ZORK I: The Great Underground Empire
Copyright (c) 1981, 1982, 1983 Infocom, Inc. All rights
reserved.
ZORK is a registered trademark of Infocom, Inc.
Revision 88 / Serial number 840726

West of House
You are standing in an open field west of a white house, with
a boarded front door.
There is a small mailbox here.

>
```

## Usage
```
iz-cpm https://github.com/ivanizag/iz-cpm
CP/M 2.2 Copyright (c) 1979 by Digital Research
Press ctrl-c ctrl-c to return to host 

USAGE:
    iz-cpm [FLAGS] [OPTIONS] [ARGS]

FLAGS:
    -t, --call-trace        Traces BDOS calls excluding screen I/O
    -T, --call-trace-all    Traces BDOS and BIOS calls
    -z, --cpu-trace         Traces Z80 instructions execution
    -h, --help              Prints help information
    -s, --slow              Runs slower
    -V, --version           Prints version information

OPTIONS:
        --cpu <model>            cpu model z80 or 8080 [default: z80]
    -a, --disk-a <path>          directory to map disk A: [default: .]
    -b, --disk-b <path>          directory to map disk B:
    -c, --disk-c <path>          directory to map disk C:
    -d, --disk-d <path>          directory to map disk D:
        --disk-e <path>          directory to map disk E:
        --disk-f <path>          directory to map disk F:
        --disk-g <path>          directory to map disk G:
        --disk-h <path>          directory to map disk H:
        --disk-i <path>          directory to map disk I:
        --disk-j <path>          directory to map disk J:
        --disk-k <path>          directory to map disk K:
        --disk-l <path>          directory to map disk L:
        --disk-m <path>          directory to map disk M:
        --disk-n <path>          directory to map disk N:
        --disk-o <path>          directory to map disk O:
        --disk-p <path>          directory to map disk P:
        --terminal <terminal>    Terminal emulation ADM-3A or ANSI [default: adm3a]


ARGS:
    <CMD>     The binay image to run, 
    <ARGS>    Parameters for the given command
```

## Features

- Execution of 8080 and Z80 binaries on top of CP/M
- Direct usage of the host computer filesystem
- Terminal emulation of ADM-3A as used in the KAYPRO computers
- Z80 emulation validated with ZEXALL
- CPU execution tracing
- BDOS and BIOS tracing
- Portable, runs in Linux, MacOS and Windows. Mmm, not in CP/M.

## How does it work

[CP/M](https://en.wikipedia.org/wiki/CP/M) was designed to be portable to a variety of devices using Intel 8080, Intel 8085 or Zilog Z80 processors thanks to a tiered architecture. The main parts of CP/M are:

- The BIOS: Basic Input/Output system. Interface with the hardware. CP/M defines a very small interface: 16 entrypoints to manage I/O and access to disk sectors. The hardware vendors provided the BIOS for their device based on source code available on the CP/M distribution.
- The BDOS: Basic Disk Operating System. Main component of CP/M. Provided by Digital Research and common to all systems. Provides the high level entrypoints to be used by application developpers.
- The CCP: Console Command Processor. The CP/M Command prompt.

To emulate this environment using the host filesystem, we have to provide a replacement BDOS translating as we don't want to relay on the physical disk sectors abstraction of the BIOS. The main components are:

- Z80 emulator. It uses [iz80](https://github.com/ivanizag/iz80)
- [BIOS](src/bios.rs) emulator. Only the I/O entrypoints. In theory, it shouldn't be necessary, but some programs use it directly, bypassing BDOS.
- [BDOS](src/bdos.rs) emulator. Traps the calls and executes code on the  host.
- CPP. Runs natively, no emulation needed. We use ZCPR1 an open source alternative. See [cpmish](http://cowlark.com/cpmish/) for other open source alternatives to the CP/M binaries. The CPP binary from CP/M 2.2 can be used optionally.
- [Terminal](src/terminal.rs) emulator. CP/M does not define how the terminal should work. Applications needed to be aware and usually could be configured for several leading options, like ADM-3a, VT-52, Hazeltine 1500 and Osborne. This emulator supports ADM-3a used also on the very popular Kaypro computers.

## Useful links:

- [The Unoffcial CP/M Web site](http://www.cpm.z80.de/)
- [CP/M Operating System Manual](http://www.gaby.de/cpm/manuals/archive/cpm22htm/)
- [CP/M Software in retroarchive.org](http://www.retroarchive.org/cpm/)

## TODO
- Proper documentation
- File level read-only option (I won't do that, the host can control that)
- BIOS support for punch cards (Nope)
- BIOS support for track/sector access to disks (Not needed)
