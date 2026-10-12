use core::f32::consts;
use micromath::F32Ext;

/// A value in a reference frame that moves with the electrical angle of the
/// motor. The two axes are orthogonal.
#[derive(Debug, Clone)]
pub struct RotatingReferenceFrame {
    pub d: f32,
    pub q: f32,
}

/// A value in a reference frame that is stationary. The two axes are
/// orthogonal.
#[derive(Debug, Clone)]
pub struct TwoPhaseReferenceFrame {
    pub alpha: f32,
    pub beta: f32,
}

/// A three-phase value in a stationary reference frame. The values do not
/// necessarily sum to 0.
#[derive(Debug, Clone)]
pub struct ThreePhaseReferenceFrame {
    pub a: f32,
    pub b: f32,
    pub c: f32,
}

/// A three-phase value in a stationary reference frame, where the three values
/// sum to 0. As such, the third value is not given.
#[derive(Debug, Clone)]
pub struct ThreePhaseBalancedReferenceFrame {
    pub a: f32,
    pub b: f32,
}

/// Inverse Park transform
///
/// Implements equations 10 and 11 from the Microsemi guide.
/// Yoinked from `foc` crate!
pub fn inverse_park(angle: f32, inputs: RotatingReferenceFrame) -> TwoPhaseReferenceFrame {
    TwoPhaseReferenceFrame {
        // Eq10
        alpha: angle.cos() * inputs.d - angle.sin() * inputs.q,
        // Eq11
        beta: angle.sin() * inputs.d + angle.cos() * inputs.q,
    }
}

/// Generate PWM values based on a space-vector method.
///
/// This method results in a waveform that is more efficient than sinusoidal
/// PWM while having better current ripple than the other methods. However, it
/// comes at the expense of a more complex computation.
///
/// Returns a value between -1 and 1 for each channel.
/// Yoinked from `foc` crate!
pub fn modulate_spacevector(value: TwoPhaseReferenceFrame) -> [f32; 3] {
    // Convert alpha/beta to x/y/z
    let sqrt_3_alpha = consts::SQRT_3 * value.alpha;
    let beta = value.beta;
    let x = beta;
    let y = (beta + sqrt_3_alpha) / 2.0;
    let z = (beta - sqrt_3_alpha) / 2.0;

    // Calculate which sector the value falls in
    let sector: u8 = match (
        x.is_sign_positive(),
        y.is_sign_positive(),
        z.is_sign_positive(),
    ) {
        (true, true, false) => 1,
        (_, true, true) => 2,
        (true, false, true) => 3,
        (false, false, true) => 4,
        (_, false, false) => 5,
        (false, true, false) => 6,
    };

    // Map a,b,c values to three phase
    let (ta, tb, tc);
    match sector {
        1 | 4 => {
            ta = x - z;
            tb = x + z;
            tc = -x + z;
        }
        2 | 5 => {
            ta = y - z;
            tb = y + z;
            tc = -y - z;
        }
        3 | 6 => {
            ta = y - x;
            tb = -y + x;
            tc = -y - x;
        }
        _ => unreachable!("invalid sector"),
    }

    [ta, tb, tc]
}
