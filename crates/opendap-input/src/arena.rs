//! One owner per contact (input events §3.2.1): recognizers that want the same contact compete
//! in an arena until one wins and the others are told they lost (Flutter's gesture arena). On
//! top, the compositions every framework offers (SwiftUI's): sequenced (a long press, then a
//! drag), simultaneous (both may win), exclusive (the first listed wins when several could).
//!
//! Everything is a pure state machine fed with [`Input`]s, time included, so the composer, a test
//! or a recording drive it the same way.

use crate::gesture::{Axis, DragOptions, DragStart};
use crate::thresholds::{DOUBLE_TAP_MS, LONG_MS, SLOP_PX};

/// What reaches a recognizer: the contact's edges and moves in its frame, and time passing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    Down { at: (f32, f32), ms: u64 },
    Move { at: (f32, f32), ms: u64 },
    Up { at: (f32, f32), ms: u64 },
    /// The system took the contact.
    Cancel,
    /// Time passed with no input (a long press's or a double tap's deadline).
    Tick { ms: u64 },
}

/// A recognizer's state in the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Still possible: wants more input.
    Possible,
    /// Claims the contact.
    Accept,
    /// Gives it up.
    Reject,
}

/// A recognizer: what it is called, what it says about each input, and when it next needs a
/// tick.
pub trait Recognizer {
    fn name(&self) -> &str;
    fn feed(&mut self, input: Input) -> Verdict;
    /// When it must be ticked even with no input (a deadline), if ever.
    fn deadline(&self) -> Option<u64> {
        None
    }
    /// Lost the arena: forget the contact.
    fn lost(&mut self) {}
}

fn far(a: (f32, f32), b: (f32, f32), unit: f32) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    (dx * dx + dy * dy).sqrt() >= SLOP_PX * unit
}

/// Down and up within the slop before the long-press time.
pub struct Tap {
    unit: f32,
    down: Option<((f32, f32), u64)>,
}

impl Tap {
    pub fn new(unit: f32) -> Self {
        Tap { unit, down: None }
    }
}

impl Recognizer for Tap {
    fn name(&self) -> &str {
        "tap"
    }
    fn feed(&mut self, input: Input) -> Verdict {
        match input {
            Input::Down { at, ms } => {
                self.down = Some((at, ms));
                Verdict::Possible
            }
            Input::Move { at, .. } => match self.down {
                Some((d, _)) if far(d, at, self.unit) => Verdict::Reject,
                _ => Verdict::Possible,
            },
            Input::Up { at, ms } => match self.down.take() {
                Some((d, t)) if !far(d, at, self.unit) && ms - t < LONG_MS => Verdict::Accept,
                _ => Verdict::Reject,
            },
            Input::Tick { ms } => match self.down {
                Some((_, t)) if ms - t >= LONG_MS => Verdict::Reject,
                _ => Verdict::Possible,
            },
            Input::Cancel => Verdict::Reject,
        }
    }
    fn deadline(&self) -> Option<u64> {
        self.down.map(|(_, t)| t + LONG_MS)
    }
    fn lost(&mut self) {
        self.down = None;
    }
}

/// Two taps within the double-tap time: only where an element opts in, since it makes a single
/// tap there wait (§3.2).
pub struct DoubleTap {
    unit: f32,
    first_up: Option<((f32, f32), u64)>,
    down: Option<((f32, f32), u64)>,
}

impl DoubleTap {
    pub fn new(unit: f32) -> Self {
        DoubleTap { unit, first_up: None, down: None }
    }
}

