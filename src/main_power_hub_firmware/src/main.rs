//! Main power module bringup
//! Reconstituted from stm32-rust-templates/g474rbt6

#![no_main]
#![no_std]

extern crate alloc;
use core::hint::unreachable_unchecked;
use embedded_alloc::LlffHeap as Heap;
use embedded_common::{
    argb::{self, Colour},
    can::CanDriver,
    clock,
};
use heapless::Vec;

use stm32g4xx_hal::{
    adc::{self, AdcClaim, AdcCommonExt}, gpio::{self, GpioExt}, pac::{self, fdcan::TEST}, prelude::*, pwr::{PwrExt, VoltageScale}, rcc::*, time::{ExtU32, RateExtU32}
};

use cortex_m_rt::entry;

use defmt_rtt as _;
use panic_probe as _;

use canadensis::{
    core::{
        time::MicrosecondDuration32, transfer::MessageTransfer, transport::Transport, Priority,
    },
    encoding::Deserialize,
    node::{
        data_types::{GetInfoResponse, Version},
        BasicNode, CoreNode,
    },
    requester::TransferIdFixedMap,
    Node, TransferHandler,
};
use canadensis_can::{CanNodeId, CanReceiver, CanTransmitter, CanTransport, Mtu};

// NOTE: import the data types you use
/* use canadensis_data_types::{
    // Even though we use it for this example, avoid using primitives like these except for
    // debugging. (See the Cyphal spec.)
    uavcan::primitive::scalar::natural8_1_0,
}; */

use canadensis_data_types::uavcan::node::execute_command_1_3::SERVICE as EXECUTE_COMMAND_SERVICE;
use canadensis_data_types::uavcan::node::execute_command_1_3::{
    ExecuteCommandRequest, ExecuteCommandResponse,
};

// Cyphal constants
const NODE_ID: u8 = 1;
const CYPHAL_CONCURRENT_TRANSFERS: usize = 4;
const CYPHAL_NUM_TOPICS: usize = 8;
const CYPHAL_NUM_SERVICES: usize = 8;

// NOTE: these are suggested constants but feel free to adjust
const HEARTBEAT_PERIOD_US: u32 = 1_000_000;
const TELEM_PERIOD_US: u32 = 50_000;
const TID_TIMEOUT_US: u32 = 100_000;
const LED_UPDATE_US: u32 = 10_000;

// Cyphal IDs
// NOTE: remove these ones and add the ones you use
/* const LED_TELEM_SUBJECT: SubjectId = SubjectId::from_truncating(3000);
const LED_UPDATE_SUBJECT: SubjectId = SubjectId::from_truncating(3001); */

// ARGB LED constants 
// NOTE: uncomment if you want to use these
const RED: Colour = Colour { r: 255, g: 0, b: 0 };
// const BLUE: Colour = Colour { r: 0, g: 0, b: 255 };
// const MAGENTA: Colour = Colour { r: 255, g: 0, b: 255 };
// const CYAN: Colour = Colour { r: 0, g: 255, b: 255 };
const YELLOW: Colour = Colour { r: 255, g: 255, b: 0 };
const GREEN: Colour = Colour { r: 0, g: 255, b: 0 };
const BLANK: Colour = Colour { r: 0, g: 0, b: 0 };
const AMBER: Colour = Colour::AMBER;
const DEFAULT_BRIGHTNESS: u8 = 15;

mod controllers;
use controllers::{PowerController, PwrChanState, PwrCtrlState};

// Global allocator -- required by canadensis.
#[global_allocator]
static HEAP: Heap = Heap::empty();

fn initialise_allocator() {
    use core::mem::MaybeUninit;
    // NOTE: You need to calculate HEAP_SIZE such that you don't crash when messages are reassembled
    // You may also want to shrink this if there's not so much CAN traffic
    const HEAP_SIZE: usize = 0x8000; // 32 KiB
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { HEAP.init(&raw mut HEAP_MEM as usize, HEAP_SIZE) }
}

