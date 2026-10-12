#![feature(unsafe_cell_access)]
#![feature(core_intrinsics)]
#![feature(never_type)]
#![no_std]
#![no_main]
#![feature(more_float_constants)]
#![feature(const_trait_impl)]

use core::convert::TryInto;
use core::f32::consts;
use core::intrinsics::breakpoint;
use core::ptr;
use core::{cell::UnsafeCell, mem::MaybeUninit};

use canadensis_data_types::reg::udral::service::actuator;
use canadensis_data_types::uavcan::node::health_1_0::Health;
use canadensis_data_types::uavcan::node::mode_1_0::Mode;

use canadensis::{
    core::time::Clock,
    node::{
        self,
        data_types::{GetInfoResponse, Version},
        BasicNode, CoreNode,
    },
    requester::TransferIdFixedMap,
    Node,
};
use canadensis_can::{CanNodeId, CanReceiver, CanTransmitter, CanTransport};
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use embedded_alloc::LlffHeap as Heap;
use embedded_common::{
    argb::{self, Colour},
    can::CanDriver,
    clock,
    debug::{itm_send_raw, setup_itm},
    dprintln,
};
use panic_semihosting as _;

use stm32g4xx_hal::pac::TIM5;
use stm32g4xx_hal::{
    gpio::{GpioExt, PinState, Speed},
    interrupt,
    pac::TIM1,
    prelude::*,
    pwr::{PwrExt, VoltageScale},
    rcc::*,
    signature::Uid,
    time::{ExtU32, RateExtU32},
};

extern crate alloc;
pub mod bsp;
pub mod comms;
pub mod config;
pub mod state;
pub mod util;

// ARGB LED constants
const RED: Colour = Colour { r: 255, g: 0, b: 0 };
const YELLOW: Colour = Colour { r: 200, g: 200, b: 0 };
const GREEN: Colour = Colour { r: 0, g: 255, b: 0 };
const BLUE: Colour = Colour { r: 0, g: 0, b: 255 };
const DEFAULT_BRIGHTNESS: u8 = 15;

use crate::state::CommutationMode;
use crate::util::{
    foc::{inverse_park, modulate_spacevector, RotatingReferenceFrame},
};
use crate::util::{motor_disable, motor_enable};
use crate::{
    bsp::{drv8301::DRV8301, six_pwm::STM32G4xxSixPwmDriver},
    comms::{CommSystem, CYPHAL_CONCURRENT_TRANSFERS, CYPHAL_NUM_SERVICES, CYPHAL_NUM_TOPICS},
    config::presets::*,
    state::CommutationState,
};

static G_COM_STATE: Mutex<UnsafeCell<CommutationState>> =
    Mutex::new(UnsafeCell::new(CommutationState {
        mode: CommutationMode::Disabled,
        halls: MaybeUninit::uninit(),
        pwm_driver: MaybeUninit::uninit(),
        glitch_accum: 0,
        hall_state: 0.0,
        full_revs: 0,
        velocity_raw: 0.0,
    }));

// Global allocator -- required by canadensis.
#[global_allocator]
static G_HEAP: Heap = Heap::empty();

fn initialise_allocator() {
    use core::mem::MaybeUninit;
    const HEAP_SIZE: usize = 0x5000;
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { G_HEAP.init(&raw mut HEAP_MEM as usize, HEAP_SIZE) }
}

