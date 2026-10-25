use crate::config::{AppConfig, CommsConfig, IdleMode, MotionConfig};
use canadensis::core::SubjectId;

pub const DRIVEBASE_FL_CONFIG: AppConfig = AppConfig {
    comms: CommsConfig {
        node_id: 10,
        ctrl_duty: SubjectId::from_truncating(3050),
    },
    motion: MotionConfig {
        idle_mode: IdleMode::Ground,
    },
};

pub const DRIVEBASE_FR_CONFIG: AppConfig = AppConfig {
    comms: CommsConfig {
        node_id: 11,
        ctrl_duty: SubjectId::from_truncating(3060),
    },
    motion: MotionConfig {
        idle_mode: IdleMode::Ground,
    },
};

pub const DRIVEBASE_BL_CONFIG: AppConfig = AppConfig {
    comms: CommsConfig {
        node_id: 12,
        ctrl_duty: SubjectId::from_truncating(3070),
    },
    motion: MotionConfig {
        idle_mode: IdleMode::Ground,
    },
};

pub const DRIVEBASE_BR_CONFIG: AppConfig = AppConfig {
    comms: CommsConfig {
        node_id: 13,
        ctrl_duty: SubjectId::from_truncating(3080),
    },
    motion: MotionConfig {
        idle_mode: IdleMode::Ground,
    },
};
