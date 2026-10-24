//! Gesture recognition, shared by the shell's debug UI mode and the UI primitives activity: what
//! a contact is while it lasts and what it was when it lifts, with the thresholds of input events
//! §7 and the design system's regions (§8.3, §8.5).
//!
//! A [`Contact`] follows one pointer from down to up (or cancel) and reports:
//! - discrete gestures: [`Gesture::Tap`], [`Gesture::LongPress`] (while still down),
//!   [`Gesture::Swipe`] (a pan that ends fast);
//! - composite, continuous ones with phases: [`Gesture::Pan`] (moved past the slop) and
//!   [`Gesture::Drag`] (drag and drop: start, move, drop or cancel), shaped by [`DragOptions`]
//!   (the axis, and whether it starts at once or after a long press).

/// How far a contact may move and still be a tap or a long press, in design pixels.
pub const SLOP_PX: f32 = 10.0;
/// Short against long (T1 controls spec §1.1; proposed, Android's default is 400 ms since 12).
pub const LONG_MS: u64 = 500;
/// The side and bottom edge-gesture zones (design system §8.5), in design pixels.
pub const EDGE_PX: f32 = 20.0;
/// The top zone is the status bar: "swipe down from the status bar" (SYS-039, design system
/// §8.5), the `size.status-bar` token.
pub const STATUS_BAR_PX: f32 = 28.0;
/// A pan ends as a swipe when it is released faster than this, in design pixels per second
/// (Android's `MINIMUM_FLING_VELOCITY`).
pub const FLING_PX_S: f32 = 50.0;
/// The window the release velocity is measured over (Android's `VelocityTracker` horizon).
const VELOCITY_MS: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Leading,
    Trailing,
    Top,
    Bottom,
}

/// Where two edge zones meet: a region of its own, so a contact there is never claimed by two
/// edges at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeading,
    TopTrailing,
    BottomLeading,
    BottomTrailing,
}

/// The region a contact starts in (design system §8.5): an edge zone, a corner where two meet, or
/// the content. Every gesture carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Content,
    Edge(Edge),
    Corner(Corner),
}

/// A frame: a box placed in its parent (input events §6.1). Every coordinate is relative to one;
/// the screen is only the outermost frame software sees. A frame has its offset in its parent
/// (logical pixels of the parent), its size, `unit` logical pixels per design pixel (so the
/// thresholds keep their physical size on any hardware), its direction (right to left mirrors
/// leading and trailing, design system §9.5), and the zone widths of its own regions (§3.2.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub origin: (f32, f32),
    pub size: (f32, f32),
    pub unit: f32,
    pub rtl: bool,
    /// The side and bottom zones, in design pixels.
    pub zone: f32,
    /// The top zone, in design pixels.
    pub top_zone: f32,
}

impl Frame {
    /// A frame at the origin of its parent, with the edge zone on every side.
    pub fn new(size: (f32, f32), unit: f32) -> Self {
        Frame {
            origin: (0.0, 0.0),
            size,
            unit,
            rtl: false,
            zone: EDGE_PX,
            top_zone: EDGE_PX,
        }
    }

    /// The screen: its top zone is the status bar, where the system's top gesture starts.
    pub fn screen(size: (f32, f32), unit: f32) -> Self {
        Frame {
            top_zone: STATUS_BAR_PX,
            ..Self::new(size, unit)
        }
    }

    /// The same frame placed at `origin` in its parent.
    pub fn at(self, origin: (f32, f32)) -> Self {
        Frame { origin, ..self }
    }

    /// A point of the parent, in this frame.
    pub fn from_parent(&self, p: (f32, f32)) -> (f32, f32) {
        (p.0 - self.origin.0, p.1 - self.origin.1)
    }

    /// A point of this frame, in the parent.
    pub fn to_parent(&self, p: (f32, f32)) -> (f32, f32) {
        (p.0 + self.origin.0, p.1 + self.origin.1)
    }

    /// The content of a scroll container whose box is this frame (its viewport): a frame
    /// `size` big, placed at minus the scroll `offset`. A point in the content stays with the
    /// content when it scrolls; a point in the viewport stays with the screen (input events
    /// §6.2).
    pub fn content(&self, size: (f32, f32), offset: (f32, f32)) -> Frame {
        Frame {
            origin: (-offset.0, -offset.1),
            size,
            ..*self
        }
    }

