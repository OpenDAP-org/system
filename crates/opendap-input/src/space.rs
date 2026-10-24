//! What a developer asks of a pointer in space (input events §6.1, §6.3): where it is, how it
//! moved and how fast, in any frame: an element, a container, or the frames every activity shares
//! (the screen, the content area, its surface, a scroll area's viewport and content). Frames can
//! move while a contact lasts (a list scrolls under the finger, a sheet slides): a [`Track`]
//! keeps each sample in the surface, which stays put, and answers in a frame as it was at that
//! sample, so motion relative to a moving container is right.

use crate::gesture::{Frame, FramePath, Region};

/// The frames every activity shares, by name: singletons, not elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Space {
    /// The full display. The shell's and the debug UI mode's only: an activity never sees it.
    Screen,
    /// What the shell gives activities: the screen minus the system's insets.
    ContentArea,
    /// The activity's surface: the space every event an activity receives is in.
    Surface,
    /// A scroll area's box, which stays with the screen.
    Viewport,
    /// A scroll area's content, which moves with the scroll.
    Content,
}

/// Where the shared frames are now, each as a path from the surface (the outermost frame an
/// activity knows) to it. An application keeps one per scroll area it cares about.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spaces {
    pub viewport: Option<FramePath>,
    pub content: Option<FramePath>,
    /// Spaces a node names for its descendants (SwiftUI's `coordinateSpace(name:)`): a group or
    /// a container that others measure against, by name, without holding the node.
    pub named: Vec<(String, FramePath)>,
}

impl Spaces {
    /// The path from the surface to a shared space; `None` for one this activity cannot see.
    pub fn path(&self, space: Space) -> Option<FramePath> {
        match space {
            Space::Surface => Some(FramePath::default()),
            Space::Viewport => self.viewport.clone(),
            Space::Content => self.content.clone(),
            Space::Screen | Space::ContentArea => None,
        }
    }

    /// Names a space for its descendants.
    pub fn name(&mut self, name: impl Into<String>, path: FramePath) {
        let name = name.into();
        self.named.retain(|(n, _)| *n != name);
        self.named.push((name, path));
    }

    /// A named space's path, if it is named now.
    pub fn named(&self, name: &str) -> Option<&FramePath> {
        self.named.iter().find(|(n, _)| n == name).map(|(_, p)| p)
    }
}

/// A point of a frame, 0 to 1 along it from its start edge (mirrored right to left) and from its
/// top: a slider's value, a seek bar's position. Transport, the seek bar and the volume ask for
/// the physical form (`rtl: false`), design system §9.5.
pub fn normalised(frame: &Frame, local: (f32, f32)) -> (f32, f32) {
    let w = frame.size.0.max(f32::EPSILON);
    let h = frame.size.1.max(f32::EPSILON);
    let x = (local.0 / w).clamp(0.0, 1.0);
    let y = (local.1 / h).clamp(0.0, 1.0);
    (if frame.rtl { 1.0 - x } else { x }, y)
}

/// A point around a centre: its angle in degrees, 0 up and clockwise, and its radius (a dial).
pub fn polar(centre: (f32, f32), local: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (local.0 - centre.0, local.1 - centre.1);
    let angle = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
    (angle, (dx * dx + dy * dy).sqrt())
}

/// The turn from one angle to the next, unwrapped across 0°/360°: -180 to 180, so a dial turned
/// past the top keeps counting instead of jumping.
pub fn angle_delta(from: f32, to: f32) -> f32 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

/// One sample of a contact: where it was in the surface, and when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub surface: (f32, f32),
    pub ms: u64,
}

/// A contact followed in the surface, answering in any frame. Give each question the frame's
/// path as it was at each sample when the frame moves (a scroll area's content during a scroll),
/// or one path for a frame that stays.
#[derive(Debug, Clone, Default)]
pub struct Track {
    samples: Vec<Sample>,
}

/// The window the velocity is fitted over (Android's VelocityTracker horizon).
const VELOCITY_MS: u64 = 100;

impl Track {
    pub fn push(&mut self, surface: (f32, f32), ms: u64) {
        self.samples.push(Sample { surface, ms });
    }

    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    /// Where the contact is now, in a frame (its path from the surface).
    pub fn position(&self, path: &FramePath) -> Option<(f32, f32)> {
        self.samples.last().map(|s| path.to_inner(s.surface))
    }

    /// Where it went down, in a frame as it was then.
    pub fn press_position(&self, path_then: &FramePath) -> Option<(f32, f32)> {
        self.samples.first().map(|s| path_then.to_inner(s.surface))
    }

    /// The last move, in a frame: `before` and `now` are the frame's paths at the previous and
    /// the last sample. Derived from positions, never summed, so it cannot drift.
    pub fn delta(&self, before: &FramePath, now: &FramePath) -> Option<(f32, f32)> {
        let n = self.samples.len();
        if n < 2 {
            return None;
        }
        let a = before.to_inner(self.samples[n - 2].surface);
        let b = now.to_inner(self.samples[n - 1].surface);
        Some((b.0 - a.0, b.1 - a.1))
    }

    /// The whole move since the press, in a frame: its paths at the press and now. In a list's
    /// content that scrolled under a still finger, the finger moved through the content though
    /// it stood still on the screen.
    pub fn translation(&self, at_press: &FramePath, now: &FramePath) -> Option<(f32, f32)> {
        let a = self.press_position(at_press)?;
        let b = self.position(now)?;
        Some((b.0 - a.0, b.1 - a.1))
    }

