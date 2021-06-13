#![no_std]
#![no_main]
#![feature(default_alloc_error_handler, alloc_error_handler)]

extern crate alloc;

#[macro_use]
extern crate num_derive;

use panic_halt as _;

use alloc_cortex_m::CortexMHeap;
use cortex_m_rt::entry;
use stm32f3_discovery::stm32f3xx_hal::{pac, prelude::*};

mod gb;
use gb::Gameboy;

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();

    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(8u32.mhz())
        .sysclk(72u32.mhz())
        .pclk1(24u32.mhz())
        .freeze(&mut flash.acr);

    unsafe { ALLOCATOR.init(cortex_m_rt::heap_start() as usize, 0x8000) };

    let bytes = include_bytes!("../../../gb-test-roms/cpu_instrs/cpu_instrs.gb");

    let mut gameboy = Gameboy::new(bytes);

    loop {
        gameboy.step();
    }
}
