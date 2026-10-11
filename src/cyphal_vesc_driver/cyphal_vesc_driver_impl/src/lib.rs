use canadensis::{Node, TransferHandler};
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

use crate::ffi::{BescCommands, BescConfig, BescState, NamedBescConfig};

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

type CanQueue = SingleQueueDriver<SystemClock, ArrayQueue<QUEUE_CAPACITY>, LinuxCan<CanSocket>>;

type BescCoreNode = CoreNode<
    SystemClock,
    CanTransmitter<SystemClock, CanQueue>,
    CanReceiver<SystemClock, CanQueue>,
    TransferIdFixedMap<CanTransport, TRANSFER_IDS>,
    CanQueue,
    PUBLISHERS,
    REQUESTERS,
>;
type BescCyphalNode = BasicNode<BescCoreNode>;

#[cxx::bridge]
mod ffi {
    #[derive(Debug, Clone)]
    struct BescConfig {
        id: u16,
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
    struct BescCommands {
        device_name: String,
        target_velocity: f64,
    }

    extern "Rust" {
        type BescDriver;

        #[Self = "BescDriver"]
        fn create(
            can_interface: String,
            node_id: u8,
            bescs: Vec<NamedBescConfig>,
        ) -> Box<BescDriver>;
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
    bescs: BTreeMap<String, (BescConfig, BescState)>,
    heartbeat_time: Instant,
}

impl BescDriver {
    pub fn create(
        can_interface: String,
        gateway_id: u8,
        bescs: Vec<NamedBescConfig>,
    ) -> Box<BescDriver> {
        let can = CanSocket::open(&can_interface).expect("Failed to open CAN interface");
        can.set_read_timeout(Duration::from_millis(100))
            .expect("Failed to set read timeout");
        can.set_write_timeout(Duration::from_millis(100))
            .expect("Failed to set write timeout");

        let linux_can = LinuxCan::new(can);

        let transmitter = CanTransmitter::new(Mtu::Can8);
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

        let queue_driver: CanQueue = SingleQueueDriver::new(ArrayQueue::new(), linux_can);

        let mut cyphal_node = BasicNode::new(
            CoreNode::new(
                SystemClock::new(),
                node_id,
                transmitter,
                receiver,
                queue_driver,
            ),
            cyphal_node_info,
        )
        .unwrap();

        for besc in &bescs {
            cyphal_node
                .start_publishing(
                    SubjectId::from_truncating(besc.config.id),
                    MicrosecondDuration32::millis(1_000),
                    Priority::Nominal,
                )
                .unwrap();
        }

        let mut driver = BescDriver {
            cyphal_node: cyphal_node,
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
            heartbeat_time: Instant::now(),
        };
        driver.deactivate();

        Box::new(driver)
    }

    pub fn destroy(_: Box<BescDriver>) {
        // TODO: research cleanup
    }

    pub fn activate(&mut self) {
        // Does nothing
    }

    pub fn deactivate(&mut self) {
        for _ in 0..3 {
            for (_, (config, _)) in &self.bescs {
                self.cyphal_node
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

    pub fn read(&mut self) -> Vec<BescState> {
        self.update();
        self.bescs
            .iter()
            .map(|(_, (_, state))| state.clone())
            .collect()
    }

    pub fn write(&mut self, commands: Vec<BescCommands>) {
        for cmd in &commands {
            let (config, state) = self.bescs.get_mut(&cmd.device_name).unwrap();
            self.cyphal_node
                .publish(
                    SubjectId::from_truncating(config.id),
                    &Scalar {
                        value: half::f16::from_f64(cmd.target_velocity),
                    },
                )
                .unwrap();

            // Fake the velocity
            state.measured_velocity = cmd.target_velocity;
        }

        self.update();
    }

    fn update(&mut self) {
        if Instant::now() >= self.heartbeat_time {
            self.cyphal_node.run_per_second_tasks().unwrap();
            self.heartbeat_time += Duration::from_millis(1000);
        }

        struct Handler<'a> {
            bescs: &'a mut BTreeMap<String, (BescConfig, BescState)>,
        }
        impl<'a> TransferHandler<CanTransport> for Handler<'a> {
            // TODO: handle incoming messages
        }

        self.cyphal_node.flush().unwrap();
        return;
        self.cyphal_node
            .receive(&mut Handler {
                bescs: &mut self.bescs,
            })
            .unwrap();
    }
}
