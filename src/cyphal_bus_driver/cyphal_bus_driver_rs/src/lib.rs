use canadensis::{Node, TransferHandler};
use canadensis_data_types::reg::udral::physics::{dynamics, kinematics};
use canadensis_data_types::uavcan::si::unit::{
    angle, angular_acceleration, angular_velocity, torque,
};
use socketcan::{CanSocket, Socket};

use canadensis::core::time::MicrosecondDuration32;
use canadensis::core::{Priority, SubjectId};
use canadensis::node::data_types::{GetInfoResponse, Version};
use canadensis::node::{BasicNode, CoreNode};
use canadensis::requester::TransferIdFixedMap;
use canadensis_can::queue::{ArrayQueue, SingleQueueDriver};
use canadensis_can::{CanNodeId, CanReceiver, CanTransmitter, CanTransport, Mtu};
use canadensis_data_types::reg::udral::service::actuator::common::sp::scalar_0_1::Scalar;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use std::vec::Vec;

use canadensis_linux::{LinuxCan, SystemClock};

use crate::ffi::{
    BescCommand, BescConfig, BescState, DeviceStates, NamedBescConfig, NamedStepperConfig,
    StepperCommand, StepperConfig, StepperState,
};

const QUEUE_CAPACITY: usize = 1024;

// Transfer id's are assigned to frames cyclically
// This ensure that large multi-frame messages are able
// to be reconstructed
// 16 should be good enough for our purposes
const TRANSFER_IDS: usize = 16;

// Number of topics supplied by the node
// In theory should be 5 (4 throttle + 1 heartbeat) but
// having more does not hurt
const PUBLISHERS: usize = 16;

// In theory we don't have any of these
const REQUESTERS: usize = 16;

type FdCanQueue = SingleQueueDriver<SystemClock, ArrayQueue<QUEUE_CAPACITY>, LinuxCan<CanSocket>>;
type ClassicCanQueue =
    SingleQueueDriver<SystemClock, ArrayQueue<QUEUE_CAPACITY>, LinuxCan<CanSocket>>;

type FdCoreNode = CoreNode<
    SystemClock,
    CanTransmitter<SystemClock, FdCanQueue>,
    CanReceiver<SystemClock, FdCanQueue>,
    TransferIdFixedMap<CanTransport, TRANSFER_IDS>,
    FdCanQueue,
    PUBLISHERS,
    REQUESTERS,
>;
type ClassicCoreNode = CoreNode<
    SystemClock,
    CanTransmitter<SystemClock, ClassicCanQueue>,
    CanReceiver<SystemClock, ClassicCanQueue>,
    TransferIdFixedMap<CanTransport, TRANSFER_IDS>,
    ClassicCanQueue,
    PUBLISHERS,
    REQUESTERS,
>;

type FdNode = BasicNode<FdCoreNode>;
type ClassicNode = BasicNode<ClassicCoreNode>;

#[cxx::bridge]
mod ffi {
    /********/
    /* BESC */
    /********/
    #[derive(Debug, Clone)]
    struct BescConfig {
        id: u16,
        invert: bool,
    }

    #[derive(Debug, Clone)]
    struct NamedBescConfig {
        name: String,
        config: BescConfig,
    }

    #[derive(Debug, Clone)]
    struct BescState {
        device_name: String,
        measured_velocity: f64,
    }

    #[derive(Debug, Clone)]
    struct BescCommand {
        device_name: String,
        target_velocity: f64,
    }

    /***********/
    /* Stepper */
    /***********/
    #[derive(Debug, Clone)]
    struct StepperConfig {
        id: u16,
    }

    #[derive(Debug, Clone)]
    struct NamedStepperConfig {
        name: String,
        config: StepperConfig,
    }

    #[derive(Debug, Clone)]
    struct StepperState {
        device_name: String,
        measured_position: f64,
    }

    #[derive(Debug, Clone)]
    struct StepperCommand {
        device_name: String,
        target_position: f64,
    }

    /************/
    /* Combined */
    /************/
    #[derive(Debug, Clone)]
    struct DeviceStates {
        bescs: Vec<BescState>,
        steppers: Vec<StepperState>,
    }

