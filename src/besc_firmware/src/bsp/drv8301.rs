use cortex_m::asm::delay;
use bitfield::bitfield;
use stm32g4xx_hal::{
    gpio::{self, AF6, Alternate, Input, Output, PC10, PC11, PC12, Pin},
    hal::digital::OutputPin,
    pac::{self, SPI3},
    prelude::SpiBus,
    rcc::Rcc,
    spi::{Mode, Phase, Polarity, Spi, SpiExt},
    time::RateExtU32,
};

use bitbang_hal::spi::{self, SPI, MODE_2};

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

    // gate current 0.7 A
    const REG_CTRL1: u16 = 0b0001_0000_0000_0001;
    const REG_CTRL2: u16 = 3 << 11;

    pub fn setup(
        mut gate_en: Pin<'B', 5, Output>,
        nfault: Pin<'B', 7, Input>,
        mut spi_ncs: Pin<'A', 15, Output>,
        spi_clk: Pin<'C', 10, Output>,
        spi_miso: Pin<'C', 11, Input>,
        spi_mosi: Pin<'C', 12, Output>,
        spip: SPI3,
        rcc: &mut Rcc,
    ) -> DRV8301 {
        gate_en.set_low();
        delay(25_000_000);
        gate_en.set_high();
        delay(25_000_000);

        defmt::println!("[INFO] DRV.FAULT present? {}", nfault.is_low());

        let tim3 = unsafe { stm32g4xx_hal::pac::TIM3::steal() };
        let mut spi3 = SPI::new(MODE_2, spi_miso, spi_mosi, spi_clk, tim3);

        defmt::println!("[INFO] DRV.Basic pin setup done.. Moving on to SPI");

        spi3.send(Self::FLAG_RD | Self::REG_STAT1);
        let mut _rxdata =
            Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD | Self::REG_STAT2);
        defmt::println!("[INFO] DRV.STATUS1 RECV: {:#?}", _rxdata);
        _rxdata = Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD);
        defmt::println!("[INFO] DRV.STATUS2 RECV: {:#?}", _rxdata);

        // Disable OCP (leave reporting enabled though) & Use lowest gate drive current
        Self::transfer_internal(
            &mut spi3,
            &mut spi_ncs,
            Self::FLAG_WR | Self::REG_CTRL1 | (2u16 << 4),
        );
        Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD | Self::REG_CTRL1);
        _rxdata = Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD);
        defmt::println!("[INFO] DRV.CTRL1 RECV: {:#?}", _rxdata);

        Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD | Self::REG_CTRL2);
        _rxdata = Self::transfer_internal(&mut spi3, &mut spi_ncs, Self::FLAG_RD);
        defmt::println!("[INFO] DRV.CTRL2 RECV: {:#?}", _rxdata);

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
}
