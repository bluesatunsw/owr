#![no_std]
#![no_main]

use canadensis::{
    core::{
        time::MicrosecondDuration32, transfer::MessageTransfer, transport::Transport, Priority,
        SubjectId,
    },
    encoding::{DataType, Deserialize},
    node::{
        data_types::{GetInfoResponse, Version},
        BasicNode, CoreNode,
    },
    requester::TransferIdFixedMap,
    Node, TransferHandler,
};
use canadensis_can::{CanNodeId, CanReceiver, CanTransmitter, CanTransport, Mtu};
use canadensis_data_types::{
    reg::udral::physics::kinematics::rotation::planar_0_1::Planar,
    uavcan::{node::health_1_0::Health, si::unit::angle},
};
use cortex_m_rt::entry;
use embedded_alloc::LlffHeap as Heap;
use embedded_common::{
    argb::{self, Colour},
    can::CanDriver,
    clock,
    stepper_bus::{Channel, StepperNcsPins},
    tmc_registers::TmcPosition,
};
use heapless::Vec;
use stm32g4xx_hal::{
    gpio::{GpioExt, PinState},
    pac::Peripherals,
    pwr::{PwrExt, VoltageScale},
    rcc::{
        Config, FdCanClockSource, PllConfig, PllMDiv, PllNMul, PllQDiv, PllRDiv, PllSrc, RccExt,
    },
    stm32g4,
    time::{ExtU32, RateExtU32},
};

use canadensis_data_types::uavcan::node::execute_command_1_3::SERVICE as EXECUTE_COMMAND_SERVICE;
use canadensis_data_types::uavcan::node::execute_command_1_3::{
    ExecuteCommandRequest, ExecuteCommandResponse,
};

use panic_probe as _;
use defmt_rtt as _;

use crate::drivebase::Drivebase;

extern crate alloc;

mod as_registers;
mod drivebase;
mod encoder_bus;

// Cyphal constants
const NODE_ID: u8 = 6;
const CYPHAL_CONCURRENT_TRANSFERS: usize = 4;
const CYPHAL_NUM_TOPICS: usize = 8;
const CYPHAL_NUM_SERVICES: usize = 8;

const HEARTBEAT_PERIOD_US: u32 = 1_000_000;
const TELEM_PERIOD_US: u32 = 1_000;
const TID_TIMEOUT_US: u32 = 100_000;

const ENABLE_TELEM: bool = true;

// Cyphal message-IDs -- as per README
const SETPOINT_MESSAGE_CHAN_0_ID: SubjectId = SubjectId::from_truncating(3000);
const SETPOINT_MESSAGE_CHAN_1_ID: SubjectId = SubjectId::from_truncating(3010);
const SETPOINT_MESSAGE_CHAN_2_ID: SubjectId = SubjectId::from_truncating(3020);
const SETPOINT_MESSAGE_CHAN_3_ID: SubjectId = SubjectId::from_truncating(3030);

const POSITION_MESSAGE_CHAN_0_ID: SubjectId = SubjectId::from_truncating(3001);
const POSITION_MESSAGE_CHAN_1_ID: SubjectId = SubjectId::from_truncating(3011);
const POSITION_MESSAGE_CHAN_2_ID: SubjectId = SubjectId::from_truncating(3021);
const POSITION_MESSAGE_CHAN_3_ID: SubjectId = SubjectId::from_truncating(3031);

// ARGB LED constants
const RED: Colour = Colour { r: 255, g: 0, b: 0 };
const BLUE: Colour = Colour { r: 0, g: 0, b: 255 };
const DEFAULT_BRIGHTNESS: u8 = 15;

// Global allocator -- required by canadensis.
#[global_allocator]
static HEAP: Heap = Heap::empty();

fn initialise_allocator() {
    use core::mem::MaybeUninit;
    // This HEAP_SIZE is about as small as we can get without canadensis crashing.
    // (luckily we have a decadent 128 KiB on the G474)
    // NOTE: this *might* still crash if large transfers are ever reassembled, although the
    // Planar types we're using at the moment should be fine
    const HEAP_SIZE: usize = 0x1_0000; // 64 KiB
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { HEAP.init(&raw mut HEAP_MEM as usize, HEAP_SIZE) }
}

///////////
// Main. //
///////////

