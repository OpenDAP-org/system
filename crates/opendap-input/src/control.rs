//! A control used by its role (§3.3): its press decided when it ends, its long press and hold
//! while it is down, once each, and its repeat at accelerating intervals. A pure state machine:
//! the caller feeds the edges and asks when to look again, so it runs the same under the shell's
//! event loop, a test or a simulator.

use crate::thresholds::{
    HOLD_MS, LONG_MS, REPEAT_DELAY_MS, REPEAT_FACTOR, REPEAT_FASTEST_MS, REPEAT_FIRST_MS,
};

/// What a control did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlEvent {
    /// Down and up before the long-press time: fires on up.
    Press,
    /// Held past the long-press time: fires while down, once.
    LongPress,
    /// Held past the hold time: fires while down, once.
    Hold,
    /// While held, where the meaning repeats; the count starts at 1.
    Repeat(u32),
}

/// What the control's meaning wants here (§5.1): repeats where it means something (focus, a
/// value), a long press where one is bound; never both (one press means one thing, T1-063).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Press only (Action without a long action bound).
    Press,
    /// Press, then long press and hold while down (Main).
    LongPress,
    /// Press, then repeats while down (Volume in focus navigation).
    Repeat,
}

#[derive(Debug, Clone)]
pub struct Control {
    kind: Kind,
    down_at: Option<u64>,
    long_fired: bool,
    hold_fired: bool,
    repeats: u32,
    next_repeat: Option<u64>,
    interval: f64,
}

impl Control {
    pub fn new(kind: Kind) -> Self {
        Control {
            kind,
            down_at: None,
            long_fired: false,
            hold_fired: false,
            repeats: 0,
            next_repeat: None,
            interval: REPEAT_FIRST_MS,
        }
    }

    pub fn down(&mut self, now_ms: u64) {
        *self = Control::new(self.kind);
        self.down_at = Some(now_ms);
        if self.kind == Kind::Repeat {
            self.next_repeat = Some(now_ms + REPEAT_DELAY_MS);
        }
    }

    /// Released: the press, unless a long press, a hold or a repeat already spoke for it.
    pub fn up(&mut self, _now_ms: u64) -> Option<ControlEvent> {
        let was_down = self.down_at.take().is_some();
        let spoken = self.long_fired || self.hold_fired || self.repeats > 0;
        self.next_repeat = None;
        (was_down && !spoken).then_some(ControlEvent::Press)
    }

    /// When to call [`Control::tick`] next, if anything is due while it is down.
    pub fn next_due(&self) -> Option<u64> {
        let down = self.down_at?;
        match self.kind {
            Kind::Press => None,
            Kind::LongPress if !self.long_fired => Some(down + LONG_MS),
            Kind::LongPress if !self.hold_fired => Some(down + HOLD_MS),
            Kind::LongPress => None,
            Kind::Repeat => self.next_repeat,
        }
    }

    /// What is due at `now_ms` while it is down.
    pub fn tick(&mut self, now_ms: u64) -> Option<ControlEvent> {
        let down = self.down_at?;
        match self.kind {
            Kind::Press => None,
            Kind::LongPress => {
                if !self.long_fired && now_ms >= down + LONG_MS {
                    self.long_fired = true;
                    Some(ControlEvent::LongPress)
                } else if self.long_fired && !self.hold_fired && now_ms >= down + HOLD_MS {
                    self.hold_fired = true;
                    Some(ControlEvent::Hold)
                } else {
                    None
                }
            }
            Kind::Repeat => {
                let due = self.next_repeat?;
                if now_ms < due {
                    return None;
                }
                self.repeats += 1;
                self.next_repeat = Some(now_ms + self.interval as u64);
                self.interval = (self.interval * REPEAT_FACTOR).max(REPEAT_FASTEST_MS);
                Some(ControlEvent::Repeat(self.repeats))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_press_fires_on_up_only() {
        let mut c = Control::new(Kind::LongPress);
        c.down(0);
        assert_eq!(c.tick(300), None);
        assert_eq!(c.up(320), Some(ControlEvent::Press));
        assert_eq!(c.up(400), None, "nothing without a down");
    }

    #[test]
    fn long_press_then_hold_fire_while_down_and_nothing_on_up() {
        let mut c = Control::new(Kind::LongPress);
        c.down(1000);
        assert_eq!(c.next_due(), Some(1500));
        assert_eq!(c.tick(1500), Some(ControlEvent::LongPress));
        assert_eq!(c.next_due(), Some(3000));
        assert_eq!(c.tick(2000), None);
        assert_eq!(c.tick(3000), Some(ControlEvent::Hold));
        assert_eq!(c.next_due(), None);
        assert_eq!(c.up(3500), None);
    }

    #[test]
    fn repeats_accelerate_and_stop_where_released() {
        let mut c = Control::new(Kind::Repeat);
        c.down(0);
        let mut at = Vec::new();
        while let Some(due) = c.next_due() {
            if due > 2000 {
                break;
            }
            assert!(matches!(c.tick(due), Some(ControlEvent::Repeat(_))));
            at.push(due);
        }
        assert_eq!(&at[..4], &[400, 650, 850, 1010]);
        let gaps: Vec<u64> = at.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.windows(2).all(|g| g[1] <= g[0]), "never slower");
        assert_eq!(*gaps.last().unwrap(), 50, "down to the fastest");
        assert_eq!(c.up(2001), None, "repeated: no press after");
        assert_eq!(c.next_due(), None, "no step after the release");
    }
}