///////////
// Main. //
///////////
#[entry]
fn main() -> ! {
    initialise_allocator();

    let config = DRIVEBASE_BR_CONFIG;

    // Embedded boilerplate...
    let mut cp = cortex_m::Peripherals::take().unwrap();
    let mut dp = stm32g4xx_hal::pac::Peripherals::take().unwrap();
    let pwr = dp
        .PWR
        .constrain()
        .vos(VoltageScale::Range1 { enable_boost: true })
        .freeze();

    let mut rcc = dp.RCC.freeze(
        Config::pll()
            .pll_cfg(PllConfig {
                mux: PllSrc::HSE(24.MHz()),
                m: PllMDiv::DIV_2,
                n: PllNMul::MUL_28,
                r: Some(PllRDiv::DIV_2),
                q: Some(PllQDiv::DIV_2),
                p: None,
            })
            .fdcan_src(FdCanClockSource::PLLQ),
        pwr
    );

    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);

    // DO NOT MAKE THESE OUTPUTS ON REV. 2P0
    gpiob.pb8.into_input();
    gpiob.pb9.into_analog();

    let mut argb = argb::Controller::new(
        dp.USART1,
        gpioc.pc4.into_alternate(),
        DEFAULT_BRIGHTNESS,
        &mut rcc
    );

    // Set up debugging support
    unsafe {
        setup_itm(&mut cp.DCB, &mut cp.DWT, &mut dp.DBGMCU, &mut cp.ITM);
    }
    dp.DBGMCU.apb2_fz().modify(|_, w| {
        w.dbg_tim1_stop().set_bit() // Stops counting AND disables outputs!
    });

    // the chain here is right-to-left on the PCB
    argb.display(&[RED, RED, RED]);

    let drv = DRV8301::setup(
        gpiob.pb5.into_push_pull_output_in_state(PinState::Low),
        gpiob.pb7.into_floating_input(),
        gpioa.pa15.into_push_pull_output_in_state(PinState::High),
        gpioc.pc10.into_alternate().speed(Speed::VeryHigh),
        gpioc.pc11.into_alternate().speed(Speed::VeryHigh),
        gpioc.pc12.into_alternate().speed(Speed::VeryHigh),
        dp.SPI3,
        &mut rcc,
    );

    let pwm_driver = STM32G4xxSixPwmDriver::setup(
        &mut rcc,
        dp.TIM1,
        gpioc.pc0.into_alternate(),
        gpioa.pa7.into_alternate(),
        gpioc.pc1.into_alternate(),
        gpiob.pb0.into_alternate(),
        gpioc.pc2.into_alternate(),
        gpiob.pb1.into_alternate(),
        config.motion.idle_mode,
    );

    // Has external 2k2 pullups but ehhhh whatevs
    let hall1 = gpioa.pa0.into_alternate::<2>().internal_pull_up(true);
    let hall2 = gpioa.pa1.into_alternate::<2>().internal_pull_up(true);
    let hall3 = gpioa.pa2.into_alternate::<2>().internal_pull_up(true);

    // Set up hall-sensor interfacing timer (TIM5, 84MHz clock)
    //
    // Mode of operation described in RM0390 16.3.18 (F4)
    // OR refer to RM0440 29.3.29 (G4)
    TIM5::enable(&mut rcc);
    TIM5::reset(&mut rcc);
    // reset cr1 (pause), counter, pending interrupt flags
    dp.TIM5.cr1().reset();
    dp.TIM5.cnt().reset();
    dp.TIM5.sr().reset();
    // Set psc (time quanta = 1/28kHz) and ARR (set to max)
    dp.TIM5.arr().write(|w| w.arr().set(u32::MAX));
    dp.TIM5.psc().write(|w| {
        w.psc().set(
            // calculate the prescaler value by dividing the TIM5 frequency by 28 kHz
            (rcc.clocks.apb1_tim_clk.raw() / 28.kHz::<1, 1>().raw()) as u16
        )
    });

    // Update on overflow so we can set velocity to 0, One punch mode
    dp.TIM5
        .cr1()
        .modify(|_, w| w.urs().counter_only().opm().enabled());
    // TI1/2/3 --[XOR]--> TI1 --> TI1F_ED
    dp.TIM5.cr2().write(|w| w.ti1s().xor());
    // TI1F_ED -> TRGI -> Reset CNT (Slave Mode)
    unsafe {
        dp.TIM5
            .smcr()
            // g4 hal doesn't seem to have ti1f_ed or reset_mode named bit patterns
            .write(|w| w.ts().bits(0b00100).sms().bits(0b0100));
    }
    // Trigger an interrupt when the hall sensors change OR on timeout
    dp.TIM5.dier().write(|w| w.tie().enabled().uie().enabled());
    // Capture CNT on TRC event (actual hall time), filter
    dp.TIM5.ccer().reset();
    dp.TIM5
        .ccmr1_input()
        .write(|w| w.cc1s().trc().ic1f().fdts_div32_n8());
    dp.TIM5.ccer().write(|w| w.cc1e().enabled());

    // Set up ADCs for voltage and motor current sensing.
    // Current sensing: SNA/B/C -> [8301/ext. op amp] -> BR_SO1/2/3 -> PC0/1/2
    // NOTE: SNC/BR_SO3/PC2 not currently used because the 3rd phase current sense op amp doesn't work.
    // Use injected channels (see README) in dual ADC mode (=> simultaneous conversion), using a
    // different ADC peripheral (ADC1/2/3) for each phase.
    // TIM1 OC4 or TIM1 TRGO?
    // centre-aligned - ideally triggered when it reaches max value
    /*let phase1_adc = Adc::new(
         dp.ADC1,
         true,
        AdcConfig::default()
            .clock(stm32g4xx_hal::adc::config::Clock::Pclk2_div_4) // 21MHz
            .resolution(Resolution::Twelve),
            //.external_trigger(TriggerMode::FallingEdge, ExternalTrigger::Tim),
        &mut rcc,
    );
    let phase2_adc = Adc::new(
        dp.ADC2,
        false, // resetting any ADC resets all of them!!!
        AdcConfig::default()
            .clock(stm32g4xx_hal::adc::config::Clock::Pclk2_div_4) // 21MHz
            .resolution(Resolution::Twelve),
        &mut rcc,
    );*/
    // FIXME: the rest of the fucking owl
    // .external_trigger(adc::config::TriggerMode::RisingEdge, adc::config::ExternalTrigger::Tim_5_cc_1),
    //Adc::new(dp.ADC1, true, config, rcc);

    // Hall effect
    cortex_m::interrupt::free(|cs| unsafe {
        let com_state = G_COM_STATE.borrow(cs).as_mut_unchecked();
        com_state.pwm_driver.write(pwm_driver);
        com_state
            .halls
            .write((hall1.into(), hall2.into(), hall3.into()));
        com_state.hall_state = com_state.get_angle_force_hw();
        dprintln!(0, "[INFO] Hall angle: {}", com_state.hall_state);
    });

    unsafe {
        cortex_m::peripheral::NVIC::unmask(interrupt::TIM1_UP_TIM16);
        cp.NVIC.set_priority(interrupt::TIM5, 31);
        cortex_m::peripheral::NVIC::unmask(interrupt::TIM5);
    }

    ///////////////////////////////////////////////////////////////////////////
    //                           CANADENSIS SETUP                            //
    ///////////////////////////////////////////////////////////////////////////

    let clock = clock::MicrosecondClock::new(dp.TIM2, &mut rcc);
    let id: CanNodeId = config.comms.node_id.try_into().unwrap();
    let mut uuid: [u8; 16] = [0x00; 16];
    unsafe {
        ptr::copy_nonoverlapping(Uid::get() as *const Uid as *const u8, uuid.as_mut_ptr(), 12);
    }
    let driver = CanDriver::new(
        dp.FDCAN1,
        gpioa.pa11.into_alternate(),
        gpioa.pa12.into_alternate(),
        &mut rcc
    );
    let transmitter = CanTransmitter::new(canadensis_can::Mtu::CanFd64);
    let receiver = CanReceiver::new(id);

    let core_node: node::CoreNode<
        _,
        _,
        _,
        TransferIdFixedMap<CanTransport, CYPHAL_CONCURRENT_TRANSFERS>,
        _,
        CYPHAL_NUM_TOPICS,
        CYPHAL_NUM_SERVICES,
    > = CoreNode::new(clock, id, transmitter, receiver, driver);

    let mut node = BasicNode::new(
        core_node,
        GetInfoResponse {
            protocol_version: Version { major: 1, minor: 0 },
            hardware_version: Version { major: 2, minor: 0 },
            software_version: Version { major: 1, minor: 0 },
            software_vcs_revision_id: 0,
            unique_id: uuid,
            name: heapless::Vec::from_slice(b"org.bluesat.owr.besc").unwrap(),
            software_image_crc: heapless::Vec::new(),
            certificate_of_authenticity: heapless::Vec::new(),
        },
    )
    .unwrap();

    node.set_mode(Mode {
        value: Mode::OPERATIONAL,
    });

    node.subscribe_message(
        config.comms.ctrl_duty,
        size_of::<actuator::common::sp::scalar_0_1::Scalar>(),
        1.secs(),
    )
    .unwrap();

    let mut handler = CommSystem {
        commutation_state: &G_COM_STATE,
        config: config.comms,
    };

    dprintln!(0, "Finished setup");

    let mut start = node.clock_mut().now();
    let mut argb_phase = true;
    argb.display(&[BLUE, GREEN, RED]);
    loop {
        if drv.has_fault() {
            motor_disable();
            node.set_health(Health {
                value: Health::WARNING,
            });
            dprintln!(0, "[WARN] DRV Fault!");
        }

        if node.clock().advance_if_elapsed(&mut start, 1.secs()) {
            if let Err(_) = node.run_per_second_tasks() {
                dprintln!(0, "[WARN] node.run_per_second_tasks failed!");
            }
            if argb_phase {
                argb.display(&[YELLOW, GREEN, RED]);
                argb_phase = false;
            } else {
                argb.display(&[BLUE, GREEN, RED]);
                argb_phase = true;
            }
        }

        if let Err(_) = node.receive(&mut handler) {
            dprintln!(0, "[ERR] node.receive failed!");
        }
    }
}

