//! `--vb-preload LOADTXT`: VRAM and palettes as `SLIDE RV` leaves them.
//!
//! `load.txt` lists one path per line, relative to its own directory. Blank
//! lines are skipped and trailing whitespace (including `\r`) trimmed. The
//! file name of each path picks what happens, by SLIDE's rules (either
//! case):
//!
//! - `vXXXXX.vbm`, five hex digits: the file's bytes go to VRAM from
//!   address `0xXXXXX`. Running past the top of VRAM is an error.
//! - `vPB.vbp`, P in 0-1 and B in 0-3: the file loads palette P (0 is
//!   palette 1) from bank B upward, 128 bytes per bank, stopping after
//!   bank 3; the rest of the file is dropped.
//! - anything else is ignored, with a line on stderr.
//!
//! Files apply in listed order, so later bytes win. Bytes are written as
//! they are: no `0x1A` stripping, no padding.

use std::path::Path;

use super::{VideoBeast, VRAM_SIZE};

/// What a SLIDE file name says to do with the file.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Target {
    /// VRAM from this address.
    Vram(u32),
    /// Palette `p` (0 or 1) from bank `bank` (0-3).
    Palette { p: u8, bank: u8 },
}

/// Classifies a file name (no directory) by SLIDE's rules, or None if it is
/// neither a VRAM nor a palette name.
pub fn classify(name: &str) -> Option<Target> {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_prefix('v')?;
    if let Some(hex) = stem.strip_suffix(".vbm") {
        if hex.len() == 5 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Some(Target::Vram(u32::from_str_radix(hex, 16).ok()?));
        }
        return None;
    }
    if let Some(digits) = stem.strip_suffix(".vbp") {
        let d = digits.as_bytes();
        if d.len() == 2 && (b'0'..=b'1').contains(&d[0]) && (b'0'..=b'3').contains(&d[1]) {
            return Some(Target::Palette { p: d[0] - b'0', bank: d[1] - b'0' });
        }
    }
    None
}

#[derive(Debug, PartialEq)]
struct Item {
    target: Target,
    data: Vec<u8>,
}

/// A loaded `load.txt`: every listed SLIDE file, read and checked.
#[derive(Debug, Default, PartialEq)]
pub struct Preload {
    items: Vec<Item>,
}

