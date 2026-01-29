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
use core::{cell::UnsafeCell, iter::zip};

use canadensis_core::time::Microseconds32;
use cortex_m::asm::delay;
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use embedded_common::{
    argb::{self, Colour},
    clock::MicrosecondClock,
};
use fugit::{ExtU32, RateExtU32};
use stm32g4xx_hal::{
    gpio::{GpioExt, Speed},
    interrupt,
    prelude::*,
    pwr::{PwrExt, VoltageScale},
    rcc::*,
};

use embedded_alloc::LlffHeap as Heap;
extern crate alloc;
// Global allocator -- required by canadensis.
// -> transitively req'd by embedded_common
#[global_allocator]
static G_HEAP: Heap = Heap::empty();

///////////
// Main. //
///////////

static G_SPECIAL_LED: Mutex<UnsafeCell<Colour>> = Mutex::new(UnsafeCell::new(Colour::AMBER));

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

    hprintln!("clocks have been set up!");

    let gpioc = dp.GPIOC.split(&mut rcc);
    let led_port_0 = gpioc.pc12.into_alternate().speed(Speed::VeryHigh);
    let mut led_ctrl = argb::Controller::new(dp.UART5, led_port_0, 255, &mut rcc);
    let mut leds = [Colour::AMBER; 300];
    let clock = MicrosecondClock::new(dp.TIM2, &mut rcc);

    loop {
        leds[0] = cortex_m::interrupt::free(|cs| unsafe {
            *G_SPECIAL_LED.borrow(cs).as_ref_unchecked()
        });
        rainbow(clock.now_const(), &mut leds[1..]);
        led_ctrl.display(&leds);
    }
}

fn rainbow(time: Microseconds32, buf: &mut [Colour]) {
    let length = buf.len();

    for (idx, led) in zip(0..length, buf) {
        let hue = ((time.ticks() / 5_000) as usize + ((idx * 255) / length)) % 255;
        *led = hsv2rgb(&HSVal { hue: hue as u8, sat: 255, val: 128 });
    }
}

#[interrupt]
fn TIM1_UP_TIM16() {
    cortex_m::interrupt::free(|cs| unsafe {
        G_SPECIAL_LED.borrow(cs).replace(Colour {
            r: 100,
            g: 0,
            b: 255,
        });
    });
}

#[derive(Copy, Clone, Default)]
pub struct HSVal {
    pub hue: u8,
    pub sat: u8,
    pub val: u8,
}

pub fn hsv2rgb(hsv: &HSVal) -> Colour {
    let v: u16 = hsv.val as u16;
    let s: u16 = hsv.sat as u16;
    let f: u16 = (hsv.hue as u16 * 2 % 85) * 3; // relative interval

    let p: u16 = v * (255 - s) / 255;
    let q: u16 = v * (255 - (s * f) / 255) / 255;
    let t: u16 = v * (255 - (s * (255 - f)) / 255) / 255;
    match hsv.hue {
        0..=42 => Colour {
            r: v as u8,
            g: t as u8,
            b: p as u8,
        },
        43..=84 => Colour {
            r: q as u8,
            g: v as u8,
            b: p as u8,
        },
        85..=127 => Colour {
            r: p as u8,
            g: v as u8,
            b: t as u8,
        },
        128..=169 => Colour {
            r: p as u8,
            g: q as u8,
            b: v as u8,
        },
        170..=212 => Colour {
            r: t as u8,
            g: p as u8,
            b: v as u8,
        },
        213..=254 => Colour {
            r: v as u8,
            g: p as u8,
            b: q as u8,
        },
        255 => Colour {
            r: v as u8,
            g: t as u8,
            b: p as u8,
        },
    }
}
