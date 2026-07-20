use stm32g4xx_hal::{
    adc,
    gpio::{self},
    pac,
};

// How often the "tick" function should be called in the superloop
pub const PWR_CTRL_TICK_US: u32 = 2_000; // 2 ms

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

impl PowerChannel {
    fn new(
        en_pin: gpio::AnyPin<gpio::Output>, 
        sense_pin: gpio::AnyPin<gpio::Input>
    ) -> Self {
        PowerChannel { enable: en_pin, sense: sense_pin }
    }
}

pub struct PowerController {
    pwr_channel: [gpio::AnyPin<gpio::Output>; 4],
    pub controller_state: PwrCtrlState,
    pub channel_state: [PwrChanState; 4],
}

impl PowerController {
    pub fn new(
        en_pin0: gpio::AnyPin<gpio::Output>,
        en_pin1: gpio::AnyPin<gpio::Output>,
        en_pin2: gpio::AnyPin<gpio::Output>,
        en_pin3: gpio::AnyPin<gpio::Output>,
        // sense_pin0: gpio::AnyPin<gpio::Input>,
        // sense_pin1: gpio::AnyPin<gpio::Input>,
        // sense_pin2: gpio::AnyPin<gpio::Input>,
        // sense_pin3: gpio::AnyPin<gpio::Input>
    ) -> Self {
        PowerController {
            pwr_channel: [
                en_pin0,
                en_pin1,
                en_pin2,
                en_pin3,
            ],
            controller_state: PwrCtrlState::Active,
            channel_state: [PwrChanState::Disabled; 4],
        }
    }

    #[inline(always)]
    pub fn tick(&mut self) {
        // TODO: check for overcurrent conditions...
    }

    #[inline(always)]
    pub fn enable(&mut self, pwr_chan: PwrChan) {
        self.pwr_channel[pwr_chan as usize].set_low();
        self.channel_state[pwr_chan as usize] = PwrChanState::Enabled;
    }

    #[inline(always)]
    pub fn disable(&mut self, pwr_chan: PwrChan) {
        self.pwr_channel[pwr_chan as usize].set_high();
        self.channel_state[pwr_chan as usize] = PwrChanState::Disabled;
    }

    #[inline(always)]
    pub fn status(&self, pwr_chan: PwrChan) -> PwrChanState {
        self.channel_state[pwr_chan as usize]
    }

    #[inline(always)]
    pub fn enable_all(&mut self) {
        // This enables all the output channels by letting going low
        // and letting the gate open on the NPN

        // Code for struct array
        // self.pwr_channel[0].enable.set_low();
        // self.pwr_channel[1].enable.set_low();
        // self.pwr_channel[2].enable.set_low();
        // self.pwr_channel[3].enable.set_low();

        for i in 0..NUM_CHANNELS {
            self.pwr_channel[i].set_low();
            self.channel_state[i] = PwrChanState::Enabled;
        }
    }

    #[inline(always)]
    pub fn disable_all(&mut self) {
        // Similar but disabling instead via inverse

        // Code for struct array
        // self.pwr_channel[0].enable.set_high();
        // self.pwr_channel[1].enable.set_high();
        // self.pwr_channel[2].enable.set_high();
        // self.pwr_channel[3].enable.set_high();

        for i in 0..NUM_CHANNELS {
            self.pwr_channel[i].set_high();
            self.channel_state[i] = PwrChanState::Disabled;
        }
    }
}

pub struct AdcController {
    pub adc1: adc::Adc<pac::ADC1, adc::Configured>,
    pub adc2: adc::Adc<pac::ADC2, adc::Configured>,
    pub adc3: adc::Adc<pac::ADC3, adc::Configured>,
    pub adc4: adc::Adc<pac::ADC4, adc::Configured>
}


