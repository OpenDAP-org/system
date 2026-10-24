//! Two contacts (input events §3.2): a pinch (their distance changes: scale) and a rotation
//! (their angle changes), about their centre, phase by phase. Faceplates with multitouch use
//! them; the T1's touch panel reports up to ten contacts.

use crate::space::angle_delta;

/// A two-contact gesture's state: where the contacts started, so scale and rotation are derived
/// from positions, never summed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TwoFinger {
    start_distance: f32,
    start_angle: f32,
    turned: f32,
    last_angle: f32,
}

/// What the two contacts did, relative to their start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// The distance now over the distance at the start (a pinch).
    pub scale: f32,
    /// Degrees turned since the start, clockwise, unwrapped across 0°/360° (a rotation).
    pub rotation: f32,
    /// The point between the contacts now.
    pub centre: (f32, f32),
}

fn measure(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let distance = (dx * dx + dy * dy).sqrt();
    let angle = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
    (distance, angle)
}

impl TwoFinger {
    /// Both contacts down, at `a` and `b`.
    pub fn start(a: (f32, f32), b: (f32, f32)) -> Self {
        let (d, angle) = measure(a, b);
        TwoFinger { start_distance: d.max(1.0), start_angle: angle, turned: 0.0, last_angle: angle }
    }

    /// The contacts moved to `a` and `b`.
    pub fn update(&mut self, a: (f32, f32), b: (f32, f32)) -> Transform {
        let (d, angle) = measure(a, b);
        self.turned += angle_delta(self.last_angle, angle);
        self.last_angle = angle;
        Transform {
            scale: d / self.start_distance,
            rotation: self.turned,
            centre: ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0),
        }
    }

    pub fn start_angle(&self) -> f32 {
        self.start_angle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinch_scales_by_the_distance_and_rotation_unwraps() {
        let mut g = TwoFinger::start((100.0, 100.0), (200.0, 100.0));
        let t = g.update((50.0, 100.0), (250.0, 100.0));
        assert!((t.scale - 2.0).abs() < 1e-4);
        assert_eq!(t.centre, (150.0, 100.0));
        // A quarter turn clockwise around the centre, in two steps.
        g.update((115.0, 65.0), (185.0, 135.0));
        let t = g.update((150.0, 50.0), (150.0, 150.0));
        assert!((t.rotation - 90.0).abs() < 1e-3, "{}", t.rotation);
    }
}