impl Preload {
    /// Reads `path` and every file it lists. Ignored names are reported on
    /// stderr. Errors are full stderr lines naming `path`, the line and the
    /// listed file.
    pub fn load(path: &Path) -> Result<Preload, String> {
        let file = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|err| format!("iz-cpm: {}: {}", file, err))?;
        let base = path.parent().unwrap_or(Path::new(""));
        let mut items = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            let at = format!("iz-cpm: {}:{}: {}", file, index + 1, line);
            let listed = Path::new(line);
            let target = match listed.file_name().and_then(|n| n.to_str()).and_then(classify) {
                Some(target) => target,
                None => {
                    eprintln!("iz-cpm: {}:{}: ignoring {} (not a SLIDE VRAM or palette name)", file, index + 1, line);
                    continue;
                }
            };
            let data = std::fs::read(base.join(listed)).map_err(|err| format!("{}: {}", at, err))?;
            if let Target::Vram(addr) = target {
                if addr as usize + data.len() > VRAM_SIZE {
                    return Err(format!(
                        "{}: {} bytes at 0x{:05X} run past the top of VRAM",
                        at,
                        data.len(),
                        addr
                    ));
                }
            }
            items.push(Item { target, data });
        }
        Ok(Preload { items })
    }

    /// Writes every file to `vb`, in listed order. Registers are unchanged.
    pub fn apply(&self, vb: &mut VideoBeast) {
        for item in &self.items {
            match item.target {
                Target::Vram(addr) => vb.preload_vram(addr, &item.data),
                Target::Palette { p, bank } => vb.preload_palette(p, bank, &item.data),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// An empty directory under the system temp dir, unique to `name`.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("iz-cpm-preload-{}-{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, data: &[u8]) {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }

    fn targets(preload: &Preload) -> Vec<(Target, usize)> {
        preload.items.iter().map(|i| (i.target, i.data.len())).collect()
    }

    #[test]
    fn vram_names() {
        assert_eq!(classify("v20000.vbm"), Some(Target::Vram(0x20000)));
        assert_eq!(classify("V0aBcD.VBM"), Some(Target::Vram(0x0ABCD)));
        assert_eq!(classify("vFFFFF.vbm"), Some(Target::Vram(0xFFFFF)));
        assert_eq!(classify("v00000.Vbm"), Some(Target::Vram(0)));
        for bad in [
            "v1234.vbm",    // 4 digits
            "v123456.vbm",  // 6 digits
            "v1234G.vbm",   // not hex
            "v+1234.vbm",   // sign
            "x12345.vbm",   // no v
            "v12345.vbm.bak", // trailing text
            "v12345.vbmx",  // trailing text
            "v12345vbm",    // no dot
            "v12345.vbp",   // wrong extension for 5 digits
            "GAME.COM",
        ] {
            assert_eq!(classify(bad), None, "{}", bad);
        }
    }

    #[test]
    fn palette_names() {
        assert_eq!(classify("v00.vbp"), Some(Target::Palette { p: 0, bank: 0 }));
        assert_eq!(classify("V13.VBP"), Some(Target::Palette { p: 1, bank: 3 }));
        assert_eq!(classify("v12.Vbp"), Some(Target::Palette { p: 1, bank: 2 }));
        for bad in [
            "v24.vbp",  // P 2
            "v04.vbp",  // B 4
            "v0.vbp",   // one digit
            "v001.vbp", // three digits
            "v0a.vbp",  // not a digit
            "v00.vbp ", // trailing text
            "v00.vbpx", // trailing text
            "v00.vbm",  // wrong extension for 2 digits
        ] {
            assert_eq!(classify(bad), None, "{}", bad);
        }
    }

    #[test]
    fn vram_file_may_end_at_the_top_but_not_past_it() {
        let dir = scratch("top");
        write(&dir, "vFFF00.vbm", &[1; 256]);
        write(&dir, "vFFF01.vbm", &[2; 256]);
        write(&dir, "load.txt", b"vFFF00.vbm\n");
        let preload = Preload::load(&dir.join("load.txt")).unwrap();
        assert_eq!(targets(&preload), [(Target::Vram(0xFFF00), 256)]);

        write(&dir, "load.txt", b"vFFF00.vbm\n\nvFFF01.vbm\n");
        let err = Preload::load(&dir.join("load.txt")).unwrap_err();
        let file = dir.join("load.txt").display().to_string();
        assert_eq!(err, format!("iz-cpm: {}:3: vFFF01.vbm: 256 bytes at 0xFFF01 run past the top of VRAM", file));
    }

    #[test]
    fn blank_lines_crlf_and_trailing_spaces() {
        let dir = scratch("crlf");
        write(&dir, "v00100.vbm", &[7; 3]);
        write(&dir, "v01.vbp", &[8; 5]);
        write(&dir, "load.txt", b"\r\n  \r\nv00100.vbm  \r\n\nv01.vbp\r\n");
        let preload = Preload::load(&dir.join("load.txt")).unwrap();
        assert_eq!(
            targets(&preload),
            [(Target::Vram(0x100), 3), (Target::Palette { p: 0, bank: 1 }, 5)]
        );
    }

    #[test]
    fn paths_are_relative_to_load_txt_and_ignored_names_are_not_read() {
        let dir = scratch("relative");
        write(&dir, "set/sub/V0aBcD.VBM", &[1, 2, 3]);
        write(&dir, "set/load.txt", b"sub/V0aBcD.VBM\nmissing/GAME.COM\nv1234.vbm\nv24.vbp\n");
        let preload = Preload::load(&dir.join("set").join("load.txt")).unwrap();
        assert_eq!(preload.items, [Item { target: Target::Vram(0x0ABCD), data: vec![1, 2, 3] }]);
    }

    #[test]
    fn missing_listed_file_names_load_txt_line_and_path() {
        let dir = scratch("missing");
        write(&dir, "load.txt", b"\nsub/v00000.vbm\n");
        let err = Preload::load(&dir.join("load.txt")).unwrap_err();
        let prefix = format!("iz-cpm: {}:2: sub/v00000.vbm: ", dir.join("load.txt").display());
        assert!(err.starts_with(&prefix), "{}", err);
    }

    #[test]
    fn missing_load_txt_names_it() {
        let err = Preload::load(Path::new("no/such/load.txt")).unwrap_err();
        assert!(err.starts_with("iz-cpm: no/such/load.txt: "), "{}", err);
    }

    #[test]
    fn palette_overflow_keeps_the_banks_up_to_3() {
        let mut vb = VideoBeast::new();
        let before: Vec<u8> = (0..512).map(|i| vb.palette_byte(2, i)).collect();
        let regs: Vec<u8> = (0..=255).map(|r| vb.register(r)).collect();
        let data: Vec<u8> = (0..256).map(|i| (i as u8) ^ 0x5A).collect();
        // B = 3 keeps (4 - 3) * 128 bytes.
        vb.preload_palette(1, 3, &data);
        let after: Vec<u8> = (0..512).map(|i| vb.palette_byte(2, i)).collect();
        assert_eq!(&after[..384], &before[..384]);
        assert_eq!(&after[384..], &data[..128]);
        assert_eq!((0..=255).map(|r| vb.register(r)).collect::<Vec<u8>>(), regs);

        // B = 1 keeps 384 bytes of a 512-byte file, into palette 1.
        let data: Vec<u8> = (0..512).map(|i| (i % 251) as u8).collect();
        let before: Vec<u8> = (0..128).map(|i| vb.palette_byte(1, i)).collect();
        vb.preload_palette(0, 1, &data);
        let after: Vec<u8> = (0..512).map(|i| vb.palette_byte(1, i)).collect();
        assert_eq!(&after[..128], &before[..]);
        assert_eq!(&after[128..], &data[..384]);
    }

    #[test]
    fn vram_copy_lands_at_its_address_up_to_the_top() {
        let mut vb = VideoBeast::new();
        vb.preload_vram(0xFFF00, &[0xAB; 256]);
        vb.preload_vram(0x20000, &[1, 2, 3]);
        let vram = vb.vram();
        assert!(vram[0xFFF00..].iter().all(|&b| b == 0xAB));
        assert_eq!(vram[0xFFEFF], 0);
        assert_eq!(&vram[0x1FFFF..0x20004], &[0, 1, 2, 3, 0]);
    }
}
