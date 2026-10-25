use cortex_m::asm::delay;
use bitfield::bitfield;
use stm32g4xx_hal::{
    delay::CountDown, gpio::{self, AF6, Alternate, Input, Output, PA15, PC10, PC11, PC12, Pin}, hal::digital::OutputPin, pac::{self, SPI3}, prelude::SpiBus, rcc::Rcc, spi::{Mode, Phase, Polarity, Spi, SpiExt}, time::{ExtU32, RateExtU32}, timer::CountDownTimer,
};

use bitbang_hal::spi::{self, SPI, MODE_2};

struct JonahSPI {
    pub spi_ncs: PA15<gpio::Output>,
    pub spi_clk: Pin<'C', 10, Output>,
    pub spi_miso: Pin<'C', 12, Input>,
    pub spi_mosi: Pin<'C', 11, Output>,
}

/// implements MODE 2
impl JonahSPI {
    fn transfer_internal(&mut self, data: u16) -> u16 {
        self.spi_ncs.set_low();
        delay(10000);
        let mut result = 0;
        for i in (0..16).rev() {
            let bit = data & (1 << i);
            self.spi_mosi.set_state((bit != 0).into());
            self.spi_clk.set_high();
            delay(10000);
            result |= (if self.spi_miso.is_high() {1} else {0}) << i;
            self.spi_clk.set_low();
            delay(10000);
        }
        self.spi_ncs.set_high();
        delay(10000);
        result
    }
}

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
type BBSPI = SPI<PC12<gpio::Input>, PC11<gpio::Output>, PC10<gpio::Output>, CountDownTimer<stm32g4xx_hal::pac::TIM3>>;

pub struct DRV8301 {
    pin_nfault: Pin<'B', 7, Input>,
    spi: JonahSPI,
}

impl DRV8301 {
    const FLAG_WR: u16 = 0x0000;
    const FLAG_RD: u16 = 0x8000;

    const REG_STAT1: u16 = 0 << 11;
    const REG_STAT2: u16 = 1 << 11;

    const REG_CTRL1_ADDR: u16 = 2 << 11;
    // last two bits are drive strength
    const REG_CTRL1_VAL: u16 = 0b000_00_10_0_0_00;
    const REG_CTRL2: u16 = 3 << 11;

    pub fn setup(
        mut gate_en: Pin<'B', 5, Output>,
        nfault: Pin<'B', 7, Input>,
        mut spi_ncs: Pin<'A', 15, Output>,
        spi_clk: Pin<'C', 10, Output>,
        spi_miso: Pin<'C', 12, Input>,
        spi_mosi: Pin<'C', 11, Output>,
        spip: SPI3,
        rcc: &mut Rcc,
    ) -> DRV8301 {
        gate_en.set_low();
        delay(25_000_000);
        gate_en.set_high();
        delay(25_000_000);

        defmt::info!("[INFO] DRV.FAULT present? {}", nfault.is_low());

        let tim3 = unsafe { stm32g4xx_hal::pac::Peripherals::steal().TIM3 };
        let mut spi3 = JonahSPI {
            spi_ncs,
            spi_clk,
            spi_miso,
            spi_mosi,
        };

        defmt::info!("[INFO] DRV.Basic pin setup done.. Moving on to SPI");

        let mut _rxdata =
            Self::transfer_internal(&mut spi3, Self::FLAG_RD | Self::REG_STAT2);
        defmt::info!("[INFO] DRV.STATUS1 RECV: {:#?}", _rxdata);
        _rxdata = Self::transfer_internal(&mut spi3, Self::FLAG_RD);
        defmt::info!("[INFO] DRV.STATUS2 RECV: {:#?}", _rxdata);

        // Disable OCP (leave reporting enabled though) & Use lowest gate drive current
        Self::transfer_internal(
            &mut spi3,
            Self::FLAG_WR | Self::REG_CTRL1_ADDR | Self::REG_CTRL1_VAL
        );
        Self::transfer_internal(&mut spi3, Self::FLAG_RD | Self::REG_CTRL1_ADDR);
        _rxdata = Self::transfer_internal(&mut spi3, Self::FLAG_RD);
        defmt::info!("[INFO] DRV.CTRL1 RECV: {:#?}", _rxdata);
        assert_eq!(_rxdata, Self::REG_CTRL1_ADDR | Self::REG_CTRL1_VAL);

        Self::transfer_internal(&mut spi3, Self::FLAG_RD | Self::REG_CTRL2);
        _rxdata = Self::transfer_internal(&mut spi3, Self::FLAG_RD);
        defmt::info!("[INFO] DRV.CTRL2 RECV: {:#?}", _rxdata);

        DRV8301 {
            pin_nfault: nfault,
            spi: spi3,
        }
    }

    // true == has fault
    pub fn has_fault(&self) -> bool {
        self.pin_nfault.is_low()
    }

    pub fn transfer(&mut self, data: u16) -> u16 {
        self.spi.transfer_internal(data)
    }

    fn transfer_internal(spi: &mut JonahSPI, data: u16) -> u16 {
        spi.transfer_internal(data)
    }
}
