// FIXME!

use cortex_m::asm::delay;
use crate::stepper_bus::Channel;
use stm32g4xx_hal::{
    gpio::{AnyPin, Output, AF5, PB13, PB14, PB15},
    pac::SPI2,
    prelude::SpiBus,
    rcc::Rcc,
    spi::{self, Spi, SpiExt, MODE_1},
    time::RateExtU32,
};

use crate::as_registers::Register;

pub type EncoderSpiPins = (PB13<AF5>, PB14<AF5>, PB15<AF5>);

pub struct EncoderNcsPins(
    pub AnyPin<Output>,
    pub AnyPin<Output>,
    pub AnyPin<Output>,
    pub AnyPin<Output>,
);

pub struct EncoderBus {
    spi_bus: Spi<SPI2, EncoderSpiPins>,
    ncs_pins: EncoderNcsPins,
}

impl EncoderBus {
    pub fn new(
        spi_bus: SPI2,
        spi_pins: EncoderSpiPins,
        ncs_pins: EncoderNcsPins,
        rcc: &mut Rcc,
    ) -> Self {
        Self {
            spi_bus: spi_bus.spi(spi_pins, MODE_1, 1.MHz(), rcc),
            ncs_pins: ncs_pins,
        }
    }

    /// Helper to add some delay between CS transitions
    fn set_ncs(&mut self, channel: Channel, high: bool) {
        let pin = match channel {
            Channel::CH0 => &mut self.ncs_pins.0,
            Channel::CH1 => &mut self.ncs_pins.1,
            Channel::CH2 => &mut self.ncs_pins.2,
            Channel::CH3 => &mut self.ncs_pins.3,
        };
        delay(1024);
        if high {
            pin.set_high();
        } else {
            pin.set_low();
        }
        delay(1024);
    }

    fn pack_frame(value: u16) -> [u8; 2] {
        let mut frame = value.to_be_bytes();
        if value.count_ones() % 2 == 1 {
            frame[0] |= 0x80;
        }

        frame
    }

    fn unpack_frame(frame: [u8; 2]) -> u16 {
        u16::from_be_bytes(frame) & 0x7F00
    }

    pub fn write_reg<R: Register>(&mut self, channel: Channel, value: R) -> Result<R, spi::Error> {
        self.set_ncs(channel, false);
        self.spi_bus
            .write(Self::pack_frame(R::ADDRESS).as_slice())?;
        self.set_ncs(channel, true);

        let mut res = [0u8; 2];
        self.set_ncs(channel, false);
        self.spi_bus
            .transfer(&mut res, Self::pack_frame(value.into()).as_slice())?;
        self.set_ncs(channel, true);

        Ok(R::from(Self::unpack_frame(res)))
    }

    pub fn read_reg<R: Register>(&mut self, channel: Channel) -> Result<R, spi::Error> {
        self.set_ncs(channel, false);
        self.spi_bus
            .write(Self::pack_frame(R::ADDRESS | 0x4000).as_slice())?;
        self.set_ncs(channel, true);

        let mut res = [0u8; 2];
        self.set_ncs(channel, false);
        self.spi_bus.transfer(&mut res, &[0x40, 0x00])?;
        self.set_ncs(channel, true);

        Ok(R::from(Self::unpack_frame(res)))
    }
}
