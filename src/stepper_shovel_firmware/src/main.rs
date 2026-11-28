#![allow(internal_features)]
#![feature(never_type)]
#![feature(core_intrinsics)]
#![no_std]
#![no_main]

use canadensis::{
    core::{
        time::MicrosecondDuration32, transfer::MessageTransfer, transport::Transport, SubjectId,
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
    reg::udral::service::actuator::common::sp::scalar_0_1::Scalar, uavcan::node::health_1_0::Health,
};
use core::{hint::unreachable_unchecked, intrinsics::breakpoint, panic::PanicInfo};
use cortex_m_rt::entry;
use embedded_alloc::LlffHeap as Heap;
use embedded_common::{
    argb::{self, Colour},
    can::CanDriver,
    clock,
    debug::{setup_itm, uuid},
    dprintln,
    stepper_bus::StepperNcsPins,
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

use crate::motion::Motion;

extern crate alloc;

mod motion;

// Cyphal constants
const NODE_ID: u8 = 8;
const CYPHAL_CONCURRENT_TRANSFERS: usize = 4;
const CYPHAL_NUM_TOPICS: usize = 8;
const CYPHAL_NUM_SERVICES: usize = 8;

const HEARTBEAT_PERIOD_US: u32 = 1_000_000;
const TID_TIMEOUT_US: u32 = 100_000;

// Cyphal message-IDs -- as per README
const SETPOINT_MESSAGE_CHAN_PIVOT_ID: SubjectId = SubjectId::from_truncating(2000);
const SETPOINT_MESSAGE_CHAN_BUCKET_ID: SubjectId = SubjectId::from_truncating(2010);
const SETPOINT_MESSAGE_CHAN_PAVER_ID: SubjectId = SubjectId::from_truncating(2020);

// ARGB LED constants
const RED: Colour = Colour { r: 255, g: 0, b: 0 };
const BLUE: Colour = Colour { r: 0, g: 0, b: 255 };
const GREEN: Colour = Colour { r: 0, g: 255, b: 0 };
const DEFAULT_BRIGHTNESS: u8 = 15;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    dprintln!(0, "{}", _info.message());
    breakpoint();
    loop {}
}

// Global allocator -- required by canadensis.
#[global_allocator]
static HEAP: Heap = Heap::empty();

fn initialise_allocator() {
    use core::mem::MaybeUninit;
    // This HEAP_SIZE is about as small as we can get without canadensis crashing.
    // (luckily we have a decadent 128 KiB on the G474)
    // NOTE: this *might* still crash if large transfers are ever reassembled, although the
    // Planar types we're using at the moment should be fine
    const HEAP_SIZE: usize = 0x6000; // 24 KiB
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { HEAP.init(&raw mut HEAP_MEM as usize, HEAP_SIZE) }
}

///////////
// Main. //
///////////

#[entry]
fn main() -> ! {
    initialise_allocator();

    let mut dp = Peripherals::take().unwrap();
    let mut cp = stm32g4xx_hal::pac::CorePeripherals::take().unwrap();
    unsafe { setup_itm(&mut cp.DCB, &mut cp.DWT, &mut dp.DBGMCU, &mut cp.ITM) };

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

    // Set up pins.
    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);
    let gpiod = dp.GPIOD.split(&mut rcc);

    // Set PB8 to output mode to limit power consumption on reset
    gpiob.pb8.into_push_pull_output();

    // Initialise various device drivers.
    let mut argb = argb::Controller::new(
        dp.USART1,
        gpiob.pb6.into_alternate(),
        DEFAULT_BRIGHTNESS,
        &mut rcc,
    );
    let clock = clock::MicrosecondClock::new(dp.TIM2, &mut rcc);
    let can = CanDriver::new(
        dp.FDCAN1,
        gpioa.pa11.into_alternate(),
        gpioa.pa12.into_alternate(),
        &mut rcc,
    );

    let mut motion = Motion::new(
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
        dp.SPI3,
        &mut rcc,
    );

    // Initialise Cyphal (canadensis) node.
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
    let mut node = BasicNode::new(
        core_node,
        GetInfoResponse {
            protocol_version: Version { major: 1, minor: 0 },
            // Hardware version refers to Rev. B2. Hopefully we can agree on a better versioning
            // system in the future.
            hardware_version: Version { major: 1, minor: 1 },
            software_version: Version { major: 1, minor: 0 },
            software_vcs_revision_id: 0,
            unique_id: uuid(),
            name: Vec::from_slice(b"org.bluesat.owr.stepper_shovel").unwrap(),
            software_image_crc: Vec::new(),
            certificate_of_authenticity: Vec::new(),
        },
    )
    .unwrap();

    node.subscribe_message(
        SETPOINT_MESSAGE_CHAN_PIVOT_ID,
        size_of::<Scalar>(),
        MicrosecondDuration32::from_ticks(TID_TIMEOUT_US),
    )
    .unwrap();
    node.subscribe_message(
        SETPOINT_MESSAGE_CHAN_BUCKET_ID,
        size_of::<Scalar>(),
        MicrosecondDuration32::from_ticks(TID_TIMEOUT_US),
    )
    .unwrap();
    node.subscribe_message(
        SETPOINT_MESSAGE_CHAN_PAVER_ID,
        size_of::<Scalar>(),
        MicrosecondDuration32::from_ticks(TID_TIMEOUT_US),
    )
    .unwrap();

    node.subscribe_request(
        EXECUTE_COMMAND_SERVICE,
        ExecuteCommandRequest::EXTENT_BYTES.unwrap() as usize,
        1000.millis(),
    )
    .unwrap();

    motion.steppers.enable_all();
    let mut comms_handler = CommsHandler { motion };

    let mut tim_heartbeat = node.clock().now_const();

    let mut tim_argb = node.clock().now_const();
    let mut argb_phase = false;

    loop {
        node.receive(&mut comms_handler).unwrap();

        match comms_handler.motion.steppers.health() {
            Ok(Some(status)) => {
                node.set_health(Health {
                    value: Health::ADVISORY,
                });
                node.set_status_code(status);
            }
            Ok(None) => node.set_health(Health {
                value: Health::NOMINAL,
            }),
            Err(_) => {
                node.set_health(Health {
                    value: Health::ADVISORY,
                });
                node.set_status_code(0xF1);
            }
        }

        if node
            .clock()
            .advance_if_elapsed(&mut tim_heartbeat, HEARTBEAT_PERIOD_US.micros())
        {
            node.run_per_second_tasks().unwrap();
        }

        // blink the ARGB LEDs
        if node
            .clock()
            .advance_if_elapsed(&mut tim_argb, 200_000.micros())
        {
            if argb_phase {
                argb.display(&[BLUE, GREEN, BLUE, GREEN, BLUE, GREEN]);
            } else {
                argb.display(&[GREEN, BLUE, GREEN, BLUE, GREEN, BLUE]);
            }
            argb_phase = !argb_phase;
        }
    }
}

