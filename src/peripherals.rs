use stm32f3_discovery::stm32f3xx_hal::{pac, prelude::*, serial::Serial};

pub fn init() {
    let dp = pac::Peripherals::take().unwrap();

    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();

    let mut gpioc = dp.GPIOC.split(&mut rcc.ahb);

    let clocks = rcc
        .cfgr
        .use_hse(8u32.mhz())
        .sysclk(72u32.mhz())
        .pclk1(24u32.mhz())
        .freeze(&mut flash.acr);
}