impl Recognizer for DoubleTap {
    fn name(&self) -> &str {
        "double tap"
    }
    fn feed(&mut self, input: Input) -> Verdict {
        match input {
            Input::Down { at, ms } => {
                if let Some((p, t)) = self.first_up
                    && (ms - t > DOUBLE_TAP_MS || far(p, at, self.unit * 4.0))
                {
                    return Verdict::Reject;
                }
                self.down = Some((at, ms));
                Verdict::Possible
            }
            Input::Move { at, .. } => match self.down {
                Some((d, _)) if far(d, at, self.unit) => Verdict::Reject,
                _ => Verdict::Possible,
            },
            Input::Up { at, ms } => {
                let Some((d, t)) = self.down.take() else {
                    return Verdict::Reject;
                };
                if far(d, at, self.unit) || ms - t >= LONG_MS {
                    return Verdict::Reject;
                }
                if self.first_up.is_some() {
                    Verdict::Accept
                } else {
                    self.first_up = Some((at, ms));
                    Verdict::Possible
                }
            }
            Input::Tick { ms } => match (self.first_up, self.down) {
                (Some((_, t)), None) if ms - t > DOUBLE_TAP_MS => Verdict::Reject,
                (_, Some((_, t))) if ms - t >= LONG_MS => Verdict::Reject,
                _ => Verdict::Possible,
            },
            Input::Cancel => Verdict::Reject,
        }
    }
    fn deadline(&self) -> Option<u64> {
        match (self.first_up, self.down) {
            (_, Some((_, t))) => Some(t + LONG_MS),
            (Some((_, t)), None) => Some(t + DOUBLE_TAP_MS + 1),
            _ => None,
        }
    }
    fn lost(&mut self) {
        self.first_up = None;
        self.down = None;
    }
}

/// Held within the slop past the long-press time: accepts while still down.
pub struct LongPress {
    unit: f32,
    down: Option<((f32, f32), u64)>,
}

impl LongPress {
    pub fn new(unit: f32) -> Self {
        LongPress { unit, down: None }
    }
}

impl Recognizer for LongPress {
    fn name(&self) -> &str {
        "long press"
    }
    fn feed(&mut self, input: Input) -> Verdict {
        match input {
            Input::Down { at, ms } => {
                self.down = Some((at, ms));
                Verdict::Possible
            }
            Input::Move { at, ms } => match self.down {
                Some((d, _)) if far(d, at, self.unit) => Verdict::Reject,
                Some((_, t)) if ms - t >= LONG_MS => Verdict::Accept,
                _ => Verdict::Possible,
            },
            Input::Tick { ms } => match self.down {
                Some((_, t)) if ms - t >= LONG_MS => Verdict::Accept,
                _ => Verdict::Possible,
            },
            Input::Up { .. } | Input::Cancel => Verdict::Reject,
        }
    }
    fn deadline(&self) -> Option<u64> {
        self.down.map(|(_, t)| t + LONG_MS)
    }
    fn lost(&mut self) {
        self.down = None;
    }
}

/// A drag or a pan along an axis: accepts at the slop when the move goes its way (an `immediate`
/// start), or after a long press (the default start); rejects a move across its axis.
pub struct Drag {
    name: String,
    unit: f32,
    options: DragOptions,
    down: Option<((f32, f32), u64)>,
}

impl Drag {
    pub fn new(name: impl Into<String>, unit: f32, options: DragOptions) -> Self {
        Drag { name: name.into(), unit, options, down: None }
    }
}

impl Recognizer for Drag {
    fn name(&self) -> &str {
        &self.name
    }
    fn feed(&mut self, input: Input) -> Verdict {
        match input {
            Input::Down { at, ms } => {
                self.down = Some((at, ms));
                Verdict::Possible
            }
            Input::Move { at, ms } => {
                let Some((d, t)) = self.down else {
                    return Verdict::Reject;
                };
                if !far(d, at, self.unit) {
                    return Verdict::Possible;
                }
                let held = ms - t >= LONG_MS;
                if self.options.start == DragStart::LongPress && !held {
                    return Verdict::Reject;
                }
                let (dx, dy) = ((at.0 - d.0).abs(), (at.1 - d.1).abs());
                let along = match self.options.axis {
                    Axis::Free | Axis::Dominant => true,
                    Axis::Horizontal => dx >= dy,
                    Axis::Vertical => dy > dx,
                };
                if along { Verdict::Accept } else { Verdict::Reject }
            }
            Input::Up { .. } | Input::Cancel => Verdict::Reject,
            Input::Tick { .. } => Verdict::Possible,
        }
    }
    fn lost(&mut self) {
        self.down = None;
    }
}

/// How a member takes part in the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// May win alongside the winner (SwiftUI's simultaneousGesture).
    Simultaneous,
    /// Competes; among several that accept together, the first listed wins.
    Normal,
}

struct Member {
    recognizer: Box<dyn Recognizer>,
    priority: Priority,
    verdict: Verdict,
}

/// What the arena decided after an input.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    /// The members that won, by name, in the order they won (the competing winner first).
    pub won: Vec<String>,
    /// The members told they lost.
    pub lost: Vec<String>,
}

