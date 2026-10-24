//! Cubic-bezier easing, solved for x exactly (Newton's method, then bisection), as CSS and the
//! motion tokens define curves: progress 0–1 in, eased progress out.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl Bezier {
    pub const LINEAR: Bezier = Bezier { x1: 0.0, y1: 0.0, x2: 1.0, y2: 1.0 };
    /// CSS `ease-out`; the motion tokens name the design system's own.
    pub const EASE_OUT: Bezier = Bezier { x1: 0.0, y1: 0.0, x2: 0.58, y2: 1.0 };
    /// The design system's standard easing (Material's standard: 0.2, 0, 0, 1).
    pub const STANDARD: Bezier = Bezier { x1: 0.2, y1: 0.0, x2: 0.0, y2: 1.0 };

    fn coord(t: f32, p1: f32, p2: f32) -> f32 {
        let u = 1.0 - t;
        3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
    }

    fn slope(t: f32, p1: f32, p2: f32) -> f32 {
        let u = 1.0 - t;
        3.0 * u * u * p1 + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
    }

    /// The eased value at progress `x` (0–1).
    pub fn at(&self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        if x == 0.0 || x == 1.0 {
            return x;
        }
        let mut t = x;
        for _ in 0..8 {
            let err = Self::coord(t, self.x1, self.x2) - x;
            if err.abs() < 1e-6 {
                return Self::coord(t, self.y1, self.y2);
            }
            let d = Self::slope(t, self.x1, self.x2);
            if d.abs() < 1e-6 {
                break;
            }
            t -= err / d;
        }
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        t = x;
        for _ in 0..40 {
            let v = Self::coord(t, self.x1, self.x2);
            if (v - x).abs() < 1e-6 {
                break;
            }
            if v < x { lo = t } else { hi = t }
            t = (lo + hi) / 2.0;
        }
        Self::coord(t, self.y1, self.y2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_hold_their_ends_and_shape() {
        assert_eq!(Bezier::LINEAR.at(0.3), 0.3);
        for b in [Bezier::EASE_OUT, Bezier::STANDARD] {
            assert_eq!(b.at(0.0), 0.0);
            assert_eq!(b.at(1.0), 1.0);
            assert!(b.at(0.5) > 0.5, "decelerating curves are past half at half time");
            let mut last = 0.0;
            for i in 1..=100 {
                let v = b.at(i as f32 / 100.0);
                assert!(v >= last - 1e-4, "monotonic");
                last = v;
            }
        }
        assert!((Bezier::EASE_OUT.at(0.5) - 0.684).abs() < 0.01, "CSS ease-out at half time");
    }
}
