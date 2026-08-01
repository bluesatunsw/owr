/* CPM firmware. Currently in the bringup stage. Uses the G474 template. */

#![no_std]
#![no_main]

extern crate alloc;
use embedded_alloc::LlffHeap as Heap;
use embedded_common::argb::{self, Colour};

use cortex_m_rt::entry;

use stm32g4xx_hal::{
    adc::{self, AdcClaim, AdcCommonExt, config::SampleTime}, gpio::{GpioExt, PinState}, opamp::*, pac, prelude::*, pwr::{PwrExt, VoltageScale}, rcc::*, time::{ExtU32, RateExtU32}
};

use panic_probe as _;
use defmt_rtt as _;

const NUM_LEDS: usize = 300;
const RED: Colour = Colour {r: 255, b: 0, g: 0};
const GREEN: Colour = Colour {r: 0, b: 0, g: 255};
const DEFAULT_BRIGHTNESS: u8 = 64;

mod controller;
use controller::{PowerCtrl, ExtARGBCtrl};

// Global allocator -- required by canadensis.
#[global_allocator]
static G_HEAP: Heap = Heap::empty();

fn initialise_allocator() {
    use core::mem::MaybeUninit;
    // NOTE: You need to calculate HEAP_SIZE such that you don't crash when messages are reassembled
    // You may also want to shrink this if there's not so much CAN traffic
    const HEAP_SIZE: usize = 0x8000; // 32 KiB
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { G_HEAP.init(&raw mut HEAP_MEM as usize, HEAP_SIZE) }
}

///////////
// Main. //
///////////

