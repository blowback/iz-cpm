//! The `--joy` script: the value port 0x9F reads, frame by frame (AD-20).
//!
//! One line per change: a decimal frame, one or more spaces, then `0x` and
//! two hex digits (either case), the raw port bits. Blank lines are ignored
//! and frames must strictly increase. A value takes effect as its frame
//! starts and holds until the next line; before the first line the port
//! reads 0x00.

use std::fmt;
use std::path::Path;

#[derive(Debug, Default, PartialEq)]
pub struct JoyScript {
    // (frame, value), frames strictly increasing.
    changes: Vec<(u64, u8)>,
}

/// Why a script was rejected. `Display` gives the full stderr line.
#[derive(Debug, PartialEq)]
pub struct JoyError {
    file: String,
    // 1-based; None when the file itself could not be read.
    line: Option<usize>,
    message: String,
}

impl fmt::Display for JoyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "iz-cpm: {}:{}: {}", self.file, line, self.message),
            None => write!(f, "iz-cpm: {}: {}", self.file, self.message),
        }
    }
}

impl JoyScript {
    pub fn load(path: &Path) -> Result<JoyScript, JoyError> {
        let file = path.display().to_string();
        match std::fs::read_to_string(path) {
            Ok(text) => JoyScript::parse(&file, &text),
            Err(err) => Err(JoyError { file, line: None, message: err.to_string() }),
        }
    }

    /// Parses `text`; `file` is only used in error messages.
    pub fn parse(file: &str, text: &str) -> Result<JoyScript, JoyError> {
        let mut changes: Vec<(u64, u8)> = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let error = |message: String| JoyError { file: file.to_string(), line: Some(index + 1), message };
            // trim_end drops the CR of a CRLF file.
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            let (frame, value) = parse_line(line)
                .ok_or_else(|| error(format!("expected \"<frame> 0xNN\", found \"{}\"", line)))?;
            if let Some(&(previous, _)) = changes.last() {
                if frame <= previous {
                    return Err(error(format!("frame {} is not after frame {}", frame, previous)));
                }
            }
            changes.push((frame, value));
        }
        Ok(JoyScript { changes })
    }

    /// The port value in `frame`: that of the last line whose frame has
    /// started, or 0x00 before the first line.
    pub fn value_at(&self, frame: u64) -> u8 {
        let started = self.changes.partition_point(|&(f, _)| f <= frame);
        match started {
            0 => 0x00,
            n => self.changes[n - 1].1,
        }
    }
}

fn parse_line(line: &str) -> Option<(u64, u8)> {
    let (frame, rest) = line.split_once(' ')?;
    let value = rest.trim_start_matches(' ').strip_prefix("0x")?;
    if frame.is_empty() || !frame.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if value.len() != 2 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some((frame.parse().ok()?, u8::from_str_radix(value, 16).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<JoyScript, JoyError> {
        JoyScript::parse("joy.txt", text)
    }

    fn error_line(text: &str) -> String {
        parse(text).unwrap_err().to_string()
    }

    #[test]
    fn empty_script_reads_0() {
        let script = parse("").unwrap();
        assert_eq!(script, JoyScript::default());
        assert_eq!(script.value_at(0), 0x00);
        assert_eq!(script.value_at(1000), 0x00);
    }

    #[test]
    fn value_holds_from_its_frame_until_the_next_line() {
        let script = parse("2 0x05\n4 0x81\n7 0x00\n").unwrap();
        let values: Vec<u8> = (0..9).map(|f| script.value_at(f)).collect();
        assert_eq!(values, [0x00, 0x00, 0x05, 0x05, 0x81, 0x81, 0x81, 0x00, 0x00]);
    }

    #[test]
    fn frame_0_line_applies_from_the_start() {
        assert_eq!(parse("0 0x10").unwrap().value_at(0), 0x10);
    }

    #[test]
    fn hex_digits_in_either_case_and_several_spaces() {
        let script = parse("1   0xaB\n2 0xFf").unwrap();
        assert_eq!(script.value_at(1), 0xAB);
        assert_eq!(script.value_at(2), 0xFF);
    }

    #[test]
    fn blank_lines_and_crlf_are_accepted() {
        let script = parse("\r\n1 0x01\r\n\n   \n3 0x03\r\n").unwrap();
        assert_eq!(script.value_at(2), 0x01);
        assert_eq!(script.value_at(3), 0x03);
    }

    #[test]
    fn frame_not_after_the_previous_one_is_rejected() {
        assert_eq!(error_line("2 0x05\n1 0x81"), "iz-cpm: joy.txt:2: frame 1 is not after frame 2");
        assert_eq!(error_line("2 0x05\n\n2 0x81"), "iz-cpm: joy.txt:3: frame 2 is not after frame 2");
    }

    #[test]
    fn malformed_lines_are_rejected_with_their_line_number() {
        for bad in [
            "1",            // no value
            "1 5",          // no 0x
            "1 0x5",        // one digit
            "1 0x123",      // three digits
            "1 0xG0",       // not hex
            "1 0X05",       // prefix is 0x
            "x 0x05",       // frame not decimal
            "-1 0x05",      // frame not decimal
            " 1 0x05",      // leading space
            "1\t0x05",      // tab, not space
            "1 0x05 junk",  // trailing text
            "99999999999999999999 0x05", // frame out of range
        ] {
            let message = error_line(&format!("0 0x00\n{}\n", bad));
            assert!(message.starts_with("iz-cpm: joy.txt:2: "), "{:?} -> {}", bad, message);
        }
    }

    #[test]
    fn missing_file_names_the_file() {
        let err = JoyScript::load(Path::new("no/such/joy.txt")).unwrap_err().to_string();
        assert!(err.starts_with("iz-cpm: no/such/joy.txt: "), "{}", err);
    }
}
