use std::fs;
use std::path::{Path, PathBuf};

use iz80::Machine;

const BANK_SIZE: usize = 0x4000;            // 16 KiB
const FLASH_BANKS: usize = 32;              // virtual banks 0..32
const RAM_BANKS: usize = 32;                // virtual banks 32..64
const FLASH_SIZE: usize = FLASH_BANKS * BANK_SIZE;
const RAM_SIZE: usize = RAM_BANKS * BANK_SIZE;
const BANK_MASK: u8 = 0x3F;                 // only the bottom 6 bits of a bank number are used

const PORT_BANK0: u8 = 0x70;
const PORT_BANK3: u8 = 0x73;
const PORT_MAP_ENABLE: u8 = 0x74;
const PORT_DUMP: u8 = 0x80;
const DUMP_FLASH_ALL: u8 = 0x81;
const DUMP_RAM_ALL: u8 = 0x82;
const DUMP_ALL: u8 = 0x83;

pub struct CpmMachine {
    flash: Vec<u8>,
    ram: Vec<u8>,
    bank_map: [u8; 4],
    mapping_enabled: bool,
    dump_basename: Option<PathBuf>,
    in_values: [u8; 256],
    in_port: Option<u8>,
    out_port: Option<u8>,
    out_value: u8,
}

impl CpmMachine {
    pub fn new() -> CpmMachine {
        CpmMachine {
            flash: vec![0xFF; FLASH_SIZE],
            ram: vec![0; RAM_SIZE],
            // Reset state for iz-cpm: mapping enabled with the four physical
            // banks pointing at the first 64 KiB of RAM. This deviates from the
            // hardware reset (which leaves mapping disabled and points at
            // Flash) so that CP/M's low-memory writes work out of the box.
            bank_map: [32, 33, 34, 35],
            mapping_enabled: true,
            dump_basename: None,
            in_values: [0; 256],
            in_port: None,
            out_port: None,
            out_value: 0,
        }
    }

    pub fn set_dump_basename(&mut self, path: PathBuf) {
        self.dump_basename = Some(path);
    }

    fn handle_dump(&self, value: u8) {
        let basename = match &self.dump_basename {
            Some(p) => p,
            None => return,
        };
        let bytes: Vec<u8> = match value {
            0x00..=0x1F => {
                let bank = value as usize;
                self.flash[bank * BANK_SIZE..(bank + 1) * BANK_SIZE].to_vec()
            }
            0x20..=0x3F => {
                let bank = (value - FLASH_BANKS as u8) as usize;
                self.ram[bank * BANK_SIZE..(bank + 1) * BANK_SIZE].to_vec()
            }
            DUMP_FLASH_ALL => self.flash.clone(),
            DUMP_RAM_ALL => self.ram.clone(),
            DUMP_ALL => {
                let mut v = Vec::with_capacity(FLASH_SIZE + RAM_SIZE);
                v.extend_from_slice(&self.flash);
                v.extend_from_slice(&self.ram);
                v
            }
            _ => return,
        };
        match next_available_path(basename) {
            Some(path) => match fs::write(&path, &bytes) {
                Ok(()) => eprintln!("[[Wrote {} bytes to {}]]", bytes.len(), path.display()),
                Err(err) => eprintln!("[[Failed to write dump to {}: {}]]", path.display(), err),
            },
            None => eprintln!(
                "[[Failed to derive an unused dump filename from {}]]",
                basename.display()
            ),
        }
    }

    pub fn load_flash(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() > FLASH_SIZE {
            return Err(format!(
                "Flash image is {} bytes, maximum is {} bytes",
                data.len(),
                FLASH_SIZE
            ));
        }
        self.flash[..data.len()].copy_from_slice(data);
        Ok(())
    }

    fn virtual_bank(&self, address: u16) -> u8 {
        let phys = (address >> 14) as usize;
        if self.mapping_enabled {
            self.bank_map[phys] & BANK_MASK
        } else {
            phys as u8
        }
    }
}

