use cortex_m_semihosting::hprintln;
use num_traits::cast::FromPrimitive;

pub struct Cartridge {
    cart_type: CartridgeType,
    bytes: &'static [u8],
    bank_idx: usize,
}

impl Cartridge {
    pub fn load(bytes: &'static [u8]) -> Option<Self> {
        Some(Self {
            cart_type: FromPrimitive::from_u8(bytes[0x0147])?,
            bytes,
            bank_idx: 1,
        })
    }

    pub fn read(&self, addr: usize) -> u8 {
        match self.cart_type {
            CartridgeType::RomOnly => self.bytes[addr],
            CartridgeType::Mbc1 => match addr {
                0x0000..=0x3FFF => self.bytes[addr],
                _ => self.bytes[addr + 0x4000 * (self.bank_idx - 1)],
            },
            _ => {
                hprintln!("Unimplemented cartridge type {:?}", self.cart_type);
                panic!();
            }
        }
    }

    pub fn write(&mut self, addr: usize, val: u8) {
        match self.cart_type {
            CartridgeType::RomOnly => {
                hprintln!("Tried to write {:X} to {:X} in cartridge", val, addr);
                panic!();
            }
            CartridgeType::Mbc1 => match addr {
                0x2000..=0x3FFF => self.bank_idx = (val & 0x1F) as usize,
                _ => unimplemented!(),
            },
            _ => {
                hprintln!("Unimplemented cartridge type {:?}", self.cart_type);
                panic!();
            }
        }
    }
}

#[derive(FromPrimitive, Debug)]
enum CartridgeType {
    RomOnly = 0x00,
    Mbc1 = 0x01,
    Mbc1Ram = 0x02,
    Mbc1RamBattery = 0x03,
    Mbc2 = 0x05,
    Mbc2Battery = 0x06,
    RomRam = 0x08,
    RomRamBattery = 0x09,
    Mmm01 = 0x0B,
    Mmm01Ram = 0x0C,
    Mmm01RamBattery = 0x0D,
    Mbc3TimerBattery = 0x0F,
    Mbc3TimerRamBattery = 0x10,
    Mbc3 = 0x11,
    Mbc3Ram = 0x12,
    Mbc3RamBattery = 0x13,
    Mbc5 = 0x19,
    Mbc5Ram = 0x1A,
    Mbc5RamBattery = 0x1B,
    Mbc5Rumble = 0x1C,
    MbcRumbleRam = 0x1D,
    MbcRumbleRamBattery = 0x1E,
    Mbc6 = 0x20,
    Mbc7SensorRumbleRamBattery = 0x22,
    PocketCamera = 0xFC,
    BandaiTama5 = 0xFD,
    HuC3 = 0xFE,
    HuC1RamBattery = 0xFF,
}