    /// Logical pixels per second over the last 100 ms, in a frame that stays (or as it is now).
    pub fn velocity(&self, path: &FramePath) -> (f32, f32) {
        let Some(last) = self.samples.last() else {
            return (0.0, 0.0);
        };
        let first = self
            .samples
            .iter()
            .find(|s| last.ms.saturating_sub(s.ms) <= VELOCITY_MS)
            .unwrap_or(last);
        let ms = last.ms.saturating_sub(first.ms).max(1) as f32;
        let (a, b) = (path.to_inner(first.surface), path.to_inner(last.surface));
        ((b.0 - a.0) * 1000.0 / ms, (b.1 - a.1) * 1000.0 / ms)
    }

    /// Where a fling would stop, in a frame, with a deceleration in logical px/s² (a scroll
    /// area's: Flickable's): for snapping and dismissing.
    pub fn predicted_end(&self, path: &FramePath, deceleration: f32) -> Option<(f32, f32)> {
        let p = self.position(path)?;
        let (vx, vy) = self.velocity(path);
        let d = deceleration.max(1.0) * 2.0;
        Some((p.0 + vx * vx.abs() / d, p.1 + vy * vy.abs() / d))
    }

    /// The region of the contact now in the innermost frame of a path: its own edges and
    /// corners (a sheet's top edge, a resize handle), never the screen's.
    pub fn region(&self, path: &FramePath) -> Option<Region> {
        let frame = path.0.last()?;
        Some(frame.region(self.position(path)?))
    }

    /// Whether the contact is inside the innermost frame of a path now.
    pub fn inside(&self, path: &FramePath) -> bool {
        match (path.0.last(), self.position(path)) {
            (Some(f), Some(p)) => f.contains(p),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gesture::{Edge, Frame};

    fn surface() -> Frame {
        Frame::new((320.0, 452.0), 1.0)
    }

    #[test]
    fn motion_relative_to_a_container_that_scrolls_under_a_still_finger() {
        let viewport = Frame::new((320.0, 400.0), 1.0).at((0.0, 48.0));
        let content = |offset: f32| viewport.content((320.0, 2000.0), (0.0, offset));
        let path = |offset| FramePath(vec![surface(), viewport, content(offset)]);
        let mut t = Track::default();
        // The finger stays at the same surface point while the list scrolls 120 px.
        t.push((100.0, 200.0), 0);
        t.push((100.0, 200.0), 80);
        assert_eq!(t.translation(&path(0.0), &path(120.0)), Some((0.0, 120.0)));
        // In the viewport, it did not move at all.
        let vp = FramePath(vec![surface(), viewport]);
        assert_eq!(t.translation(&vp, &vp), Some((0.0, 0.0)));
    }

    #[test]
    fn shared_spaces_resolve_by_name() {
        let viewport = Frame::new((320.0, 400.0), 1.0).at((0.0, 48.0));
        let mut spaces = Spaces {
            viewport: Some(FramePath(vec![viewport])),
            content: None,
            named: Vec::new(),
        };
        spaces.name("queue", FramePath(vec![viewport, Frame::new((320.0, 56.0), 1.0).at((0.0, 10.0))]));
        assert_eq!(spaces.named("queue").map(|p| p.to_inner((100.0, 60.0))), Some((100.0, 2.0)));
        let mut t = Track::default();
        t.push((100.0, 60.0), 0);
        let vp = spaces.path(Space::Viewport).unwrap();
        assert_eq!(t.position(&vp), Some((100.0, 12.0)));
        assert_eq!(t.position(&spaces.path(Space::Surface).unwrap()), Some((100.0, 60.0)));
        assert_eq!(spaces.path(Space::Screen), None, "an activity never sees the screen");
        assert_eq!(t.region(&vp), Some(Region::Edge(Edge::Top)));
        assert!(t.inside(&vp));
    }

    #[test]
    fn normalised_mirrors_right_to_left_and_polar_unwraps() {
        let track = Frame::new((200.0, 20.0), 1.0);
        assert_eq!(normalised(&track, (50.0, 10.0)), (0.25, 0.5));
        let rtl = Frame { rtl: true, ..track };
        assert_eq!(normalised(&rtl, (50.0, 10.0)).0, 0.75);
        let (a, r) = polar((50.0, 50.0), (50.0, 0.0));
        assert!((a - 0.0).abs() < 1e-3 && (r - 50.0).abs() < 1e-3, "up is 0°");
        let (a, _) = polar((50.0, 50.0), (100.0, 50.0));
        assert!((a - 90.0).abs() < 1e-3, "clockwise: right is 90°");
        assert!((angle_delta(350.0, 10.0) - 20.0).abs() < 1e-3, "past the top keeps counting");
        assert!((angle_delta(10.0, 350.0) + 20.0).abs() < 1e-3);
    }

    #[test]
    fn velocity_and_predicted_end() {
        let p = FramePath(vec![surface()]);
        let mut t = Track::default();
        for i in 0..=10 {
            t.push((0.0, i as f32 * 10.0), i * 10);
        }
        let (_, vy) = t.velocity(&p);
        assert!((vy - 1000.0).abs() < 1.0, "100 px in 100 ms");
        let end = t.predicted_end(&p, 2000.0).unwrap();
        assert!((end.1 - (100.0 + 250.0)).abs() < 1.0, "v²/2a past the last point");
    }
}
