use embedded_common::argb;
use stm32g4xx_hal::{adc::{self, config::SampleTime}, gpio, opamp, pac::{self, uart4::cr1::RE}};

const VDDA_CENTIVOLT: u32 = 320;
const RESOLUTION_BITS: usize = 12;

/// Power controller for both the +19V rail, used for the OBC (Intel NUC), and the +24V rail, used
/// for the radio (PoE).
///
/// Incorporates OCP; this requires tick() to be called in the main loop every
/// TICK_PERIOD_US. The tick returns current draw information for telemetry.
///
/// Tested:
/// - 24V and 19V enable/disable
pub struct PowerCtrl {
    en24v: gpio::PC6<gpio::Output>,
    en19v: gpio::PC7<gpio::Output>,
    vsense24v: gpio::PC0<gpio::Analog>,
    vsense19v: gpio::PC1<gpio::Analog>,
    adc2: adc::Adc<pac::ADC2, adc::Configured>,
    adc3: adc::Adc<pac::ADC3, adc::Configured>,
    i24v_opamp: opamp::Pga<opamp::Opamp2, gpio::PB0, gpio::PA6>,
    i19v_opamp: opamp::Pga<opamp::Opamp3, gpio::PB13, gpio::PB1>,
    isense24v: gpio::PA6<gpio::Analog>,
    isense19v: gpio::PB1<gpio::Analog>,
}

pub struct CorePowerData {
    pub v24v_centivolts: u32,
    pub i24v_milliamps: u32,
    pub i24v_raw: u32,
    pub v19v_centivolts: u32,
    pub i19v_milliamps: u32,
    pub i19v_raw: u32,
}

impl PowerCtrl {
    // TODO: proper state machine
    const TICK_PERIOD_US: u32 = 1_000; // 1 ms

    pub fn new(
        en24v: gpio::PC6<gpio::Output>,
        en19v: gpio::PC7<gpio::Output>,
        adc2: adc::Adc<pac::ADC2, adc::Configured>,
        adc3: adc::Adc<pac::ADC3, adc::Configured>,
        vsense24v: gpio::PC0<gpio::Analog>,
        vsense19v: gpio::PC1<gpio::Analog>,
        i24v_opamp: opamp::Pga<opamp::Opamp2, gpio::PB0, gpio::PA6>,
        i19v_opamp: opamp::Pga<opamp::Opamp3, gpio::PB13, gpio::PB1>,
        isense24v: gpio::PA6<gpio::Analog>,
        isense19v: gpio::PB1<gpio::Analog>,
    ) -> Self {
        Self {
            en24v, en19v, vsense24v, vsense19v, adc2,
            adc3, i24v_opamp, i19v_opamp, isense24v, isense19v
        }
    }

    pub fn enable_19v(&mut self) {
        self.en19v.set_high();
    }

    pub fn disable_19v(&mut self) {
        self.en19v.set_low();
    }

    pub fn enable_24v(&mut self) {
        self.en24v.set_high();
    }

    pub fn disable_24v(&mut self) {
        self.en19v.set_low();
    }

    pub fn tick(&mut self) -> CorePowerData {
        // hardcoded because ADC2 doesn't get Vref!!!
        const V24V_DIVIDER_DENOM: u32 = 11; // 10k - 100k divider
        const V19V_DIVIDER_DENOM: u32 = 11; // 10k - 100k divider
        
        const SENSE_24V_RESISTOR_MILLIOHM: u32 = 100;
        const OPAMP_24V_GAIN: u32 = 16;
        const SENSE_19V_RESISTOR_MILLIOHM: u32 = 20;
        const OPAMP_19V_GAIN: u32 = 8;

        let i24_sample_to_milliamps = |sample: u32| -> u32 {
            sample * VDDA_CENTIVOLT * 10_000 / (1 << RESOLUTION_BITS) / (SENSE_24V_RESISTOR_MILLIOHM * OPAMP_24V_GAIN)
        };

        let i19_sample_to_milliamps = |sample: u32| -> u32 {
            sample * VDDA_CENTIVOLT * 10_000 / (1 << RESOLUTION_BITS) / (SENSE_19V_RESISTOR_MILLIOHM * OPAMP_19V_GAIN)
        };

        let v19_sample = self.adc2.convert(&self.vsense19v, SampleTime::Cycles_640_5) as u32;
        let v24_sample = self.adc2.convert(&self.vsense24v, SampleTime::Cycles_640_5) as u32;
        // i think this should work but the HAL misses it...?
        // let i19_sample = self.adc2.convert(&self.i19v_opamp, SampleTime::Cycles_640_5) as u32;
        let i19_sample = self.adc3.convert(&self.isense19v, SampleTime::Cycles_640_5) as u32;
        let i24_sample = self.adc2.convert(&self.isense24v, SampleTime::Cycles_640_5) as u32;
        CorePowerData {
            v24v_centivolts: v24_sample * VDDA_CENTIVOLT * V24V_DIVIDER_DENOM / (1 << RESOLUTION_BITS),
            v19v_centivolts: v19_sample * VDDA_CENTIVOLT * V19V_DIVIDER_DENOM / (1 << RESOLUTION_BITS),
            i24v_milliamps: i24_sample_to_milliamps(i24_sample),
            i24v_raw: i24_sample,
            i19v_milliamps: i19_sample_to_milliamps(i19_sample),
            i19v_raw: i19_sample,
        }
    }
}

