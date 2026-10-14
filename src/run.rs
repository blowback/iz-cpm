use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::thread;
use std::time::Duration;

use clap::{Arg, App};
#[cfg(feature = "videobeast")]
use clap::ArgMatches;
use iz80::*;


use crate::bdos::Bdos;
use crate::bdos::execute_bdos;
use crate::console_emulator::ConsoleEmulator;
use crate::bios::Bios;
use crate::constants::*;
use crate::cpm_machine::CpmMachine;
use crate::fcb::*;
use crate::terminal::TerminalEmulator;
use crate::terminal::Transparent;
use crate::terminal_adm3a::Adm3aToAnsi;
#[cfg(feature = "videobeast")]
use std::collections::BTreeSet;
#[cfg(feature = "videobeast")]
use std::path::{Path, PathBuf};
#[cfg(feature = "videobeast")]
use crate::videobeast::{VideoBeast, PS_PER_T_STATE};
#[cfg(feature = "videobeast")]
use crate::videobeast::joy::JoyScript;
#[cfg(feature = "videobeast")]
use crate::videobeast::capture::{self, FrameList};
#[cfg(feature = "videobeast")]
use crate::videobeast::preload::Preload;
#[cfg(feature = "videobeast")]
use crate::videobeast::window::{joy_bits, Pacer};

// Welcome message
const WELCOME: &str =
"iz-cpm https://github.com/ivanizag/iz-cpm
CP/M 2.2 Emulation
Press ctrl-c ctrl-c Y to return to host";

// Exit codes returned by `run` (architecture AD-20). Under feature
// `videobeast` main passes them to the process; the stock build ignores them.
const EXIT_OK: i32 = 0;
const EXIT_ERROR: i32 = 1;
#[cfg(feature = "videobeast")]
const EXIT_CYCLE_LIMIT: i32 = 2;

static CCP_BINARY: &[u8] = include_bytes!("../third-party/bin/zcpr.bin");

