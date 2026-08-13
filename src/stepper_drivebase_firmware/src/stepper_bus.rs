use bitfield_struct::bitfield;
use cortex_m::asm::delay;
use stm32g4xx_hal::{
    gpio::{self, AF6, AnyPin, Output, PC10, PC11, PC12},
    pac::SPI3,
    prelude::*,
    rcc::{self, Rcc},
    spi::{self, MODE_3, Spi},
    time::RateExtU32,
};

use embedded_common::tmc_registers::{DrvStatus, GStat, Register};

#[derive(Debug, Clone, Copy)]
pub enum Channel {
    CH0,
    CH1,
    CH2,
    CH3,
}

pub const ALL_CHANNELS: [Channel; 4] = [Channel::CH0, Channel::CH1, Channel::CH2, Channel::CH3];

#[bitfield(u8)]
pub struct SpiFlags {
    #[bits(1)]
    pub reset_flag: bool,

    #[bits(1)]
    pub driver_error: bool,

    #[bits(1)]
    pub sg2: bool,

    #[bits(1)]
    pub standstill: bool,

    #[bits(1)]
    pub velocity_reached: bool,

    #[bits(1)]
    pub position_reached: bool,

    #[bits(1)]
    pub status_stop_l: bool,

    #[bits(1)]
    pub status_stop_r: bool,
}

impl SpiFlags {
    pub fn ok(&self) -> bool {
        !self.reset_flag() && !self.driver_error() && !self.sg2()
    }
}

// SCK, MISO, MOSI
pub type StepperSpiPins = (PC10<AF6>, PC11<AF6>, PC12<AF6>);

pub struct StepperNcsPins(
    pub AnyPin<Output>,
    pub AnyPin<Output>,
    pub AnyPin<Output>,
    pub AnyPin<Output>,
);

pub type EnnPin = gpio::PD2<gpio::Output>;
pub type ClkPin = gpio::PA8<gpio::Analog>; // MCO
pub type DiagPin = gpio::PA10<gpio::Input>;

pub struct StepperBus {
    spi_bus: Spi<SPI3, StepperSpiPins>,
    ncs_pins: StepperNcsPins,

    _clk: rcc::Mco<gpio::PA8<gpio::AF0>>,
    enn: EnnPin,
    #[allow(dead_code)]
    diag: DiagPin,
}

impl StepperBus {
    pub fn new(
        spi_bus: SPI3,
        spi_pins: StepperSpiPins,
        ncs_pins: StepperNcsPins,

        clk_pin: ClkPin,
        enn: EnnPin,
        diag: DiagPin,

        rcc: &mut Rcc,
    ) -> Self {
        // initialise the MCO (CLK pin) at 12 MHz
        let clk = clk_pin.mco(rcc::MCOSrc::HSE, rcc::Prescaler::Div2, rcc);
        clk.enable();

        Self {
            spi_bus: spi_bus.spi(spi_pins, MODE_3, 1.MHz(), rcc),
            ncs_pins: ncs_pins,

            _clk: clk,
            enn,
            diag,
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

    fn pack_frame(addr: u8, value: u32) -> [u8; 5] {
        let mut frame = [0u8; 5];
        frame[0..1].copy_from_slice(&addr.to_be_bytes());
        frame[1..5].copy_from_slice(&value.to_be_bytes());
        frame
    }

    fn unpack_frame(frame: [u8; 5]) -> (u8, u32) {
        (
            frame[0],
            u32::from_be_bytes(*frame[1..5].as_array().unwrap()),
        )
    }

    pub fn write_reg<R: Register>(
        &mut self,
        channel: Channel,
        value: R,
    ) -> Result<SpiFlags, spi::Error> {
        self.set_ncs(channel, false);
        let mut res = [0u8; 5];
        // Set the MSB to indicate a write
        self.spi_bus
            .transfer(&mut res, &Self::pack_frame(R::ADDRESS | 0x80, value.into()))?;
        self.set_ncs(channel, true);

        Ok(SpiFlags(res[0]))
    }
    // TMC5160 SPI interface sends data back on the *subsequent* read. A fun exercise would be to
    // write a pipelined API to optimise read chains using the Typestate pattern. However, we don't
    // actually do any reads in any significant quantity, so this probably wouldn't be worth it.
    pub fn read_reg<R: Register>(&mut self, channel: Channel) -> Result<(SpiFlags, R), spi::Error> {
        self.set_ncs(channel, false);
        self.spi_bus.write(&Self::pack_frame(R::ADDRESS, 0))?;
        // embedded_hal gotcha: writes may return BEFORE they've actually written out all data,
        // so we need to flush before setting CS high again.
        <spi::Spi<SPI3, StepperSpiPins> as SpiBus<u8>>::flush(&mut self.spi_bus)?;
        self.set_ncs(channel, true);

        self.set_ncs(channel, false);
        let mut res = [0u8; 5];
        self.spi_bus.transfer(&mut res, &[0u8; 5])?;
        self.set_ncs(channel, true);

        let (flags, value) = Self::unpack_frame(res);
        Ok((SpiFlags(flags), R::from(value)))
    }

    /// Enables all drivebase channels. Must be called before any actuator commands (e.g.
    /// `set_position()`) in order to actually cause the motors to move. See also `disable_all()`.
    pub fn enable_all(&mut self) {
        self.enn.set_low();
    }

    /// Disables all drivebase channels. Actuator commands (e.g. `set_position()`) will have no
    /// effect until a call to `enable_all()`.
    #[allow(unused)]
    pub fn disable_all(&mut self) {
        self.enn.set_high();
    }

    fn append_health(health: &mut Option<u8>, cond: bool, code: u8) {
        if !cond {
            return;
        }

        *health = Some(if let Some(curr) = *health {
            if curr >= 100 { curr } else { curr + 100 }
        } else {
            code
        });
    }

    pub fn health(&mut self) -> Option<u8> {
        let mut health = None;
        for (&chan, prefix) in ALL_CHANNELS.iter().zip([0, 10, 20, 30]) {
            let (_, gstat) = self.read_reg::<GStat>(chan).unwrap();
            Self::append_health(&mut health, gstat.reset(), prefix + 0);
            Self::append_health(&mut health, gstat.uv_cp(), prefix + 1);
            if !gstat.drv_err() {
                continue;
            }

            let (_, dstat) = self.read_reg::<DrvStatus>(chan).unwrap();
            Self::append_health(&mut health, dstat.ot(), prefix + 2);
            Self::append_health(&mut health, dstat.otpw(), prefix + 3);

            Self::append_health(&mut health, dstat.s2ga(), prefix + 4);
            Self::append_health(&mut health, dstat.s2gb(), prefix + 5);
            Self::append_health(&mut health, dstat.s2vsa(), prefix + 6);
            Self::append_health(&mut health, dstat.s2vsb(), prefix + 7);

            Self::append_health(&mut health, dstat.ola(), prefix + 8);
            Self::append_health(&mut health, dstat.olb(), prefix + 9);
        }

        health
    }
}

