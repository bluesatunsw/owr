#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(unsafe_cell_access)]
#![no_std]
#![no_main]

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    abort()
}

use core::intrinsics::abort;
use core::panic::PanicInfo;
use cortex_m_rt::entry;
use embedded_common::argb::{self, Colour};
use fugit::RateExtU32;
use stm32g4xx_hal::{
    gpio::{GpioExt, Speed},
    prelude::*,
    pwr::{PwrExt, VoltageScale},
    rcc::*,
};

const NUM_LEDS: usize = 300;

use embedded_alloc::LlffHeap as Heap;
extern crate alloc;
// Global allocator -- required by canadensis.
// -> transitively req'd by embedded_common
#[global_allocator]
static G_HEAP: Heap = Heap::empty();

///////////
// Main. //
///////////

#[entry]
fn main() -> ! {
    // Embedded boilerplate...
    let dp = stm32g4xx_hal::pac::Peripherals::take().unwrap();
    let pwr = dp
        .PWR
        .constrain()
        .vos(VoltageScale::Range1 { enable_boost: true })
        .freeze();
    let mut rcc = dp.RCC.freeze(
        Config::pll()
            .pll_cfg(PllConfig {
                mux: PllSrc::HSE(24.MHz()),
                m: PllMDiv::DIV_2,
                n: PllNMul::MUL_28,
                r: Some(PllRDiv::DIV_2),
                q: Some(PllQDiv::DIV_2),
                p: None,
            })
            .fdcan_src(FdCanClockSource::PLLQ),
        pwr,
    );

    let gpioc = dp.GPIOC.split(&mut rcc);
    let led_port_0 = gpioc.pc12.into_alternate().speed(Speed::VeryHigh);
    let mut led_ctrl = argb::Controller::new(dp.UART5, led_port_0, 255, &mut rcc);
    let leds = [Colour::AMBER; NUM_LEDS];

    loop {
        led_ctrl.display(&leds);
    }
}