/// Recognizers competing for one contact. Rules (Flutter's arena, SwiftUI's priorities):
/// - a competing member that accepts wins at once, and the other competing members lose;
/// - when every competing member but one has rejected, that one wins when it accepts (it no
///   longer waits for the others);
/// - members declared simultaneous win or lose on their own;
/// - when several competing members accept on the same input, the first listed wins (exclusive
///   composition: list a double tap before a tap).
#[derive(Default)]
pub struct Arena {
    members: Vec<Member>,
    winner: Option<usize>,
}

impl Arena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(mut self, recognizer: impl Recognizer + 'static, priority: Priority) -> Self {
        self.members.push(Member {
            recognizer: Box::new(recognizer),
            priority,
            verdict: Verdict::Possible,
        });
        self
    }

    /// The earliest deadline among the members still possible.
    pub fn deadline(&self) -> Option<u64> {
        self.members
            .iter()
            .filter(|m| m.verdict == Verdict::Possible)
            .filter_map(|m| m.recognizer.deadline())
            .min()
    }

    pub fn winner(&self) -> Option<&str> {
        self.winner.map(|i| self.members[i].recognizer.name())
    }

    pub fn feed(&mut self, input: Input) -> Outcome {
        let mut out = Outcome::default();
        if matches!(input, Input::Down { .. }) && self.members.iter().all(|m| m.verdict != Verdict::Possible) {
            // A new contact after the arena closed: a fresh arena, unless a member still waits
            // (a double tap's second tap).
            for m in &mut self.members {
                m.verdict = Verdict::Possible;
                m.recognizer.lost();
            }
            self.winner = None;
        }
        let mut accepted = Vec::new();
        for (i, m) in self.members.iter_mut().enumerate() {
            if m.verdict != Verdict::Possible {
                continue;
            }
            let v = m.recognizer.feed(input);
            match v {
                Verdict::Possible => {}
                Verdict::Reject => {
                    m.verdict = Verdict::Reject;
                    out.lost.push(m.recognizer.name().to_string());
                }
                Verdict::Accept => accepted.push(i),
            }
        }
        for i in accepted {
            let simultaneous = self.members[i].priority == Priority::Simultaneous;
            if simultaneous {
                self.members[i].verdict = Verdict::Accept;
                out.won.push(self.members[i].recognizer.name().to_string());
                continue;
            }
            // An earlier-listed competing member still possible (a double tap waiting for its
            // second tap) keeps the later one waiting, as the exclusive composition says.
            let earlier_waits = self.winner.is_none()
                && self.members[..i]
                    .iter()
                    .any(|m| m.priority == Priority::Normal && m.verdict == Verdict::Possible);
            if self.winner.is_some() || earlier_waits {
                if self.winner.is_some() {
                    self.members[i].verdict = Verdict::Reject;
                    self.members[i].recognizer.lost();
                    out.lost.push(self.members[i].recognizer.name().to_string());
                } else {
                    // Waits: remembered as accepted-pending, decided when the earlier resolves.
                    self.members[i].verdict = Verdict::Accept;
                }
                continue;
            }
            self.members[i].verdict = Verdict::Accept;
            self.winner = Some(i);
            out.won.push(self.members[i].recognizer.name().to_string());
        }
        // Pending accepts: a later member that accepted while an earlier one waited wins once
        // every earlier competing member has rejected.
        if self.winner.is_none()
            && let Some(i) = self.members.iter().position(|m| m.priority == Priority::Normal && m.verdict == Verdict::Accept)
            && self.members[..i]
                .iter()
                .all(|m| m.priority != Priority::Normal || m.verdict == Verdict::Reject)
        {
            self.winner = Some(i);
            out.won.push(self.members[i].recognizer.name().to_string());
        }
        // A winner among the competing members: the others lose.
        if let Some(w) = self.winner {
            for (i, m) in self.members.iter_mut().enumerate() {
                if i != w && m.priority == Priority::Normal && m.verdict != Verdict::Reject {
                    let pending_or_possible = m.verdict != Verdict::Reject;
                    m.verdict = Verdict::Reject;
                    m.recognizer.lost();
                    if pending_or_possible && !out.lost.contains(&m.recognizer.name().to_string()) {
                        out.lost.push(m.recognizer.name().to_string());
                    }
                }
            }
        }
        out
    }
}