impl Machine for CpmMachine {
    fn peek(&self, address: u16) -> u8 {
        let virt = self.virtual_bank(address) as usize;
        let offset = (address as usize) & (BANK_SIZE - 1);
        if virt < FLASH_BANKS {
            self.flash[virt * BANK_SIZE + offset]
        } else {
            self.ram[(virt - FLASH_BANKS) * BANK_SIZE + offset]
        }
    }

    fn poke(&mut self, address: u16, value: u8) {
        let virt = self.virtual_bank(address) as usize;
        if virt < FLASH_BANKS {
            // Flash ROM: writes are silently ignored.
            return;
        }
        let offset = (address as usize) & (BANK_SIZE - 1);
        self.ram[(virt - FLASH_BANKS) * BANK_SIZE + offset] = value;
    }

    fn port_in(&mut self, address: u16) -> u8 {
        let port = address as u8;
        match port {
            PORT_BANK0..=PORT_BANK3 => self.bank_map[(port - PORT_BANK0) as usize],
            PORT_MAP_ENABLE => self.mapping_enabled as u8,
            _ => {
                let value = self.in_values[port as usize];
                self.in_port = Some(port);
                value
            }
        }
    }

    fn port_out(&mut self, address: u16, value: u8) {
        let port = address as u8;
        match port {
            PORT_BANK0..=PORT_BANK3 => {
                self.bank_map[(port - PORT_BANK0) as usize] = value & BANK_MASK;
            }
            PORT_MAP_ENABLE => {
                self.mapping_enabled = (value & 1) != 0;
            }
            PORT_DUMP => self.handle_dump(value),
            _ => {
                self.out_port = Some(port);
                self.out_value = value;
            }
        }
    }
}

