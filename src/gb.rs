use alloc::rc::Rc;
use core::cell::RefCell;
use core::future::Future;
use core::task::{Context, Poll, Waker};

mod apu;
mod cartridge;
mod cpu;
mod mem;
mod ppu;

use crate::coroutines::create_waker;
use crate::pin_mut;
use alloc::boxed::Box;
use core::pin::Pin;

pub struct Gameboy {
    cpu: cpu::Cpu,
    ppu: ppu::Ppu,
    mem: mem::SharedMem,
    waker: Waker,
}

impl Gameboy {
    pub fn new(rom: &'static [u8]) -> Self {
        let mem = Rc::new(RefCell::new(mem::Memory::new(rom)));
        Self {
            cpu: cpu::Cpu::new(mem.clone()),
            ppu: ppu::Ppu::new(mem.clone()),
            mem,
            waker: create_waker(),
        }
    }

    pub fn run(&mut self) -> ! {
        let mut ctx = Context::from_waker(&self.waker);
        loop {
            let future = self.cpu.step();
            pin_mut!(future);
            while future.as_mut().poll(&mut ctx).is_pending() {}
        }
    }
}
