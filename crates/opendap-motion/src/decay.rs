//! A fling's deceleration: exponential decay of velocity (iOS's `decelerationRate`, Android's
//! fling with friction), its position at any time and where it stops, so a list can snap to the
//! item it will stop on before it gets there.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decay {
    /// The fraction of velocity kept per millisecond (iOS's normal rate: 0.998).
    pub rate: f32,
}

impl Decay {
    pub const NORMAL: Decay = Decay { rate: 0.998 };

    fn k(&self) -> f32 {
        // Per second.
        -(self.rate.clamp(1e-6, 0.999_999).ln()) * 1000.0
    }

    /// Position and velocity `t` seconds after leaving `x0` at `v0`.
    pub fn at(&self, x0: f32, v0: f32, t: f32) -> (f32, f32) {
        let k = self.k();
        let e = (-k * t).exp();
        (x0 + v0 / k * (1.0 - e), v0 * e)
    }

    /// Where it stops.
    pub fn end(&self, x0: f32, v0: f32) -> f32 {
        x0 + v0 / self.k()
    }

    /// When it is slower than `min` px/s (stopped for the eye).
    pub fn duration(&self, v0: f32, min: f32) -> f32 {
        if v0.abs() <= min {
            return 0.0;
        }
        (v0.abs() / min).ln() / self.k()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fling_slows_and_stops_where_predicted() {
        let d = Decay::NORMAL;
        let end = d.end(0.0, 2000.0);
        assert!((end - 999.0).abs() < 2.0, "{end}");
        let t = d.duration(2000.0, 5.0);
        let (x, v) = d.at(0.0, 2000.0, t);
        assert!(v.abs() <= 5.01 && (end - x).abs() < 3.0);
        assert_eq!(d.duration(3.0, 5.0), 0.0);
    }
}