fn next_available_path(basename: &Path) -> Option<PathBuf> {
    if !basename.exists() {
        return Some(basename.to_path_buf());
    }
    let parent = basename.parent().unwrap_or_else(|| Path::new(""));
    let stem = basename.file_stem().and_then(|s| s.to_str()).unwrap_or("dump");
    let ext = basename.extension().and_then(|s| s.to_str());
    for n in 1..10_000 {
        let name = match ext {
            Some(e) => format!("{}.{:03}.{}", stem, n, e),
            None => format!("{}.{:03}", stem, n),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mapping_targets_first_64k_of_ram() {
        let mut m = CpmMachine::new();
        // Each physical bank should round-trip a distinct byte in RAM.
        m.poke(0x0000, 0xA0);
        m.poke(0x4000, 0xA1);
        m.poke(0x8000, 0xA2);
        m.poke(0xC000, 0xA3);
        assert_eq!(m.peek(0x0000), 0xA0);
        assert_eq!(m.peek(0x4000), 0xA1);
        assert_eq!(m.peek(0x8000), 0xA2);
        assert_eq!(m.peek(0xC000), 0xA3);
        // And those bytes should live at the expected RAM offsets
        // (RAM banks 32..35 → ram offsets 0, 0x4000, 0x8000, 0xC000).
        assert_eq!(m.ram[0x0000], 0xA0);
        assert_eq!(m.ram[0x4000], 0xA1);
        assert_eq!(m.ram[0x8000], 0xA2);
        assert_eq!(m.ram[0xC000], 0xA3);
    }

    #[test]
    fn disable_mapping_exposes_flash_banks_0_to_3() {
        let mut m = CpmMachine::new();
        // Stash a marker into Flash bank 1 (the region that physical bank 1
        // will see when mapping is disabled).
        m.flash[BANK_SIZE + 0x123] = 0x5A;

        m.port_out(PORT_MAP_ENABLE as u16, 0);
        assert_eq!(m.peek(0x4000 + 0x123), 0x5A);

        // Writes are dropped because Flash is read-only.
        m.poke(0x4000 + 0x123, 0xFF);
        assert_eq!(m.peek(0x4000 + 0x123), 0x5A);
    }

    #[test]
    fn remapping_a_physical_bank_points_at_a_different_ram_bank() {
        let mut m = CpmMachine::new();
        // Map physical bank 1 (0x4000-0x7FFF) at RAM virtual bank 40.
        m.port_out(0x71, 40);
        m.poke(0x4000, 0xC7);

        // The byte should land at offset (40-32)*16K within RAM.
        let expected_offset = (40 - FLASH_BANKS) * BANK_SIZE;
        assert_eq!(m.ram[expected_offset], 0xC7);

        // Reading back through the same physical bank returns the value.
        assert_eq!(m.peek(0x4000), 0xC7);

        // The default mapping at 0x0000 (RAM bank 32) is untouched.
        assert_eq!(m.peek(0x0000), 0x00);
    }

    #[test]
    fn bank_register_is_masked_to_six_bits() {
        let mut m = CpmMachine::new();
        m.port_out(0x70, 0xFF);
        assert_eq!(m.port_in(0x70), 0x3F);
    }

    #[test]
    fn writes_to_flash_through_mapping_are_ignored() {
        let mut m = CpmMachine::new();
        // Map physical bank 0 at Flash virtual bank 0.
        m.port_out(0x70, 0);
        // Default Flash content is 0xFF.
        assert_eq!(m.peek(0x0000), 0xFF);
        m.poke(0x0000, 0x42);
        assert_eq!(m.peek(0x0000), 0xFF);
    }

    #[test]
    fn mapping_enable_register_round_trips() {
        let mut m = CpmMachine::new();
        assert_eq!(m.port_in(PORT_MAP_ENABLE as u16), 1);
        m.port_out(PORT_MAP_ENABLE as u16, 0);
        assert_eq!(m.port_in(PORT_MAP_ENABLE as u16), 0);
        m.port_out(PORT_MAP_ENABLE as u16, 1);
        assert_eq!(m.port_in(PORT_MAP_ENABLE as u16), 1);
    }

    #[test]
    fn load_flash_makes_image_visible_through_mapping() {
        let mut m = CpmMachine::new();
        let mut image = vec![0u8; BANK_SIZE + 16];
        image[0] = 0xDE;
        image[1] = 0xAD;
        image[BANK_SIZE] = 0xBE;
        image[BANK_SIZE + 1] = 0xEF;
        m.load_flash(&image).unwrap();

        // Map physical bank 0 to Flash bank 0.
        m.port_out(0x70, 0);
        assert_eq!(m.peek(0x0000), 0xDE);
        assert_eq!(m.peek(0x0001), 0xAD);

        // Map physical bank 0 to Flash bank 1.
        m.port_out(0x70, 1);
        assert_eq!(m.peek(0x0000), 0xBE);
        assert_eq!(m.peek(0x0001), 0xEF);
    }

    #[test]
    fn load_flash_rejects_oversized_images() {
        let mut m = CpmMachine::new();
        let oversized = vec![0u8; FLASH_SIZE + 1];
        assert!(m.load_flash(&oversized).is_err());
    }

    fn temp_basename(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "izcpm-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("dump.bin")
    }

    #[test]
    fn dump_single_flash_page_writes_one_bank() {
        let basename = temp_basename("flash-page");
        let mut m = CpmMachine::new();
        m.flash[5 * BANK_SIZE] = 0x11;
        m.flash[5 * BANK_SIZE + 1] = 0x22;
        m.set_dump_basename(basename.clone());

        m.port_out(PORT_DUMP as u16, 0x05);

        let bytes = std::fs::read(&basename).unwrap();
        assert_eq!(bytes.len(), BANK_SIZE);
        assert_eq!(bytes[0], 0x11);
        assert_eq!(bytes[1], 0x22);
        std::fs::remove_dir_all(basename.parent().unwrap()).unwrap();
    }

    #[test]
    fn dump_single_ram_page_writes_one_bank() {
        let basename = temp_basename("ram-page");
        let mut m = CpmMachine::new();
        // Write into RAM bank 33 (=virtual bank 0x21).
        m.ram[1 * BANK_SIZE] = 0xAB;
        m.set_dump_basename(basename.clone());

        m.port_out(PORT_DUMP as u16, 0x21);

        let bytes = std::fs::read(&basename).unwrap();
        assert_eq!(bytes.len(), BANK_SIZE);
        assert_eq!(bytes[0], 0xAB);
        std::fs::remove_dir_all(basename.parent().unwrap()).unwrap();
    }

    #[test]
    fn dump_flash_all_writes_512k() {
        let basename = temp_basename("flash-all");
        let mut m = CpmMachine::new();
        m.set_dump_basename(basename.clone());

        m.port_out(PORT_DUMP as u16, DUMP_FLASH_ALL);

        let bytes = std::fs::read(&basename).unwrap();
        assert_eq!(bytes.len(), FLASH_SIZE);
        assert!(bytes.iter().all(|&b| b == 0xFF));
        std::fs::remove_dir_all(basename.parent().unwrap()).unwrap();
    }

    #[test]
    fn dump_ram_all_writes_512k() {
        let basename = temp_basename("ram-all");
        let mut m = CpmMachine::new();
        m.ram[123] = 0x77;
        m.set_dump_basename(basename.clone());

        m.port_out(PORT_DUMP as u16, DUMP_RAM_ALL);

        let bytes = std::fs::read(&basename).unwrap();
        assert_eq!(bytes.len(), RAM_SIZE);
        assert_eq!(bytes[123], 0x77);
        std::fs::remove_dir_all(basename.parent().unwrap()).unwrap();
    }

    #[test]
    fn dump_all_writes_flash_then_ram() {
        let basename = temp_basename("all");
        let mut m = CpmMachine::new();
        m.flash[0] = 0xF0;
        m.ram[0] = 0x55;
        m.set_dump_basename(basename.clone());

        m.port_out(PORT_DUMP as u16, DUMP_ALL);

        let bytes = std::fs::read(&basename).unwrap();
        assert_eq!(bytes.len(), FLASH_SIZE + RAM_SIZE);
        assert_eq!(bytes[0], 0xF0);
        assert_eq!(bytes[FLASH_SIZE], 0x55);
        std::fs::remove_dir_all(basename.parent().unwrap()).unwrap();
    }

    #[test]
    fn collision_derives_numbered_filenames() {
        let basename = temp_basename("collision");
        let mut m = CpmMachine::new();
        m.set_dump_basename(basename.clone());

        m.port_out(PORT_DUMP as u16, 0x00);
        m.port_out(PORT_DUMP as u16, 0x00);
        m.port_out(PORT_DUMP as u16, 0x00);

        let dir = basename.parent().unwrap();
        let stem = basename.file_stem().unwrap().to_str().unwrap();
        let ext = basename.extension().unwrap().to_str().unwrap();
        assert!(basename.exists(), "first dump should use the basename");
        assert!(dir.join(format!("{}.001.{}", stem, ext)).exists());
        assert!(dir.join(format!("{}.002.{}", stem, ext)).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dump_with_no_basename_is_a_noop() {
        let mut m = CpmMachine::new();
        // No panic, no file written, no side effects.
        m.port_out(PORT_DUMP as u16, 0x00);
        m.port_out(PORT_DUMP as u16, DUMP_ALL);
    }

    #[test]
    fn dump_with_invalid_value_is_a_noop() {
        let basename = temp_basename("invalid");
        let mut m = CpmMachine::new();
        m.set_dump_basename(basename.clone());

        // 0x40..=0x80 (excl. 0x80 itself which is the trigger) and 0x84..=0xFF
        // are reserved/no-op.
        m.port_out(PORT_DUMP as u16, 0x40);
        m.port_out(PORT_DUMP as u16, 0x84);
        m.port_out(PORT_DUMP as u16, 0xFF);

        assert!(!basename.exists());
        std::fs::remove_dir_all(basename.parent().unwrap()).unwrap();
    }

    #[test]
    fn unrelated_ports_still_record_last_value() {
        let mut m = CpmMachine::new();
        m.port_out(0x10, 0x99);
        assert_eq!(m.out_port, Some(0x10));
        assert_eq!(m.out_value, 0x99);
        let _ = m.port_in(0x20);
        assert_eq!(m.in_port, Some(0x20));
    }
}