/// Runs iz-cpm to completion and returns the exit code: EXIT_OK for a normal
/// end, EXIT_CYCLE_LIMIT for `--max-cycles`, EXIT_ERROR for anything else.
pub fn run(command_line: Option<Vec<&str>>, console: &mut dyn ConsoleEmulator) -> i32 {
    // Parse arguments
    let app = App::new(WELCOME)
    .arg(Arg::with_name("CMD")
        .help("The binary image to run, usually a .COM file")
        .required(false)
        .index(1))
        .arg(Arg::with_name("ARGS")
        .help("Parameters for the given command")
        .required(false)
        .index(2))
    .arg(Arg::with_name("call_trace")
        .short("t")
        .long("call-trace")
        .help("Traces BDOS calls excluding screen I/O"))
    .arg(Arg::with_name("call_trace_all")
        .short("T")
        .long("call-trace-all")
        .help("Traces BDOS and BIOS calls"))
    .arg(Arg::with_name("cpu_trace")
        .short("z")
        .long("cpu-trace")
        .help("Traces CPU instructions execution"))
    .arg(Arg::with_name("slow")
        .short("s")
        .long("slow")
        .help("Runs slower"))
    .arg(Arg::with_name("cpu")
        .long("cpu")
        .value_name("model")
        .default_value("z80")
        .help("Cpu model z80 or 8080"))
    .arg(Arg::with_name("terminal")
        .long("terminal")
        .default_value("adm3a")
        .help("Terminal emulation ADM-3A or ANSI"))
    .arg(Arg::with_name("ccp")
        .long("ccp")
        .value_name("ccp")
        .help("Alternative CPP binary, it must be compiled with CCP_BASE=$f000"))
    .arg(Arg::with_name("flash")
        .long("flash")
        .value_name("path")
        .help("Path to a Flash ROM image (up to 512KB) loaded into virtual banks 0-31"))
    .arg(Arg::with_name("dump")
        .long("dump")
        .value_name("path")
        .help("Basename for memory dumps triggered by OUT (80h)"))
    .arg(Arg::with_name("disk_a").long("disk-a").value_name("path").short("a").default_value(".").help("directory to map disk A:"))
    .arg(Arg::with_name("disk_b").long("disk-b").value_name("path").short("b").help("directory to map disk B:"))
    .arg(Arg::with_name("disk_c").long("disk-c").value_name("path").short("c").help("directory to map disk C:"))
    .arg(Arg::with_name("disk_d").long("disk-d").value_name("path").short("d").help("directory to map disk D:"))
    .arg(Arg::with_name("disk_e").long("disk-e").value_name("path").help("directory to map disk E:"))
    .arg(Arg::with_name("disk_f").long("disk-f").value_name("path").help("directory to map disk F:"))
    .arg(Arg::with_name("disk_g").long("disk-g").value_name("path").help("directory to map disk G:"))
    .arg(Arg::with_name("disk_h").long("disk-h").value_name("path").help("directory to map disk H:"))
    .arg(Arg::with_name("disk_i").long("disk-i").value_name("path").help("directory to map disk I:"))
    .arg(Arg::with_name("disk_j").long("disk-j").value_name("path").help("directory to map disk J:"))
    .arg(Arg::with_name("disk_k").long("disk-k").value_name("path").help("directory to map disk K:"))
    .arg(Arg::with_name("disk_l").long("disk-l").value_name("path").help("directory to map disk L:"))
    .arg(Arg::with_name("disk_m").long("disk-m").value_name("path").help("directory to map disk M:"))
    .arg(Arg::with_name("disk_n").long("disk-n").value_name("path").help("directory to map disk N:"))
    .arg(Arg::with_name("disk_o").long("disk-o").value_name("path").help("directory to map disk O:"))
    .arg(Arg::with_name("disk_p").long("disk-p").value_name("path").help("directory to map disk P:"));
    #[cfg(feature = "videobeast")]
    let app = app
    .arg(Arg::with_name("exit_on_warmboot")
        .long("exit-on-warmboot")
        .help("Ends iz-cpm when a program warm boots, instead of reloading the CCP"))
    .arg(Arg::with_name("max_cycles")
        .long("max-cycles")
        .value_name("N")
        .help("Ends iz-cpm with exit code 2 once the CPU has run N T-states"))
    .arg(Arg::with_name("frames")
        .long("frames")
        .value_name("N")
        .help("Ends iz-cpm as frame N starts; frame 0 starts with the program at 0x100"))
    .arg(Arg::with_name("joy")
        .long("joy")
        .value_name("FILE")
        .help("Joystick script for port 0x9F: lines of <frame> 0xNN"))
    .arg(Arg::with_name("dump_at")
        .long("dump-at")
        .value_name("LIST")
        .help("Writes VRAM, VideoBeast registers and palettes, the Z80 64K view and RAM to --out as each listed frame starts (e.g. 0,2)"))
    .arg(Arg::with_name("png_at")
        .long("png-at")
        .value_name("LIST")
        .help("Writes a PNG of the VideoBeast window to --out as each listed frame starts"))
    .arg(Arg::with_name("out")
        .long("out")
        .value_name("DIR")
        .help("Directory for --dump-at and --png-at files; created if missing"))
    .arg(Arg::with_name("vb_preload")
        .long("vb-preload")
        .value_name("LOADTXT")
        .help("Before the program starts, loads the vXXXXX.vbm and vPB.vbp files listed in LOADTXT into VRAM and palettes, as SLIDE RV does"))
    .arg(Arg::with_name("window")
        .long("window")
        .help("Shows the VideoBeast in a window at real speed; arrows and Z/X/C drive port 0x9F unless --joy is given. Closing the window ends iz-cpm"));

    let matches = match command_line {
        None => app.get_matches_safe(),
        Some(args) => {
            let mut args_complete = vec!("testbin");
            args_complete.extend(args.iter());
            app.get_matches_from_safe(args_complete)
        }
    };

    let matches = match matches {
        Ok(m) => m,
        Err(e) => {
            eprint!("{}", e);
            return match e.kind {
                clap::ErrorKind::HelpDisplayed | clap::ErrorKind::VersionDisplayed => EXIT_OK,
                _ => EXIT_ERROR,
            };
        }
    };
        
    let filename = matches.value_of("CMD");
    let params = matches.value_of("ARGS");
    let cpu_trace = matches.is_present("cpu_trace");
    let call_trace = matches.is_present("call_trace") || matches.is_present("call_trace_all");
    let call_trace_all = matches.is_present("call_trace_all");
    let slow = matches.is_present("slow");
    let cpu_model = matches.value_of("cpu");
    let terminal = matches.value_of("terminal");
    let ccp_filename = matches.value_of("ccp");
    let flash_filename = matches.value_of("flash");
    let dump_basename = matches.value_of("dump");
    let use_tpa = filename.is_none();
    #[cfg(feature = "videobeast")]
    let exit_on_warmboot = matches.is_present("exit_on_warmboot");
    #[cfg(not(feature = "videobeast"))]
    let exit_on_warmboot = false;
    #[cfg(feature = "videobeast")]
    let mut vb = match VideoBeastRun::from_matches(&matches) {
        Ok(vb) => vb,
        Err(()) => return EXIT_ERROR,
    };

    // Init device
    let mut machine = CpmMachine::new();
    #[cfg(feature = "videobeast")]
    vb.attach(&mut machine);
    if let Some(path) = dump_basename {
        machine.set_dump_basename(std::path::PathBuf::from(path));
    }
    if let Some(path) = flash_filename {
        let mut buf = Vec::new();
        match File::open(path) {
            Err(err) => {
                eprintln!("Error opening flash image \"{}\": {}", path, err);
                return EXIT_ERROR;
            }
            Ok(mut file) => {
                if let Err(err) = file.read_to_end(&mut buf) {
                    eprintln!("Error reading flash image \"{}\": {}", path, err);
                    return EXIT_ERROR;
                }
            }
        }
        if let Err(err) = machine.load_flash(&buf) {
            eprintln!("Error loading flash image \"{}\": {}", path, err);
            return EXIT_ERROR;
        }
    }
    let mut cpu = match cpu_model {
        Some("z80") => Cpu::new_z80(),
        Some("8080") => Cpu::new_8080(),
        _ => {
            eprintln!("Invalid CPU model. Choose \"z80\" or \"8080\" as the CPU.");
            return EXIT_ERROR;
        }
    };

    // Init BIOS
    let term_emu: Box<dyn TerminalEmulator> = match terminal {
        Some("adm3a") => Box::new(Adm3aToAnsi::new()),
        Some("ansi") => Box::new(Transparent::new()),
        _ => {
            eprintln!("Unkown terminal emulation. Choose \"adm3a\" or \"ansi\".");
            return EXIT_ERROR;
        }
    };
/*     let console = match console {
        None => Console::new(),
        Some(console) => *console
    };
*/
    let mut bios = Bios::new(/*console, */term_emu);
    bios.setup(&mut machine);
    #[cfg(feature = "videobeast")]
    machine.guard_firmware_byte();

    // Init BDOS
    let mut bdos = Bdos::new();
    bdos.reset(&mut machine);

    // Assign drives
    for i in 0..16 {
        let res = matches.value_of(format!("disk_{}", (i + b'a') as char));
        if let Some(path) = res {
            if let Err(err) = fs::read_dir(path) {
                eprintln!("Error with directory \"{}\": {}", path, err);
                return EXIT_ERROR;
            }
            bdos.assign_drive(i, path.to_string());
        }
    }

    // Load CCP or program
    let binary: &[u8];
    let binary_address: u16;
    let binary_size: usize;
    let mut buf = [0u8;65536 - (TPA_BASE_ADDRESS as usize)];
    match filename {
        None => {
            // Load TPA
            binary = match ccp_filename {
                None => {
                    binary_size = CCP_BINARY.len();
                    CCP_BINARY
                },
                Some(name) =>{
                    match File::open(name) {
                        Err(err) => {
                            eprintln!("Error opening ccp \"{}\": {}", name, err);
                            return EXIT_ERROR;
                        },
                        Ok(mut file) => {
                            match file.read(&mut buf) {
                                Err(err) => {
                                    eprintln!("Error loading ccp \"{}\": {}", name, err);
                                    return EXIT_ERROR;
                                },
                                Ok(size) => {
                                    binary_size = size;
                                    &buf
                                }
                            }
                        }
                    }
                }
            };
            binary_address = CCP_BASE_ADDRESS;
            diagln!("{}", WELCOME);
        },
        Some(name) => {
            /*
            If the file is found, it is assumed to be a memory image of a
            program that executes in the TPA and thus implicity originates
            at TBASE in memory.
            */
            match File::open(name) {
                Err(err) => {
                    eprintln!("Error opening \"{}\": {}", name, err);
                    return EXIT_ERROR;
                },
                Ok(mut file) => {
                    match file.read(&mut buf) {
                        Err(err) => {
                            eprintln!("Error loading \"{}\": {}", name, err);
                            return EXIT_ERROR;
                        },
                        Ok(size) => {
                            binary = &buf;
                            binary_address = TPA_BASE_ADDRESS;
                            binary_size = size;
                        }
                    };
                }
            }
        }
    }

    // Load the code in memory
    for i in 0..binary_size {
        machine.poke(binary_address + i as u16, binary[i]);
    }

    if !use_tpa {
        // Upon entry to a transient program, the CCP leaves the stack pointer
        // set to an eight-level stack area with the CCP return address pushed
        // onto the stack, leaving seven levels before overflow occurs.
        if binary_address == TPA_BASE_ADDRESS {
            let mut sp = TPA_STACK_ADDRESS;
            // Push 0x0000
            machine.poke(sp, (0x0000 >> 8) as u8);
            sp -= 1;
            machine.poke(sp, 0x0000_u8);
            sp -= 1;
            cpu.registers().set16(Reg16::SP, sp);
        }

        // Copy parameters As an added convenience, the default buffer area at
        // location BOOT+0080H is initialized to the command line tail typed by
        // the operator following the program name. The first position contains
        // the number of characters, with the characters themselves following
        // the character count. The characters are translated to upper-case
        // ASCII with uninitialized memory following the last valid character.
        Fcb::new(FCB1_ADDRESS).set_name_direct(&mut machine, "        .   ".to_string());
        Fcb::new(FCB2_ADDRESS).set_name_direct(&mut machine, "        .   ".to_string());
        match params {
            None => machine.poke(SYSTEM_PARAMS_ADDRESS, 0),
            Some(p) => {
                let mut len = p.len();
                if len > 0x7E {
                    len = 0x7E; // Max 0x7E chars for parameters
                }
                machine.poke(SYSTEM_PARAMS_ADDRESS, (len + 1) as u8);
                machine.poke(SYSTEM_PARAMS_ADDRESS + 1, b' ');
                let p_bytes = p.as_bytes();
                for i in 0..len {
                    machine.poke(SYSTEM_PARAMS_ADDRESS + (i as u16) + 2, p_bytes[i]);
                }

                // As a convenience, the CCP takes the first two parameters that
                // appear in the command tail, attempts to parse them as though
                // they were file names, and places the results in FCBI and
                // FCB2. The results, in this context, mean that the logical
                // disk letter is converted to its FCB representation, and the
                // file name and type, converted to uppercase, are placed in the
                // FCB in the correct bytes. In addition, any use of "*" in the
                // file name is expanded to one or more question marks. For
                // example, a file name of "abc*.*" will be converted to a name
                // of "ABC!!???" and type of "???". Notice that FCB2 starts only
                // 16 bytes above FCBI, yet a normal FCB is at least 33 bytes
                // long (36 bytes if you want to use random access). In many
                // cases, programs only require a single file name. Therefore,
                // you can proceed to use FCBI straight away, not caring that
                // FCB2 will be overwritten.
                // Both are initialized with spaces.
                let mut parts = p.split_ascii_whitespace();
                if let Some(arg1) = parts.next() {
                    if let Some(file1) = name_to_8_3(arg1) {
                        if call_trace {
                            diagln!("[[FCB1 loaded with {}]]", file1);
                        }
                        Fcb::new(FCB1_ADDRESS).set_name_direct(&mut machine, file1);
                    }
                }
                if let Some(arg2) = parts.next() {
                    if let Some(file2) = name_to_8_3(arg2) {
                        if call_trace {
                            diagln!("[[FCB2 loaded with {}]]", file2);
                        }
                        Fcb::new(FCB2_ADDRESS).set_name_direct(&mut machine, file2);
                    }
                }
            }
        }
    }

    // Run the emulation
    cpu.registers().set_pc(binary_address);
    cpu.set_trace(cpu_trace);
    // Emulated time 0 is now.
    #[cfg(feature = "videobeast")]
    vb.start_clock();
    let mut n = 0;
    let exit_code = loop {
        #[cfg(feature = "videobeast")]
        if let Some(code) = vb.before_instruction(&mut cpu, &mut machine) {
            break code;
        }
        cpu.execute_instruction(&mut machine);
        #[cfg(feature = "videobeast")]
        if let Some(code) = vb.after_instruction(&mut cpu, &mut machine) {
            break code;
        }

        // Runaway guard (single-program mode): a loaded .COM is entered once at
        // the TPA base. Nothing in a well-behaved program jumps back to it — a
        // PC landing there mid-run means control was lost (stack/IP corruption
        // sliding into cold start), which otherwise spins forever with no
        // timeout. Turn that silent hang into an immediate diagnostic.
        if !use_tpa && cpu.registers().pc() == binary_address {
            eprintln!("iz-cpm: program re-entered the TPA base ({:#06x}) — aborting (stack/IP corruption?)", binary_address);
            break EXIT_ERROR;
        }

        // MicroBeast firmware page-mapping routines (MBB_SET_PAGE / MBB_GET_PAGE).
        // The page registers are write-only; the BIOS owns the authoritative
        // shadow, so antforth calls these instead of poking the ports directly.
        // A CALL lands on the RET poked at the address; we perform the op here,
        // then the RET executes next iteration and returns to the caller.
        match cpu.registers().pc() {
            MBB_SET_PAGE_ADDRESS => {
                let logical = cpu.registers().get8(Reg8::A);
                let physical = cpu.registers().get8(Reg8::E);
                machine.mbb_set_page(logical, physical);
            }
            MBB_GET_PAGE_ADDRESS => {
                let logical = cpu.registers().get8(Reg8::C);
                let physical = machine.mbb_get_page(logical);
                cpu.registers().set8(Reg8::A, physical);
            }
            _ => {}
        }

        if cpu.is_halted() {
            diagln!("HALT instruction");
            break EXIT_ERROR;
        }

        let mut er = bios.execute(console, cpu.registers(), call_trace_all);
        if er == ExecutionResult::Continue {
            er = execute_bdos(&mut bdos, &mut bios, console, &mut machine,
                cpu.registers(), call_trace || call_trace_all, call_trace && !call_trace_all);
        }

        match er {
            ExecutionResult::Continue => (),
            // Only unimplemented BIOS or BDOS calls stop the run this way.
            ExecutionResult::Stop => {
                break EXIT_ERROR;
            },
            ExecutionResult::StopConfirm => {
                eprintln!();
                eprintln!("Press Y to exit iz-cpm. Any other key to continue.");
                let ch = bios.read(console) as char;
                if ch == 'Y' || ch == 'y' {
                    break EXIT_OK;
                }
            },
            ExecutionResult::WarmBoot => {
                if call_trace || call_trace_all {
                    diag!("[[Warm boot]]");
                }
                if use_tpa && !exit_on_warmboot {
                    bdos.warm_reset(&mut machine);
                    for i in 0..binary_size {
                        machine.poke(binary_address + i as u16, binary[i]);
                    }
                    cpu.registers().set_pc(binary_address);
                    let user_drive = machine.peek(CCP_USER_DRIVE_ADDRESS);
                    cpu.registers().set8(Reg8::C, user_drive);
                } else {
                    break EXIT_OK;
                }
            },
            ExecutionResult::ColdBoot => {
                if call_trace || call_trace_all {
                    diag!("[[Cold boot]]");
                }
                if use_tpa {
                    bdos.reset(&mut machine);
                    for i in 0..binary_size {
                        machine.poke(binary_address + i as u16, binary[i]);
                    }
                    cpu.registers().set_pc(binary_address);
                    let user_drive = machine.peek(CCP_USER_DRIVE_ADDRESS);
                    cpu.registers().set8(Reg8::C, user_drive);
                    bdos.reset(&mut machine); // Reset Bdos
                } else {
                    break EXIT_OK;
                }
            }
        }

        #[cfg(feature = "videobeast")]
        if console.input_ended() {
            eprintln!("iz-cpm: console input ended");
            break EXIT_ERROR;
        }

        if console.terminated() {
            break EXIT_OK;
        }

        if slow {
            n += 1;
            if n > 20 {
                thread::sleep(Duration::from_nanos(1000));
                n = 0;
            }
        }
    };
    #[cfg(feature = "videobeast")]
    vb.finish();
    exit_code
}

