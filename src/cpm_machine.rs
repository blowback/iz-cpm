use std::fs;
use std::path::{Path, PathBuf};

use iz80::Machine;

#[cfg(feature = "videobeast")]
use crate::constants::FIRMWARE_CONSOLE_BYTE;
#[cfg(feature = "videobeast")]
use crate::videobeast::VideoBeast;

const BANK_SIZE: usize = 0x4000;            // 16 KiB
const FLASH_BANKS: usize = 32;              // virtual banks 0..32
const RAM_BANKS: usize = 32;                // virtual banks 32..64
const FLASH_SIZE: usize = FLASH_BANKS * BANK_SIZE;
const RAM_SIZE: usize = RAM_BANKS * BANK_SIZE;
const PAGE_TYPE_MASK: u8 = 0xE0;            // page & 0xE0 selects flash, RAM or VideoBeast (as beastem)
const PAGE_BANK_MASK: u8 = 0x1F;            // page & 0x1F selects the bank within flash or RAM
const PAGE_TYPE_RAM: u8 = 0x20;
const PAGE_TYPE_VIDEOBEAST: u8 = 0x40;

const PORT_BANK0: u8 = 0x70;
const PORT_BANK3: u8 = 0x73;
const PORT_MAP_ENABLE: u8 = 0x74;
const PORT_DUMP: u8 = 0x80;
const DUMP_FLASH_ALL: u8 = 0x81;
const DUMP_RAM_ALL: u8 = 0x82;
const DUMP_ALL: u8 = 0x83;

/// Where a CPU address lands after page translation; indices are into
/// `flash` or `ram`, the VideoBeast offset is within its 16 KB window.
enum Target {
    Flash(usize),
    Ram(usize),
    VideoBeast(u16),
}