    extern "Rust" {
        type BusDriver;

        #[Self = "BusDriver"]
        fn create(
            can_interface: String,
            node_id: u8,
            bescs: Vec<NamedBescConfig>,
            steppers: Vec<NamedStepperConfig>,
        ) -> Box<BusDriver>;
        #[Self = "BusDriver"]
        fn destroy(driver: Box<BusDriver>);

        fn activate(&mut self);
        fn deactivate(&mut self);

        fn read(&mut self) -> DeviceStates;
        fn write(&mut self, besc_commands: Vec<BescCommand>, stepper_commands: Vec<StepperCommand>);
    }
}

struct BusDriver {
    fd_node: FdNode,
    classic_node: ClassicNode,
    bescs: BTreeMap<String, (BescConfig, BescState)>,
    steppers: BTreeMap<String, (StepperConfig, StepperState)>,
    heartbeat_time: Instant,
}

impl BusDriver {
    pub fn create(
        can_interface: String,
        gateway_id: u8,
        bescs: Vec<NamedBescConfig>,
        steppers: Vec<NamedStepperConfig>,
    ) -> Box<BusDriver> {
        /**********/
        /* FD CAN */
        /**********/
        let fd_can = CanSocket::open(&can_interface).expect("Failed to open CAN interface");
        fd_can
            .set_read_timeout(Duration::from_millis(100))
            .expect("Failed to set read timeout");
        fd_can
            .set_write_timeout(Duration::from_millis(100))
            .expect("Failed to set write timeout");

        let linux_fd_can = LinuxCan::new(fd_can);

        let fd_transmitter = CanTransmitter::new(Mtu::CanFd64);
        let fd_node_id = CanNodeId::try_from(gateway_id).unwrap();
        let fd_receiver = CanReceiver::new(fd_node_id);

        let fd_cyphal_node_info = GetInfoResponse {
            protocol_version: Version { major: 1, minor: 0 },
            hardware_version: Version { major: 0, minor: 1 },
            software_version: Version { major: 0, minor: 1 },
            software_vcs_revision_id: 0,
            unique_id: rand::random(),
            name: heapless::Vec::from_slice(b"org.bluesat.obc.vesc").unwrap(),
            software_image_crc: heapless::Vec::new(),
            certificate_of_authenticity: Default::default(),
        };

        let fd_queue_driver: FdCanQueue = SingleQueueDriver::new(ArrayQueue::new(), linux_fd_can);

        let mut fd_node = BasicNode::new(
            CoreNode::new(
                SystemClock::new(),
                fd_node_id,
                fd_transmitter,
                fd_receiver,
                fd_queue_driver,
            ),
            fd_cyphal_node_info,
        )
        .unwrap();

        /**************/
        /* Normal CAN */
        /**************/
        let classic_can = CanSocket::open(&"can-vesc").expect("Failed to open CAN interface");
        classic_can
            .set_read_timeout(Duration::from_millis(100))
            .expect("Failed to set read timeout");
        classic_can
            .set_write_timeout(Duration::from_millis(100))
            .expect("Failed to set write timeout");

        let linux_classic_can = LinuxCan::new(classic_can);

        let classic_transmitter = CanTransmitter::new(Mtu::Can8);
        let classic_node_id = CanNodeId::try_from(gateway_id).unwrap();
        let classic_receiver = CanReceiver::new(classic_node_id);

        let classic_node_info = GetInfoResponse {
            protocol_version: Version { major: 1, minor: 0 },
            hardware_version: Version { major: 0, minor: 1 },
            software_version: Version { major: 0, minor: 1 },
            software_vcs_revision_id: 0,
            unique_id: rand::random(),
            name: heapless::Vec::from_slice(b"org.bluesat.obc.vesc").unwrap(),
            software_image_crc: heapless::Vec::new(),
            certificate_of_authenticity: Default::default(),
        };

        let queue_driver: ClassicCanQueue =
            SingleQueueDriver::new(ArrayQueue::new(), linux_classic_can);

        let mut classic_node = BasicNode::new(
            CoreNode::new(
                SystemClock::new(),
                classic_node_id,
                classic_transmitter,
                classic_receiver,
                queue_driver,
            ),
            classic_node_info,
        )
        .unwrap();

        /***********/
        /* Devices */
        /***********/
        for besc in &bescs {
            classic_node
                .start_publishing(
                    SubjectId::from_truncating(besc.config.id),
                    MicrosecondDuration32::millis(1_000),
                    Priority::Nominal,
                )
                .unwrap();
        }
        for stepper in &steppers {
            fd_node
                .start_publishing(
                    SubjectId::from_truncating(stepper.config.id),
                    MicrosecondDuration32::millis(1_000),
                    Priority::Nominal,
                )
                .unwrap();
        }

        let mut driver = BusDriver {
            fd_node,
            classic_node,
            bescs: bescs
                .iter()
                .map(|besc| {
                    (
                        besc.name.clone(),
                        (
                            besc.config.clone(),
                            BescState {
                                device_name: besc.name.clone(),
                                measured_velocity: 0.0,
                            },
                        ),
                    )
                })
                .collect(),
            steppers: steppers
                .iter()
                .map(|stepper| {
                    (
                        stepper.name.clone(),
                        (
                            stepper.config.clone(),
                            StepperState {
                                device_name: stepper.name.clone(),
                                measured_position: 0.0,
                            },
                        ),
                    )
                })
                .collect(),
            heartbeat_time: Instant::now(),
        };
        driver.deactivate();

        Box::new(driver)
    }