#[entry]
fn main() -> ! {
    initialise_allocator();

    let dp = Peripherals::take().unwrap();
    let pwr = dp
        .PWR
        .constrain()
        .vos(VoltageScale::Range1 { enable_boost: true })
        .freeze();

    let mut rcc = dp.RCC.freeze(
        // enable HSE @ 24 MHz (stepper board)
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
        pwr,
    );

    defmt::debug!("Configured clocks");

    // Set up pins.
    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);
    let gpiod = dp.GPIOD.split(&mut rcc);

    // Set PB8 to output mode to limit power consumption on reset
    gpiob.pb8.into_push_pull_output();

    defmt::debug!("Setting up ARGB LED driver...");
    let mut argb = argb::Controller::new(
        dp.USART1,
        gpiob.pb6.into_alternate(),
        DEFAULT_BRIGHTNESS,
        &mut rcc,
    );
    defmt::debug!("Setting up microsecond clock...");
    let clock = clock::MicrosecondClock::new(dp.TIM2, &mut rcc);
    defmt::debug!("Setting up CAN driver...");
    let can = CanDriver::new(
        dp.FDCAN1,
        gpioa.pa11.into_alternate(),
        gpioa.pa12.into_alternate(),
        &mut rcc,
    );

    defmt::debug!("Setting up drivebase driver...");
    let mut drivebase = Drivebase::new(
        (
            gpioc.pc10.into_alternate(),
            gpioc.pc11.into_alternate(),
            gpioc.pc12.into_alternate(),
        ),
        StepperNcsPins(
            gpiob
                .pb9
                .into_push_pull_output_in_state(PinState::High)
                .into(),
            gpioc
                .pc13
                .into_push_pull_output_in_state(PinState::High)
                .into(),
            gpioc
                .pc14
                .into_push_pull_output_in_state(PinState::High)
                .into(),
            gpioc
                .pc15
                .into_push_pull_output_in_state(PinState::High)
                .into(),
        ),
        gpioa.pa8.into_analog(),
        gpiod.pd2.into_push_pull_output(),
        gpioa.pa10.into_input(),
        // Currently, the encoder does not work.
        // TODO: fix encoder, uncomment this when working
        /*
        (
            gpiob.pb13.into_alternate(),
            gpiob.pb14.into_alternate(),
            gpiob.pb15.into_alternate(),
        ),
        EncoderNcsPins(
            gpioc
                .pc6
                .into_push_pull_output_in_state(PinState::High)
                .into(),
            gpioc
                .pc7
                .into_push_pull_output_in_state(PinState::High)
                .into(),
            gpioc
                .pc8
                .into_push_pull_output_in_state(PinState::High)
                .into(),
            gpioc
                .pc9
                .into_push_pull_output_in_state(PinState::High)
                .into(),
        ),
        dp.SPI2,
        */
        dp.SPI3,
        &mut rcc,
    );

    defmt::debug!("Setting up Cyphal core node...");
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

    // NOTE: node initialisation is a non-recoverable error and should only happen if we run out of
    // memory or the hardware is completely broken, hence all the unwrapping.
    defmt::debug!("Setting up Cyphal basic node...");
    let mut node = BasicNode::new(
        core_node,
        GetInfoResponse {
            protocol_version: Version { major: 1, minor: 0 },
            // Hardware version refers to Rev. B2. Hopefully we can agree on a better versioning
            // system in the future.
            hardware_version: Version { major: 1, minor: 1 },
            software_version: Version { major: 1, minor: 0 },
            software_vcs_revision_id: 0,
            unique_id: [
                // TODO: Replace this with the ID fetched from the chip
                0xFF, 0x55, 0x13, 0x31, 0x42, 0x69, 0x2A, 0xEE, 0x78, 0x12, 0x99, 0x10, 0x00, 0x03,
                0x00, 0x00,
            ],
            name: Vec::from_slice(b"org.bluesat.owr.stepper_drivebase").unwrap(),
            software_image_crc: Vec::new(),
            certificate_of_authenticity: Vec::new(),
        },
    )
    .unwrap();

    defmt::debug!("Subscribing to subject-IDs...");
    node.subscribe_message(
        SETPOINT_MESSAGE_CHAN_0_ID,
        size_of::<Planar>(),
        MicrosecondDuration32::from_ticks(TID_TIMEOUT_US),
    )
    .unwrap();
    node.subscribe_message(
        SETPOINT_MESSAGE_CHAN_1_ID,
        size_of::<Planar>(),
        MicrosecondDuration32::from_ticks(TID_TIMEOUT_US),
    )
    .unwrap();
    node.subscribe_message(
        SETPOINT_MESSAGE_CHAN_2_ID,
        size_of::<Planar>(),
        MicrosecondDuration32::from_ticks(TID_TIMEOUT_US),
    )
    .unwrap();
    node.subscribe_message(
        SETPOINT_MESSAGE_CHAN_3_ID,
        size_of::<Planar>(),
        MicrosecondDuration32::from_ticks(TID_TIMEOUT_US),
    )
    .unwrap();

    node.subscribe_request(
        EXECUTE_COMMAND_SERVICE,
        ExecuteCommandRequest::EXTENT_BYTES.unwrap() as usize,
        1000.millis(),
    )
    .unwrap();

    defmt::debug!("Starting publication of telemetry subjects...");
    if ENABLE_TELEM {
        node.start_publishing(POSITION_MESSAGE_CHAN_0_ID, 10.millis(), Priority::Nominal)
            .unwrap();
        node.start_publishing(POSITION_MESSAGE_CHAN_1_ID, 10.millis(), Priority::Nominal)
            .unwrap();
        node.start_publishing(POSITION_MESSAGE_CHAN_2_ID, 10.millis(), Priority::Nominal)
            .unwrap();
        node.start_publishing(POSITION_MESSAGE_CHAN_3_ID, 10.millis(), Priority::Nominal)
            .unwrap();
    }

    defmt::debug!("Enabling steppers...");
    drivebase.steppers.enable_all();
    let mut comms_handler = CommsHandler { drivebase };

    let mut tim_heartbeat = node.clock().now_const();
    let mut tim_telem = node.clock().now_const();

    let mut tim_argb = node.clock().now_const();
    let mut argb_phase = false;

    defmt::info!("System initialised. Entering superloop.");
    loop {
        node.receive(&mut comms_handler).unwrap();

        if let Some(status) = comms_handler.drivebase.steppers.health() {
            node.set_health(Health {
                value: Health::ADVISORY,
            });
            node.set_status_code(status);
        } else {
            node.set_health(Health {
                value: Health::NOMINAL,
            });
        }

        if node
            .clock()
            .advance_if_elapsed(&mut tim_heartbeat, HEARTBEAT_PERIOD_US.micros())
        {
            node.run_per_second_tasks().unwrap();
        }

        if ENABLE_TELEM
            && node
                .clock()
                .advance_if_elapsed(&mut tim_telem, TELEM_PERIOD_US.micros())
        {
            node.publish(
                POSITION_MESSAGE_CHAN_0_ID,
                &angle::scalar_1_0::Scalar {
                    radian: comms_handler
                        .drivebase
                        .get_position(Channel::CH0)
                        .unwrap()
                        .0,
                },
            )
            .unwrap();

            node.publish(
                POSITION_MESSAGE_CHAN_1_ID,
                &angle::scalar_1_0::Scalar {
                    radian: comms_handler
                        .drivebase
                        .get_position(Channel::CH1)
                        .unwrap()
                        .0,
                },
            )
            .unwrap();

            node.publish(
                POSITION_MESSAGE_CHAN_2_ID,
                &angle::scalar_1_0::Scalar {
                    radian: comms_handler
                        .drivebase
                        .get_position(Channel::CH2)
                        .unwrap()
                        .0,
                },
            )
            .unwrap();

            node.publish(
                POSITION_MESSAGE_CHAN_3_ID,
                &angle::scalar_1_0::Scalar {
                    radian: comms_handler
                        .drivebase
                        .get_position(Channel::CH3)
                        .unwrap()
                        .0,
                },
            )
            .unwrap();
        }

        // blink the ARGB LEDs
        if node
            .clock()
            .advance_if_elapsed(&mut tim_argb, 200_000.micros())
        {
            if argb_phase {
                argb.display(&[BLUE, RED, BLUE, RED, BLUE, RED]);
            } else {
                argb.display(&[RED, BLUE, RED, BLUE, RED, BLUE]);
            }
            argb_phase = !argb_phase;
        }
    }
}