// Hall sensor interfacing interrupt
//
// This will NOT capture the velocity on the first hall transition after it
// times out, it requires another (first enables the counter) for a capture
#[interrupt]
fn TIM5() {
    // If we overflowed the timer period at some point
    let tim = unsafe { TIM5::steal() };
    tim.cr1().modify(|_, w| w.cen().enabled());

    /*if tim.sr().read().cc1of().is_overcapture() {
        dprintln!(0, "[ERR] Hall overcapture!");
    }*/

    // Get old and new hall sensor states to check if we need to wrap around
    // AND wtf the direction of our velocity is!
    let (halls_old, halls_new) = cortex_m::interrupt::free(|cs| unsafe {
        let com_state = G_COM_STATE.borrow(cs).as_mut_unchecked();
        (com_state.hall_state, com_state.get_halls())
    });

    // This is still fine in the case of UIF firing (timer overflow)
    let (direction, wrap) = match (halls_old, halls_new) {
        (state::K_HALL_MIN, state::K_HALL_MAX) => (-1, 1),
        (state::K_HALL_MAX, state::K_HALL_MIN) => (1, 1),
        _ if halls_new > halls_old => (1, 0),
        _ => (-1, 0),
    };

    // ticks (one per 1/28kHz) between hall transition --> speed (rad/s)
    // speed = (28k / ticks) * 1/6 (hall trans/turn * 2pi (turn/s->rad/s)
    let velocity = if tim.sr().read().uif().is_update_pending() {
        0.0
    } else if tim.ccr1().read().ccr().bits() <= 2u32 {
        // Assume hall sensor glitched
        cortex_m::interrupt::free(|cs| unsafe {
            G_COM_STATE.borrow(cs).as_mut_unchecked().glitch_accum +=
                tim.ccr1().read().ccr().bits();
        });
        tim.sr().reset();
        return;
    } else {
        let glitches = cortex_m::interrupt::free(|cs| unsafe {
            G_COM_STATE.borrow(cs).as_mut_unchecked().glitch_accum
        });
        (28000.0 / ((tim.ccr1().read().ccr().bits() + glitches) as f32 * 6.0))
            * consts::TAU
            * direction as f32
    };
    tim.sr().reset();

    // Write new numbers
    cortex_m::interrupt::free(|cs| unsafe {
        let com_state = G_COM_STATE.borrow(cs).as_mut_unchecked();
        com_state.glitch_accum = 0; // Clear glitch velocity accumulator
        com_state.hall_state = halls_new;
        com_state.velocity_raw = velocity;
        com_state.full_revs += wrap * direction;

        itm_send_raw(1, &halls_new);
        itm_send_raw(2, &velocity);
        itm_send_raw(3, &com_state.full_revs);
    });
}