/// A long press, then a drag from it (SwiftUI's `sequenced(before:)`): the drag recognizer only
/// starts once the long press has been recognized, and the composition accepts when the drag
/// does. The commonest sequence; others compose the same way.
pub struct Sequenced {
    name: String,
    first: Box<dyn Recognizer>,
    second: Box<dyn Recognizer>,
    first_done: bool,
    last_down: Option<Input>,
}

impl Sequenced {
    pub fn new(name: impl Into<String>, first: impl Recognizer + 'static, second: impl Recognizer + 'static) -> Self {
        Sequenced {
            name: name.into(),
            first: Box::new(first),
            second: Box::new(second),
            first_done: false,
            last_down: None,
        }
    }
}

impl Recognizer for Sequenced {
    fn name(&self) -> &str {
        &self.name
    }
    fn feed(&mut self, input: Input) -> Verdict {
        if let Input::Down { .. } = input {
            self.last_down = Some(input);
        }
        if !self.first_done {
            match self.first.feed(input) {
                Verdict::Accept => {
                    self.first_done = true;
                    // The second starts from the same down, its time moved to now so its own
                    // start rules (a long-press start already satisfied) hold.
                    if let Some(Input::Down { at, .. }) = self.last_down {
                        let now = match input {
                            Input::Move { ms, .. } | Input::Tick { ms } | Input::Up { ms, .. } | Input::Down { ms, .. } => ms,
                            Input::Cancel => 0,
                        };
                        self.second.feed(Input::Down { at, ms: now.saturating_sub(LONG_MS) });
                    }
                    Verdict::Possible
                }
                Verdict::Reject => Verdict::Reject,
                Verdict::Possible => Verdict::Possible,
            }
        } else {
            self.second.feed(input)
        }
    }
    fn deadline(&self) -> Option<u64> {
        if self.first_done { self.second.deadline() } else { self.first.deadline() }
    }
    fn lost(&mut self) {
        self.first_done = false;
        self.first.lost();
        self.second.lost();
    }
}

/// A value that lives as long as one gesture (SwiftUI's `@GestureState`): set while the
/// gesture runs, back to its default when it ends or is cancelled, so no transient state (a
/// drag's offset, a pressed look) can stay stuck after the finger leaves.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GestureState<T: Default> {
    value: T,
}

impl<T: Default> GestureState<T> {
    pub fn get(&self) -> &T {
        &self.value
    }