/// The `videobeast` options and the run loop's state for them: frames,
/// captures, `--max-cycles` and `--window`.
#[cfg(feature = "videobeast")]
struct VideoBeastRun {
    max_cycles: Option<u64>,
    frames: Option<u64>,
    window: bool,
    // With --joy the script drives port 0x9F and the keys are ignored.
    keys_drive_joy: bool,
    joy_script: JoyScript,
    preload: Preload,
    dump_at: FrameList,
    png_at: FrameList,
    out_dir: Option<PathBuf>,
    // Listed frames not yet captured, in order.
    pending_captures: BTreeSet<u64>,
    last_frame: Option<u64>,
    // --window: beastem's frame starts as of the last event poll.
    polled_frame_starts: u64,
    pacer: Pacer,
}

#[cfg(feature = "videobeast")]
impl VideoBeastRun {
    /// Reads and checks the options, creating `--out`. On a bad option,
    /// prints why on stderr and returns Err.
    fn from_matches(matches: &ArgMatches) -> Result<VideoBeastRun, ()> {
        let max_cycles = parse_u64_flag(matches, "max_cycles", "--max-cycles", "T-states")?;
        let frames = parse_u64_flag(matches, "frames", "--frames", "frames")?;
        let window = matches.is_present("window");
        let keys_drive_joy = window && !matches.is_present("joy");
        let joy_script = load_or_default(matches, "joy", JoyScript::load)?;

        let parse = |name: &str, flag: &str| match matches.value_of(name) {
            None => Ok(FrameList::default()),
            Some(text) => FrameList::parse(flag, text),
        };
        let (dump_at, png_at) = match (parse("dump_at", "--dump-at"), parse("png_at", "--png-at")) {
            (Ok(dump_at), Ok(png_at)) => (dump_at, png_at),
            (Err(err), _) | (_, Err(err)) => {
                eprintln!("{}", err);
                return Err(());
            }
        };
        let out_dir = match matches.value_of("out") {
            None => {
                if !dump_at.is_empty() || !png_at.is_empty() {
                    eprintln!("iz-cpm: --dump-at and --png-at need --out DIR to say where the files go");
                    return Err(());
                }
                None
            }
            Some(path) => {
                let dir = PathBuf::from(path);
                if dir.exists() && !dir.is_dir() {
                    eprintln!("iz-cpm: --out {}: not a directory", path);
                    return Err(());
                }
                if let Err(err) = fs::create_dir_all(&dir) {
                    eprintln!("iz-cpm: --out {}: {}", path, err);
                    return Err(());
                }
                Some(dir)
            }
        };
        let preload = load_or_default(matches, "vb_preload", Preload::load)?;
        let pending_captures = dump_at.frames().iter().chain(png_at.frames()).copied().collect();
        Ok(VideoBeastRun {
            max_cycles,
            frames,
            window,
            keys_drive_joy,
            joy_script,
            preload,
            dump_at,
            png_at,
            out_dir,
            pending_captures,
            last_frame: None,
            polled_frame_starts: 0,
            pacer: Pacer::new(),
        })
    }

