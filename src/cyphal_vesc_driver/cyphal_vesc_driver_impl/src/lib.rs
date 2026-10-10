use canadensis::Node;
use socketcan::{id, CanFdSocket, Socket};

use canadensis::core::time::MicrosecondDuration32;
use canadensis::core::{Priority, SubjectId};
use canadensis::node::data_types::{GetInfoResponse, Version};
use canadensis::node::{BasicNode, CoreNode};
use canadensis::requester::TransferIdFixedMap;
use canadensis_can::queue::{ArrayQueue, SingleQueueDriver};
use canadensis_can::{CanNodeId, CanReceiver, CanTransmitter, CanTransport, Mtu};
use canadensis_data_types::reg::udral::service::actuator::common::sp::scalar_0_1::Scalar;

use std::time::{Duration, Instant};
use std::vec::Vec;

use canadensis_linux::{LinuxCan, SystemClock};

use crate::ffi::{BescCommands, BescState};

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

type FDQueue = SingleQueueDriver<SystemClock, ArrayQueue<QUEUE_CAPACITY>, LinuxCan<CanFdSocket>>;

type BescCoreNode = CoreNode<
    SystemClock,
    CanTransmitter<SystemClock, FDQueue>,
    CanReceiver<SystemClock, FDQueue>,
    TransferIdFixedMap<CanTransport, TRANSFER_IDS>,
    FDQueue,
    PUBLISHERS,
    REQUESTERS,
>;
type BescCyphalNode = BasicNode<BescCoreNode>;

#[cxx::bridge]
mod ffi {
    struct BescState {
        id: u16,
        measured_velocity: f32,
    }

    struct BescCommands {
        id: u16,
        target_velocity: f32,
    }

    extern "Rust" {
        type BescDriver;

        #[Self = "BescDriver"]
        fn create(can_interface: String, node_id: u8, ids: Vec<u16>) -> Box<BescDriver>;
        #[Self = "BescDriver"]
        fn destroy(driver: Box<BescDriver>);

        fn activate(&mut self);
        fn deactivate(&mut self);

        fn read(&mut self) -> Vec<BescState>;
        fn write(&mut self, commands: Vec<BescCommands>);

    }
}

struct BescDriver {
    cyphal_node: BescCyphalNode,
    device_ids: Vec<u16>,
    heartbeat_time: Instant,
    // TODO: this is to fudge the returned state
    prev_commands: Vec<BescCommands>,
}

impl BescDriver {
    pub fn create(can_interface: String, gateway_id: u8, device_ids: Vec<u16>) -> Box<BescDriver> {
        let can = CanFdSocket::open(&can_interface).expect("Failed to open CAN interface");
        can.set_read_timeout(Duration::from_millis(100))
            .expect("Failed to set read timeout");
        can.set_write_timeout(Duration::from_millis(100))
            .expect("Failed to set write timeout");

        let linux_can = LinuxCan::new(can);

        let transmitter = CanTransmitter::new(Mtu::CanFd64);
        let node_id = CanNodeId::try_from(gateway_id).unwrap();
        let receiver = CanReceiver::new(node_id);

        let cyphal_node_info = GetInfoResponse {
            protocol_version: Version { major: 1, minor: 0 },
            hardware_version: Version { major: 0, minor: 1 },
            software_version: Version { major: 0, minor: 1 },
            software_vcs_revision_id: 0,
            unique_id: rand::random(),
            name: heapless::Vec::from_slice(b"org.bluesat.obc.vesc").unwrap(),
            software_image_crc: heapless::Vec::new(),
            certificate_of_authenticity: Default::default(),
        };

        let queue_driver: FDQueue = SingleQueueDriver::new(ArrayQueue::new(), linux_can);

        let mut node: BescCoreNode = CoreNode::new(
            SystemClock::new(),
            node_id,
            transmitter,
            receiver,
            queue_driver,
        );

        for id in &device_ids {
            node.start_publishing(
                SubjectId::from_truncating(*id),
                MicrosecondDuration32::millis(1_000),
                Priority::Nominal,
            )
            .unwrap();
        }

        let mut driver = BescDriver {
            cyphal_node: BasicNode::new(node, cyphal_node_info).unwrap(),
            device_ids,
            heartbeat_time: Instant::now(),
            prev_commands: vec![],
        };
        driver.deactivate();

        Box::new(driver)
    }

    pub fn destroy(driver: Box<BescDriver>) {
        // TODO: research cleanup
    }

    pub fn activate(&mut self) {
        // Does nothing
    }

    pub fn deactivate(&mut self) {
        for _ in 0..3 {
            for id in &self.device_ids {
                self.cyphal_node
                    .publish(
                        SubjectId::from_truncating(*id),
                        &Scalar {
                            value: half::f16::from_f32(0.0),
                        },
                    )
                    .unwrap();
            }
        }

        // Hack to make it look like the velocity is zero
        self.prev_commands = self
            .device_ids
            .iter()
            .map(|id| BescCommands {
                id: *id,
                target_velocity: 0.0,
            })
            .collect();
    }

    pub fn read(&mut self) -> Vec<ffi::BescState> {
        self.maybe_heartbeat();

        self.prev_commands
            .iter()
            .map(|cmd| BescState {
                id: cmd.id,
                measured_velocity: cmd.target_velocity,
            })
            .collect()
    }

    pub fn write(&mut self, commands: Vec<ffi::BescCommands>) {
        self.maybe_heartbeat();

        for cmd in &commands {
            self.cyphal_node
                .publish(
                    SubjectId::from_truncating(cmd.id),
                    &Scalar {
                        value: half::f16::from_f32(cmd.target_velocity),
                    },
                )
                .unwrap();
        }
        self.prev_commands = commands;
    }

    fn maybe_heartbeat(&mut self) {
        if Instant::now() >= self.heartbeat_time {
            self.cyphal_node.run_per_second_tasks().unwrap();
            self.heartbeat_time += Duration::from_millis(1000);
        }
    }
}
