use alloc::rc::Rc;
use core::cell::RefCell;

mod apu;
mod cartridge;
mod cpu;
mod mem;
mod ppu;

pub struct Gameboy {
    cpu: cpu::Cpu,
    ppu: ppu::Ppu,
    mem: mem::SharedMem,
}

impl Gameboy {
    pub fn new(rom: &'static [u8]) -> Self {
        let mem = Rc::new(RefCell::new(mem::Memory::new(rom)));
        Self {
            cpu: cpu::Cpu::new(mem.clone()),
            ppu: ppu::Ppu::new(mem.clone()),
            mem,
        }
    }

    pub fn step(&mut self) {
        self.cpu.step();
    }
}
