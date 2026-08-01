use stm32g4xx_hal::pac::TIM1;

pub mod foc;

// Number of pole pairs for a NEO, translates electrical revolutions
// to shaft revolutions etc...
pub const K_POLE_PAIRS: usize = 7;
pub const K_HALL_TABLE: [f32; 8] = [
    0.0, // 000 = xxx impossible
    3.0, // 001 = 3 (180 deg)
    5.0, // 010 = 5 (300 deg)
    4.0, // 011 = 4 (240 deg)
    1.0, // 100 = 1 (060 deg)
    2.0, // 101 = 2 (120 deg)
    0.0, // 110 = 0 (000 deg)
    0.0, // 111 = xxx impossible
];
pub const K_HALL_MAX: usize = 0b010;
pub const K_HALL_MIN: usize = 0b110;

// Freewheeling, TIM1.BDTR.Main Output Disable!
pub fn motor_disable() {
    unsafe {
        TIM1::steal()
            .bdtr()
            .modify(|_, w| w.aoe().manual().moe().disabled_idle());
    }
}

// TIM1.BDTR.Main Output Enable!
pub fn motor_enable() {
    unsafe {
        TIM1::steal().bdtr().modify(|_, w| w.moe().enabled());
    }
}