// Motion control interrupt

// Commutation interrupt
#[interrupt]
fn TIM1_UP_TIM16() {
    unsafe {
        TIM1::steal().sr().modify(|_, w| w.uif().clear());
    }

    if cortex_m::interrupt::free(|cs| {
        let control = unsafe { G_COM_STATE.borrow(cs).as_ref_unchecked() };
        match control.mode {
            CommutationMode::Disabled => {
                motor_disable();
                true
            }
            _ => {
                motor_enable();
                false
            }
        }
    }) {
        // Exit early if we're idle
        return;
    };

    // Voltage expressed as a duty cycle... I don't care vro
    const V_D: f32 = 0.0;
    // Duty cycle from -1 to 1
    let v_q: f32 = cortex_m::interrupt::free(|cs| {
        let control = unsafe { G_COM_STATE.borrow(cs).as_ref_unchecked() };
        if let CommutationMode::DutyCycle { duty } = control.mode {
            return duty;
        }

        0.0
    });

    let angle = cortex_m::interrupt::free(|cs| unsafe {
        G_COM_STATE
            .borrow(cs)
            .as_ref_unchecked()
            .get_angle_force_hw()
    });

    // Saving grace... at least these expensive ass math ops aren't in an int::free
    let v_ab = inverse_park(angle, RotatingReferenceFrame { d: V_D, q: v_q });
    let v_abc = modulate_spacevector(v_ab);

    cortex_m::interrupt::free(|cs| unsafe {
        let pwm = G_COM_STATE
            .borrow(cs)
            .as_mut_unchecked()
            .pwm_driver
            .assume_init_mut();
        pwm.set_duty(v_abc);
    });
}
