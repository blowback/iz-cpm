//! `--window`: the keyboard as the port 0x9F joystick, and pacing emulated
//! time to wall-clock time.

use std::thread;
use std::time::{Duration, Instant};

/// SDL scancodes (physical key positions) and the port 0x9F bit each sets
/// while held, 1 = pressed: arrows for the directions, Z for JUMP, X for
/// FIRE, C for DASH.
pub const KEYS: [(i32, u8); 7] = [
    (79, 0x01), // Right
    (80, 0x02), // Left
    (81, 0x04), // Down
    (82, 0x08), // Up
    (29, 0x10), // Z: JUMP
    (27, 0x20), // X: FIRE
    (6, 0x80),  // C: DASH
];

/// The port 0x9F value for the keys `down` reports held.
pub fn joy_bits(down: impl Fn(i32) -> bool) -> u8 {
    KEYS.iter().filter(|&&(scancode, _)| down(scancode)).fold(0, |bits, &(_, bit)| bits | bit)
}

/// How far wall time may run ahead of emulated time, about one frame,
/// before the pacer gives up on that time instead of letting the emulator
/// run unthrottled to catch up.
const MAX_LAG: Duration = Duration::from_millis(20);

/// Holds emulated time to wall-clock time: emulated time 0 is when the
/// pacer was made, moved later by any stall of more than MAX_LAG.
pub struct Pacer {
    start: Instant,
}

impl Pacer {
    pub fn new() -> Pacer {
        Pacer { start: Instant::now() }
    }

    /// How long to sleep at wall time `now` so that wall time catches up
    /// with `emulated_ps`; None when it already has. If wall time is more
    /// than MAX_LAG ahead (the run stalled: console input, a slow capture, a
    /// host hiccup), moves `start` on so that it is level again, so the run
    /// continues at real speed rather than in a burst.
    fn delay(&mut self, emulated_ps: u64, now: Instant) -> Option<Duration> {
        let due = self.start + Duration::from_nanos(emulated_ps / 1000);
        let lag = now.saturating_duration_since(due);
        if lag > MAX_LAG {
            self.start += lag;
        }
        due.checked_duration_since(now).filter(|d| !d.is_zero())
    }

    /// Sleeps while wall time is behind `emulated_ps`. A host that cannot
    /// keep up never sleeps and runs slower than real time.
    pub fn wait_until(&mut self, emulated_ps: u64) {
        if let Some(delay) = self.delay(emulated_ps, Instant::now()) {
            thread::sleep(delay);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(keys: &[i32]) -> impl Fn(i32) -> bool + '_ {
        move |scancode| keys.contains(&scancode)
    }

    #[test]
    fn each_key_alone_gives_its_bit() {
        assert_eq!(joy_bits(held(&[79])), 0x01);
        assert_eq!(joy_bits(held(&[80])), 0x02);
        assert_eq!(joy_bits(held(&[81])), 0x04);
        assert_eq!(joy_bits(held(&[82])), 0x08);
        assert_eq!(joy_bits(held(&[29])), 0x10);
        assert_eq!(joy_bits(held(&[27])), 0x20);
        assert_eq!(joy_bits(held(&[6])), 0x80);
    }

    #[test]
    fn all_seven_keys_give_0xbf() {
        assert_eq!(joy_bits(held(&[79, 80, 81, 82, 29, 27, 6])), 0xBF);
    }

    #[test]
    fn no_keys_give_0() {
        assert_eq!(joy_bits(held(&[])), 0x00);
    }

    #[test]
    fn other_scancodes_are_ignored() {
        // A (4), space (44), Return (40), Escape (41).
        assert_eq!(joy_bits(held(&[4, 44, 40, 41])), 0x00);
        assert_eq!(joy_bits(|_| true), 0xBF);
    }

    #[test]
    fn pacer_does_not_sleep_when_wall_time_is_ahead() {
        let mut pacer = Pacer::new();
        let later = pacer.start + Duration::from_millis(100);
        // 90 ms of emulated time, 100 ms of wall time.
        assert_eq!(pacer.delay(90_000_000_000, later), None);
        assert_eq!(pacer.delay(100_000_000_000, later), None);
    }

    #[test]
    fn pacer_rebases_after_a_stall() {
        let mut pacer = Pacer::new();
        let start = pacer.start;
        // A 1 s stall at 50 ms of emulated time: no sleep, start moves on.
        let now = start + Duration::from_millis(1050);
        assert_eq!(pacer.delay(50_000_000_000, now), None);
        assert_eq!(pacer.start, start + Duration::from_millis(1000));
        // The next frame, 16 ms of emulated time later, waits about a frame.
        assert_eq!(pacer.delay(66_000_000_000, now), Some(Duration::from_millis(16)));
    }

    #[test]
    fn pacer_keeps_its_start_within_one_frame_of_lag() {
        let mut pacer = Pacer::new();
        let start = pacer.start;
        assert_eq!(pacer.delay(50_000_000_000, start + Duration::from_millis(65)), None);
        assert_eq!(pacer.start, start);
    }

    #[test]
    fn pacer_sleeps_while_wall_time_is_behind() {
        let mut pacer = Pacer::new();
        let later = pacer.start + Duration::from_millis(10);
        assert_eq!(pacer.delay(30_000_000_000, later), Some(Duration::from_millis(20)));
    }
}