    /// The gesture's input: an update while it runs, the reset at its end.
    pub fn feed(&mut self, input: Input, update: impl FnOnce(&mut T, Input)) {
        match input {
            Input::Up { .. } | Input::Cancel => self.value = T::default(),
            _ => update(&mut self.value, input),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(x: f32, y: f32, ms: u64) -> Input {
        Input::Down { at: (x, y), ms }
    }
    fn mv(x: f32, y: f32, ms: u64) -> Input {
        Input::Move { at: (x, y), ms }
    }
    fn up(x: f32, y: f32, ms: u64) -> Input {
        Input::Up { at: (x, y), ms }
    }

    #[test]
    fn gesture_state_resets_on_end_and_on_cancel() {
        let mut offset: GestureState<f32> = GestureState::default();
        let follow = |v: &mut f32, i: Input| {
            if let Input::Move { at, .. } = i {
                *v = at.1;
            }
        };
        offset.feed(down(0.0, 0.0, 0), follow);
        offset.feed(mv(0.0, 42.0, 20), follow);
        assert_eq!(*offset.get(), 42.0);
        offset.feed(Input::Cancel, follow);
        assert_eq!(*offset.get(), 0.0, "never stuck after a cancel");
    }

    #[test]
    fn a_tap_alone_wins_at_once() {
        let mut a = Arena::new().add(Tap::new(1.0), Priority::Normal).add(LongPress::new(1.0), Priority::Normal);
        a.feed(down(10.0, 10.0, 0));
        let o = a.feed(up(11.0, 10.0, 90));
        assert_eq!(o.won, vec!["tap"]);
        assert_eq!(a.winner(), Some("tap"));
    }

    #[test]
    fn a_long_press_wins_while_down_and_the_tap_loses() {
        let mut a = Arena::new().add(Tap::new(1.0), Priority::Normal).add(LongPress::new(1.0), Priority::Normal);
        a.feed(down(10.0, 10.0, 0));
        assert_eq!(a.deadline(), Some(500));
        let o = a.feed(Input::Tick { ms: 500 });
        assert_eq!(o.won, vec!["long press"]);
        assert!(o.lost.contains(&"tap".to_string()));
    }

    #[test]
    fn exclusive_double_tap_before_tap_makes_the_single_tap_wait() {
        let mut a = Arena::new().add(DoubleTap::new(1.0), Priority::Normal).add(Tap::new(1.0), Priority::Normal);
        a.feed(down(10.0, 10.0, 0));
        let o = a.feed(up(10.0, 10.0, 80));
        assert!(o.won.is_empty(), "the tap waits for the double tap's window");
        // No second tap: the double tap gives up and the tap wins.
        let o = a.feed(Input::Tick { ms: 80 + DOUBLE_TAP_MS + 1 });
        assert_eq!(o.won, vec!["tap"]);
        // A second tap in time: the double tap wins.
        let mut a = Arena::new().add(DoubleTap::new(1.0), Priority::Normal).add(Tap::new(1.0), Priority::Normal);
        a.feed(down(10.0, 10.0, 0));
        a.feed(up(10.0, 10.0, 80));
        a.feed(down(11.0, 10.0, 200));
        let o = a.feed(up(11.0, 10.0, 260));
        assert_eq!(a.winner(), Some("double tap"), "{o:?}");
    }

    #[test]
    fn a_knob_keeps_its_axis_and_the_scroll_keeps_the_other() {
        let knob = Drag::new("knob", 1.0, DragOptions { axis: Axis::Horizontal, start: DragStart::Immediate });
        let scroll = Drag::new("scroll", 1.0, DragOptions { axis: Axis::Vertical, start: DragStart::Immediate });
        let mut a = Arena::new().add(knob, Priority::Normal).add(scroll, Priority::Normal);
        a.feed(down(100.0, 100.0, 0));
        let o = a.feed(mv(130.0, 104.0, 40));
        assert_eq!(o.won, vec!["knob"]);
        let knob = Drag::new("knob", 1.0, DragOptions { axis: Axis::Horizontal, start: DragStart::Immediate });
        let scroll = Drag::new("scroll", 1.0, DragOptions { axis: Axis::Vertical, start: DragStart::Immediate });
        let mut a = Arena::new().add(knob, Priority::Normal).add(scroll, Priority::Normal);
        a.feed(down(100.0, 100.0, 0));
        let o = a.feed(mv(103.0, 140.0, 40));
        assert_eq!(a.winner(), Some("scroll"), "{o:?}");
    }

    #[test]
    fn simultaneous_members_win_alongside() {
        let pan = Drag::new("pan", 1.0, DragOptions { axis: Axis::Free, start: DragStart::Immediate });
        let mut a = Arena::new().add(Tap::new(1.0), Priority::Normal).add(pan, Priority::Simultaneous);
        a.feed(down(0.0, 0.0, 0));
        let o = a.feed(mv(40.0, 0.0, 30));
        assert_eq!(o.won, vec!["pan"]);
        assert!(o.lost.contains(&"tap".to_string()));
    }

    #[test]
    fn a_long_press_then_a_drag_in_sequence() {
        let seq = Sequenced::new(
            "reorder",
            LongPress::new(1.0),
            Drag::new("move", 1.0, DragOptions { axis: Axis::Vertical, start: DragStart::Immediate }),
        );
        let scroll = Drag::new("scroll", 1.0, DragOptions { axis: Axis::Vertical, start: DragStart::Immediate });
        // A quick vertical move: the scroll wins, the reorder never started.
        let mut a = Arena::new().add(Sequenced::new(
            "reorder",
            LongPress::new(1.0),
            Drag::new("move", 1.0, DragOptions { axis: Axis::Vertical, start: DragStart::Immediate }),
        ), Priority::Normal).add(Drag::new("scroll", 1.0, DragOptions { axis: Axis::Vertical, start: DragStart::Immediate }), Priority::Normal);
        a.feed(down(50.0, 50.0, 0));
        a.feed(mv(50.0, 90.0, 60));
        assert_eq!(a.winner(), Some("scroll"));
        // Held first, then moved: the reorder.
        let mut a = Arena::new().add(seq, Priority::Normal).add(Sequenced::new("hold-scroll", LongPress::new(1.0), scroll), Priority::Normal);
        a.feed(down(50.0, 50.0, 0));
        a.feed(Input::Tick { ms: 500 });
        a.feed(mv(50.0, 90.0, 650));
        assert_eq!(a.winner(), Some("reorder"));
    }
}
