use hal::gpio::{Alternate, Pin};
use cortex_m::prelude::*;
use hal::pwm::{self, Pwm, PwmAdvExt};
use hal::{pac::TIM1, rcc::Rcc};
use hal::time::{ExtU32, RateExtU32};
use stm32g4xx_hal as hal;

use crate::config::IdleMode;
use crate::util::motor_disable;

pub struct STM32G4xxSixPwmDriver {
    ph_a: Pwm<TIM1, pwm::C1, pwm::ComplementaryEnabled, pwm::ActiveHigh, pwm::ActiveHigh>,
    ph_b: Pwm<TIM1, pwm::C2, pwm::ComplementaryEnabled, pwm::ActiveHigh, pwm::ActiveHigh>,
    ph_c: Pwm<TIM1, pwm::C3, pwm::ComplementaryEnabled, pwm::ActiveHigh, pwm::ActiveHigh>,
}

impl STM32G4xxSixPwmDriver {
    // Cap max duty across all phases since we don't have current sense on phase C
    // -> we always need to be able to measure A & B currents (on low-side!)
    // -> A or B can never reach 100% duty (to reconstruct Iq and Id)
    // -> Cap all of them because capping just A&B would be cursed (torque ripple)
    // 168MHz / 28kHz = 6000, 100 derived in README.md
    const PWM_SENSE_LIMIT: f32 = 6000.0 - 100.0;

    pub fn setup(
        rcc: &mut Rcc,
        mut tim: TIM1,
        pin_ah: Pin<'C', 0, Alternate<2>>,
        pin_al: Pin<'A', 7, Alternate<6>>,
        pin_bh: Pin<'C', 1, Alternate<2>>,
        pin_bl: Pin<'B', 0, Alternate<6>>,
        pin_ch: Pin<'C', 2, Alternate<2>>,
        pin_cl: Pin<'B', 1, Alternate<6>>,
        mode: IdleMode,
    ) -> Self {
        // configure all the timer crap before we make it PWM

        // The G4 HAL takes alignment into account so we don't have to do the 28 kHz silly
        // ⚠️ Change dead_time and PWM_SENSE_LIMIT if changing this! ⚠️
        let (mut _pwm_mgr, (ch1, ch2, ch3)) = tim.pwm_advanced(
            // PC0 = C1, PC1 = C2, PC2 = C3
            (pin_ah, pin_bh, pin_ch),
            rcc
        ).frequency(14.kHz())
            .center_aligned()
            .with_deadtime(298.nanos()) // see README
            .finalize();
        motor_disable();

        assert!(ch1.get_max_duty() == 6000);
        assert!(ch2.get_max_duty() == 6000);
        assert!(ch3.get_max_duty() == 6000);

        // TIM1 is both eaten and reset by PWM HAL, so we have to use unsafe to set these TIM1
        // fields for our own nefarious purposes here.
        unsafe {
            let mut tim = hal::pac::Peripherals::steal().TIM1;
            // trig UEV on *full* pwm cycle (1 bcuz center aligned :sob:)
            tim.rcr().write(|w| w.rep().set(1));
            // Enable update interrupts on the underlying timer for commutation handler to run.
            // TIM1's NVIC is unmasked in main.
            tim.dier().write(|w| w.uie().enabled());
            // set idle mode
            Self::set_idle_mode(mode, &mut tim);
            // off-state selection for idle mode: force to idle level (bit 1)
            tim.bdtr().modify(|_, w| w.ossi().idle_level());
        }

        // default is active high for both regular and complementary, which we want
        let mut pwm_c1 = ch1
            .into_complementary(pin_al);
        let mut pwm_c2 = ch2
            .into_complementary(pin_bl);
        let mut pwm_c3 = ch3
            .into_complementary(pin_cl);

        pwm_c1.enable();
        pwm_c2.enable();
        pwm_c3.enable();

        STM32G4xxSixPwmDriver {
            ph_a: pwm_c1,
            ph_b: pwm_c2,
            ph_c: pwm_c3,
        }
    }

    pub fn set_idle_mode(
        mode: IdleMode,
        tim1: &mut TIM1
    ) {
        // set idle state of C1, C2, C3 to reset (bit 0)
        tim1.cr2().modify(
            |_, w| w.ois1().bit(false)
            .ois2().bit(false)
            .ois3().bit(false)
        );

        match mode {
            IdleMode::Ground => {
                // set complementary idlestate
                tim1.cr2().modify(
                    |_, w| w.ois1n().bit(true)
                    .ois2n().bit(true)
                    .ois3n().bit(true)
                );
            }
            IdleMode::HiZ => {
                // reset complementary idlestate
                tim1.cr2().modify(
                    |_, w| w.ois1n().bit(false)
                    .ois2n().bit(false)
                    .ois3n().bit(false)
                );
            }
        }
    }

    // 1. x = x+1 --> shift [-1, 1] to [0, 2]
    // 2. mul (16 bit wide TIMx.ARR capped for curr sense) --> [0, 11800], uncapped is 12000
    //  - Cap all of them equally cause capping just 2 would be goofy asf
    // 3. div 2 --> [0, 5900] A.K.A [0%, 98.3%]
    pub fn set_duty(&mut self, duties: [f32; 3]) {
        let v_abc_norm = duties.map(|x| ((x + 1.0) * (Self::PWM_SENSE_LIMIT / 2.0)) as u16);

        self.ph_a.set_duty(v_abc_norm[0]);
        self.ph_b.set_duty(v_abc_norm[1]);
        self.ph_c.set_duty(v_abc_norm[2]);
    }
}
