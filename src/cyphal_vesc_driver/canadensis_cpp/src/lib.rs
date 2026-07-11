use socketcan::{CanFdSocket};

use canadensis::requester::TransferIdFixedMap;
use canadensis::core::{SubjectId, Priority};
use canadensis::core::time::MicrosecondDuration32;
use canadensis::node::{BasicNode, CoreNode};
use canadensis::node::data_types::{GetInfoResponse, Version};
use canadensis_can::queue::{ArrayQueue, SingleQueueDriver};
use canadensis_can::{CanTransmitter, Mtu, CanNodeId, CanReceiver, CanTransport};
use canadensis_data_types::reg::udral::service::actuator::comon::sp::scalar_0_1::Scalar as Scalar;

use std::thread;
use std::time::Duration;
use std::vec::Vec;

use canadensis_linux::{LinuxCan, SystemClock};

struct Members<N> {
   data: Box<Data<N>>,
}

struct Data <N> {
    cyphal_node: BasicNode<N>
}

const VESC_SPEED_SUB: [u16; 4] = [3050, 3060, 3070, 3080];

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

impl Members {
    pub fn on_init(can_interface: String, node_id: u8) -> Self {
        // start telling to go 0 rads
        let can = CanFdSocket::open(&can_interface);
        can.set_read_timeout(Duration::from_millis(100))?;
        can.set_write_timeout(Duration::from_millis(100))?;

        let linux_can = LinuxCan::new(can);
 
        let transmitter = CanTransmitter::new(Mtu::CanFd64);
        let node_id = CanNodeId::try_from(node_id);
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

        const QUEUE_CAPACITY: usize = 1210;
        type FDQueue = SingleQueueDriver<SystemClock, ArrayQueue<QUEUE_CAPACITY>, LinuxCan<CanFdSocket>>;
        let queue_driver: FDQueue = SingleQueueDriver::new(ArrayQueue::new(), linux_can);

        let node: CoreNode<
            SystemClock,
            CanTransmitter<SystemClock, FDQueue>,
            CanReceiver<SystemClock>,
            TransferIdFixedMap<CanTransport, TRANSFER_IDS>,
            FDQueue,
            PUBLISHERS,
            REQUESTERS,
        > = CoreNode::new(
            SystemClock::new(),
            node_id,
            transmitter,
            receiver,
            queue_driver,
        );
        let mut node = BasicNode::new(node, cyphal_node_info);
   
        for subject in VESC_SPEED_SUB {
            node.start_publishing(
                SubjectId::from_truncating(),
                MicrosecondDuration32::millis(1_000),
                Priority::Nominal
            ).unwrap();
        }

        for _ in 0..3 {
            for i : VESC_SPEED_SUB {
                let besc = Scalar {
                    value: 0
                };
                node.publish(i.try_into().unwrap(), &besc).unwrap();
            }
            node.flush().unwrap();
        }

        node.flush().unwrap();
        
        Members { data: Data { cyphal_node } }
    }    

    pub fn on_deactivate() -> {
        // should start going to 0 rads again 
        // 20 ms for periodic  
    }

    pub fn read() ->  {
       // perioidic 
        // called at peripheral freq
        // done for us so dont need to worry just broadcast 
    }

    pub fn write(&mut self, node_id: u8, message: u32) -> {
        // self.data = node

        // messages from cpp 
        // store set points in local variables 

    } 
}
