#![no_std]
#![no_main]

use cortex_m_rt::entry;
use panic_halt as _;

use stm32g4xx_hal::{
    gpio::{self, GpioExt}, pac, prelude::*, pwm::PwmExt, pwr::{PwrExt, VoltageScale}, rcc::{
        Config,
        PllConfig,
        PllMDiv,
        PllNMul,
        PllQDiv,
        PllRDiv,
        PllSrc,
        RccExt
    }, time::{ExtU32, RateExtU32},
};

#[entry]
fn main() -> ! {
    
    let dp = pac::Peripherals::take().unwrap();
    let cp = pac::CorePeripherals::take().unwrap();
    let pwr = dp.PWR.constrain().vos(VoltageScale::Range1 { enable_boost: true }).freeze();
        
    let mut rcc = dp.RCC.freeze(
        Config::pll()
            .pll_cfg(PllConfig {
                mux: PllSrc::HSE(24.MHz()),
                m: PllMDiv::DIV_2,
                n: PllNMul::MUL_12,
                r: Some(PllRDiv::DIV_2),
                q: Some(PllQDiv::DIV_2),
                p: None
            }),
            pwr
        );


    // Internal Clock
    // let mut rcc = dp.RCC.freeze(
    // Config::pll()
    //     .pll_cfg(PllConfig {
    //         mux: PllSrc::HSI, // Changed to Internal Oscillator
    //         m: PllMDiv::DIV_2,
    //         n: PllNMul::MUL_14, // Adjusted for 16MHz clock to hit 56MHz
    //         r: Some(PllRDiv::DIV_2),
    //         q: Some(PllQDiv::DIV_2),
    //         p: None,
    //     }),
    //     pwr
    // );

    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);
    let gpiod = dp.GPIOD.split(&mut rcc);

    let mut chan0 = gpioa.pa10.into_push_pull_output();
    let mut chan1 = gpioc.pc11.into_push_pull_output();
    let mut chan2 = gpiob.pb9.into_push_pull_output();
    let mut chan3 = gpiod.pd2.into_push_pull_output();
    let mut chan4 = gpiob.pb10.into_push_pull_output();
    let mut chan5 = gpioa.pa4.into_push_pull_output();
    let mut chan6 = gpioc.pc6.into_push_pull_output();
    let mut chan7 = gpioc.pc7.into_push_pull_output();

    let mut vdrive_clk = gpioa.pa15.into_push_pull_output();

    let mut delay_syst = cp.SYST.delay(&rcc.clocks);
    
    // Channels are active low
    chan0.set_low();
    chan1.set_high();
    chan2.set_high();
    chan3.set_high();
    chan4.set_high();
    chan5.set_high();
    chan6.set_high();
    chan7.set_high();

    loop {
        
        // bit banged 5% duty cycleat 50khz, probably replace with timer
        // note that the duty cycle cannot be too high. Consider the power through R202
        vdrive_clk.set_low();
        delay_syst.delay(19.micros());
        vdrive_clk.set_high();
        delay_syst.delay(1.micros());

    };
}