    pub fn contains(&self, p: (f32, f32)) -> bool {
        (0.0..self.size.0).contains(&p.0) && (0.0..self.size.1).contains(&p.1)
    }

    /// The region of a point of this frame: its own edges and corners.
    pub fn region(&self, at: (f32, f32)) -> Region {
        let e = self.zone * self.unit;
        let (left, right) = (at.0 < e, at.0 > self.size.0 - e);
        let (leading, trailing) = if self.rtl {
            (right, left)
        } else {
            (left, right)
        };
        let (top, bottom) = (at.1 < self.top_zone * self.unit, at.1 > self.size.1 - e);
        match (top, bottom, leading, trailing) {
            (true, _, true, _) => Region::Corner(Corner::TopLeading),
            (true, _, _, true) => Region::Corner(Corner::TopTrailing),
            (_, true, true, _) => Region::Corner(Corner::BottomLeading),
            (_, true, _, true) => Region::Corner(Corner::BottomTrailing),
            (true, ..) => Region::Edge(Edge::Top),
            (_, true, ..) => Region::Edge(Edge::Bottom),
            (_, _, true, _) => Region::Edge(Edge::Leading),
            (_, _, _, true) => Region::Edge(Edge::Trailing),
            _ => Region::Content,
        }
    }
}

/// Nested frames from an outer one to an inner one (the screen, the content area, the surface, a
/// group, an element): a point converts down and up the path, and between two paths through
/// their common outer frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FramePath(pub Vec<Frame>);

impl FramePath {
    /// A point of the outermost frame's parent, in the innermost frame.
    pub fn to_inner(&self, p: (f32, f32)) -> (f32, f32) {
        self.0.iter().fold(p, |p, f| f.from_parent(p))
    }

    /// A point of the innermost frame, in the outermost frame's parent.
    pub fn to_outer(&self, p: (f32, f32)) -> (f32, f32) {
        self.0.iter().rev().fold(p, |p, f| f.to_parent(p))
    }

    /// A point of this path's innermost frame, in another path's innermost frame (both from the
    /// same outer parent).
    pub fn convert(&self, p: (f32, f32), to: &FramePath) -> (f32, f32) {
        to.to_inner(self.to_outer(p))
    }

    /// The innermost frame the point (in the outer parent) falls in, as the depth in the path:
    /// the level that may recognize a contact there (§3.2.1).
    pub fn deepest_containing(&self, p: (f32, f32)) -> Option<usize> {
        let mut at = p;
        let mut deepest = None;
        for (i, f) in self.0.iter().enumerate() {
            at = f.from_parent(at);
            if !f.contains(at) {
                break;
            }
            deepest = Some(i);
        }
        deepest
    }
}

/// The phase of a composite gesture (W3C's pointer stream: down, moves, up or cancel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Start,
    Move,
    /// It lifted: for a drag, the drop.
    End,
    /// The system took the contact over (pointercancel): nothing that started acts.
    Cancel,
}

/// The axis a pan or drag follows (Compose's `draggable(orientation)`, iOS's axis-locked
/// scrolling).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Axis {
    /// Both axes: the free primitive.
    #[default]
    Free,
    Horizontal,
    Vertical,
    /// Locked to the axis of the first move past the slop (a list inside a pager).
    Dominant,
}

/// When a drag starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DragStart {
    /// After a long press, then a move (Android's and iOS's drag and drop): a plain move stays a
    /// pan, so content under it still scrolls.
    #[default]
    LongPress,
    /// At the first move past the slop (a slider's knob, a draggable panel): no pan.
    Immediate,
}