    pub fn destroy(_: Box<BusDriver>) {
        // TODO: research cleanup
    }

    pub fn activate(&mut self) {
        // Does nothing
    }

    pub fn deactivate(&mut self) {
        for _ in 0..3 {
            for (_, (config, _)) in &self.bescs {
                self.classic_node
                    .publish(
                        SubjectId::from_truncating(config.id),
                        &Scalar {
                            value: half::f16::from_f64(0.0),
                        },
                    )
                    .unwrap();
            }
        }

        // Hack to make it look like the velocity is zero
        for (_, (_, state)) in &mut self.bescs {
            state.measured_velocity = 0.0;
        }
    }

    pub fn read(&mut self) -> DeviceStates {
        self.update();

        DeviceStates {
            bescs: self
                .bescs
                .iter()
                .map(|(_, (_, state))| state.clone())
                .collect(),
            steppers: self
                .steppers
                .iter()
                .map(|(_, (_, state))| state.clone())
                .collect(),
        }
    }

    pub fn write(
        &mut self,
        besc_commands: Vec<BescCommand>,
        stepper_commands: Vec<StepperCommand>,
    ) {
        for cmd in &besc_commands {
            let (config, state) = self.bescs.get_mut(&cmd.device_name).unwrap();
            self.classic_node
                .publish(
                    SubjectId::from_truncating(config.id),
                    &Scalar {
                        value: half::f16::from_f64(
                            cmd.target_velocity * (if config.invert { -1.0 } else { 1.0 }),
                        ),
                    },
                )
                .unwrap();

            // Fake the velocity
            state.measured_velocity = cmd.target_velocity;
        }

        for cmd in &stepper_commands {
            let (config, _) = self.steppers.get_mut(&cmd.device_name).unwrap();
            self.fd_node
                .publish(
                    SubjectId::from_truncating(config.id),
                    &dynamics::rotation::planar_0_1::Planar {
                        kinematics: kinematics::rotation::planar_0_1::Planar {
                            angular_position: angle::scalar_1_0::Scalar {
                                radian: cmd.target_position as f32,
                            },
                            angular_velocity: angular_velocity::scalar_1_0::Scalar {
                                radian_per_second: 0.0,
                            },
                            angular_acceleration: angular_acceleration::scalar_1_0::Scalar {
                                radian_per_second_per_second: 0.0,
                            },
                        },
                        torque: torque::scalar_1_0::Scalar { newton_meter: 0.0 },
                    },
                )
                .unwrap();
        }

        self.update();
    }

    fn update(&mut self) {
        if Instant::now() >= self.heartbeat_time {
            self.classic_node.run_per_second_tasks().unwrap();
            self.fd_node.run_per_second_tasks().unwrap();
            self.heartbeat_time += Duration::from_millis(1000);
        }

        struct Handler<'a> {
            bescs: &'a mut BTreeMap<String, (BescConfig, BescState)>,
        }
        impl<'a> TransferHandler<CanTransport> for Handler<'a> {
            // TODO: handle incoming messages
        }

        self.classic_node.flush().unwrap();
        self.fd_node.flush().unwrap();
        /*self.cyphal_node
        .receive(&mut Handler {
            bescs: &mut self.bescs,
        })
        .unwrap();*/
    }
}
