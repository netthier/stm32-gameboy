use crate::gb::cartridge::Cartridge;
use alloc::vec;
use alloc::vec::Vec;
use cortex_m_semihosting::{hprint, hprintln};

pub type SharedMem = alloc::rc::Rc<core::cell::RefCell<Memory>>;

pub struct Memory {
    rom: Cartridge,
    vram: Vec<u8>,
    wram_0: Vec<u8>,
    wram_n: Vec<u8>,
    oam: Oam,
    pub io_regs: IoRegs,
    hram: Vec<u8>,
    ie: u8,
}

impl Memory {
    pub fn new(rom: &'static [u8]) -> Self {
        Self {
            rom: Cartridge::load(rom).unwrap(),
            vram: vec![0; 0x2000],
            wram_0: vec![0; 0x1000],
            wram_n: vec![0; 0x1000],
            oam: Oam,
            io_regs: IoRegs::new(),
            hram: vec![0; 0x7F],
            ie: 0,
        }
    }

    pub fn read_word(&mut self, addr: u16) -> u8 {
        let addr = addr as usize;

        match addr {
            0x0000..=0x7FFF => self.rom.read(addr),
            0x8000..=0x9FFF => self.vram[addr - 0x8000],
            0xA000..=0xBFFF => self.rom.read(addr),
            0xC000..=0xCFFF => self.wram_0[addr - 0xC000],
            0xD000..=0xDFFF => self.wram_n[addr - 0xD000], // needs to be adjusted for potential CGB support
            0xE000..=0xFDFF => {
                let addr = (addr - 0xE000) % 0x1E00;
                if addr >= 0x1000 {
                    self.wram_n[addr - 0x1000]
                } else {
                    self.wram_0[addr]
                }
            }
            0xFE00..=0xFE9F => {
                hprintln!("Tried to access OAM at {:X}", addr);
                0
            } // OAM here
            0xFEA0..=0xFEFF => {
                hprintln!("Tried to access prohibited memory at {:X}", addr);
                0
            } // use prohibited
            0xFF00..=0xFF7F => self.io_regs.read(addr),
            0xFF80..=0xFFFE => self.hram[addr - 0xFF80],
            0xFFFF => self.ie,
            _ => unreachable!(),
        }
    }

    pub fn write_word(&mut self, addr: u16, val: u8) {
        let addr = addr as usize;

        match addr {
            0x0000..=0x7FFF => self.rom.write(addr, val),
            0x8000..=0x9FFF => self.vram[addr - 0x8000] = val,
            0xA000..=0xBFFF => self.rom.write(addr, val),
            0xC000..=0xCFFF => self.wram_0[addr - 0xC000] = val,
            0xD000..=0xDFFF => self.wram_n[addr - 0xD000] = val, // needs to be adjusted for potential CGB support
            0xE000..=0xFDFF => {
                let addr = (addr - 0xE000) % 0x1E00;
                if addr >= 0x1000 {
                    self.wram_n[addr - 0x1000] = val;
                } else {
                    self.wram_0[addr] = val;
                }
            }
            0xFE00..=0xFE9F => {
                hprintln!("Tried to write {:X} to OAM at {:X}", val, addr);
            } // OAM here
            0xFEA0..=0xFEFF => {
                hprintln!(
                    "Tried to write {:X} to prohibited memory at {:X}",
                    val,
                    addr
                );
            } // use prohibited
            0xFF00..=0xFF7F => self.io_regs.write(addr, val),
            0xFF80..=0xFFFE => self.hram[addr - 0xFF80] = val,
            0xFFFF => self.ie = val,
            _ => unreachable!(),
        }
    }
}

struct Oam;
pub struct IoRegs {
    pub tim_div: [u8; 4],
    pub int_f: u8,
}

// TODO: Initialize these correctly
impl IoRegs {
    pub fn new() -> Self {
        Self {
            tim_div: [0; 4],
            int_f: 0,
        }
    }

    pub fn write(&mut self, addr: usize, val: u8) {
        match addr {
            0xFF01 => {
                hprint!("{}", val as char);
            }
            0xFF00 => {}
            0xFF02 => {} // no-op for now
            0xFF04..=0xFF07 => self.tim_div[addr - 0xFF04] = val,
            0xFF0F => self.int_f = val,
            0xFF24..=0xFF26 => {} // no-op for now
            0xFF42 => {}          //no-op for now
            _ => {
                hprintln!(
                    "Tried to write {:X} to unimplemented IO register at {:X}",
                    val,
                    addr
                );
            }
        };
    }

    pub fn read(&self, addr: usize) -> u8 {
        match addr {
            0xFF04..=0xFF07 => self.tim_div[addr - 0xFF04],
            0xFF0F => self.int_f,
            0xFF44 => 0x90,
            _ => {
                hprintln!("Tried to access unimplemented IO register at {:X}", addr);
                0
            }
        }
    }
}
