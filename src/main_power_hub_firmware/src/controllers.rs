use stm32g4xx_hal::{
    adc::{self, config::SampleTime}, gpio::{self}, opamp, pac, pwm
};

// How often the "tick" function should be called in the superloop
pub const PWR_CTRL_TICK_US: u32 = 10_000; // 10 ms

//////////////////////////////
// POWER CHANNEL CODE BLOCK //
//////////////////////////////

const NUM_CHANNELS: usize = 4;

// These channels should be marked physically so that we know which is which
struct PowerChannel {
    enable: gpio::AnyPin<gpio::Output>,
    sense: gpio::AnyPin<gpio::Input>
}

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum PwrChan {
    CH0,
    CH1,
    CH2,
    CH3,
}

pub enum PwrCtrlState {
    SysInit,
    Active,
    EStopped,
    Error
}

#[derive(Copy, Clone, PartialEq)]
pub enum PwrChanState {
    SysInit,
    Enabled,
    Disabled,
    Fault
}

pub struct PowerController {
    pwr_channel: [gpio::AnyPin<gpio::Output>; 4],
    // TODO: opamps are actually phantom data
    opamps: (
        //opamp::Pga<opamp::Opamp1, gpio::PA1, gpio::PA2>,
        opamp::Pga<opamp::Opamp1, gpio::PA1, opamp::InternalOutput>,
        opamp::Pga<opamp::Opamp3, gpio::PB0, gpio::PB1>,
        //opamp::Pga<opamp::Opamp2, gpio::PA7, opamp::InternalOutput>,
        opamp::Pga<opamp::Opamp2, gpio::PA7, gpio::PA6>,
        //opamp::Pga<opamp::Opamp4, gpio::PB11, opamp::InternalOutput>,
        opamp::Pga<opamp::Opamp4, gpio::PB11, gpio::PB12>,
    ),
    sense_pins: (
        gpio::PA2<gpio::Analog>,
        gpio::PB1<gpio::Analog>,
        gpio::PA6<gpio::Analog>,
        gpio::PB12<gpio::Analog>,
    ),
    pub controller_state: PwrCtrlState,
    pub channel_state: [PwrChanState; 4],
    vsense_pin: gpio::Pin<'B', 14>,
    adc1: adc::Adc<pac::ADC1, adc::Configured>,
    adc2: adc::Adc<pac::ADC2, adc::Configured>,
    //adc3: adc::Adc<pac::ADC3, adc::Configured>,
    //adc4: adc::Adc<pac::ADC4, adc::Configured>,
    charge_pump_pwm: pwm::Pwm<pac::TIM3, pwm::C4, pwm::ComplementaryImpossible, pwm::ActiveHigh, pwm::ActiveHigh>,
}

impl PowerController {
    pub fn new(
        en_pin0: gpio::AnyPin<gpio::Output>,
        en_pin1: gpio::AnyPin<gpio::Output>,
        en_pin2: gpio::AnyPin<gpio::Output>,
        en_pin3: gpio::AnyPin<gpio::Output>,
        vsense_pin: gpio::Pin<'B', 14>,
        adc1: adc::Adc<pac::ADC1, adc::Configured>,
        adc2: adc::Adc<pac::ADC2, adc::Configured>,
        //adc3: adc::Adc<pac::ADC3, adc::Configured>,
        //adc4: adc::Adc<pac::ADC4, adc::Configured>,
        //opamp1: opamp::Pga<opamp::Opamp1, gpio::PA1, gpio::PA2>,
        ch0_opamp: opamp::Pga<opamp::Opamp1, gpio::PA1, opamp::InternalOutput>,
        opamp3: opamp::Pga<opamp::Opamp3, gpio::PB0, gpio::PB1>,
        //ch2_opamp: opamp::Pga<opamp::Opamp2, gpio::PA7, opamp::InternalOutput>,
        ch2_opamp: opamp::Pga<opamp::Opamp2, gpio::PA7, gpio::PA6>,
        opamp4: opamp::Pga<opamp::Opamp4, gpio::PB11, gpio::PB12>,
        sense0: gpio::PA2<gpio::Analog>,
        sense1: gpio::PA6<gpio::Analog>,
        sense2: gpio::PB1<gpio::Analog>,
        sense3: gpio::PB12<gpio::Analog>,
        charge_pump_pwm: pwm::Pwm<pac::TIM3, pwm::C4, pwm::ComplementaryImpossible, pwm::ActiveHigh, pwm::ActiveHigh>,
    ) -> Self {
        // assume opamps configured correctly...
        PowerController {
            pwr_channel: [
                en_pin0,
                en_pin1,
                en_pin2,
                en_pin3,
            ],
            opamps: (
                ch0_opamp,
                opamp3,
                ch2_opamp,
                opamp4,
            ),
            sense_pins: (
                sense0,
                sense2,
                sense1,
                sense3
            ),
            controller_state: PwrCtrlState::Active,
            channel_state: [
                PwrChanState::Disabled,
                PwrChanState::Fault,
                PwrChanState::Disabled,
                PwrChanState::Fault,
            ],
            vsense_pin,
            adc1,
            adc2,
            //adc3,
            //adc4,
            charge_pump_pwm,
        }
    }

