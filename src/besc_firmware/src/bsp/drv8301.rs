use cortex_m::asm::delay;
use bitfield::bitfield;
use stm32g4xx_hal::{
    gpio::{AF6, Alternate, Input, Output, PC10, PC11, PC12, Pin},
    hal::digital::OutputPin,
    pac::SPI3,
    prelude::SpiBus,
    rcc::Rcc,
    spi::{Mode, Phase, Polarity, Spi, SpiExt},
    time::RateExtU32,
};

use embedded_common::dprintln;

bitfield! {
    pub struct RegStatus1(u16);
    impl Debug;

    pub fault, _ : 10;
    pub gvdd_uv, _ : 9;
    pub pvdd_uv, _ : 8;
    pub otsd, _ : 7;
    pub otw, _ : 6;
    pub fetha_oc, _ : 5;
    pub fetla_oc, _ : 4;
    pub fethb_oc, _ : 3;
    pub fetlb_oc, _ : 2;
    pub fethc_oc, _ : 1;
    pub fetlc_oc, _ : 0;
}

bitfield! {
    pub struct RegStatus2(u16);
    impl Debug;

    pub gvdd_ov, _ : 7;
    pub device_id, _ : 3, 0;
}

bitfield! {
    pub struct RegCtrl1(u16);
    impl Debug;

    pub ocp_adj_set, _ : 10,6;
    pub ocp_mode, _ : 5,4;
    pub pwm_mode, _ : 3;
    pub gate_reset, _ : 2;
    pub gate_current, _ : 1,0;
}

bitfield::bitfield! {
    pub struct RegCtrl2(u16);
    impl Debug;

    pub oc_toff, _ : 6;
    pub dc_cal_ch2, _ : 5;
    pub dc_cal_ch1, _ : 4;
    pub gain, _ : 3,2;
    pub octw_mode, _ : 1,0;
}

type DRV8301Pins = (PC10<AF6>, PC11<AF6>, PC12<AF6>); // SCK, MISO, MOSI

pub struct DRV8301 {
    pin_spi_ncs: Pin<'A', 15, Output>,
    pin_nfault: Pin<'B', 7, Input>,
    spi: Spi<SPI3, DRV8301Pins>,
}

impl DRV8301 {
    const FLAG_WR: u16 = 0x0000;
    const FLAG_RD: u16 = 0x8000;

    const REG_STAT1: u16 = 0 << 11;
    const REG_STAT2: u16 = 1 << 11;
    const REG_CTRL1: u16 = 2 << 11;
    const REG_CTRL2: u16 = 3 << 11;

    pub fn setup(
        mut gate_en: Pin<'B', 5, Output>,
        nfault: Pin<'B', 7, Input>,
        mut spi_ncs: Pin<'A', 15, Output>,
        spi_clk: Pin<'C', 10, Alternate<6>>,
        spi_miso: Pin<'C', 11, Alternate<6>>,
        spi_mosi: Pin<'C', 12, Alternate<6>>,
        spip: SPI3,
        rcc: &mut Rcc,
    ) -> DRV8301 {
        gate_en.set_low();
        delay(250_000);
        gate_en.set_high();
        delay(250_000);

        dprintln!(0, "[INFO] DRV.FAULT present? {}", nfault.is_low());

        let spi_mode = Mode {
            polarity: Polarity::IdleLow,
            phase: Phase::CaptureOnSecondTransition,
        };

        let mut spi3 = spip.spi(
            (spi_clk, spi_miso, spi_mosi),
            spi_mode,
            1.MHz(),
            rcc,
        );

        dprintln!(0, "[INFO] DRV.Basic pin setup done.. Moving on to SPI");

        Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD | Self::REG_STAT1);
        let mut _rxdata =
            Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD | Self::REG_STAT2);
        dprintln!(0, "[INFO] DRV.STATUS1 RECV: {:#?}", RegStatus1(_rxdata));
        _rxdata = Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD);
        dprintln!(0, "[INFO] DRV.STATUS2 RECV: {:#?}", RegStatus2(_rxdata));

        // Disable OCP (leave reporting enabled though) & Use lowest gate drive current
        Self::transfer_internal(
            &mut spi3,
            &mut spi_ncs,
            Self::FLAG_WR | Self::REG_CTRL1 | (2u16 << 4),
        );
        Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD | Self::REG_CTRL1);
        _rxdata = Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD);
        dprintln!(0, "[INFO] DRV.CTRL1 RECV: {:#?}", RegCtrl1(_rxdata));

        Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD | Self::REG_CTRL2);
        _rxdata = Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD);
        dprintln!(0, "[INFO] DRV.CTRL2 RECV: {:#?}", RegCtrl2(_rxdata));

        DRV8301 {
            pin_spi_ncs: spi_ncs,
            pin_nfault: nfault,
            spi: spi3,
        }
    }

    // true == has fault
    pub fn has_fault(&self) -> bool {
        self.pin_nfault.is_low()
    }

    pub fn transfer(&mut self, data: u16) -> u16 {
        Self::transfer_internal(&mut self.spi, &mut self.pin_spi_ncs, data)
    }

    fn transfer_internal<E: stm32g4xx_hal::hal::digital::Error>(
        spi: &mut Spi<SPI3, DRV8301Pins>,
        ncs: &mut dyn OutputPin<Error = E>,
        data: u16,
    ) -> u16 {
        let mut data: [u8; 2] = data.to_be_bytes();

        let _ = ncs.set_low();
        delay(100);
        let _ = spi.transfer_in_place(&mut data);
        delay(100);
        let _ = ncs.set_high();

        u16::from_be_bytes([data[0], data[1]])
    }
}