struct CommsHandler {
    pub drivebase: Drivebase,
}

impl<T: Transport> TransferHandler<T> for CommsHandler {
    fn handle_message<N: Node<Transport = T>>(
        &mut self,
        _node: &mut N,
        transfer: &MessageTransfer<alloc::vec::Vec<u8>, T>,
    ) -> bool {
        defmt::debug!("Received message, subject {}", u32::from(transfer.header.subject));
        let op = match transfer.header.subject {
            SETPOINT_MESSAGE_CHAN_0_ID => Some(Channel::CH0),
            SETPOINT_MESSAGE_CHAN_1_ID => Some(Channel::CH1),
            SETPOINT_MESSAGE_CHAN_2_ID => Some(Channel::CH2),
            SETPOINT_MESSAGE_CHAN_3_ID => Some(Channel::CH3),
            _ => None,
        };
        if let Some(chan) = op {
            let msg = Planar::deserialize_from_bytes(&transfer.payload).unwrap();
            self.drivebase
                .set_position(chan, TmcPosition(msg.angular_position.radian))
                .unwrap();
            true
        } else {
            false
        }
    }

    fn handle_request<N: Node<Transport = T>>(
        &mut self,
        node: &mut N,
        token: canadensis::ResponseToken<T>,
        transfer: &canadensis::core::transfer::ServiceTransfer<alloc::vec::Vec<u8>, T>,
    ) -> bool {
        if transfer.header.service != EXECUTE_COMMAND_SERVICE {
            return false;
        }

        let req =
            ExecuteCommandRequest::deserialize_from_bytes(transfer.payload.as_slice()).unwrap();
        match req.command {
            ExecuteCommandRequest::COMMAND_RESTART => {
                stm32g4::stm32g474::SCB::sys_reset();
            }
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