pub struct CpmMachine {
    flash: Vec<u8>,
    ram: Vec<u8>,
    bank_map: [u8; 4],
    // The MicroBeast page registers (ports 0x70-0x73) are WRITE-ONLY on real
    // hardware: you cannot read back the latched page. The BIOS keeps the
    // authoritative shadow of the mapping (and can re-page under disk ops),
    // exposed via MBB_GET_PAGE / MBB_SET_PAGE. We model that shadow separately
    // so that direct `OUT (0x7x)` (which writes the register but NOT the BIOS
    // shadow) visibly desyncs from MBB_GET_PAGE — the real hazard antforth hit.
    bios_shadow: [u8; 4],
    mapping_enabled: bool,
    dump_basename: Option<PathBuf>,
    in_values: [u8; 256],
    in_port: Option<u8>,
    out_port: Option<u8>,
    out_value: u8,
    // Emulated time at which the current instruction started; VideoBeast
    // accesses are stamped with it.
    #[cfg(feature = "videobeast")]
    instr_start_ps: u64,
    // Attached by `run()`, never by `new()`: unit tests build machines in
    // parallel threads and must not each start SDL.
    #[cfg(feature = "videobeast")]
    videobeast: Option<VideoBeast>,
    // Set by `run()` after the BIOS table is written: from then on writes
    // reaching RAM byte FIRMWARE_CONSOLE_BYTE are dropped.
    #[cfg(feature = "videobeast")]
    firmware_byte_guarded: bool,
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
            bios_shadow: [32, 33, 34, 35],
            mapping_enabled: true,
            dump_basename: None,
            in_values: [0; 256],
            in_port: None,
            out_port: None,
            out_value: 0,
            #[cfg(feature = "videobeast")]
            instr_start_ps: 0,
            #[cfg(feature = "videobeast")]
            videobeast: None,
            #[cfg(feature = "videobeast")]
            firmware_byte_guarded: false,
        }
    }

    #[cfg(feature = "videobeast")]
    pub fn attach_videobeast(&mut self, videobeast: VideoBeast) {
        self.videobeast = Some(videobeast);
    }

    /// From now on, drop writes whose target is the RAM byte at
    /// FIRMWARE_CONSOLE_BYTE (the reset mapping puts it at CPU 0xFF2B). A
    /// program storing the firmware console byte would otherwise overwrite
    /// the fake BIOS jump table. Call after `Bios::setup`.
    #[cfg(feature = "videobeast")]
    pub fn guard_firmware_byte(&mut self) {
        self.firmware_byte_guarded = true;
    }

    #[cfg(feature = "videobeast")]
    fn firmware_byte_write_ignored(&self, index: usize, value: u8) -> bool {
        if self.firmware_byte_guarded && index == FIRMWARE_CONSOLE_BYTE as usize {
            eprintln!(
                "iz-cpm: ignored write of 0x{:02X} to 0x{:04X} (firmware console byte)",
                value, FIRMWARE_CONSOLE_BYTE
            );
            return true;
        }
        false
    }

    #[cfg(not(feature = "videobeast"))]
    fn firmware_byte_write_ignored(&self, _index: usize, _value: u8) -> bool {
        false
    }

    #[cfg(feature = "videobeast")]
    pub fn set_instr_start_ps(&mut self, ps: u64) {
        self.instr_start_ps = ps;
    }

    /// Brings the attached VideoBeast up to `now_ps`.
    #[cfg(feature = "videobeast")]
    pub fn videobeast_catch_up(&mut self, now_ps: u64) {
        if let Some(vb) = &mut self.videobeast {
            vb.catch_up(now_ps);
        }
    }

    #[cfg(feature = "videobeast")]
    fn videobeast_read(&mut self, offset: u16) -> u8 {
        match &mut self.videobeast {
            Some(vb) => vb.read(offset, self.instr_start_ps),
            None => 0xFF,
        }
    }

    #[cfg(feature = "videobeast")]
    fn videobeast_write(&mut self, offset: u16, value: u8) {
        if let Some(vb) = &mut self.videobeast {
            vb.write(offset, value, self.instr_start_ps);
        }
    }

    // No VideoBeast attached: the bus floats high.
    #[cfg(not(feature = "videobeast"))]
    fn videobeast_read(&mut self, _offset: u16) -> u8 {
        0xFF
    }

    // No VideoBeast attached: writes go nowhere.
    #[cfg(not(feature = "videobeast"))]
    fn videobeast_write(&mut self, _offset: u16, _value: u8) {}

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

    fn decode(&self, address: u16) -> Target {
        let phys = (address >> 14) as usize;
        let offset = (address as usize) & (BANK_SIZE - 1);
        if !self.mapping_enabled {
            return Target::Flash(phys * BANK_SIZE + offset);
        }
        let page = self.bank_map[phys];
        let bank = (page & PAGE_BANK_MASK) as usize;
        match page & PAGE_TYPE_MASK {
            PAGE_TYPE_RAM => Target::Ram(bank * BANK_SIZE + offset),
            PAGE_TYPE_VIDEOBEAST => Target::VideoBeast(offset as u16),
            _ => Target::Flash(bank * BANK_SIZE + offset),
        }
    }

    /// MBB_SET_PAGE (BIOS 0xFDDF): set logical CPU page (0-2) to physical page
    /// E (00-1F ROM, 20-3F RAM, 40-5F VideoBeast, all 8 bits kept; see
    /// `decode`). Writes BOTH the hardware register (bank_map,
    /// used for address translation) AND the BIOS shadow — this is the blessed
    /// way to switch a page and the only way the shadow stays authoritative.
    /// Logical page 3 (the BIOS's own page) is not user-settable; ignored.
    pub fn mbb_set_page(&mut self, logical: u8, physical: u8) {
        let logical = logical as usize;
        if logical > 2 {
            return;
        }
        self.bank_map[logical] = physical;
        self.bios_shadow[logical] = physical;
    }

    /// MBB_GET_PAGE (BIOS 0xFDDC): return the physical page mapped to logical
    /// CPU page C (0-2) per the BIOS shadow, or 0xFF on error (logical > 2).
    /// Note: returns the SHADOW, not the live register — a prior direct
    /// `OUT (0x7x)` that bypassed the shadow will read back stale here, exactly
    /// as on hardware.
    pub fn mbb_get_page(&self, logical: u8) -> u8 {
        if logical > 2 {
            return 0xFF;
        }
        self.bios_shadow[logical as usize]
    }
}

impl Machine for CpmMachine {
    fn peek(&mut self, address: u16) -> u8 {
        match self.decode(address) {
            Target::Flash(index) => self.flash[index],
            Target::Ram(index) => self.ram[index],
            Target::VideoBeast(offset) => self.videobeast_read(offset),
        }
    }

    fn poke(&mut self, address: u16, value: u8) {
        match self.decode(address) {
            Target::Ram(index) => {
                if !self.firmware_byte_write_ignored(index, value) {
                    self.ram[index] = value;
                }
            }
            Target::VideoBeast(offset) => self.videobeast_write(offset, value),
            // Flash ROM: writes are silently ignored.
            Target::Flash(_) => {}
        }
    }

