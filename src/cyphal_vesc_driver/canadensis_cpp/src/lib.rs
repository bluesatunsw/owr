struct Members {
   data: Box<Data>,
}
struct Data {
    linux_can: LinuxCan<dyn Socket>,
}

let vesc_node_info = GetInfoResponse {
    protocol_version: Version { major: 1, minor: 0 },
    hardware_version: Version { major: 0, minor: 1 },
    software_version: Version { major: 0, minor: 1 },
    software_vcs_revision_id: 0,
    unique_id: rand::random(),
    name: heapless::Vec::from_slice(b"org.bluesat.obc.vesc").unwrap(),
    software_image_crc: heapless::Vec::new(),
    certificate_of_authenticity: Default::default(),
};

impl Members {
    pub fn on_init(can_interface: std::string) -> Self {
        let can = CanFdSocket::open(&can_interface);
        can.set_read_timeout(Duration::from_millis(100))?;
        can.set_write_timeout(Duration::from_millis(100))?;

        let linux_can = LinuxCan::new(can);

        Members { data: Data { linux_can } }
    }
    
    pub fn on_activate() -> {
         
    }

    pub fn on_deactivate() -> {

    }

    pub fn read() ->  {

    }

    pub fn write() ->  {

    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
