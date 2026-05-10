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

pub struct CpmMachine {
    flash: Vec<u8>,
    ram: Vec<u8>,
    bank_map: [u8; 4],
    mapping_enabled: bool,
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
            in_values: [0; 256],
            in_port: None,
            out_port: None,
            out_value: 0,
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
            _ => {
                self.out_port = Some(port);
                self.out_value = value;
            }
        }
    }
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
