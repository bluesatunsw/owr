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

fn main() {
    println!("Hello, world!");
}
