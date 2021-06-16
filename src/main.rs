#![no_std]
#![no_main]
#![feature(default_alloc_error_handler, alloc_error_handler)]

extern crate alloc;

#[macro_use]
extern crate num_derive;

use panic_halt as _;

use alloc_cortex_m::CortexMHeap;
use cortex_m_rt::entry;

mod gb;
use gb::Gameboy;

mod coroutines;
mod peripherals;

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

#[entry]
fn main() -> ! {
    peripherals::init();

    unsafe { ALLOCATOR.init(cortex_m_rt::heap_start() as usize, 0x8000) };

    let bytes = include_bytes!("../../../gb-test-roms/cpu_instrs/cpu_instrs.gb");

    let mut gameboy = Gameboy::new(bytes);

    gameboy.run();
}
