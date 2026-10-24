//! An interruptible animation of one value (ui-model §8: everything is interruptible): a new
//! target takes over from where the value is, with the velocity it has, never jumping and never
//! waiting for the old animation to end. Timed (a curve over a duration, for what the system
//! starts) or a spring (for what a hand lets go). Reduce motion settles at once (DS-012).

use crate::easing::Bezier;
use crate::spring::Spring;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Timed { duration: f32, curve: Bezier },
    Spring(Spring),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Animator {
    kind: Kind,
    from: f32,
    velocity: f32,
    target: f32,
    start: f32,
    /// Reduce motion: every change lands at once.
    pub reduce_motion: bool,
}

impl Animator {
    pub fn new(value: f32, kind: Kind) -> Self {
        Animator { kind, from: value, velocity: 0.0, target: value, start: 0.0, reduce_motion: false }
    }

    /// The value and its velocity at `now` (seconds).
    pub fn sample(&self, now: f32) -> (f32, f32) {
        if self.reduce_motion {
            return (self.target, 0.0);
        }
        let t = (now - self.start).max(0.0);
        match self.kind {
            Kind::Spring(s) => s.at(self.from, self.velocity, self.target, t),
            Kind::Timed { duration, curve } => {
                if duration <= 0.0 || t >= duration {
                    return (self.target, 0.0);
                }
                let p = curve.at(t / duration);
                let h = 1e-3;
                let p2 = curve.at(((t + h) / duration).min(1.0));
                let span = self.target - self.from;
                (self.from + span * p, span * (p2 - p) / h)
            }
        }
    }

    /// A new target at `now`: from where the value is, with its velocity (and a hand's
    /// `release_velocity`, when it was thrown).
    pub fn retarget(&mut self, target: f32, now: f32, release_velocity: Option<f32>) {
        let (x, v) = self.sample(now);
        self.from = x;
        self.velocity = release_velocity.unwrap_or(v);
        self.target = target;
        self.start = now;
    }

    /// Whether it still moves at `now` (frames are needed only while it does).
    pub fn running(&self, now: f32) -> bool {
        if self.reduce_motion {
            return false;
        }
        match self.kind {
            Kind::Timed { duration, .. } => now - self.start < duration,
            Kind::Spring(s) => !s.settled(self.from, self.velocity, self.target, now - self.start, 0.05),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_target_takes_over_without_a_jump() {
        let mut a = Animator::new(0.0, Kind::Timed { duration: 0.2, curve: Bezier::STANDARD });
        a.retarget(100.0, 0.0, None);
        let (mid, v) = a.sample(0.1);
        assert!(mid > 0.0 && mid < 100.0 && v > 0.0);
        a.retarget(0.0, 0.1, None);
        let (now, _) = a.sample(0.1);
        assert!((now - mid).abs() < 1e-3, "continues from where it was");
        assert!(!a.running(0.31));
        assert_eq!(a.sample(1.0).0, 0.0);
    }

    #[test]
    fn a_thrown_value_springs_with_the_hand_velocity_and_reduce_motion_lands_at_once() {
        let mut a = Animator::new(0.0, Kind::Spring(Spring::SETTLE));
        a.retarget(0.0, 0.0, Some(1500.0));
        assert!(a.sample(0.05).0 > 20.0, "carried by the throw");
        assert!(a.running(0.05));
        assert!(!a.running(3.0));
        a.reduce_motion = true;
        a.retarget(300.0, 3.0, None);
        assert_eq!(a.sample(3.0), (300.0, 0.0));
        assert!(!a.running(3.0));
    }
}
