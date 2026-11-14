use core::{f32::consts, mem::MaybeUninit};

use stm32g4xx_hal::gpio::{Alternate, AnyPin};

use crate::bsp::six_pwm::STM32G4xxSixPwmDriver;

pub struct AppState {
    pub motioncontrol: MotionControlState,
    pub commutation: CommutationState,
}

pub enum MotionControlState {
    Position,
    Velocity,
    None,
}

pub enum CommutationMode {
    /* Current {
        current: I16F16,
    }, TODO: After Jonah implements current sensing (assuming we do the transform) */
    // Voltage { voltage: f32 }, // vvvv + FIXME: Vbus sense
    DutyCycle { duty: f32 }, // Testing -> Hall effects
    Disabled,
}

pub const K_HALL_TABLE: [f32; 8] = [
    0.0,  // 000 = xxx impossible
    3.0,  // 001 = 3 (180 deg)
    -1.0, // 010 = 5 (300 deg --> -060 deg)
    -2.0, // 011 = 4 (240 deg --> -120 deg)
    1.0,  // 100 = 1 (060 deg)
    2.0,  // 101 = 2 (120 deg)
    0.0,  // 110 = 0 (000 deg)
    0.0,  // 111 = xxx impossible
];
pub const K_HALL_MAX: f32 = K_HALL_TABLE[0b010];
pub const K_HALL_MIN: f32 = K_HALL_TABLE[0b110];

// All angles & velocities in electrical radians!!! (and rad/s)
pub struct CommutationState {
    pub halls: MaybeUninit<(
        AnyPin<Alternate<2>>,
        AnyPin<Alternate<2>>,
        AnyPin<Alternate<2>>,
    )>,
    pub mode: CommutationMode,
    pub hall_state: f32,
    pub glitch_accum: u32,
    pub velocity_raw: f32,
    pub full_revs: i32,
    // pub velocity: f32, TODO filtered in motion control interrupt!
    pub pwm_driver: MaybeUninit<STM32G4xxSixPwmDriver>,
}

impl CommutationState {
    pub const fn get_angle(&self) -> f32 {
        self.hall_state * consts::FRAC_PI_3
    }

    pub fn get_angle_force_hw(&self) -> f32 {
        self.get_halls() * consts::FRAC_PI_3
    }

    pub fn get_halls(&self) -> f32 {
        let halls = unsafe { self.halls.assume_init_ref() };
        K_HALL_TABLE[(usize::from(halls.0.is_high()) << 2)
            + (usize::from(halls.1.is_high()) << 1)
            + usize::from(halls.2.is_high())]
    }
}
