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
    adc::{self, AdcClaim, AdcCommonExt}, gpio::{self, GpioExt}, i2c::{self, I2cExt}, independent_watchdog, opamp::Gain, pac::{self, fdcan::TEST}, prelude::*, pwr::{PwrExt, VoltageScale}, rcc::*, time::{ExtU32, RateExtU32}, stm32
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
 * ...
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

const HEARTBEAT_PERIOD_US: u32 = 1_000_000;
const TELEM_PERIOD_US: u32 = 500_000;
const TID_TIMEOUT_US: u32 = 100_000;
const LED_UPDATE_US: u32 = 10_000;

// Cyphal IDs
// TODO
/* const LED_TELEM_SUBJECT: SubjectId = SubjectId::from_truncating(3000);
const LED_UPDATE_SUBJECT: SubjectId = SubjectId::from_truncating(3001); */

// ARGB LED constants 
const RED: Colour = Colour { r: 255, g: 0, b: 0 };
const DIM_YELLOW: Colour = Colour { r: 96, g: 48, b: 0 };
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
                p: Some(PllPDiv::DIV_20), // 16.8 MHz before prescaling
            })
            .fdcan_src(FdCanClockSource::PLLQ),
            pwr
        );

    defmt::debug!("Initialised memory allocator and configured clock");

    defmt::trace!("Setting up pins...");
    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);

    // SAFETY: This is for the opamps. The HAL won't let us both give the
    // sense pin to the opamp and then use it to do ADC sampling later.
    // TODO: fix HAL to let us do this safely
    // NOTE: split() resets the GPIOA and GPIOB peripherals so the ordering of stealing is
    // important here...
    let (sense0, sense1, sense2, sense3) = unsafe {
        let stolen_gpioa = pac::GPIOA::steal().split(&mut rcc);
        let stolen_gpiob = pac::GPIOB::steal().split(&mut rcc);
        (
            stolen_gpioa.pa2.into_analog(),
            stolen_gpioa.pa6.into_analog(),
            stolen_gpiob.pb1.into_analog(),
            stolen_gpiob.pb12.into_analog()
        )
    };

    let mut delay = cp.SYST.delay(&rcc.clocks);

    defmt::debug!("Setting up LED driver...");
    let mut argb = argb::Controller::new(
        dp.USART1,
        gpioa.pa9.into_alternate(),
        DEFAULT_BRIGHTNESS,
        &mut rcc,
    );
    // just to prevent invalid colours on startup
    delay.delay(100.micros());
    // there are six (6) LEDs on this board
    argb.display(&[AMBER; 6]);

    // ADC setup
    defmt::debug!("Configuring ADC12...");
    let mut adc12_common = dp.ADC12_COMMON
        .claim(adc::config::ClockMode::AdcKerCk {
            prescaler: (adc::config::Prescaler::Div_4),
            src: (adc::config::ClockSource::PllP)
        }, 
        &mut rcc
    );
    adc12_common.enable_vref();

    defmt::debug!("Configuring ADC1...");
    let mut adc1 = adc12_common
        .claim(dp.ADC1, &mut delay);
    defmt::debug!("Configuring ADC2...");
    let mut adc2 = adc12_common
        .claim(dp.ADC2, &mut delay);

    adc1.calibrate_all();
    let adc1 = adc1.enable();
    adc2.calibrate_all();
    let adc2 = adc2.enable();

    let vsense_pin: gpio::Pin<'B', 14> = gpiob.pb14.into_analog();

    // Opamp configuration
    let (opamp1, opamp2, opamp3, opamp4, ..) = dp.OPAMP.split(&mut rcc);
    // NOTE: on r2p0, PA3 is not connected. PC5 used by mistake instead. See errata
    // PA1: CH0 (silkscreen)
    // PA7: CH2 (silkscreen)
    let ch0_opamp = opamp1
        //.pga_external_filter(gpioa.pa1.into_analog(), gpioa.pa3.into_analog(), Gain::Gain64)
        //.enable_output(gpioa.pa2.into_analog());
        .pga(gpioa.pa1.into_analog(), Gain::Gain64);
    let ch2_opamp = opamp2
        .pga_external_filter(gpioa.pa7.into_analog(), gpioa.pa5.into_analog(), Gain::Gain64)
        .enable_output(gpioa.pa6.into_analog());
    let opamp3 = opamp3
        .pga_external_filter(gpiob.pb0.into_analog(), gpiob.pb2.into_analog(), Gain::Gain64)
        .enable_output(gpiob.pb1.into_analog());
    let opamp4 = opamp4
        .pga_external_filter(gpiob.pb11.into_analog(), gpiob.pb10.into_analog(), Gain::Gain64)
        .enable_output(gpiob.pb12.into_analog());

    // Configure charge pump clock. Should not be enabled until after the power channels are set up and disabled.
    let clock_pin: gpio::PB7<gpio::AF10> = gpiob.pb7.into_alternate();
    let mut charge_pump_pwm = dp.TIM3.pwm(clock_pin, 100.kHz(), &mut rcc);
    charge_pump_pwm.set_duty_cycle_percent(5);

    defmt::debug!("Initialising power controller...");
    let mut power_controller = PowerController::new(
        // TODO: check CH1 and CH3 pins for all of these (only CH0 and CH2 tested)
        // enable pins -- start them high to inhibit
        gpioc.pc9.into_push_pull_output_in_state(gpio::PinState::High).into(),
        gpioc.pc6.into_push_pull_output_in_state(gpio::PinState::High).into(),
        gpioc.pc8.into_push_pull_output_in_state(gpio::PinState::High).into(),
        gpioc.pc7.into_push_pull_output_in_state(gpio::PinState::High).into(),
        // ADCs
        vsense_pin,
        adc1,
        adc2,
        // opamps
        ch0_opamp,
        opamp3,
        ch2_opamp,
        opamp4,
        // sense pins
        sense0,
        sense1,
        sense2,
        sense3,
        charge_pump_pwm,
    );

    defmt::debug!("Starting 100 kHz charge pump clock...");
    power_controller.charge_pump_enable();

    defmt::debug!("Setting up I2C EEPROM...");
    const EEPROM_ADDR: u8 = 0b1010_000;
    let sda = gpiob.pb9.into_alternate_open_drain();
    let sda = sda.internal_pull_up(true);
    let scl = gpioa.pa15.into_alternate_open_drain();
    let scl = scl.internal_pull_up(true);
    let mut i2c_bus = dp.I2C1.i2c((sda, scl), 200.kHz(), &mut rcc);

    /*defmt::debug!("Reading 8 bytes from address 0x000...");
    let address: u16 = 0x000;
    let mut data: [u8; 8] = [0; 8];
    match i2c_bus.write_read(EEPROM_ADDR, &[((address & 0x3F00) >> 8) as u8, (address & 0xFF) as u8], &mut data) {
        Ok(_) => defmt::debug!("Got {}", data),
        Err(e) => defmt::warn!("Some error reading {:?}", defmt::Debug2Format(&e)),
    }

    defmt::debug!("Incrementing first four bytes at 0x000...");
    let address = 0x000;
    let addr_words = [((address & 0x3F00) >> 8) as u8, (address & 0xFF) as u8];
    let send_bytes: [u8; 6] = [addr_words[0], addr_words[1], data[0] + 1, data[1] + 1, data[2] + 1, data[3] + 1];
    match i2c_bus.write(EEPROM_ADDR, &send_bytes) {
        Ok(_) => defmt::debug!("No error writing data"),
        Err(e) => defmt::warn!("Error writing: {:?}", defmt::Debug2Format(&e)),
    };

    // clear write cycle
    delay.delay(5.millis());

    defmt::debug!("Reading 8 bytes from address 0x000...");
    let address: u16 = 0x000;
    for i in 0..8 {
        data[i] = 0;
    }
    match i2c_bus.write_read(EEPROM_ADDR, &[((address & 0x3F00) >> 8) as u8, (address & 0xFF) as u8], &mut data) {
        Ok(_) => defmt::debug!("Got {}", data),
        Err(e) => defmt::warn!("Error writing: {:?}", defmt::Debug2Format(&e)),
    }*/

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

    defmt::debug!("Setting up watchdog...");
    let mut iwdg = independent_watchdog::IndependentWatchdog::new(dp.IWDG);
    iwdg.start(controllers::PWR_CTRL_TICK_US.micros() * 2);

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
    let mut tim_test0 = node.clock().now_const();
    let mut tim_test1 = node.clock().now_const();
    let mut tim_telem = node.clock().now_const();
    let mut tim_argb = node.clock().now_const();
    let mut tim_1_hz = node.clock().now_const();
    let mut tim_pwr = node.clock().now_const();
  
    let mut comms_state = CommsState { did_tx: false, did_rx: false };

    let mut blink_phase = true;

    defmt::info!("System initialised. Entering superloop...");
    loop {
        // Handle Cyphal tasks
        // node.receive(&mut CommsHandler { state: &mut comms_state, subsystem: &mut subsystem } ).unwrap();

        // You can set the health of the node to represent the state of the subsystem to be
        // signalled over Cyphal/CAN in the heartbeat messages:
        // node.set_health(...); node.set_status_code(...);

        if node
            .clock()
            .advance_if_elapsed(&mut tim_heartbeat, HEARTBEAT_PERIOD_US.micros())
        {
            // node.run_per_second_tasks().unwrap();
        }

        // TEST LOOPS
        if node
            .clock()
            .advance_if_elapsed(&mut tim_test0, 3.secs())
        {
            const TEST_CHANNEL: controllers::PwrChan = controllers::PwrChan::CH0;
            if power_controller.status(TEST_CHANNEL) == PwrChanState::Enabled {
                defmt::info!("Disabling CH0...");
                power_controller.disable(TEST_CHANNEL);
            } else {
                defmt::info!("Enabling CH0...");
                power_controller.enable(TEST_CHANNEL);
                power_controller.enable(controllers::PwrChan::CH1);
            }
        }
        if node
            .clock()
            .advance_if_elapsed(&mut tim_test1, 9.secs())
        {
            const TEST_CHANNEL2: controllers::PwrChan = controllers::PwrChan::CH2;
            if power_controller.status(TEST_CHANNEL2) == PwrChanState::Enabled {
                defmt::info!("Disabling CH2...");
                power_controller.disable(TEST_CHANNEL2);
            } else {
                defmt::info!("Enabling CH2...");
                power_controller.enable(TEST_CHANNEL2);
            }
        }

        if node
            .clock()
            .advance_if_elapsed(&mut tim_pwr, controllers::PWR_CTRL_TICK_US.micros())
        {
            defmt::trace!("Doing power subsystem tick...");
            power_controller.tick();
            iwdg.feed();
        }

        if node
            .clock()
            .advance_if_elapsed(&mut tim_telem, TELEM_PERIOD_US.micros())
        {
            defmt::trace!("Publishing power telemetry...");
            /*node.publish(
                LED_TELEM_SUBJECT,
                &natural8_1_0::Natural8 {
                    value: subsystem.hue as u8
                },
            )
            .unwrap();*/
            comms_state.did_tx = true;
        }

        // for blinkenlicht
        // blinking = 1 Hz, 50% duty cycle
        if node.clock().advance_if_elapsed(&mut tim_1_hz, 500.millis()) { blink_phase = !blink_phase; }

        if node
            .clock()
            .advance_if_elapsed(&mut tim_argb, LED_UPDATE_US.micros())
        {
            fn state_to_colour_blinking(state: PwrChanState) -> (Colour, bool) {
                match state {
                    PwrChanState::Enabled => (GREEN, false),
                    PwrChanState::Disabled => (DIM_YELLOW, false),
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

            let ch0_col = state_to_colour(power_controller.channel_state[0]);
            let ch1_col = state_to_colour(power_controller.channel_state[1]);
            let ch2_col = state_to_colour(power_controller.channel_state[2]);
            let ch3_col = state_to_colour(power_controller.channel_state[3]);

            // handle CAN activity indication
            let can_col = Colour {
                r: 0,
                g: if comms_state.did_rx { 255 } else { 0 },
                b: if comms_state.did_tx { 255 } else { 0 }
            };
            comms_state.did_rx = false;
            comms_state.did_tx = false;

            let status_col = match power_controller.controller_state {
                PwrCtrlState::Active => GREEN,
                PwrCtrlState::SysInit => AMBER,
                PwrCtrlState::EStopped => DIM_YELLOW,
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

struct CommsHandler<'a> {
    state: &'a mut CommsState,
    subsystem: &'a mut PowerController,
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
                defmt::warn!("Cyphal restart command received. Restarting...");
                self.subsystem.disable_all();
                self.subsystem.charge_pump_disable();
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