    /// Starts the VideoBeast, with `--window` if given, applies
    /// `--vb-preload` and attaches it to `machine` with the `--joy` script.
    fn attach(&mut self, machine: &mut CpmMachine) {
        let mut videobeast = if self.window { VideoBeast::with_window() } else { VideoBeast::new() };
        std::mem::take(&mut self.preload).apply(&mut videobeast);
        machine.attach_videobeast(videobeast);
        machine.set_joy_script(std::mem::take(&mut self.joy_script));
    }

    /// Makes emulated time 0 now for `--window` pacing.
    fn start_clock(&mut self) {
        self.pacer = Pacer::new();
    }

    /// Runs before each instruction: starts frames, writes captures, ends
    /// the run at `--frames`, and stamps the instruction's start time.
    /// Returns the exit code if the run ends here.
    fn before_instruction(&mut self, cpu: &mut Cpu, machine: &mut CpmMachine) -> Option<i32> {
        // Frames (AD-20): frame 0 starts as the .COM's first instruction is
        // about to run; later entries at 0x100 (CCP mode) do not restart it.
        // Checked before the instruction, so no instruction of frame N runs.
        if cpu.registers().pc() == TPA_BASE_ADDRESS {
            machine.start_frames();
        }
        // Captures for frame n come before the --frames check, so
        // `--frames N --png-at N` writes frame N.
        if let Some(frame) = machine.current_frame() {
            if self.last_frame != Some(frame) {
                self.last_frame = Some(frame);
                if self.pending_captures.remove(&frame) {
                    let dir = self.out_dir.as_deref().expect("--out is checked before the run");
                    if let Err(err) = capture_frame(machine, dir, frame, self.dump_at.contains(frame), self.png_at.contains(frame)) {
                        eprintln!("{}", err);
                        return Some(EXIT_ERROR);
                    }
                }
            }
        }
        if let (Some(limit), Some(frame)) = (self.frames, machine.current_frame()) {
            if frame >= limit {
                eprintln!("iz-cpm: frame {} started (--frames {}) at cycle {}", frame, limit, cpu.cycle_count());
                return Some(EXIT_OK);
            }
        }

        // VideoBeast accesses are stamped with the time the instruction
        // starts; the catch-up after the previous instruction already brought
        // the VideoBeast to that time, so its clock never goes backwards.
        machine.set_instr_start_ps(cpu.cycle_count() * PS_PER_T_STATE);
        None
    }