    #[inline(always)]
    pub fn tick(&mut self) {
        const OPAMP_GAIN: u32 = 64;
        const SENSE_RESISTOR_MILLIOHM: u32 = 1;
        const RESOLUTION_BITS: usize = 12;
        const VREFINT_CALREF_MILLIVOLT: u32 = 3000;
        // TODO: add a way of getting the calibration value easily to HAL
        const VREFINT_CAL_ADDR: *mut u16 = 0x1FFF_75AA as *mut u16;
        // fetch VDDA every tick? screw it, sure
        let vref_data: u32 = self.adc1.convert(&adc::Vref, adc::config::SampleTime::Cycles_24_5) as u32;
        let vrefint_cal: u32 = unsafe {
            let value = VREFINT_CAL_ADDR.read_volatile();
            value as u32
        };
        let vdda_millivolt: u32 = VREFINT_CALREF_MILLIVOLT * vrefint_cal / vref_data;

        // this technically has worst-case error of +/- 1500 mA for 1 mOhm resistor due to input offset voltage,
        // but seems to be accurate to within maybe +/- 500 mA in practice
        let sample_to_milliamps = |sample: u32| -> u32 {
            sample * vdda_millivolt / (1 << RESOLUTION_BITS) * 1_000 / (SENSE_RESISTOR_MILLIOHM * OPAMP_GAIN)
        };

        // TODO: Check for overcurrent conditions and trip fault if so
        // TODO: test ch1, ch3
        // TODO: calculate and verify sample time so we have some filtering but also don't miss deadlines
        let ch0_sample = self.adc1.convert(&self.opamps.0, SampleTime::Cycles_640_5) as u32;
        //let ch0_sample = self.adc1.convert(&self.sense_pins.0, SampleTime::Cycles_640_5) as u32;
        let ch1_sample = self.adc1.convert(&self.sense_pins.1, SampleTime::Cycles_640_5) as u32;
        let ch2_sample = self.adc2.convert(&self.sense_pins.2, SampleTime::Cycles_640_5) as u32;
        let ch3_sample = self.adc1.convert(&self.sense_pins.3, SampleTime::Cycles_640_5) as u32;

        const VBUS_DIVIDER_DENOM: u32 = 23; // 10k - 220k divider
        let vbus_sample = self.adc1.convert(&self.vsense_pin, SampleTime::Cycles_640_5) as u32;

        defmt::trace!("CH0 {} mA\tCH1 {} mA\tCH2 {} mA\tCH3 {} mA\tVBUS {} mV\tVDDA {} mV",
            sample_to_milliamps(ch0_sample),
            sample_to_milliamps(ch1_sample),
            sample_to_milliamps(ch2_sample),
            sample_to_milliamps(ch3_sample),
            vbus_sample * vdda_millivolt * VBUS_DIVIDER_DENOM / (1 << RESOLUTION_BITS),
            vdda_millivolt
        );
    }

    #[inline(always)]
    pub fn charge_pump_enable(&mut self) {
        //if self.controller_state == PwrCtrlState::
        self.charge_pump_pwm.enable();
    }

    #[inline(always)]
    pub fn charge_pump_disable(&mut self) {
        //if self.controller_state == PwrCtrlState::
        self.charge_pump_pwm.disable();
    }

    #[inline(always)]
    pub fn enable(&mut self, pwr_chan: PwrChan) {
        let idx = pwr_chan as usize;
        // Do not enable if in fault state
        // Note that the power channels are active-low
        if self.channel_state[idx] != PwrChanState::Fault {
            self.pwr_channel[idx].set_low();
            self.channel_state[idx] = PwrChanState::Enabled;
        } else {
            defmt::warn!("Can't enable CH{} due to fault state", pwr_chan as u8);
        }
    }

    #[inline(always)]
    pub fn disable(&mut self, pwr_chan: PwrChan) {
        let idx = pwr_chan as usize;
        // Do not change state if in fault state
        if self.channel_state[idx] != PwrChanState::Fault {
            self.pwr_channel[idx].set_high();
            self.channel_state[idx] = PwrChanState::Disabled;
        } else {
            defmt::warn!("Tried to enable CH{} in fault state", pwr_chan as u8);
        }
    }

    #[inline(always)]
    pub fn status(&self, pwr_chan: PwrChan) -> PwrChanState {
        self.channel_state[pwr_chan as usize]
    }

    #[inline(always)]
    pub fn enable_all(&mut self) {
        for chan in [PwrChan::CH0, PwrChan::CH1, PwrChan::CH2, PwrChan::CH3] {
            self.enable(chan);
        }
    }

    #[inline(always)]
    pub fn disable_all(&mut self) {
        for chan in [PwrChan::CH0, PwrChan::CH1, PwrChan::CH2, PwrChan::CH3] {
            self.disable(chan);
        }
    }
}