struct CommsHandler {
    pub motion: Motion,
}

impl<T: Transport> TransferHandler<T> for CommsHandler {
    fn handle_message<N: Node<Transport = T>>(
        &mut self,
        _node: &mut N,
        transfer: &MessageTransfer<alloc::vec::Vec<u8>, T>,
    ) -> bool {
        enum Op {
            PIVOT,
            BUCKET,
            PAVER,
        }

        let maybe_op = match transfer.header.subject {
            SETPOINT_MESSAGE_CHAN_PIVOT_ID => Some(Op::PIVOT),
            SETPOINT_MESSAGE_CHAN_BUCKET_ID => Some(Op::BUCKET),
            SETPOINT_MESSAGE_CHAN_PAVER_ID => Some(Op::PAVER),
            _ => None,
        };
        if let Some(op) = maybe_op {
            let target = Scalar::deserialize_from_bytes(&transfer.payload)
                .unwrap()
                .value
                .to_f32();
            match op {
                Op::PIVOT => self.motion.set_pivot(target).unwrap(),
                Op::BUCKET => self.motion.set_bucket(target).unwrap(),
                Op::PAVER => self.motion.set_paver(target).unwrap(),
            }
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
                unsafe {
                    stm32g4::stm32g474::CorePeripherals::steal()
                        .SCB
                        .aircr
                        .write(0x05FA_0004);
                };
                // SAFETY: The above operation will instantly reset the  MCU
                unsafe { unreachable_unchecked() }
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