    /// Runs after each instruction: brings the VideoBeast up to now, handles
    /// `--window` and ends the run at `--max-cycles`. Returns the exit code
    /// if the run ends here.
    fn after_instruction(&mut self, cpu: &mut Cpu, machine: &mut CpmMachine) -> Option<i32> {
        machine.videobeast_catch_up(cpu.cycle_count() * PS_PER_T_STATE);

        // --window: at each VideoBeast frame start, whether or not program
        // frames have started, handle window events, sample the keys for the
        // frame and hold emulated time to wall-clock time.
        if self.window {
            let starts = machine.videobeast().map_or(0, |vb| vb.frame_starts());
            if starts != self.polled_frame_starts {
                self.polled_frame_starts = starts;
                let vb = machine.videobeast_mut().expect("--window runs with a VideoBeast");
                if vb.poll_quit() {
                    match machine.current_frame() {
                        Some(frame) => eprintln!("iz-cpm: window closed at frame {}", frame),
                        None => eprintln!("iz-cpm: window closed at frame -"),
                    }
                    return Some(EXIT_OK);
                }
                if self.keys_drive_joy {
                    let bits = joy_bits(|scancode| vb.key_down(scancode));
                    machine.set_live_joy(Some(bits));
                }
                self.pacer.wait_until(cpu.cycle_count() * PS_PER_T_STATE);
            }
        }

        if let Some(limit) = self.max_cycles {
            if cpu.cycle_count() >= limit {
                eprintln!("iz-cpm: cycle limit reached (--max-cycles {}) at PC {:#06x}", limit, cpu.registers().pc());
                return Some(EXIT_CYCLE_LIMIT);
            }
        }
        None
    }