/// The directives of a drag (input events §5): what an element declares, everything else is the
/// free primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DragOptions {
    pub axis: Axis,
    pub start: DragStart,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    Tap {
        region: Region,
    },
    /// Held within the slop past [`LONG_MS`]: reported while still down, once.
    LongPress {
        region: Region,
    },
    /// A pan released faster than [`FLING_PX_S`], after its `Pan` end.
    Swipe {
        direction: Direction,
        region: Region,
        distance: f32,
        ms: u64,
        /// At release, logical pixels per second.
        velocity: f32,
        /// The edge it swiped away from, if it is an edge swipe (see [`Gesture::edge_swipe`]).
        edge: Option<Edge>,
    },
    /// Moved past the slop with no drag: scrolling, paging, an edge gesture in progress. `at` is
    /// constrained to the axis.
    Pan {
        phase: Phase,
        region: Region,
        from: (f32, f32),
        at: (f32, f32),
        /// Logical pixels per second, on the axis.
        velocity: (f32, f32),
    },
    /// Drag and drop: `Start` when it begins, `Move` while it follows the finger, `End` at the
    /// drop, `Cancel` when the system takes it. `at` is constrained to the axis.
    Drag {
        phase: Phase,
        region: Region,
        from: (f32, f32),
        at: (f32, f32),
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Track {
    None,
    Pan,
    Drag,
}

/// A contact followed while it lasts: feed it every move, call [`Contact::long_due`] at
/// [`LONG_MS`] after the press, and [`Contact::up`] or [`Contact::cancel`] at the end.
#[derive(Debug, Clone)]
pub struct Contact {
    at: (f32, f32),
    since_ms: u64,
    region: Region,
    frame: Frame,
    options: DragOptions,
    /// The axis once it is known (`Dominant` resolves at the first move past the slop).
    axis: Axis,
    /// It moved past the slop: never a tap or a long press any more.
    far: bool,
    long_fired: bool,
    cancelled: bool,
    track: Track,
    /// Recent samples (position, ms) for the release velocity.
    samples: Vec<((f32, f32), u64)>,
}

impl Contact {
    /// A contact down at `at` (logical pixels) at `now_ms`, with the default directives: a free
    /// pan, and drag and drop after a long press.
    pub fn down(at: (f32, f32), now_ms: u64, frame: Frame) -> Self {
        Self::with(at, now_ms, frame, DragOptions::default())
    }

    /// A contact on an element that declared its drag directives.
    pub fn with(at: (f32, f32), now_ms: u64, frame: Frame, options: DragOptions) -> Self {
        Contact {
            at,
            since_ms: now_ms,
            region: frame.region(at),
            frame,
            options,
            axis: options.axis,
            far: false,
            long_fired: false,
            cancelled: false,
            track: Track::None,
            samples: vec![(at, now_ms)],
        }
    }

    pub fn region(&self) -> Region {
        self.region
    }

    fn constrain(&self, at: (f32, f32)) -> (f32, f32) {
        match self.axis {
            Axis::Horizontal => (at.0, self.at.1),
            Axis::Vertical => (self.at.0, at.1),
            Axis::Free | Axis::Dominant => at,
        }
    }

    fn velocity(&self) -> (f32, f32) {
        let (Some(first), Some(last)) = (self.samples.first(), self.samples.last()) else {
            return (0.0, 0.0);
        };
        let ms = last.1.saturating_sub(first.1).max(1) as f32;
        let v = (
            (last.0.0 - first.0.0) * 1000.0 / ms,
            (last.0.1 - first.0.1) * 1000.0 / ms,
        );
        match self.axis {
            Axis::Horizontal => (v.0, 0.0),
            Axis::Vertical => (0.0, v.1),
            Axis::Free | Axis::Dominant => v,
        }
    }

    /// A move to `at` at `now_ms`: the composite gesture it starts or continues, if any.
    pub fn moved(&mut self, at: (f32, f32), now_ms: u64) -> Option<Gesture> {
        self.samples.push((at, now_ms));
        self.samples
            .retain(|s| now_ms.saturating_sub(s.1) <= VELOCITY_MS);
        let (dx, dy) = (at.0 - self.at.0, at.1 - self.at.1);
        let phase = if self.track != Track::None {
            Phase::Move
        } else if (dx * dx + dy * dy).sqrt() >= SLOP_PX * self.frame.unit {
            self.far = true;
            if self.axis == Axis::Dominant {
                self.axis = if dx.abs() >= dy.abs() {
                    Axis::Horizontal
                } else {
                    Axis::Vertical
                };
            }
            self.track = match (self.options.start, self.long_fired) {
                (DragStart::Immediate, _) | (DragStart::LongPress, true) => Track::Drag,
                (DragStart::LongPress, false) => Track::Pan,
            };
            Phase::Start
        } else {
            return None;
        };
        Some(self.track_event(phase, at))
    }

    fn track_event(&self, phase: Phase, at: (f32, f32)) -> Gesture {
        let at = self.constrain(at);
        match self.track {
            Track::Drag => Gesture::Drag {
                phase,
                region: self.region,
                from: self.at,
                at,
            },
            _ => Gesture::Pan {
                phase,
                region: self.region,
                from: self.at,
                at,
                velocity: self.velocity(),
            },
        }
    }

    /// The long press, once, if the contact is still within the slop at `now_ms`.
    pub fn long_due(&mut self, now_ms: u64) -> Option<Gesture> {
        if self.far || self.long_fired || now_ms.saturating_sub(self.since_ms) < LONG_MS {
            return None;
        }
        self.long_fired = true;
        Some(Gesture::LongPress {
            region: self.region,
        })
    }

    /// What the contact was when it lifts at `at`, `now_ms`, in order: a tap; nothing after a
    /// long press (it was reported while down); a drag's drop; a pan's end, then a swipe if it
    /// was released fast.
    pub fn up(&mut self, at: (f32, f32), now_ms: u64) -> Vec<Gesture> {
        if self.cancelled {
            return vec![];
        }
        if self.track == Track::None {
            // Lifted within the slop (a contact that went far and came back is not a tap:
            // Android's GestureDetector).
            let _ = self.moved(at, now_ms);
        } else {
            self.samples.push((at, now_ms));
        }
        match self.track {
            Track::None if self.long_fired => vec![],
            Track::None => vec![Gesture::Tap {
                region: self.region,
            }],
            Track::Drag => vec![self.track_event(Phase::End, at)],
            Track::Pan => {
                let end = self.track_event(Phase::End, at);
                let (vx, vy) = self.velocity();
                let speed = (vx * vx + vy * vy).sqrt();
                let at = self.constrain(at);
                let (dx, dy) = (at.0 - self.at.0, at.1 - self.at.1);
                if speed < FLING_PX_S * self.frame.unit {
                    return vec![end];
                }
                let direction = if dx.abs() >= dy.abs() {
                    if dx > 0.0 {
                        Direction::Right
                    } else {
                        Direction::Left
                    }
                } else if dy > 0.0 {
                    Direction::Down
                } else {
                    Direction::Up
                };
                // Away from the edge it started on, leading and trailing by the layout.
                let (leading_away, trailing_away) = if self.frame.rtl {
                    (Direction::Left, Direction::Right)
                } else {
                    (Direction::Right, Direction::Left)
                };
                let edge = match self.region {
                    Region::Edge(e @ Edge::Leading) if direction == leading_away => Some(e),
                    Region::Edge(e @ Edge::Trailing) if direction == trailing_away => Some(e),
                    Region::Edge(e @ Edge::Top) if direction == Direction::Down => Some(e),
                    Region::Edge(e @ Edge::Bottom) if direction == Direction::Up => Some(e),
                    _ => None,
                };
                vec![
                    end,
                    Gesture::Swipe {
                        direction,
                        region: self.region,
                        distance: (dx * dx + dy * dy).sqrt(),
                        ms: now_ms.saturating_sub(self.since_ms),
                        velocity: speed,
                        edge,
                    },
                ]
            }
        }
    }

    /// The system took the contact over: the composite gesture in progress is cancelled, and
    /// nothing else follows.
    pub fn cancel(&mut self) -> Option<Gesture> {
        let at = self.samples.last().map_or(self.at, |s| s.0);
        let g = (self.track != Track::None).then(|| self.track_event(Phase::Cancel, at));
        self.cancelled = true;
        g
    }
}

/// A whole contact at once, from `down` to `up` at a steady speed over `ms`: its last gesture
/// (a swipe over the pan it ends). For tests and tools that only know the two ends.
pub fn classify(down: (f32, f32), up: (f32, f32), ms: u64, frame: Frame) -> Gesture {
    let mut c = Contact::down(down, 0, frame);
    if ms >= LONG_MS {
        let _ = c.long_due(LONG_MS);
    }
    let _ = c.moved(
        (
            down.0 + (up.0 - down.0) * 0.9,
            down.1 + (up.1 - down.1) * 0.9,
        ),
        ms * 9 / 10,
    );
    let mut all = c.up(up, ms);
    all.pop().unwrap_or(Gesture::LongPress {
        region: frame.region(down),
    })
}

impl Gesture {
    /// The edge of an edge swipe: one that starts in an edge zone (not a corner) and moves away
    /// from that edge; `None` for any other gesture.
    pub fn edge_swipe(&self) -> Option<Edge> {
        match self {
            Gesture::Swipe { edge, .. } => *edge,
            _ => None,
        }
    }
}

/// The level a gesture is recognized at (input events §3.2.1): what its regions mean depends on
/// it. Only the screen's edges are the system's (the status bar, back, more); a surface's, a
/// container's or an element's edges are just edges, whatever the screen's are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Screen,
    Surface,
    Container,
    Element,
}

