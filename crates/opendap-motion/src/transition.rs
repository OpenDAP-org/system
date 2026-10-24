//! A transition between two states (ui-model §8; input events §3.7): its progress, from 0 (where it
//! starts) to 1 (where it goes), driven by time when the system starts it and by a hand when a
//! gesture does (a swipe back that shows the screen below as the finger moves, a pager, a sheet).
//! Let go, it settles on the end the gesture meant, judged from where it is and how fast it moves,
//! with that velocity; it can be grabbed again mid-way, reversed, and lands at once with reduce
//! motion. Every property that moves with it (a position, a size, a corner, a colour, an opacity)
//! is a function of the progress (`interpolate`), so a transition is one number and its rules.

use crate::animator::{Animator, Kind};
use crate::decay::Decay;
use crate::easing::Bezier;
use crate::spring::Spring;

/// Which end a transition rests at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// Progress 0: where it started (a cancelled back).
    From,
    /// Progress 1: where it went.
    To,
}

/// The time a system-started transition takes (motion token `standard`, design system §11).
pub const DURATION_S: f32 = 0.2;

#[derive(Debug, Clone, Copy)]
pub struct Transition {
    animator: Animator,
    /// A hand holds it: the progress follows the gesture, no animation runs.
    held: Option<f32>,
    /// How many pixels of a gesture make the whole transition (a screen's width for a swipe).
    pub distance: f32,
    pub reduce_motion: bool,
}

impl Transition {
    /// At rest at `end`, a gesture of `distance` pixels making the whole way.
    pub fn new(end: End, distance: f32) -> Self {
        let p = if end == End::To { 1.0 } else { 0.0 };
        Transition { animator: Animator::new(p, Kind::Spring(Spring::SETTLE)), held: None, distance: distance.max(1.0), reduce_motion: false }
    }

    /// Where it is at `now` (seconds), 0 to 1.
    pub fn progress(&self, now: f32) -> f32 {
        if let Some(p) = self.held {
            return p;
        }
        // Settled, it is exactly at its end: a spring is "settled" within a small distance, which
        // on a 0–1 progress would leave what should be gone faintly there.
        if !self.animator.running(now) {
            return self.animator.target();
        }
        self.animator.sample(now).0.clamp(0.0, 1.0)
    }

    /// Whether frames are needed: it moves by itself.
    pub fn running(&self, now: f32) -> bool {
        self.held.is_none() && self.animator.running(now)
    }

    /// The end it rests at, once it does.
    pub fn settled(&self, now: f32) -> Option<End> {
        if self.held.is_some() || self.running(now) {
            return None;
        }
        Some(if self.animator.target() >= 0.5 { End::To } else { End::From })
    }

    /// The system starts it toward `end` (a timed curve), from wherever it is: a second start
    /// mid-way reverses it without a jump.
    pub fn go(&mut self, end: End, now: f32) {
        let at = self.progress(now);
        self.held = None;
        let to = if end == End::To { 1.0 } else { 0.0 };
        // The time left is in proportion to the way left, so a reversal does not crawl.
        let duration = DURATION_S * (to - at).abs().max(0.25);
        self.animator = Animator::new(at, Kind::Timed { duration, curve: Bezier::STANDARD });
        self.animator.reduce_motion = self.reduce_motion;
        self.animator.retarget(to, now, None);
    }

    /// A hand takes it, wherever it is (interrupting any animation).
    pub fn grab(&mut self, now: f32) {
        self.held = Some(self.progress(now));
    }

    /// The hand moved by `delta` pixels toward the end (negative: back toward the start).
    pub fn drag(&mut self, delta: f32) {
        if let Some(p) = self.held.as_mut() {
            *p = (*p + delta / self.distance).clamp(0.0, 1.0);
        }
    }

    /// The hand lets go with `velocity` (pixels a second toward the end): it settles on the end
    /// where a fling from here would stop nearer to (Android's and iOS's rule for interactive
    /// transitions), with the hand's velocity carried by a spring. Returns that end.
    pub fn release(&mut self, velocity: f32, now: f32) -> End {
        let at = self.held.take().unwrap_or_else(|| self.progress(now));
        let projected = Decay::NORMAL.end(at * self.distance, velocity) / self.distance;
        let end = if projected >= 0.5 { End::To } else { End::From };
        let to = if end == End::To { 1.0 } else { 0.0 };
        self.animator = Animator::new(at, Kind::Spring(Spring::SETTLE));
        self.animator.reduce_motion = self.reduce_motion;
        self.animator.retarget(to, now, Some(velocity / self.distance));
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_starts_it_and_it_lands() {
        let mut t = Transition::new(End::From, 320.0);
        t.go(End::To, 0.0);
        assert!(t.running(0.05));
        let mid = t.progress(0.1);
        assert!(mid > 0.3 && mid < 1.0, "{mid}");
        assert_eq!(t.settled(1.0), Some(End::To));
        assert!((t.progress(1.0) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn going_back_mid_way_reverses_without_a_jump() {
        let mut t = Transition::new(End::From, 320.0);
        t.go(End::To, 0.0);
        let at = t.progress(0.08);
        t.go(End::From, 0.08);
        assert!((t.progress(0.08) - at).abs() < 1e-4, "no jump");
        assert_eq!(t.settled(2.0), Some(End::From));
    }

    #[test]
    fn a_hand_drives_it_and_a_fling_decides_the_end() {
        let mut t = Transition::new(End::From, 320.0);
        t.grab(0.0);
        t.drag(96.0);
        assert!((t.progress(0.0) - 0.3).abs() < 1e-4, "a third of the screen: 0.3");
        assert!(!t.running(0.0), "held: no frames of its own");
        // Let go slowly at 0.3: back where it was (a cancelled swipe back).
        assert_eq!(t.release(0.0, 0.0), End::From);
        assert_eq!(t.settled(2.0), Some(End::From));
        // Again, with a flick toward the end: it goes, though only a third of the way across.
        t.grab(2.0);
        t.drag(96.0);
        assert_eq!(t.release(600.0, 2.0), End::To);
        assert_eq!(t.settled(4.0), Some(End::To));
        // Past half-way, a flick back cancels it.
        let mut t = Transition::new(End::From, 320.0);
        t.grab(0.0);
        t.drag(200.0);
        assert_eq!(t.release(-800.0, 0.0), End::From);
    }

    #[test]
    fn grabbed_mid_animation_it_stops_under_the_finger() {
        let mut t = Transition::new(End::From, 320.0);
        t.go(End::To, 0.0);
        let at = t.progress(0.06);
        t.grab(0.06);
        assert!((t.progress(0.5) - at).abs() < 1e-6, "held where it was caught");
        t.drag(-1000.0);
        assert_eq!(t.progress(0.5), 0.0, "clamped at the start");
    }

    #[test]
    fn once_settled_it_is_exactly_at_its_end() {
        let mut t = Transition::new(End::To, 190.0);
        t.grab(0.0);
        t.drag(-45.0);
        t.release(-1500.0, 0.0);
        assert_eq!(t.settled(3.0), Some(End::From));
        assert_eq!(t.progress(3.0), 0.0, "not 0.02: what closed is gone");
    }

    #[test]
    fn reduce_motion_lands_at_once() {
        let mut t = Transition::new(End::From, 320.0);
        t.reduce_motion = true;
        t.go(End::To, 0.0);
        assert_eq!(t.progress(0.0), 1.0);
        assert!(!t.running(0.0));
    }
}