#[entry]
fn main() -> ! {
    initialise_allocator();

    // Initialise peripherals and clock.
    let dp = pac::Peripherals::take().unwrap();
    let cp = pac::CorePeripherals::take().unwrap();
    // This power mode allows us to run at the highest clock frequency
    let pwr = dp.PWR.constrain().vos(VoltageScale::Range1 { enable_boost: true }).freeze();
    let mut rcc = dp.RCC.freeze(
        Config::pll()
            .pll_cfg(PllConfig {
                mux: PllSrc::HSE(24.MHz()),
                m: PllMDiv::DIV_2,
                n: PllNMul::MUL_28,
                // Run PLLR and PLLQ at 168 MHz (the maximum)
                r: Some(PllRDiv::DIV_2), // used for SYSCLK
                q: Some(PllQDiv::DIV_2), // used for FDCAN
                p: None,
            })
            .fdcan_src(FdCanClockSource::PLLQ),
            pwr
        );

    defmt::debug!("Initialised memory allocator and configured clock");

    defmt::trace!("Setting up pins...");
    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);
    // let gpiod = dp.GPIOD.split(&mut rcc);

    defmt::debug!("Setting up LED driver...");
    let mut argb = argb::Controller::new(
        dp.USART1,
        gpioa.pa9.into_alternate(),
        DEFAULT_BRIGHTNESS,
        &mut rcc,
    );
    // there are six (6) LEDs on this board
    argb.display(&[AMBER; 6]);

    // This doesn't get used later
    defmt::debug!("Setting up 100 kHz gate driver clock...");
    let clock_pin: gpio::PB7<gpio::AF10> = gpiob.pb7.into_alternate();
    let mut pwm = dp.TIM3.pwm(clock_pin, 100.kHz(), &mut rcc);
    let _ = pwm.set_duty_cycle_percent(5);
    pwm.enable();

    // Sense setup
    // This initialises all of the ADCs and then sets all the pins to: into_analog()
    // This lets you poll it directly from the pins so no need to mess around with ADC in main
    // We need the SYSTICK delay to set up the ADCs...
    let mut delay = cp.SYST.delay(&rcc.clocks);

    defmt::debug!("Configuring ADC12...");
    let mut adc12_common = dp.ADC12_COMMON
        .claim(adc::config::ClockMode::AdcKerCk {
            prescaler: (adc::config::Prescaler::Div_4),
            src: (adc::config::ClockSource::SystemClock) // NOTE: setting to PLLP doesn't work?????
        }, 
        &mut rcc
    );

    defmt::debug!("Configuring ADC1...");
    let adc1 = adc12_common
        .claim_and_configure(dp.ADC1, adc::config::AdcConfig::default(), &mut delay);

    defmt::debug!("Configuring ADC2...");
    let adc2 = adc12_common
        .claim_and_configure(dp.ADC2, adc::config::AdcConfig::default(), &mut delay);

    defmt::debug!("Configuring ADC345...");
    let adc345_common = dp.ADC345_COMMON
        .claim(adc::config::ClockMode::AdcKerCk {    // Same here
            prescaler: (adc::config::Prescaler::Div_4),
            src: (adc::config::ClockSource::SystemClock)
        }, 
        &mut rcc
    );

    defmt::debug!("Configuring ADC3...");
    let adc3 = adc345_common
        .claim_and_configure(dp.ADC3, adc::config::AdcConfig::default(), &mut delay);

    defmt::debug!("Configuring ADC4...");
    let adc4 = adc345_common
        .claim_and_configure(dp.ADC4, adc::config::AdcConfig::default(), &mut delay);

    // Pin configuration, not needed for now I think after they've been set
    let vsense_pin: gpio::Pin<'B', 14> = gpiob.pb14.into_analog();
    let ch0_sense_pin = gpioa.pa2.into_analog();
    let ch1_sense_pin = gpioa.pa6.into_analog();
    let ch2_sense_pin = gpiob.pb12.into_analog();
    let ch3_sense_pin = gpiob.pb1.into_analog();

    let adc_controller = controllers::AdcController { 
        adc1,
        adc2,
        adc3,
        adc4 
    };

    defmt::debug!("Initialising power controller...");
    let power_controller = PowerController::new(
        // ENABLE PINS -- start them high to inhibit
        // J8 CH0
        gpioc.pc9.into_push_pull_output_in_state(gpio::PinState::High).into(),
        // J9 CH1
        gpioc.pc8.into_push_pull_output().into(),
        // J2 CH2
        gpioc.pc7.into_push_pull_output().into(),
        // J4 CH3
        gpioc.pc6.into_push_pull_output().into(),
        // SENSE PINS
        // ch0_sense_pin,
        // ch1_sense_pin.into(),
        // ch2_sense_pin.into(),
        // ch3_sense_pin.into(),
    );

    defmt::debug!("Setting up microsecond clock...");
    let clock = clock::MicrosecondClock::new(dp.TIM2, &mut rcc);
    defmt::debug!("Setting up FDCAN driver...");
    let can = CanDriver::new(
        dp.FDCAN1,
        gpioa.pa11.into_alternate(),
        gpioa.pa12.into_alternate(),
        &mut rcc,
    );
    
    defmt::debug!("Setting up core Cyphal node...");
    let id = CanNodeId::from_truncating(NODE_ID);
    let transmitter = CanTransmitter::new(Mtu::CanFd64);
    let receiver = CanReceiver::new(id);
    let core_node: CoreNode<
        _,
        _,
        _,
        TransferIdFixedMap<CanTransport, CYPHAL_CONCURRENT_TRANSFERS>,
        _,
        CYPHAL_NUM_TOPICS,
        CYPHAL_NUM_SERVICES,
    > = CoreNode::new(clock, id, transmitter, receiver, can);

    // Node initialisation is a non-recoverable error and should only happen if we run out of
    // memory or the hardware is completely broken, hence all the unwrapping.
    defmt::debug!("Setting up basic Cyphal node...");
    let mut node = BasicNode::new(
        core_node,
        GetInfoResponse {
            protocol_version: Version { major: 1, minor: 0 },
            // Hardware r2p0
            hardware_version: Version { major: 2, minor: 0 },
            // TODO: update software version and VCS ID
            // Pending requirements versioning...
            software_version: Version { major: 0, minor: 1 },
            software_vcs_revision_id: 0,
            unique_id: embedded_common::debug::uuid(),
            name: Vec::from_slice(b"org.bluesat.owr.mpm").unwrap(),
            software_image_crc: Vec::new(),
            certificate_of_authenticity: Vec::new(),
        },
    )
    .unwrap();

    // NOTE: add calls like the following if you want to listen for specific messages
    /*node.subscribe_message(
        LED_UPDATE_SUBJECT,
        size_of::<natural8_1_0::Natural8>(),
        MicrosecondDuration32::from_ticks(TID_TIMEOUT_US),
    )
    .unwrap();*/

    // NOTE: add calls like the following to publish specific messages
    /*defmt::trace!("Starting publication of LED messages...");
    node.start_publishing(LED_TELEM_SUBJECT, 10.millis(), Priority::Nominal).unwrap();*/

    // Start the superloop.
    let mut tim_heartbeat = node.clock().now_const();
    let mut tim_telem = node.clock().now_const();
    let mut tim_argb = node.clock().now_const();
    let mut tim_1_hz = node.clock().now_const();
    let mut tim_pwr = node.clock().now_const();
  
    let mut comms_state = CommsState { did_tx: false, did_rx: false };
    let mut subsystem = PowerSubsystem { power_controller };

    defmt::info!("System initialised. Entering superloop...");
    loop {
        // Handle Cyphal tasks
        // node.receive(&mut CommsHandler { state: &mut comms_state, subsystem: &mut subsystem } ).unwrap();

        // You can set the health of the node to represent the state of the subsystem to be
        // signalled over Cyphal/CAN in the heartbeat messages:
        // node.set_health(...); node.set_status_code(...);

        // TEST LOOP
        if node
            .clock()
            .advance_if_elapsed(&mut tim_heartbeat, 1.secs())
        {
            const TEST_CHANNEL: controllers::PwrChan = controllers::PwrChan::CH0;
            if subsystem.power_controller.status(TEST_CHANNEL) == PwrChanState::Enabled {
                defmt::info!("Disabling CH0...");
                subsystem.power_controller.disable(TEST_CHANNEL);
            } else {
                defmt::info!("Enabling CH0...");
                subsystem.power_controller.enable(TEST_CHANNEL);
            }
        }

        if node
            .clock()
            .advance_if_elapsed(&mut tim_pwr, controllers::PWR_CTRL_TICK_US.micros())
        {
            defmt::trace!("Doing power subsystem tick...");
            subsystem.power_controller.tick();
        }

        /*if node
            .clock()
            .advance_if_elapsed(&mut tim_telem, TELEM_PERIOD_US.micros())
        {
            node.publish(
                LED_TELEM_SUBJECT,
                &natural8_1_0::Natural8 {
                    value: subsystem.hue as u8
                },
            )
            .unwrap();
            comms_state.did_tx = true;
        }*/

        if node
            .clock()
            .advance_if_elapsed(&mut tim_argb, LED_UPDATE_US.micros())
        {
            let blink_phase = node.clock().advance_if_elapsed(&mut tim_1_hz, 500.millis());

            // blinking = 1 Hz, 50% duty cycle
            fn state_to_colour_blinking(state: PwrChanState) -> (Colour, bool) {
                match state {
                    PwrChanState::Enabled => (GREEN, true),
                    PwrChanState::Disabled => (YELLOW, true),
                    PwrChanState::SysInit => (AMBER, false),
                    PwrChanState::Fault => (RED, true),
                }
            }

            let state_to_colour = |state| -> Colour {
                let col = state_to_colour_blinking(state);
                if blink_phase && col.1 {
                    BLANK
                }  else {
                    col.0
                }
            };

            let ch0_col = state_to_colour(subsystem.power_controller.channel_state[0]);
            let ch1_col = state_to_colour(subsystem.power_controller.channel_state[1]);
            let ch2_col = state_to_colour(subsystem.power_controller.channel_state[2]);
            let ch3_col = state_to_colour(subsystem.power_controller.channel_state[3]);

            let can_col = Colour { r: 0, g: if comms_state.did_rx { 255 } else { 0 }, b: if comms_state.did_tx { 255 } else { 0 } };
            comms_state.did_rx = false;
            comms_state.did_tx = false;

            let status_col = match subsystem.power_controller.controller_state {
                PwrCtrlState::Active => GREEN,
                PwrCtrlState::SysInit => AMBER,
                PwrCtrlState::EStopped => YELLOW,
                PwrCtrlState::Error => RED,
            };

            defmt::trace!("Refreshing LEDs...");
            // LED order is CH0, CH2, IDC upper, IDC lower, CH3, CH1
            argb.display(&[ch0_col, ch2_col, status_col, can_col, ch3_col, ch1_col]);
        }
    }
}

