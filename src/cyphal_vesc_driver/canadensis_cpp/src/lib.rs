use canadensis::requester::TransferIdFixedMap;

struct Members {
   data: Box<Data>,
}
struct Data <S, C, DT, DR> {
    linux_can: LinuxCan<S>,
    transmitter: CanTransmitter<C, DT>,
    receiver: CanReciever<C, DR>
}
const VESC_SPEED_SUB: [u16; 4] = [3050, 3060, 3070, 3080];

impl Members {
    pub fn on_init(can_interface: std::string) -> Self {
        // start telling to go 0 rads
        let can = CanFdSocket::open(&can_interface);
        can.set_read_timeout(Duration::from_millis(100))?;
        can.set_write_timeout(Duration::from_millis(100))?;

        let linux_can = LinuxCan::new(can);
 
        let transmitter = CanTransmitter::new(Mtu::CanFd64);
        let node_id = CanNodeId::(67);
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
        type FDQueue = SingleQueueDrvier<SystemClock, ArrayQueue<QUEUE_CAPACITY>, LinuxCan<CanFdSocket>>;
        let queue_driver: FDQueue = SingleQueueDriver::new(ArrayQueue::new(), can);

        let node: CoreNode<
            SystemClock,
            CanTransmitter<SystemClock, FDQueue>,
            CanReciever<SystemClock>,
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
        let mut cyphal_node = BasicNode::new(node, vesc_node_info);
   
        for subject in VESC_SPEED_SUB {
            cyphal_node.start_publishing(
                SubjectId::from_truncating(),
                MicrosecondDuration32::millis(1_000),
                Priority::Nominal
            ).unwrap();
        }
        
        Members { data: Data { linux_can, transmitter, receiver } }
    }    
    pub fn on_activate() -> {
       // start actually sending info  
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

    pub fn write() -> {
        // messages from cpp 
        // store set points in local variables 
    } 
}