fn region_name(region: Region, level: Level) -> &'static str {
    match region {
        Region::Content => "",
        Region::Edge(Edge::Leading) => " on the leading edge",
        Region::Edge(Edge::Trailing) => " on the trailing edge",
        Region::Edge(Edge::Top) if level == Level::Screen => " on the status bar",
        Region::Edge(Edge::Top) => " on the top edge",
        Region::Edge(Edge::Bottom) => " on the bottom edge",
        Region::Corner(Corner::TopLeading) => " in the top-leading corner",
        Region::Corner(Corner::TopTrailing) => " in the top-trailing corner",
        Region::Corner(Corner::BottomLeading) => " in the bottom-leading corner",
        Region::Corner(Corner::BottomTrailing) => " in the bottom-trailing corner",
    }
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Start => "start",
        Phase::Move => "move",
        Phase::End => "end",
        Phase::Cancel => "cancel",
    }
}

impl Gesture {
    /// Its name at any level but the screen: regions are the frame's own edges and corners, and
    /// no gesture claims a system action (the UI primitives activity's pad, an element).
    pub fn describe(&self) -> String {
        self.describe_at(Level::Element)
    }

    /// Its name at `level`. At the screen level (the shell's debug UI mode) the top zone is the
    /// status bar and an edge swipe carries the action the design system gives it (§8.5).
    pub fn describe_at(&self, level: Level) -> String {
        let region_name = |r| region_name(r, level);
        match self {
            Gesture::Tap { region } => format!("tap{}", region_name(*region)),
            Gesture::LongPress { region } => format!("long press{}", region_name(*region)),
            Gesture::Pan {
                phase,
                region,
                from,
                at,
                ..
            } => format!(
                "pan {}{} ({:.0}, {:.0})",
                phase_name(*phase),
                region_name(*region),
                at.0 - from.0,
                at.1 - from.1
            ),
            Gesture::Drag {
                phase,
                region,
                from,
                at,
            } => {
                let what = match phase {
                    Phase::End => "drop",
                    p => phase_name(*p),
                };
                format!(
                    "drag {what}{} ({:.0}, {:.0})",
                    region_name(*region),
                    at.0 - from.0,
                    at.1 - from.1
                )
            }
            Gesture::Swipe {
                direction,
                region,
                distance,
                ms,
                ..
            } => {
                let dir = match direction {
                    Direction::Left => "left",
                    Direction::Right => "right",
                    Direction::Up => "up",
                    Direction::Down => "down",
                };
                let from = match (self.edge_swipe(), level) {
                    (Some(Edge::Leading), Level::Screen) => " from the leading edge: back".to_string(),
                    (Some(Edge::Trailing), Level::Screen) => {
                        " from the trailing edge: companion screen".to_string()
                    }
                    (Some(Edge::Bottom), Level::Screen) => " from the bottom: more".to_string(),
                    (Some(Edge::Top), Level::Screen) => {
                        " from the status bar: quick settings".to_string()
                    }
                    (Some(Edge::Leading), _) => " in from the leading edge".to_string(),
                    (Some(Edge::Trailing), _) => " in from the trailing edge".to_string(),
                    (Some(Edge::Bottom), _) => " in from the bottom edge".to_string(),
                    (Some(Edge::Top), _) => " in from the top edge".to_string(),
                    // A corner's swipe has no action of its own yet; it is named, never an
                    // edge's.
                    (None, _) => match region {
                        Region::Corner(_) => format!(", starting{}", region_name(*region)),
                        Region::Edge(_) => format!(
                            ", along or out of{}",
                            region_name(*region).replacen(" on", "", 1)
                        ),
                        Region::Content => String::new(),
                    },
                };
                format!("swipe {dir}{from}, {distance:.0} px in {ms} ms")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Frame = Frame {
        origin: (0.0, 0.0),
        size: (320.0, 480.0),
        unit: 1.0,
        rtl: false,
        zone: EDGE_PX,
        top_zone: STATUS_BAR_PX,
    };

    #[test]
    fn taps_and_long_presses_stay_within_the_slop() {
        assert_eq!(
            classify((100.0, 100.0), (104.0, 103.0), 120, SCREEN),
            Gesture::Tap {
                region: Region::Content
            }
        );
        assert_eq!(
            classify((100.0, 100.0), (101.0, 100.0), 600, SCREEN),
            Gesture::LongPress {
                region: Region::Content
            }
        );
    }

    #[test]
    fn swipes_take_the_major_axis() {
        let g = classify((100.0, 300.0), (160.0, 290.0), 200, SCREEN);
        assert!(matches!(
            g,
            Gesture::Swipe {
                direction: Direction::Right,
                region: Region::Content,
                ..
            }
        ));
        let g = classify((100.0, 300.0), (110.0, 200.0), 200, SCREEN);
        assert!(matches!(
            g,
            Gesture::Swipe {
                direction: Direction::Up,
                ..
            }
        ));
    }

    #[test]
    fn a_slow_release_ends_a_pan_without_a_swipe() {
        let mut c = Contact::down((100.0, 300.0), 0, SCREEN);
        assert!(matches!(
            c.moved((160.0, 300.0), 400),
            Some(Gesture::Pan { phase: Phase::Start, .. })
        ));
        // Held still for the last 200 ms, then lifted.
        let _ = c.moved((160.0, 300.0), 500);
        let end = c.up((160.0, 300.0), 700);
        assert_eq!(end.len(), 1);
        assert!(matches!(end[0], Gesture::Pan { phase: Phase::End, .. }));
    }

    #[test]
    fn thresholds_follow_the_hardware() {
        let dense = Frame::screen((640.0, 960.0), 2.0);
        // At twice the density, a 12 logical px move is a 6 design px move: still a tap.
        assert!(matches!(
            classify((100.0, 100.0), (112.0, 100.0), 100, dense),
            Gesture::Tap { .. }
        ));
        // And the edge zone is 40 logical px wide.
        let g = classify((30.0, 480.0), (200.0, 480.0), 180, dense);
        assert_eq!(g.edge_swipe(), Some(Edge::Leading));
    }

    #[test]
    fn edge_swipes_start_in_an_edge_and_move_away_from_it() {
        assert_eq!(
            classify((8.0, 240.0), (120.0, 240.0), 180, SCREEN).edge_swipe(),
            Some(Edge::Leading)
        );
        assert_eq!(
            classify((315.0, 240.0), (200.0, 240.0), 180, SCREEN).edge_swipe(),
            Some(Edge::Trailing)
        );
        assert_eq!(
            classify((160.0, 475.0), (160.0, 360.0), 180, SCREEN).edge_swipe(),
            Some(Edge::Bottom)
        );
        // The top zone is the status bar's height.
        assert_eq!(
            classify((160.0, 25.0), (160.0, 140.0), 180, SCREEN).edge_swipe(),
            Some(Edge::Top)
        );
        // Starting at an edge but moving along it, or outwards, is not an edge swipe.
        assert_eq!(
            classify((8.0, 240.0), (2.0, 120.0), 180, SCREEN).edge_swipe(),
            None
        );
    }

    #[test]
    fn right_to_left_mirrors_leading_and_trailing() {
        let rtl = Frame { rtl: true, ..SCREEN };
        assert_eq!(rtl.region((315.0, 240.0)), Region::Edge(Edge::Leading));
        assert_eq!(
            classify((315.0, 240.0), (200.0, 240.0), 180, rtl).edge_swipe(),
            Some(Edge::Leading)
        );
    }

    #[test]
    fn a_long_press_fires_while_down_and_nothing_follows_on_release() {
        let mut c = Contact::down((100.0, 100.0), 0, SCREEN);
        assert_eq!(c.long_due(300), None, "not yet");
        assert_eq!(
            c.long_due(500),
            Some(Gesture::LongPress {
                region: Region::Content
            })
        );
        assert_eq!(c.long_due(900), None, "once");
        assert_eq!(c.up((100.0, 100.0), 2500), vec![]);
        // Moved past the slop first: no long press; a pan, then a swipe when it lifts fast.
        let mut c = Contact::down((100.0, 100.0), 0, SCREEN);
        let _ = c.moved((140.0, 100.0), 50);
        assert_eq!(c.long_due(600), None);
        let _ = c.moved((170.0, 100.0), 660);
        assert!(matches!(
            c.up((180.0, 100.0), 700).last(),
            Some(Gesture::Swipe { .. })
        ));
    }

    #[test]
    fn a_contact_that_went_far_and_came_back_is_not_a_tap() {
        let mut c = Contact::down((100.0, 100.0), 0, SCREEN);
        let _ = c.moved((130.0, 100.0), 80);
        let _ = c.moved((100.0, 100.0), 600);
        let end = c.up((100.0, 100.0), 900);
        assert!(!end.iter().any(|g| matches!(g, Gesture::Tap { .. })));
    }

    #[test]
    fn drag_and_drop_starts_after_a_long_press_and_drops_on_release() {
        let mut c = Contact::down((100.0, 100.0), 0, SCREEN);
        assert!(c.long_due(500).is_some());
        assert!(matches!(
            c.moved((140.0, 130.0), 700),
            Some(Gesture::Drag { phase: Phase::Start, .. })
        ));
        assert!(matches!(
            c.moved((180.0, 160.0), 800),
            Some(Gesture::Drag { phase: Phase::Move, at: (180.0, 160.0), .. })
        ));
        assert_eq!(
            c.up((200.0, 170.0), 900),
            vec![Gesture::Drag {
                phase: Phase::End,
                region: Region::Content,
                from: (100.0, 100.0),
                at: (200.0, 170.0)
            }]
        );
    }

    #[test]
    fn drag_directives_set_the_axis_and_the_start() {
        // Immediate and horizontal: a slider's knob.
        let knob = DragOptions {
            axis: Axis::Horizontal,
            start: DragStart::Immediate,
        };
        let mut c = Contact::with((100.0, 100.0), 0, SCREEN, knob);
        assert!(matches!(
            c.moved((130.0, 140.0), 50),
            Some(Gesture::Drag { phase: Phase::Start, at: (130.0, 100.0), .. })
        ));
        // Dominant: locked to the first move's axis.
        let mut c = Contact::with(
            (100.0, 100.0),
            0,
            SCREEN,
            DragOptions {
                axis: Axis::Dominant,
                start: DragStart::Immediate,
            },
        );
        let _ = c.moved((100.0, 130.0), 50);
        assert!(matches!(
            c.moved((160.0, 170.0), 100),
            Some(Gesture::Drag { at: (100.0, 170.0), .. })
        ));
    }

    #[test]
    fn a_cancel_ends_the_drag_and_nothing_follows() {
        let mut c = Contact::with(
            (100.0, 100.0),
            0,
            SCREEN,
            DragOptions {
                start: DragStart::Immediate,
                ..Default::default()
            },
        );
        let _ = c.moved((140.0, 100.0), 50);
        assert!(matches!(
            c.cancel(),
            Some(Gesture::Drag { phase: Phase::Cancel, .. })
        ));
        assert_eq!(c.up((150.0, 100.0), 90), vec![]);
    }

    #[test]
    fn corners_are_regions_of_their_own() {
        assert_eq!(SCREEN.region((5.0, 5.0)), Region::Corner(Corner::TopLeading));
        assert_eq!(SCREEN.region((315.0, 5.0)), Region::Corner(Corner::TopTrailing));
        assert_eq!(SCREEN.region((5.0, 475.0)), Region::Corner(Corner::BottomLeading));
        assert_eq!(SCREEN.region((315.0, 475.0)), Region::Corner(Corner::BottomTrailing));
        assert_eq!(SCREEN.region((5.0, 240.0)), Region::Edge(Edge::Leading));
        assert_eq!(SCREEN.region((160.0, 240.0)), Region::Content);
        // A swipe from a corner is never an edge swipe, whichever way it goes.
        let g = classify((6.0, 470.0), (6.0, 300.0), 180, SCREEN);
        assert!(matches!(
            g,
            Gesture::Swipe {
                region: Region::Corner(Corner::BottomLeading),
                ..
            }
        ));
        assert_eq!(g.edge_swipe(), None);
        let g = classify((6.0, 470.0), (150.0, 470.0), 180, SCREEN);
        assert_eq!(g.edge_swipe(), None);
        // Taps and long presses carry their corner too.
        assert_eq!(
            classify((314.0, 474.0), (314.0, 474.0), 700, SCREEN),
            Gesture::LongPress {
                region: Region::Corner(Corner::BottomTrailing)
            }
        );
    }

    #[test]
    fn frames_are_relative_and_convert_through_their_parents() {
        // The screen, the content area under the status bar, a card in it.
        let content_area = Frame::new((320.0, 452.0), 1.0).at((0.0, 28.0));
        let card = Frame::new((200.0, 100.0), 1.0).at((60.0, 172.0));
        let path = FramePath(vec![SCREEN, content_area, card]);
        // A screen point at the card's top left is its local (0, 0).
        assert_eq!(path.to_inner((60.0, 200.0)), (0.0, 0.0));
        assert_eq!(path.to_outer((10.0, 10.0)), (70.0, 210.0));
        assert_eq!(path.deepest_containing((70.0, 210.0)), Some(2));
        assert_eq!(path.deepest_containing((5.0, 10.0)), Some(0));
        // The card's own edges are not the screen's: its leading edge is mid-screen.
        assert_eq!(card.region(path.to_inner((65.0, 250.0))), Region::Edge(Edge::Leading));
        assert_eq!(SCREEN.region((65.0, 250.0)), Region::Content);
        // Between two paths sharing the screen.
        let other = FramePath(vec![SCREEN, content_area]);
        assert_eq!(path.convert((0.0, 0.0), &other), (60.0, 172.0));
    }


    #[test]
    fn scrolled_content_moves_under_a_still_viewport() {
        let viewport = Frame::new((320.0, 400.0), 1.0).at((0.0, 76.0));
        // 300 px scrolled down a 1200 px list.
        let content = viewport.content((320.0, 1200.0), (0.0, 300.0));
        let path = FramePath(vec![SCREEN, viewport, content]);
        // The same screen point is 300 px further down the content.
        assert_eq!(path.to_inner((100.0, 176.0)), (100.0, 400.0));
        // A row at content y 400 is on screen at 176 now, and at 126 after 50 more px of scroll.
        let later = FramePath(vec![SCREEN, viewport, viewport.content((320.0, 1200.0), (0.0, 350.0))]);
        assert_eq!(later.to_outer((100.0, 400.0)), (100.0, 126.0));
        // The viewport's edges are the container's regions, not the content's.
        assert_eq!(viewport.region((5.0, 200.0)), Region::Edge(Edge::Leading));
    }


    #[test]
    fn only_the_screen_level_names_system_actions() {
        let g = classify((160.0, 10.0), (160.0, 140.0), 180, SCREEN);
        assert!(g.describe_at(Level::Screen).contains("from the status bar: quick settings"));
        let pad = g.describe();
        assert!(pad.contains("in from the top edge"), "{pad}");
        assert!(!pad.contains("status bar") && !pad.contains("quick settings"), "{pad}");
        let back = classify((8.0, 240.0), (120.0, 240.0), 180, SCREEN);
        assert!(back.describe_at(Level::Screen).ends_with("ms") && back.describe_at(Level::Screen).contains(": back"));
        assert!(!back.describe().contains("back"));
        let tap = classify((160.0, 10.0), (160.0, 10.0), 80, SCREEN);
        assert_eq!(tap.describe_at(Level::Screen), "tap on the status bar");
        assert_eq!(tap.describe(), "tap on the top edge");
    }

}