struct CommsState {
    did_tx: bool,
    did_rx: bool,
}

struct PowerSubsystem {
    power_controller: PowerController
}

struct CommsHandler<'a> {
    state: &'a mut CommsState,
    subsystem: &'a mut PowerSubsystem,
}

impl<T: Transport> TransferHandler<T> for CommsHandler<'_> {
    fn handle_message<N: Node<Transport = T>>(
        &mut self,
        _node: &mut N,
        transfer: &MessageTransfer<alloc::vec::Vec<u8>, T>,
    ) -> bool {
        // NOTE: add your message handler here
        defmt::debug!("Received a message with subject ID {}", u16::from(transfer.header.subject));
        self.state.did_rx = true;
        match transfer.header.subject {
            // TODO: add a new case for each subject you want to handle
            /*LED_UPDATE_SUBJECT => {
                let new_hue = natural8_1_0::Natural8::deserialize_from_bytes(&transfer.payload).unwrap();
                self.subsystem.hue = new_hue.value;
            },*/
            _ => {},
        }
        true
    }

    fn handle_request<N: Node<Transport = T>>(
        &mut self,
        node: &mut N,
        token: canadensis::ResponseToken<T>,
        transfer: &canadensis::core::transfer::ServiceTransfer<alloc::vec::Vec<u8>, T>,
    ) -> bool {
        self.state.did_rx = true;
        if transfer.header.service != EXECUTE_COMMAND_SERVICE {
            return false;
        }

        let req =
            ExecuteCommandRequest::deserialize_from_bytes(transfer.payload.as_slice()).unwrap();
        match req.command {
            // handle COMMAND_RESTART
            ExecuteCommandRequest::COMMAND_RESTART => {
                unsafe {
                    stm32g4xx_hal::stm32g4::stm32g474::CorePeripherals::steal()
                        .SCB
                        .aircr
                        .write(0x05FA_0004);
                };
                // SAFETY: The above operation will instantly reset the MCU
                unsafe { unreachable_unchecked() }
            }
            // NOTE: add any other request handlers here
            _ => {
                node.send_response(
                    token,
                    1000.millis(),
                    &ExecuteCommandResponse {
                        status: ExecuteCommandResponse::STATUS_BAD_COMMAND,
                        output: Vec::new(),
                    },
                )
                .unwrap();
            }
        }

        true
    }
}
