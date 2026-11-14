use canadensis::core::SubjectId;

pub mod presets;

pub struct AppConfig {
    pub comms: CommsConfig,
    pub motion: MotionConfig,
}

pub struct CommsConfig {
    // Cyphal node id
    pub node_id: u8,
    // Cyphal subject id to subscribe for duty cycle messages
    pub ctrl_duty: SubjectId,
    // TODO Motor inversion here?
}

pub struct MotionConfig {
    pub idle_mode: IdleMode,
}

pub enum IdleMode {
    // Connect phases to ground
    Ground,
    // Leave phases high-impedance
    HiZ,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            comms: CommsConfig {
                node_id: 1,
                ctrl_duty: SubjectId::from_truncating(10),
            },
            motion: MotionConfig {
                idle_mode: IdleMode::HiZ,
            },
        }
    }
}
