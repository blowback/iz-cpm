use izcpm::{run, Console};

fn main() {
    let mut console = Console::new();
    let code = run(None, &mut console);
    // Under feature `videobeast` the process reports how the run ended
    // (0 normal end, 1 emulator error, 2 cycle limit). The console is dropped
    // first so the host terminal settings are restored; process::exit skips
    // destructors.
    #[cfg(feature = "videobeast")]
    {
        drop(console);
        std::process::exit(code);
    }
    #[cfg(not(feature = "videobeast"))]
    let _ = code;
}
