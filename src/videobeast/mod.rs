//! Safe wrapper over the C++ shim (`shim/vbshim.h`) around beastem's
//! VideoBeast, vendored unmodified in `vendor/beastem/`.
//!
//! Only one VideoBeast may exist per process at a time: SDL video is
//! process-global and not thread-safe. `VideoBeast::new` blocks until the
//! previous one is dropped, so parallel `run()` calls (integration tests in
//! one binary) take turns instead of crashing inside SDL.

pub mod capture;
pub mod joy;
pub mod preload;

use std::ffi::{c_char, c_int, CString};
use std::ptr::NonNull;
use std::sync::{Mutex, MutexGuard};

/// Emulated time per Z80 T-state at 8 MHz, in picoseconds.
pub const PS_PER_T_STATE: u64 = 125_000;

#[repr(C)]
struct VbShim {
    _private: [u8; 0],
}

extern "C" {
    fn vbshim_create(assets_dir: *const c_char) -> *mut VbShim;
    fn vbshim_destroy(h: *mut VbShim);
    fn vbshim_tick(h: *mut VbShim, ps: u64) -> u64;
    fn vbshim_read(h: *mut VbShim, addr: u16, ps: u64) -> u8;
    fn vbshim_write(h: *mut VbShim, addr: u16, data: u8, ps: u64);
    fn vbshim_frame_count(h: *mut VbShim) -> u64;
    fn vbshim_frame(h: *mut VbShim, width: *mut c_int, height: *mut c_int, pitch: *mut c_int) -> *const u32;
    fn vbshim_vram(h: *mut VbShim) -> *const u8;
    fn vbshim_read_register(h: *mut VbShim, r: u8) -> u8;
    fn vbshim_read_palette(h: *mut VbShim, palette: c_int, byte: u16) -> u8;
    fn vbshim_preload_vram(h: *mut VbShim, addr: u32, data: *const u8, len: u32);
    fn vbshim_preload_palette(h: *mut VbShim, p: c_int, bank: c_int, data: *const u8, len: u32);
}

/// Bytes of VRAM.
pub const VRAM_SIZE: usize = 1 << 20;

/// The window surface: `pixels` holds `height` rows of `pitch / 4` words,
/// of which the first `width` are visible. Each word is `0x00RRGGBB`.
pub struct Frame<'a> {
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub pixels: &'a [u32],
}

static SDL_OWNER: Mutex<()> = Mutex::new(());

pub struct VideoBeast {
    handle: NonNull<VbShim>,
    next_tick_ps: u64,
    // Dropped after `Drop::drop` has shut SDL down.
    _sdl_owner: MutexGuard<'static, ()>,
}

impl VideoBeast {
    /// Creates a VideoBeast at time 0, loading its start-up registers and
    /// palettes from `$BEASTEM_ASSETS`, or from the vendored assets when that
    /// is unset. A missing asset ends the process (the vendored `exit(1)`).
    pub fn new() -> VideoBeast {
        // A panic in another run (a failed test) must not block this one.
        let sdl_owner = SDL_OWNER.lock().unwrap_or_else(|e| e.into_inner());
        let dir = match std::env::var("BEASTEM_ASSETS") {
            Ok(dir) if !dir.is_empty() => dir,
            _ => concat!(env!("CARGO_MANIFEST_DIR"), "/vendor/beastem/assets").to_string(),
        };
        let dir = CString::new(dir).expect("asset path contains a NUL byte");
        let handle = unsafe { vbshim_create(dir.as_ptr()) };
        let handle = NonNull::new(handle).expect("VideoBeast: SDL failed to start");
        VideoBeast { handle, next_tick_ps: 0, _sdl_owner: sdl_owner }
    }

    /// Reads at `offset` (masked to 14 bits) within the 16 KB window.
    pub fn read(&mut self, offset: u16, ps: u64) -> u8 {
        unsafe { vbshim_read(self.handle.as_ptr(), offset & 0x3FFF, ps) }
    }

    /// Writes at `offset` (masked to 14 bits) within the 16 KB window.
    pub fn write(&mut self, offset: u16, data: u8, ps: u64) {
        unsafe { vbshim_write(self.handle.as_ptr(), offset & 0x3FFF, data, ps) }
    }

    /// Runs every VideoBeast deadline up to and including `now_ps`.
    pub fn catch_up(&mut self, now_ps: u64) {
        while self.next_tick_ps <= now_ps {
            self.next_tick_ps = unsafe { vbshim_tick(self.handle.as_ptr(), self.next_tick_ps) };
        }
    }

