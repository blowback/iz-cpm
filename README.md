# iz-cpm — Microbeast fork

This fork extends [iz-cpm](https://github.com/ivanizag/iz-cpm) with hardware features from the [Microbeast](https://github.com/) Z80 SBC so that Microbeast software can be developed and exercised on a host machine while still benefiting from iz-cpm's CP/M 2.2 emulation.

## Memory banking

The CPU's 64 KiB address space is divided into four 16 KiB *physical* banks. Each physical bank can be remapped to any of 64 *virtual* 16 KiB banks, giving 1 MiB of addressable memory:

| Virtual bank range | Storage              |
| ------------------ | -------------------- |
| `0x00`–`0x1F`      | 512 KiB Flash ROM    |
| `0x20`–`0x3F`      | 512 KiB RAM          |

Only the bottom 6 bits of a bank-select value are used, so writing `0xFF` is equivalent to writing `0x3F`.

### Banking I/O ports

| Port    | Purpose                                                                       |
| ------- | ----------------------------------------------------------------------------- |
| `0x70`  | Set the virtual bank that physical bank 0 (`0x0000`–`0x3FFF`) maps to         |
| `0x71`  | Same for physical bank 1 (`0x4000`–`0x7FFF`)                                  |
| `0x72`  | Same for physical bank 2 (`0x8000`–`0xBFFF`)                                  |
| `0x73`  | Same for physical bank 3 (`0xC000`–`0xFFFF`)                                  |
| `0x74`  | `0` = mapping disabled (CPU sees Flash banks 0–3), `1` = mapping enabled      |

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
