//! Dumps and PNGs at chosen frames (`--dump-at`, `--png-at`, `--out`;
//! architecture AD-20). File names and formats are listed in the README
//! under "Scripted runs".

use std::fs;
use std::path::{Path, PathBuf};

use super::{Frame, VideoBeast};

/// The frames named by `--dump-at` or `--png-at`, sorted, without repeats.
#[derive(Debug, Default, PartialEq)]
pub struct FrameList(Vec<u64>);

impl FrameList {
    /// Parses a LIST: comma-separated decimal frame numbers, no spaces.
    /// Repeats are allowed; empty items are errors. The error names `flag`
    /// and the whole value.
    pub fn parse(flag: &str, text: &str) -> Result<FrameList, String> {
        let mut frames = Vec::new();
        for (i, item) in text.split(',').enumerate() {
            let bad = |why: String| format!("iz-cpm: invalid {} value \"{}\": {}", flag, text, why);
            if item.is_empty() {
                return Err(bad(format!("item {} is empty", i + 1)));
            }
            if !item.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad(format!("\"{}\" is not a decimal frame number", item)));
            }
            let frame = item
                .parse::<u64>()
                .map_err(|_| bad(format!("frame {} is too large", item)))?;
            frames.push(frame);
        }
        frames.sort_unstable();
        frames.dedup();
        Ok(FrameList(frames))
    }

    pub fn contains(&self, frame: u64) -> bool {
        self.0.binary_search(&frame).is_ok()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn frames(&self) -> &[u64] {
        &self.0
    }
}

/// `dir/fNNNNN<suffix>`, the frame in decimal with at least 5 digits.
pub fn file_path(dir: &Path, frame: u64, suffix: &str) -> PathBuf {
    dir.join(format!("f{:05}{}", frame, suffix))
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|err| format!("iz-cpm: cannot write {}: {}", path.display(), err))
}

/// Writes frame `frame`'s dumps to `dir`: VRAM, registers, both palettes,
/// the CPU's 64K view (`z80`) and the RAM banks (`ram`).
pub fn write_dumps(dir: &Path, frame: u64, vb: &VideoBeast, z80: &[u8], ram: &[u8]) -> Result<(), String> {
    write_file(&file_path(dir, frame, "-vram.bin"), vb.vram())?;
    let regs: Vec<u8> = (0..=255u8).map(|r| vb.register(r)).collect();
    write_file(&file_path(dir, frame, "-regs.bin"), &regs)?;
    for palette in [1u8, 2] {
        let bytes: Vec<u8> = (0..512u16).map(|b| vb.palette_byte(palette, b)).collect();
        write_file(&file_path(dir, frame, &format!("-pal{}.bin", palette)), &bytes)?;
    }
    write_file(&file_path(dir, frame, "-z80.bin"), z80)?;
    write_file(&file_path(dir, frame, "-ram.bin"), ram)?;
    Ok(())
}

/// The visible part of `frame` as a PNG: 8-bit RGB, no chunks but IHDR,
/// IDAT and IEND.
pub fn encode_png(frame: &Frame) -> Result<Vec<u8>, String> {
    let stride = frame.pitch / 4;
    let mut rgb = Vec::with_capacity(frame.width * frame.height * 3);
    for y in 0..frame.height {
        for &px in &frame.pixels[y * stride..y * stride + frame.width] {
            rgb.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, px as u8]);
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, frame.width as u32, frame.height as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// Writes `dir/fNNNNN.png` from the VideoBeast's window surface.
pub fn write_png(dir: &Path, frame: u64, vb: &VideoBeast) -> Result<(), String> {
    let path = file_path(dir, frame, ".png");
    let surface = vb
        .frame()
        .ok_or_else(|| format!("iz-cpm: cannot write {}: no RGB888 window surface", path.display()))?;
    let bytes = encode_png(&surface).map_err(|err| format!("iz-cpm: cannot write {}: {}", path.display(), err))?;
    write_file(&path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_is_sorted_without_repeats() {
        let list = FrameList::parse("--dump-at", "5,0,2,5,00007").unwrap();
        assert_eq!(list.frames(), &[0, 2, 5, 7]);
        assert!(list.contains(5));
        assert!(!list.contains(3));
    }

    #[test]
    fn list_of_one() {
        assert_eq!(FrameList::parse("--png-at", "0").unwrap().frames(), &[0]);
    }

    #[test]
    fn empty_items_are_errors() {
        for text in ["", ",", "1,", ",1", "1,,2"] {
            let err = FrameList::parse("--dump-at", text).unwrap_err();
            assert!(err.starts_with(&format!("iz-cpm: invalid --dump-at value \"{}\": item ", text)), "{}", err);
            assert!(err.ends_with("is empty"), "{}", err);
        }
    }

    #[test]
    fn non_decimal_items_are_errors() {
        for (text, item) in [("1,,x", ""), ("x", "x"), ("1, 2", " 2"), ("+1", "+1"), ("-1", "-1"), ("0x10", "0x10")] {
            let err = FrameList::parse("--png-at", text).unwrap_err();
            assert!(err.starts_with(&format!("iz-cpm: invalid --png-at value \"{}\": ", text)), "{}", err);
            if !item.is_empty() {
                assert!(err.contains(&format!("\"{}\" is not a decimal frame number", item)), "{}", err);
            }
        }
    }

    #[test]
    fn too_large_frame_is_an_error() {
        let err = FrameList::parse("--png-at", "99999999999999999999").unwrap_err();
        assert!(err.contains("too large"), "{}", err);
    }

    #[test]
    fn file_names_have_at_least_five_digits() {
        let dir = Path::new("d");
        assert_eq!(file_path(dir, 0, "-vram.bin"), Path::new("d/f00000-vram.bin"));
        assert_eq!(file_path(dir, 42, ".png"), Path::new("d/f00042.png"));
        assert_eq!(file_path(dir, 1234567, ".png"), Path::new("d/f1234567.png"));
    }

    #[test]
    fn png_of_a_padded_surface_decodes_to_its_visible_rgb() {
        // 3 x 2 pixels, pitch of 5 words; the padding words must not appear.
        let pixels: Vec<u32> = vec![
            0x00FF0000, 0x0000FF00, 0x000000FF, 0xDEADBEEF, 0xDEADBEEF,
            0x00123456, 0xFF000000, 0x00FFFFFF, 0xDEADBEEF, 0xDEADBEEF,
        ];
        let frame = Frame { width: 3, height: 2, pitch: 20, pixels: &pixels };
        let bytes = encode_png(&frame).unwrap();

        let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (3, 2));
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        assert_eq!(
            &buf[..info.buffer_size()],
            &[
                0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF,
                0x12, 0x34, 0x56, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF,
            ]
        );
    }

    #[test]
    fn png_has_only_ihdr_idat_and_iend() {
        let pixels = vec![0u32; 4];
        let frame = Frame { width: 2, height: 2, pitch: 8, pixels: &pixels };
        let bytes = encode_png(&frame).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let mut pos = 8;
        let mut types = Vec::new();
        while pos < bytes.len() {
            let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            types.push(String::from_utf8_lossy(&bytes[pos + 4..pos + 8]).into_owned());
            pos += 12 + len;
        }
        assert_eq!(types.first().map(String::as_str), Some("IHDR"));
        assert_eq!(types.last().map(String::as_str), Some("IEND"));
        assert!(types[1..types.len() - 1].iter().all(|t| t == "IDAT"), "{:?}", types);
    }
}