/// VBUS sense is in here too because of ADC limitations
/// Untested
pub struct ExtARGBCtrl {
    // can be publically accessed to send colours to the channels
    pub ctrl: (
        // LED0
        argb::Controller<pac::UART5, gpio::PC12<gpio::AF5>>,
        // LED1
        argb::Controller<pac::USART2, gpio::PA2<gpio::AF7>>,
        // LED2
        argb::Controller<pac::USART3, gpio::PB9<gpio::AF7>>,
        // LED3
        argb::Controller<pac::USART1, gpio::PC4<gpio::AF7>>,
    ),
    nen_a: gpio::PC8<gpio::Output>,
    nen_b: gpio::PC9<gpio::Output>,
    adc1: adc::Adc<pac::ADC1, adc::Configured>,
    adc5: adc::Adc<pac::ADC5, adc::Configured>,
    vsense5v: gpio::PC2<gpio::Analog>,
    isense_a: gpio::PB12<gpio::Analog>,
    isense_b: gpio::PA8<gpio::Analog>,
    i5a_opamp: opamp::Pga<opamp::Opamp4, gpio::PB11, gpio::PB12>,
    i5b_opamp: opamp::Pga<opamp::Opamp5, gpio::PC3, gpio::PA8>,
}

pub struct LEDPowerData {
    pub v5v_centivolts: u32,
    pub ia_milliamps: u32,
    pub ib_milliamps: u32,
}

impl ExtARGBCtrl {
    pub fn new(
        led0: argb::Controller<pac::UART5, gpio::PC12<gpio::AF5>>,
        led1: argb::Controller<pac::USART2, gpio::PA2<gpio::AF7>>,
        led2: argb::Controller<pac::USART3, gpio::PB9<gpio::AF7>>,
        led3: argb::Controller<pac::USART1, gpio::PC4<gpio::AF7>>,
        nen_a: gpio::PC8<gpio::Output>,
        nen_b: gpio::PC9<gpio::Output>,
        adc1: adc::Adc<pac::ADC1, adc::Configured>,
        adc5: adc::Adc<pac::ADC5, adc::Configured>,
        vsense5v: gpio::PC2<gpio::Analog>,
        isense_a: gpio::PB12<gpio::Analog>,
        isense_b: gpio::PA8<gpio::Analog>,
        i5a_opamp: opamp::Pga<opamp::Opamp4, gpio::PB11, gpio::PB12>,
        i5b_opamp: opamp::Pga<opamp::Opamp5, gpio::PC3, gpio::PA8>,
    ) -> Self {
        Self {
            ctrl: (
                led0,
                led1,
                led2,
                led3,
            ),
            nen_a,
            nen_b,
            adc1,
            adc5,
            vsense5v,
            isense_a,
            isense_b,
            i5a_opamp,
            i5b_opamp,
        }
    }

    pub fn enable_a(&mut self) {
        self.nen_a.set_low();
    }

    pub fn enable_b(&mut self) {
        self.nen_b.set_low();
    }

    pub fn disable_a(&mut self) {
        self.nen_a.set_high();
    }

    pub fn disable_b(&mut self) {
        self.nen_b.set_high();
    }

    pub fn tick(&mut self) -> LEDPowerData {
        const VBUS_DIVIDER_DENOM: u32 = 23; // 10k - 220k divider
        const V5V_DIVIDER_DENOM: u32 = 2; // 10k - 10k divider
        const SENSE_RESISTOR_MILLIOHM: u32 = 50;
        const OPAMP_GAIN: u32 = 8;

        let v5v_sample = self.adc1.convert(&self.vsense5v, SampleTime::Cycles_640_5) as u32;

        let sample_to_milliamps = |sample: u32| -> u32 {
            sample * VDDA_CENTIVOLT * 10_000 / (1 << RESOLUTION_BITS) / (SENSE_RESISTOR_MILLIOHM * OPAMP_GAIN)
        };

        let a_sample = self.adc1.convert(&self.isense_a, SampleTime::Cycles_640_5) as u32;
        let b_sample = self.adc5.convert(&self.isense_b, SampleTime::Cycles_640_5) as u32;

        LEDPowerData {
            v5v_centivolts: v5v_sample * VDDA_CENTIVOLT * V5V_DIVIDER_DENOM / (1 << RESOLUTION_BITS),
            ia_milliamps: sample_to_milliamps(a_sample),
            ib_milliamps: sample_to_milliamps(b_sample),
        }
    }
}