    /// Frame starts so far that wrapped the line counter to 0 (AD-20).
    /// That is every beastem frame start but the first: `init` leaves the
    /// VideoBeast on its last line, so its first frame start comes one line
    /// after time 0, while the counter still reads 0 from `init`.
    pub fn frame_count(&self) -> u64 {
        unsafe { vbshim_frame_count(self.handle.as_ptr()) }.saturating_sub(1)
    }

    pub fn frame(&self) -> Option<Frame<'_>> {
        let (mut w, mut h, mut pitch): (c_int, c_int, c_int) = (0, 0, 0);
        let ptr = unsafe { vbshim_frame(self.handle.as_ptr(), &mut w, &mut h, &mut pitch) };
        if ptr.is_null() || w <= 0 || h <= 0 || pitch < w * 4 {
            return None;
        }
        let (w, h, pitch) = (w as usize, h as usize, pitch as usize);
        let pixels = unsafe { std::slice::from_raw_parts(ptr, pitch / 4 * h) };
        Some(Frame { width: w, height: h, pitch, pixels })
    }
}

impl VideoBeast {
    /// The 1 MiB of VRAM, by address. Has no effect on the VideoBeast.
    pub fn vram(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(vbshim_vram(self.handle.as_ptr()), VRAM_SIZE) }
    }

    /// Register `r` as stored, without the side effects of a CPU read.
    /// 0xFA/0xFB (line counter) and 0xE4-0xE7 (multiply product) hold their
    /// values as of the last unlocked CPU read of a register 0x80-0xFD, not
    /// live values.
    pub fn register(&self, r: u8) -> u8 {
        unsafe { vbshim_read_register(self.handle.as_ptr(), r) }
    }

    /// Byte `byte` (0-511) of palette 1 or 2: entry `byte >> 1`, low byte
    /// when even, high when odd.
    pub fn palette_byte(&self, palette: u8, byte: u16) -> u8 {
        assert!(palette == 1 || palette == 2, "palette {}", palette);
        unsafe { vbshim_read_palette(self.handle.as_ptr(), palette as c_int, byte) }
    }

    /// Copies `data` into VRAM from `addr`. Panics if it would run past the
    /// top of VRAM.
    pub fn preload_vram(&mut self, addr: u32, data: &[u8]) {
        assert!(addr as usize + data.len() <= VRAM_SIZE, "{} bytes at {:#X} run past VRAM", data.len(), addr);
        unsafe { vbshim_preload_vram(self.handle.as_ptr(), addr, data.as_ptr(), data.len() as u32) }
    }

    /// Loads palette `p` (0 for palette 1, 1 for palette 2) from `bank`
    /// (0-3) upward, 128 bytes per bank, as CPU writes to the palette window
    /// would. Bytes past the end of bank 3 are dropped. Registers are left
    /// as they were.
    pub fn preload_palette(&mut self, p: u8, bank: u8, data: &[u8]) {
        assert!(p <= 1 && bank <= 3, "palette {} bank {}", p, bank);
        let len = data.len().min((4 - bank as usize) * 128);
        unsafe {
            vbshim_preload_palette(self.handle.as_ptr(), p as c_int, bank as c_int, data.as_ptr(), len as u32)
        }
    }
}

impl Drop for VideoBeast {
    fn drop(&mut self) {
        unsafe { vbshim_destroy(self.handle.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_steps_at_each_line_counter_wrap() {
        let mut vb = VideoBeast::new();
        let t = PS_PER_T_STATE;
        // Start-up mode 0x08: frames of 525 lines x 256 T-states. beastem's
        // first frame start, one line in, leaves the counter at 0.
        let frame = 525 * 256;
        vb.catch_up(256 * t);
        assert_eq!(vb.frame_count(), 0);
        vb.catch_up((256 + frame - 1) * t);
        assert_eq!(vb.frame_count(), 0);
        vb.catch_up((256 + frame) * t);
        assert_eq!(vb.frame_count(), 1);
        vb.catch_up((256 + 3 * frame) * t);
        assert_eq!(vb.frame_count(), 3);
    }

    #[test]
    fn frame_is_the_640x480_window_surface() {
        let mut vb = VideoBeast::new();
        // One mode-0 frame: 525 lines of 256 T-states.
        vb.catch_up(525 * 256 * PS_PER_T_STATE);
        let frame = vb.frame().expect("window surface");
        assert_eq!((frame.width, frame.height), (640, 480));
        assert!(frame.pitch >= frame.width * 4);
        assert_eq!(frame.pixels.len(), frame.pitch / 4 * frame.height);
    }
}