    /// Runs once the loop has ended: names each listed frame not reached.
    fn finish(self) {
        for frame in self.pending_captures {
            eprintln!("iz-cpm: frame {} not reached; nothing written for it", frame);
        }
    }
}

/// The value of whole-number option `name` (`flag` on the command line),
/// None if absent. On a bad value, prints why on stderr and returns Err.
#[cfg(feature = "videobeast")]
fn parse_u64_flag(matches: &ArgMatches, name: &str, flag: &str, unit: &str) -> Result<Option<u64>, ()> {
    match matches.value_of(name) {
        None => Ok(None),
        Some(text) => match text.parse::<u64>() {
            Ok(n) => Ok(Some(n)),
            Err(_) => {
                eprintln!("Invalid {} value \"{}\": expected a whole number of {}.", flag, text, unit);
                Err(())
            }
        },
    }
}

/// `load(path)` for the path given to option `name`, or `T::default()` if
/// the option is absent. On a load error, prints it on stderr and returns
/// Err.
#[cfg(feature = "videobeast")]
fn load_or_default<T: Default, E: std::fmt::Display>(
    matches: &ArgMatches,
    name: &str,
    load: impl FnOnce(&Path) -> Result<T, E>,
) -> Result<T, ()> {
    match matches.value_of(name) {
        None => Ok(T::default()),
        Some(path) => match load(Path::new(path)) {
            Ok(value) => Ok(value),
            Err(err) => {
                eprintln!("{}", err);
                Err(())
            }
        },
    }
}

/// Writes frame `frame`'s dumps and/or PNG to `dir` (`--dump-at`, `--png-at`).
#[cfg(feature = "videobeast")]
fn capture_frame(machine: &CpmMachine, dir: &Path, frame: u64, dump: bool, png: bool) -> Result<(), String> {
    let vb = machine.videobeast().expect("frames only start with a VideoBeast attached");
    if dump {
        capture::write_dumps(dir, frame, vb, &machine.cpu_view(), machine.ram())?;
    }
    if png {
        capture::write_png(dir, frame, vb)?;
    }
    Ok(())
}