#[entry]
fn main() -> ! {
    initialise_allocator();

    let dp = stm32g4xx_hal::pac::Peripherals::take().unwrap();
    let cp = stm32g4xx_hal::pac::CorePeripherals::take().unwrap();
    let pwr = dp
        .PWR
        .constrain()
        .vos(VoltageScale::Range1 { enable_boost: true })
        .freeze();
    let mut rcc = dp.RCC.freeze(
        Config::pll()
            .pll_cfg(PllConfig {
                mux: PllSrc::HSE(24.MHz()),
                m: PllMDiv::DIV_2, // 12 MHz
                n: PllNMul::MUL_28, // 336 MHz
                r: Some(PllRDiv::DIV_2), // 168 MHz
                q: Some(PllQDiv::DIV_2), // 168 MHz
                p: Some(PllPDiv::DIV_20), // 16.8 MHz
            })
            .fdcan_src(FdCanClockSource::PLLQ),
        pwr,
    );
    defmt::debug!("Initialised memory allocator and configured clock");

    defmt::trace!("Setting up pins...");
    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);
    
    let (isense24v, isense19v, isense5a, isense5b) = unsafe {
        let stolen_gpioa = pac::GPIOA::steal().split(&mut rcc);
        let stolen_gpiob = pac::GPIOB::steal().split(&mut rcc);
        (
            stolen_gpioa.pa6.into_analog(),
            stolen_gpiob.pb1.into_analog(),
            stolen_gpiob.pb12.into_analog(),
            stolen_gpioa.pa8.into_analog()
        )
    };

    let mut delay = cp.SYST.delay(&rcc.clocks);

    defmt::debug!("Setting up ARGB LED control (on-board)...");
    let mut onboard_leds = argb::Controller::new(
        dp.UART4,
        gpioc.pc10.into_alternate(),
        DEFAULT_BRIGHTNESS,
        &mut rcc
    );
    delay.delay(100.micros());
    onboard_leds.display(&[Colour::AMBER; 3]);

    // r2p0 errata: need to bit-bang I2C
    /*defmt::debug!("Testing EEPROM...");
    defmt::debug!("Setting up I2C EEPROM...");
    const EEPROM_ADDR: u8 = 0b1010_000;
    let sda = gpioa.pa15.into_open_drain_output();
    let sda = sda.internal_pull_up(true);
    let scl = gpiob.pb7.into_alternate_open_drain();
    let scl = scl.internal_pull_up(true);
    let mut i2c_bus = dp.I2C1.i2c((sda, scl), 200.kHz(), &mut rcc);

    defmt::debug!("Reading 8 bytes from address 0x000...");
    let address: u16 = 0x000;
    let mut data: [u8; 8] = [0; 8];
    match i2c_bus.write_read(EEPROM_ADDR, &[((address & 0x3F00) >> 8) as u8, (address & 0xFF) as u8], &mut data) {
        Ok(_) => defmt::debug!("Got {}", data),
        Err(e) => defmt::warn!("Some error reading {:?}", defmt::Debug2Format(&e)),
    }

    defmt::debug!("Incrementing first four bytes at 0x000...");
    let address = 0x000;
    let addr_words = [((address & 0x3F00) >> 8) as u8, (address & 0xFF) as u8];
    let send_bytes: [u8; 6] = [addr_words[0], addr_words[1], data[0] + 1, data[1] + 1, data[2] + 1, data[3] + 1];
    match i2c_bus.write(EEPROM_ADDR, &send_bytes) {
        Ok(_) => defmt::debug!("No error writing data"),
        Err(e) => defmt::warn!("Error writing: {:?}", defmt::Debug2Format(&e)),
    };

    // clear write cycle
    delay.delay(5.millis());

    defmt::debug!("Reading 8 bytes from address 0x000...");
    let address: u16 = 0x000;
    for i in 0..8 {
        data[i] = 0;
    }
    match i2c_bus.write_read(EEPROM_ADDR, &[((address & 0x3F00) >> 8) as u8, (address & 0xFF) as u8], &mut data) {
        Ok(_) => defmt::debug!("Got {}", data),
        Err(e) => defmt::warn!("Error writing: {:?}", defmt::Debug2Format(&e)),
    }*/

    // ADC setup
    defmt::debug!("Configuring ADC12...");
    let mut adc12_common = dp.ADC12_COMMON
        .claim(adc::config::ClockMode::AdcKerCk {
            prescaler: (adc::config::Prescaler::Div_4),
            src: (adc::config::ClockSource::PllP)
        }, 
        &mut rcc
    );
    adc12_common.enable_vref();

    defmt::debug!("Setting up ADC1...");
    let adc1 = adc12_common
        .claim_and_configure(dp.ADC1, adc::config::AdcConfig::default(), &mut delay);
    defmt::debug!("Setting up ADC2...");
    let adc2 = adc12_common
        .claim_and_configure(dp.ADC2, adc::config::AdcConfig::default(), &mut delay);

    defmt::debug!("Configuring ADC3...");
    let mut adc345_common = dp.ADC345_COMMON
        .claim(adc::config::ClockMode::AdcKerCk {
            prescaler: (adc::config::Prescaler::Div_4),
            src: (adc::config::ClockSource::PllP)
        }, 
        &mut rcc
    );
    adc345_common.enable_vref();

    defmt::debug!("Setting up ADC3...");
    let adc3 = adc345_common
        .claim_and_configure(dp.ADC3, adc::config::AdcConfig::default(), &mut delay);
    defmt::debug!("Setting up ADC4...");
    let mut adc4 = adc345_common
        .claim_and_configure(dp.ADC4, adc::config::AdcConfig::default(), &mut delay);
    defmt::debug!("Setting up ADC5...");
    let adc5 = adc345_common
        .claim_and_configure(dp.ADC5, adc::config::AdcConfig::default(), &mut delay);
    
    let (_, opamp2, opamp3, opamp4, opamp5, ..) = dp.OPAMP.split(&mut rcc);
    // 24V draws 1A max -- so 0.1 V drop across sense-resistor full-scale
    // so gain of 16 should be comfortable full-scale
    let i24v_opamp = opamp2
        .pga_external_filter(gpiob.pb0.into_analog(), gpioa.pa5.into_analog(), Gain::Gain16)
        .enable_output(gpioa.pa6.into_analog());
    // 19V draws 10A max -- so 0.2 V drop across sense-resistor full-scale
    // so gain of 8 should be comfortable full-scale (accounting for OCP)
    let i19v_opamp = opamp3
        .pga_external_filter(gpiob.pb13.into_analog(), gpiob.pb2.into_analog(), Gain::Gain8)
        .enable_output(gpiob.pb1.into_analog());
    // LEDs draw max 4A per channel pair, so 0.2 drop across sense resistor full-scale
    // so gain of 8 should be comfortable (w OCP)
    let i5a_opamp = opamp4
        .pga_external_filter(gpiob.pb11.into_analog(), gpiob.pb10.into_analog(), Gain::Gain8)
        .enable_output(gpiob.pb12.into_analog());
    let i5b_opamp = opamp5
        .pga_external_filter(gpioc.pc3.into_analog(), gpiob.pb15.into_analog(), Gain::Gain8)
        .enable_output(gpioa.pa8.into_analog());

    defmt::debug!("Setting up 24V and 19V power control...");
    // initialise in disabled state
    let en24v = gpioc.pc6.into_push_pull_output_in_state(PinState::Low);
    let en19v = gpioc.pc7.into_push_pull_output_in_state(PinState::Low);
    let vsense24v = gpioc.pc0.into_analog();
    let vsense19v = gpioc.pc1.into_analog();
    let mut power_ctrl = PowerCtrl::new(
        en24v, en19v,
        adc2, adc3, vsense24v, vsense19v,
        i24v_opamp, i19v_opamp, isense24v, isense19v
    );

    let vbus_sense = gpiob.pb14.into_analog();

    defmt::debug!("Setting up ARGB LED control (external)...");
    let nen5a = gpioc.pc8.into_push_pull_output_in_state(PinState::High);
    let nen5b = gpioc.pc9.into_push_pull_output_in_state(PinState::High);
    let lc0 = argb::Controller::new(dp.UART5, gpioc.pc12.into_alternate(), 255, &mut rcc);
    let lc1 = argb::Controller::new(dp.USART2, gpioa.pa2.into_alternate(), 255, &mut rcc);
    let lc2 = argb::Controller::new(dp.USART3, gpiob.pb9.into_alternate(), 255, &mut rcc);
    let lc3 = argb::Controller::new(dp.USART1, gpioc.pc4.into_alternate(), 255, &mut rcc);
    let vsense5v = gpioc.pc2.into_analog();
    let mut ext_led_ctrl = ExtARGBCtrl::new(
        lc0, lc1, lc2, lc3,
        nen5a, nen5b,
        adc1, adc5,
        vsense5v,
        isense5a, isense5b,
        i5a_opamp, i5b_opamp,
    );

    defmt::debug!("Turning on 24V and 19V rails...");
    power_ctrl.enable_24v();
    power_ctrl.enable_19v();
    defmt::debug!("Turning on all LED rails...");
    ext_led_ctrl.enable_a();
    ext_led_ctrl.enable_b();
    delay.delay(100.millis());

    // all good!
    onboard_leds.display(&[Colour {r: 0, g: 255, b: 0}; 3]);

    let cpwrdata = power_ctrl.tick();
    let mut min_24v_centivolts: u64 = cpwrdata.v24v_centivolts as u64;
    let mut max_24v_centivolts: u64 = cpwrdata.v24v_centivolts as u64;
    let mut rolling_24v_sum_centivolts: u64 = min_24v_centivolts;
    let mut min_19v_centivolts: u64 = cpwrdata.v19v_centivolts as u64;
    let mut max_19v_centivolts: u64 = cpwrdata.v19v_centivolts as u64;
    let mut rolling_19v_sum_centivolts: u64 = min_19v_centivolts;
    let mut num_samples: u64 = 1;

    loop {
        power_ctrl.enable_19v();
        power_ctrl.enable_24v();
        onboard_leds.display(&[GREEN, GREEN, GREEN]);
        // 15 seconds
        for _ in 0..150 {
            let cpwrdata = power_ctrl.tick();
            /*lc0.display(&leds);
            lc1.display(&leds);
            lc2.display(&leds);
            lc3.display(&leds);*/
            let v24v_cv = cpwrdata.v24v_centivolts as u64;
            rolling_24v_sum_centivolts += v24v_cv;
            num_samples += 1;
            if v24v_cv < min_24v_centivolts { min_24v_centivolts = v24v_cv }
            if v24v_cv > max_24v_centivolts { max_24v_centivolts = v24v_cv }
            let avg_24v_centivolts: u32 = (rolling_24v_sum_centivolts / num_samples) as u32;
            defmt::println!("+24V line: min {}.{:02} V, curr {}.{:02} V, max {}.{:02} V\tdrawing {} mA",
                min_24v_centivolts / 100,
                min_24v_centivolts % 100,
                v24v_cv / 100,
                v24v_cv % 100,
                max_24v_centivolts / 100,
                max_24v_centivolts % 100,
                cpwrdata.i24v_milliamps
            );
            defmt::println!("Raw 24V current ADC sample: {}", cpwrdata.i24v_raw);
            let v19v_cv = cpwrdata.v19v_centivolts as u64;
            rolling_19v_sum_centivolts += v19v_cv;
            if v19v_cv < min_19v_centivolts { min_19v_centivolts = v19v_cv }
            if v19v_cv > max_19v_centivolts { max_19v_centivolts = v19v_cv }
            let avg_19v_centivolts: u32 = (rolling_19v_sum_centivolts / num_samples) as u32;
            defmt::println!("+19V line: min {}.{:02} V, curr {}.{:02} V, max {}.{:02} V\tdrawing {} mA",
                min_19v_centivolts / 100,
                min_19v_centivolts % 100,
                v19v_cv / 100,
                v19v_cv % 100,
                max_19v_centivolts / 100,
                max_19v_centivolts % 100,
                cpwrdata.i19v_milliamps
            );
            defmt::println!("Raw 19V current ADC sample: {}", cpwrdata.i19v_raw);
            defmt::println!("This implies sense resistor voltage {} mV", cpwrdata.i19v_raw * 3200 / (1 << 12) / 8);
            let vbus_sample = adc4.convert(&vbus_sense, SampleTime::Cycles_640_5) as u32;
            const VBUS_DIVIDER_DENOM: u32 = 23; // 10k - 220k divider
            defmt::println!("VBUS = {} mV", vbus_sample * 3200 * VBUS_DIVIDER_DENOM / (1 << 12));
            let ledpwrdata = ext_led_ctrl.tick();
            defmt::println!("+5V line: curr {}.{:02} V", ledpwrdata.v5v_centivolts / 100, ledpwrdata.v5v_centivolts % 100);
            defmt::println!("+5V A: {} mA\tB: {} mA", ledpwrdata.ia_milliamps, ledpwrdata.ib_milliamps);

            delay.delay(100.millis());
        }
        onboard_leds.display(&[RED; 3]);
        power_ctrl.disable_19v();
        power_ctrl.disable_24v();
        for _ in 0..50 { // 5 seconds
            let cpwrdata = power_ctrl.tick();
            /*lc0.display(&leds);
            lc1.display(&leds);
            lc2.display(&leds);
            lc3.display(&leds);*/
            let v24v_cv = cpwrdata.v24v_centivolts as u64;
            rolling_24v_sum_centivolts += v24v_cv;
            num_samples += 1;
            if v24v_cv < min_24v_centivolts { min_24v_centivolts = v24v_cv }
            if v24v_cv > max_24v_centivolts { max_24v_centivolts = v24v_cv }
            let avg_24v_centivolts: u32 = (rolling_24v_sum_centivolts / num_samples) as u32;
            defmt::println!("+24V line: min {}.{:02} V, curr {}.{:02} V, max {}.{:02} V\tdrawing {} mA",
                min_24v_centivolts / 100,
                min_24v_centivolts % 100,
                v24v_cv / 100,
                v24v_cv % 100,
                max_24v_centivolts / 100,
                max_24v_centivolts % 100,
                cpwrdata.i24v_milliamps
            );
            defmt::println!("Raw 24V current ADC sample: {}", cpwrdata.i24v_raw);
            let v19v_cv = cpwrdata.v19v_centivolts as u64;
            rolling_19v_sum_centivolts += v19v_cv;
            if v19v_cv < min_19v_centivolts { min_19v_centivolts = v19v_cv }
            if v19v_cv > max_19v_centivolts { max_19v_centivolts = v19v_cv }
            let avg_19v_centivolts: u32 = (rolling_19v_sum_centivolts / num_samples) as u32;
            defmt::println!("+19V line: min {}.{:02} V, curr {}.{:02} V, max {}.{:02} V\tdrawing {} mA",
                min_19v_centivolts / 100,
                min_19v_centivolts % 100,
                v19v_cv / 100,
                v19v_cv % 100,
                max_19v_centivolts / 100,
                max_19v_centivolts % 100,
                cpwrdata.i19v_milliamps
            );
            defmt::println!("Raw 19V current ADC sample: {}", cpwrdata.i19v_raw);
            defmt::println!("This implies sense resistor voltage {} mV", cpwrdata.i19v_raw * 3200 / (1 << 12) / 8);
            let vbus_sample = adc4.convert(&vbus_sense, SampleTime::Cycles_640_5) as u32;
            const VBUS_DIVIDER_DENOM: u32 = 23; // 10k - 220k divider
            defmt::println!("VBUS = {} mV", vbus_sample * 3200 * VBUS_DIVIDER_DENOM / (1 << 12));
            delay.delay(100.millis());
        }
    }
}
