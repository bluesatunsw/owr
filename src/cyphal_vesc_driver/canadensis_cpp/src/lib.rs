use canadensis::Node;
use socketcan::{CanFdSocket, Socket};

use canadensis::core::time::MicrosecondDuration32;
use canadensis::core::{Priority, SubjectId};
use canadensis::node::data_types::{GetInfoResponse, Version};
use canadensis::node::{BasicNode, CoreNode};
use canadensis::requester::TransferIdFixedMap;
use canadensis_can::queue::{ArrayQueue, SingleQueueDriver};
use canadensis_can::{CanNodeId, CanReceiver, CanTransmitter, CanTransport, Mtu};
use canadensis_data_types::reg::udral::service::actuator::common::sp::scalar_0_1::Scalar;

use std::thread;
use std::time::Duration;
use std::vec::Vec;

use canadensis_linux::{LinuxCan, SystemClock};

const QUEUE_CAPACITY: usize = 1210;

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

const VESC_SPEED_SUB: [u16; 4] = [3050, 3060, 3070, 3080];

#[cxx::bridge]
mod ffi {
    extern "Rust" {
        type Opaque;
    }
}

struct Opaque {
    cyphal_node: Box<BescCyphalNode>,
    setpoints: [f32; 4],
}

pub fn on_init(can_interface: String, node_id: u8) -> Opaque {
    // start telling to go 0 rads
    let can = CanFdSocket::open(&can_interface).expect("Failed to open CAN interface");
    can.set_read_timeout(Duration::from_millis(100))
        .expect("Failed to set read timeout");
    can.set_write_timeout(Duration::from_millis(100))
        .expect("Failed to set write timeout");

    let linux_can = LinuxCan::new(can);

    let transmitter = CanTransmitter::new(Mtu::CanFd64);
    let node_id = CanNodeId::try_from(node_id).unwrap();
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

    let node: BescCoreNode = CoreNode::new(
        SystemClock::new(),
        node_id,
        transmitter,
        receiver,
        queue_driver,
    );
    let mut node = BasicNode::new(node, cyphal_node_info).unwrap();

    for subject in VESC_SPEED_SUB {
        node.start_publishing(
            SubjectId::from_truncating(subject),
            MicrosecondDuration32::millis(1_000),
            Priority::Nominal,
        )
        .unwrap();
    }

    for _ in 0..3 {
        for i in VESC_SPEED_SUB {
            let besc = Scalar {
                value: half::f16::from_f32(0.0),
            };
            node.publish(i.try_into().unwrap(), &besc).unwrap();
        }
        node.flush().unwrap();
    }

    node.flush().unwrap();

    Opaque {
        cyphal_node: Box::new(node),
        setpoints: [0.0, 0.0, 0.0, 0.0],
    }
}

pub fn on_deactivate(opaque: &mut Opaque) {
    // should start going to 0 rads again
    // 20 ms for periodic
    for _ in 0..3 {
        for i in VESC_SPEED_SUB {
            let besc = Scalar {
                value: half::f16::from_f32(0.0),
            };
            opaque
                .cyphal_node
                .publish(i.try_into().unwrap(), &besc)
                .unwrap();
        }
        opaque.cyphal_node.flush().unwrap();
    }
}

pub fn read(opaque: &Opaque, besc_id: usize) -> f32 {
    // perioidic
    // called at peripheral freq
    // done for us so dont need to worry just broadcast

    opaque.setpoints[besc_id]
}

pub fn write(opaque: &mut Opaque, besc_id: usize, message: f32) {
    // self.data = node

    // messages from cpp
    // store set points in local variables

    let besc = Scalar {
        value: half::f16::from_f32(message),
    };
    opaque.setpoints[besc_id] = message;
    opaque
        .cyphal_node
        .publish(VESC_SPEED_SUB[besc_id].try_into().unwrap(), &besc)
        .unwrap();
}
