//! Safe wrapper over the C++ shim (`shim/vbshim.h`) around beastem's
//! VideoBeast, vendored unmodified in `vendor/beastem/`.
//!
//! Only one VideoBeast may exist per process at a time: SDL video is
//! process-global and not thread-safe. `VideoBeast::new` blocks until the
//! previous one is dropped, so parallel `run()` calls (integration tests in
//! one binary) take turns instead of crashing inside SDL.

pub mod joy;

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
}

/// The window surface: `pixels` holds `height` rows of `pitch / 4` words,
/// of which the first `width` are visible.
#[allow(dead_code)] // read by the dump and PNG entries
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

    #[allow(dead_code)] // read by the dump and PNG entries
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
