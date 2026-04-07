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
    gpio::{GpioExt, PinState, Speed},
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

    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);

    let _en24 = gpioc.pc6.into_push_pull_output_in_state(PinState::High);
    let _en19 = gpioc.pc7.into_push_pull_output_in_state(PinState::High);
    let _nen5a = gpioc.pc8.into_push_pull_output_in_state(PinState::Low);
    let _nen5b = gpioc.pc9.into_push_pull_output_in_state(PinState::Low);

    let lpi = gpioc.pc10.into_alternate().speed(Speed::Low);    
    let lp0 = gpioc.pc12.into_alternate().speed(Speed::Low);
    let lp1 = gpioa.pa2.into_alternate().speed(Speed::Low);
    let lp2 = gpiob.pb9.into_alternate().speed(Speed::Low);
    let lp3 = gpioc.pc4.into_alternate().speed(Speed::Low);

    let mut lci = argb::Controller::new(dp.UART4, lpi, 255, &mut rcc);
    let mut lc0 = argb::Controller::new(dp.UART5, lp0, 255, &mut rcc);
    let mut lc1 = argb::Controller::new(dp.USART2, lp1, 255, &mut rcc);
    let mut lc2 = argb::Controller::new(dp.USART3, lp2, 255, &mut rcc);
    let mut lc3 = argb::Controller::new(dp.USART1, lp3, 255, &mut rcc);

    let leds = [Colour::AMBER; NUM_LEDS];

    loop {
        lci.display(&[Colour::AMBER; 3]);
        lc0.display(&leds);
        lc1.display(&leds);
        lc2.display(&leds);
        lc3.display(&leds);
    }
}