    fn port_in(&mut self, address: u16) -> u8 {
        let port = address as u8;
        match port {
            // Page registers are WRITE-ONLY on real hardware. A read floats
            // (open bus); we approximate with the low address byte (the port
            // number), which is what the bus tends to reflect and matches the
            // one observed silicon reading. Code must use MBB_GET_PAGE to read
            // the mapping, never IN on these ports.
            PORT_BANK0..=PORT_BANK3 => port,
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
                self.bank_map[(port - PORT_BANK0) as usize] = value;
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
    fn page_registers_are_write_only() {
        // Reading a page register does NOT return the latched page (the
        // registers are write-only on real hardware); it returns open bus,
        // approximated by the port number. The write itself still takes effect
        // (verified via address translation), it just can't be read back.
        let mut m = CpmMachine::new();
        m.port_out(0x72, 0x2A);
        assert_eq!(m.port_in(0x72), 0x72, "read floats to the port number, not the latch");
        // The write landed: logical page 2 now translates through RAM bank 0x2A.
        m.poke(0x8000, 0x5C);
        assert_eq!(m.ram[(0x2A - FLASH_BANKS) * BANK_SIZE], 0x5C);
    }

    #[test]
    fn pages_00_to_1f_select_flash() {
        let mut m = CpmMachine::new();
        m.flash[0x1F * BANK_SIZE] = 0x5A;
        m.port_out(0x71, 0x1F);
        assert_eq!(m.peek(0x4000), 0x5A);
        m.poke(0x4000, 0x00);              // flash is read-only
        assert_eq!(m.flash[0x1F * BANK_SIZE], 0x5A);
    }

    #[test]
    fn pages_20_to_3f_select_ram() {
        let mut m = CpmMachine::new();
        m.port_out(0x71, 0x3F);
        m.poke(0x4000, 0xC7);
        assert_eq!(m.ram[0x1F * BANK_SIZE], 0xC7);
        assert_eq!(m.peek(0x4000), 0xC7);
    }

    #[test]
    fn pages_40_to_5f_select_videobeast_which_is_absent() {
        // Page registers keep all 8 bits and decode by page & 0xE0, as in
        // beastem. With no VideoBeast attached, reads float to 0xFF and
        // writes reach neither RAM nor flash.
        for page in [0x40, 0x45, 0x5F] {
            let mut m = CpmMachine::new();
            m.port_out(0x71, page);
            assert_eq!(m.peek(0x4000), 0xFF, "page {:#04x}", page);
            m.poke(0x4000, 0x12);
            assert_eq!(m.peek(0x4000), 0xFF, "page {:#04x}", page);
            assert!(m.ram.iter().all(|&b| b == 0), "page {:#04x} wrote RAM", page);
            assert!(m.flash.iter().all(|&b| b == 0xFF), "page {:#04x} wrote flash", page);
        }
    }

    #[test]
    fn pages_60_to_ff_select_flash_by_low_five_bits() {
        let mut m = CpmMachine::new();
        m.flash[3 * BANK_SIZE] = 0x33;
        m.flash[0x1F * BANK_SIZE] = 0x1F;
        m.port_out(0x71, 0xA3);
        assert_eq!(m.peek(0x4000), 0x33);
        m.port_out(0x71, 0xFF);            // no longer masked to RAM bank 0x3F
        assert_eq!(m.peek(0x4000), 0x1F);
        m.poke(0x4000, 0x00);
        assert_eq!(m.flash[0x1F * BANK_SIZE], 0x1F);
        assert!(m.ram.iter().all(|&b| b == 0));
    }

    #[test]
    fn mbb_set_page_keeps_videobeast_page() {
        let mut m = CpmMachine::new();
        m.mbb_set_page(1, 0x40);
        assert_eq!(m.mbb_get_page(1), 0x40);
        assert_eq!(m.peek(0x4000), 0xFF);
        m.mbb_set_page(1, 0xFF);
        assert_eq!(m.mbb_get_page(1), 0xFF, "shadow keeps all 8 bits");
    }

    #[test]
    fn mbb_set_get_page_round_trips_via_shadow() {
        let mut m = CpmMachine::new();
        // Set logical page 2 to physical RAM page 0x37 via the BIOS routine.
        m.mbb_set_page(2, 0x37);
        assert_eq!(m.mbb_get_page(2), 0x37);
        // It also moved the live mapping: page 2 translates through bank 0x37.
        m.poke(0x8000, 0xA5);
        assert_eq!(m.ram[(0x37 - FLASH_BANKS) * BANK_SIZE], 0xA5);
        // Defaults elsewhere intact.
        assert_eq!(m.mbb_get_page(0), 32);
        assert_eq!(m.mbb_get_page(1), 33);
        // Logical page 3 (BIOS page) is not gettable/settable.
        assert_eq!(m.mbb_get_page(3), 0xFF);
    }

    #[test]
    fn direct_out_desyncs_the_bios_shadow() {
        // The whole point of the model: a direct OUT writes the register but
        // NOT the BIOS shadow, so MBB_GET_PAGE reads back stale — exactly the
        // hazard that bit antforth on hardware.
        let mut m = CpmMachine::new();
        m.mbb_set_page(2, 0x30);           // shadow and register agree: 0x30
        assert_eq!(m.mbb_get_page(2), 0x30);
        m.port_out(0x72, 0x3B);            // direct write: register -> 0x3B, shadow untouched
        assert_eq!(m.mbb_get_page(2), 0x30, "shadow is stale after a direct OUT");
        // The live mapping did change (register is authoritative for translation).
        m.poke(0x8000, 0x11);
        assert_eq!(m.ram[(0x3B - FLASH_BANKS) * BANK_SIZE], 0x11);
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

    #[test]
    fn ldir_takes_21_t_states_per_repeat_and_16_on_the_last() {
        // VideoBeast raster timing is derived from the cycle count, so block
        // moves must cost what the real Z80 charges (fixed in iz80 0.5).
        let mut m = CpmMachine::new();
        let program = [
            0x01, 0x03, 0x00, // LD BC,3
            0x21, 0x00, 0x02, // LD HL,0x0200
            0x11, 0x00, 0x03, // LD DE,0x0300
            0xED, 0xB0,       // LDIR
        ];
        for (i, b) in program.iter().enumerate() {
            m.poke(0x0100 + i as u16, *b);
        }
        for i in 0..3 {
            m.poke(0x0200 + i, 0xC0 + i as u8);
        }

        let mut cpu = iz80::Cpu::new_z80();
        cpu.registers().set_pc(0x0100);
        for _ in 0..3 {
            cpu.execute_instruction(&mut m);
        }

        let mut costs = Vec::new();
        for _ in 0..3 {
            let before = cpu.cycle_count();
            cpu.execute_instruction(&mut m);
            costs.push(cpu.cycle_count() - before);
        }
        assert_eq!(costs, vec![21, 21, 16]);
        assert_eq!(cpu.registers().pc(), 0x010B);
        for i in 0..3 {
            assert_eq!(m.peek(0x0300 + i), 0xC0 + i as u8);
        }
    }

    #[cfg(feature = "videobeast")]
    #[test]
    fn guarded_firmware_byte_write_is_ignored() {
        let mut m = CpmMachine::new();
        m.poke(0xFF2B, 0x8E);
        m.guard_firmware_byte();
        m.poke(0xFF2B, 0x24);
        assert_eq!(m.peek(0xFF2B), 0x8E);
        // Neighbours are not guarded.
        m.poke(0xFF2A, 0x11);
        m.poke(0xFF2C, 0x22);
        assert_eq!(m.peek(0xFF2A), 0x11);
        assert_eq!(m.peek(0xFF2C), 0x22);
    }

    #[cfg(feature = "videobeast")]
    #[test]
    fn unguarded_firmware_byte_write_lands() {
        let mut m = CpmMachine::new();
        m.poke(0xFF2B, 0x24);
        assert_eq!(m.peek(0xFF2B), 0x24);
    }

    #[cfg(feature = "videobeast")]
    #[test]
    fn firmware_byte_write_lands_when_page_3_is_remapped() {
        let mut m = CpmMachine::new();
        m.guard_firmware_byte();
        // Page 3 to RAM bank 4: CPU 0xFF2B is no longer RAM byte 0xFF2B.
        m.port_out(PORT_BANK3 as u16, PAGE_TYPE_RAM | 4);
        m.poke(0xFF2B, 0x24);
        assert_eq!(m.peek(0xFF2B), 0x24);
        assert_eq!(m.ram[4 * BANK_SIZE + 0x3F2B], 0x24);
        assert_eq!(m.ram[0xFF2B], 0);
    }
}
