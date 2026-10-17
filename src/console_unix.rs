use std::io::{Read, stdin, Write, stdout};
use std::thread;
use std::time::Duration;

use termios::*;

use crate::console_emulator::ConsoleEmulator;

const STDIN_FD: i32 = 0;

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
    // Set when a blocking read finds stdin at end of file.
    #[cfg(feature = "videobeast")]
    input_ended: bool,
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
}

impl ConsoleEmulator for Console {
    fn status(&mut self) -> bool {
        match self.next_char {
            Some(_) => true,
            None => {
                let mut buf = [0];
                let size = stdin().read(&mut buf).unwrap_or(0);
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

    fn put(&mut self, sequence: Option<String>) {
        if let Some(sequence) = sequence {
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
