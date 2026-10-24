//! A damped spring, solved in closed form (no integration drift): where a value is and how fast
//! it moves `t` seconds after it started at `x0` with velocity `v0`, pulled toward `target`
//! (SwiftUI's and Compose's springs, UIKit dynamics). Described by its response (seconds, the
//! period it would oscillate at undamped) and damping ratio (1: critically damped, no overshoot;
//! below 1: bounce).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// Seconds: the undamped period.
    pub response: f32,
    /// 1.0 critically damped; < 1 bounces; > 1 slower without overshoot.
    pub damping: f32,
}

impl Spring {
    /// The default for what a hand lets go of (ui-model §8: critically damped, `spring(0)`).
    pub const SETTLE: Spring = Spring { response: 0.35, damping: 1.0 };

    /// Position and velocity at `t` seconds.
    pub fn at(&self, x0: f32, v0: f32, target: f32, t: f32) -> (f32, f32) {
        let w0 = std::f32::consts::TAU / self.response.max(1e-3);
        let z = self.damping.max(0.0);
        let d = x0 - target;
        if (z - 1.0).abs() < 1e-4 {
            // Critically damped: (d + (v0 + w0 d) t) e^{-w0 t}.
            let b = v0 + w0 * d;
            let e = (-w0 * t).exp();
            let x = (d + b * t) * e;
            let v = (b - w0 * (d + b * t)) * e;
            (target + x, v)
        } else if z < 1.0 {
            let wd = w0 * (1.0 - z * z).sqrt();
            let b = (v0 + z * w0 * d) / wd;
            let e = (-z * w0 * t).exp();
            let (s, c) = (wd * t).sin_cos();
            let x = e * (d * c + b * s);
            let v = e * ((b * wd - z * w0 * d) * c - (d * wd + z * w0 * b) * s);
            (target + x, v)
        } else {
            let r = w0 * (z * z - 1.0).sqrt();
            let (r1, r2) = (-z * w0 + r, -z * w0 - r);
            let c2 = (v0 - r1 * d) / (r2 - r1);
            let c1 = d - c2;
            let (e1, e2) = ((r1 * t).exp(), (r2 * t).exp());
            (target + c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
        }
    }

    /// Whether it has settled at `t`: within `epsilon` of the target, nearly still.
    pub fn settled(&self, x0: f32, v0: f32, target: f32, t: f32, epsilon: f32) -> bool {
        let (x, v) = self.at(x0, v0, target, t);
        (x - target).abs() < epsilon && v.abs() < epsilon * 10.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critically_damped_never_overshoots_and_settles() {
        let s = Spring::SETTLE;
        let mut max = f32::MIN;
        for i in 0..200 {
            let (x, _) = s.at(0.0, 0.0, 100.0, i as f32 / 100.0);
            max = max.max(x);
        }
        assert!(max <= 100.0 + 1e-3, "no overshoot: {max}");
        assert!(s.settled(0.0, 0.0, 100.0, 1.0, 0.5));
        assert_eq!(s.at(0.0, 0.0, 100.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn a_throw_carries_its_velocity() {
        let s = Spring::SETTLE;
        let (x_still, _) = s.at(0.0, 0.0, 100.0, 0.05);
        let (x_thrown, _) = s.at(0.0, 2000.0, 100.0, 0.05);
        // v0 · t · e^(−ω0 t) ≈ 2000 · 0.05 · 0.41 ≈ 41 px further.
        assert!(x_thrown > x_still + 35.0, "{x_thrown} vs {x_still}");
    }

    #[test]
    fn a_bouncy_spring_overshoots_once_then_settles() {
        let s = Spring { response: 0.4, damping: 0.5 };
        let peak = (0..100).map(|i| s.at(0.0, 0.0, 100.0, i as f32 / 100.0).0).fold(f32::MIN, f32::max);
        assert!(peak > 110.0, "{peak}");
        assert!(s.settled(0.0, 0.0, 100.0, 3.0, 0.5));
        let over = Spring { response: 0.4, damping: 2.0 };
        assert!((0..300).all(|i| over.at(0.0, 0.0, 100.0, i as f32 / 100.0).0 <= 100.0 + 1e-3));
    }
}
