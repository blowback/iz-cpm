use std::io::{Read, stdin, Write, stdout};
use std::thread;
use std::time::Duration;

use termios::*;

use crate::console_emulator::ConsoleEmulator;

const STDIN_FD: i32 = 0;

// A status check that finds stdin, not a terminal, at end of file more than
// this many T-states after the previous one starts a new run of polls.
#[cfg(feature = "videobeast")]
const POLL_RUN_MAX_GAP_T_STATES: u64 = 1_000;

// A run of polls at end of file that spans this many T-states (one frame)
// ends console input.
#[cfg(feature = "videobeast")]
const POLL_RUN_END_T_STATES: u64 = 134_400;

// The terminal settings found at start-up, for `restore_at_exit`.
#[cfg(feature = "videobeast")]
static AT_EXIT_TERMIOS: std::sync::OnceLock<Termios> = std::sync::OnceLock::new();

#[cfg(feature = "videobeast")]
extern "C" {
    fn atexit(cb: extern "C" fn()) -> std::ffi::c_int;
}

// Runs on every exit, `std::process::exit` in main included; on a normal
// exit it re-applies the settings `Drop` already restored. It is needed for
// C `exit` calls that skip `Drop`, such as the vendored beastem code's when
// it cannot create the window. Errors are ignored; the process is ending.
#[cfg(feature = "videobeast")]
extern "C" fn restore_at_exit() {
    if let Some(initial) = AT_EXIT_TERMIOS.get() {
        let _ = tcsetattr(STDIN_FD, TCSANOW, initial);
    }
}

// Records `initial` and registers `restore_at_exit`, on the first call only.
#[cfg(feature = "videobeast")]
fn restore_terminal_at_exit(initial: Termios) {
    if AT_EXIT_TERMIOS.set(initial).is_ok() {
        unsafe {
            atexit(restore_at_exit);
        }
    }
}

pub struct Console {
    initial_termios: Option<Termios>,
    next_char: Option<u8>,
    // Set when a blocking read finds stdin at end of file, or when a
    // program keeps polling console status at end of file (see `status`).
    #[cfg(feature = "videobeast")]
    input_ended: bool,
    // The CPU's T-state count, given before each instruction, so a status
    // check sees the count at its trap.
    #[cfg(feature = "videobeast")]
    cycle_count: u64,
    // The current run of status checks at end of file: the T-state count
    // at its first poll and at its latest. A console read or write ends it.
    #[cfg(feature = "videobeast")]
    poll_run: Option<(u64, u64)>,
}

impl Console {
    pub fn new() -> Console {
        // Prepare terminal
        let initial_termios = Termios::from_fd(STDIN_FD).ok();
        #[cfg(feature = "videobeast")]
        if let Some(initial) = initial_termios {
            restore_terminal_at_exit(initial);
        }

        let c = Console {
            initial_termios,
            next_char: None,
            #[cfg(feature = "videobeast")]
            input_ended: false,
            #[cfg(feature = "videobeast")]
            cycle_count: 0,
            #[cfg(feature = "videobeast")]
            poll_run: None,
        };

        c.setup_host_terminal(false);
        c
    }

    fn setup_host_terminal(&self, blocking: bool) {
        if let Some(mut initial) = self.initial_termios {
            initial.c_iflag &= !(IXON | ICRNL);
            initial.c_lflag &= !(ISIG | ECHO | ICANON | IEXTEN);
            initial.c_cc[VMIN] = if blocking {1} else {0};
            initial.c_cc[VTIME] = 0;
            tcsetattr(STDIN_FD, TCSANOW, &initial).unwrap();
        }
    }

    #[cfg(not(feature = "videobeast"))]
    fn read_blocking(&mut self) -> u8 {
        let mut buf = [0];
        stdin().read_exact(&mut buf).unwrap();
        buf[0]
    }

    // At end of file, records that input has ended and returns a carriage
    // return, which ends a BDOS line read without being echoed; the run loop
    // then ends the run.
    #[cfg(feature = "videobeast")]
    fn read_blocking(&mut self) -> u8 {
        let mut buf = [0];
        let result = stdin().read_exact(&mut buf);
        if matches!(&result, Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof) {
            self.input_ended = true;
            return b'\r';
        }
        result.unwrap();
        buf[0]
    }

    // Records a status check that found stdin, not a terminal, at end of
    // file. Only a program that keeps polling, with no console read or write
    // and no gap over POLL_RUN_MAX_GAP_T_STATES between polls, for
    // POLL_RUN_END_T_STATES ends console input; a CCP `DIR` or an abort-key
    // check between other work keeps seeing "no key".
    #[cfg(feature = "videobeast")]
    fn note_poll_at_end_of_file(&mut self) {
        let now = self.cycle_count;
        match self.poll_run {
            Some((start, last)) if now.saturating_sub(last) <= POLL_RUN_MAX_GAP_T_STATES => {
                self.poll_run = Some((start, now));
                if now.saturating_sub(start) >= POLL_RUN_END_T_STATES {
                    self.input_ended = true;
                }
            }
            _ => self.poll_run = Some((now, now)),
        }
    }
}

impl ConsoleEmulator for Console {
    fn status(&mut self) -> bool {
        match self.next_char {
            Some(_) => true,
            None => {
                let mut buf = [0];
                let result = stdin().read(&mut buf);
                // A terminal set up with VMIN 0 also reads 0 bytes when no
                // key is waiting, so only stdin that is not a terminal is at
                // end of file here. A read error is never end of file.
                #[cfg(feature = "videobeast")]
                if matches!(result, Ok(0)) && self.initial_termios.is_none() {
                    self.note_poll_at_end_of_file();
                    return false;
                }
                let size = result.unwrap_or(0);
                if size != 0 {
                    self.next_char = Some(buf[0]);
                    true
                } else {
                    // Avoid 100% CPU usage waiting for input.
                    thread::sleep(Duration::from_nanos(100));
                    false
                }
            }
        }
    }

    fn read(&mut self) -> u8 {
        #[cfg(feature = "videobeast")]
        {
            self.poll_run = None;
        }
        match self.next_char {
            Some(ch) => {
                self.next_char = None;
                ch
            },
            None => {
                // Blocks waiting for char
                self.setup_host_terminal(true);
                let ch = self.read_blocking();
                self.setup_host_terminal(false);
                ch
            }
        }
    }

    #[cfg(feature = "videobeast")]
    fn input_ended(&self) -> bool {
        self.input_ended
    }

    #[cfg(feature = "videobeast")]
    fn note_cycle_count(&mut self, t_states: u64) {
        self.cycle_count = t_states;
    }

    fn put(&mut self, sequence: Option<String>) {
        if let Some(sequence) = sequence {
            #[cfg(feature = "videobeast")]
            {
                self.poll_run = None;
            }
            print!("{}", sequence);
            stdout().flush().unwrap();
        }
    }

    fn terminated(&self) -> bool {
        false
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        if let Some(initial) = self.initial_termios {
            tcsetattr(STDIN_FD, TCSANOW, &initial).unwrap();
        }
    }
}
