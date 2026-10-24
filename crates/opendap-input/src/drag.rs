//! A drag's modifiers (input events §3.2): where it may go (`bounds`), where it settles at the
//! drop (`snap`), and what accepts the drop (`targets`). The axis and the start are the
//! recognizer's ([`crate::gesture::DragOptions`]); these act on its positions.

use crate::gesture::FramePath;

/// Where a dragged point may go, in its frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bounds {
    None,
    /// A rectangle: `min` and `max` corners (a knob on its track: the track's extent).
    Rect { min: (f32, f32), max: (f32, f32) },
}

impl Bounds {
    pub fn clamp(&self, p: (f32, f32)) -> (f32, f32) {
        match *self {
            Bounds::None => p,
            Bounds::Rect { min, max } => (p.0.clamp(min.0, max.0), p.1.clamp(min.1, max.1)),
        }
    }
}

/// Where a dragged point settles when dropped.
#[derive(Debug, Clone, PartialEq)]
pub enum Snap {
    None,
    /// To a grid of `step` from `origin` (stops every 5 %).
    Step { origin: (f32, f32), step: (f32, f32) },
    /// To the nearest of these points (stops on an axis).
    Points(Vec<(f32, f32)>),
}

impl Snap {
    pub fn settle(&self, p: (f32, f32)) -> (f32, f32) {
        match self {
            Snap::None => p,
            Snap::Step { origin, step } => {
                let s = |v: f32, o: f32, st: f32| if st > 0.0 { o + ((v - o) / st).round() * st } else { v };
                (s(p.0, origin.0, step.0), s(p.1, origin.1, step.1))
            }
            Snap::Points(points) => points
                .iter()
                .copied()
                .min_by(|a, b| {
                    let d = |q: (f32, f32)| (q.0 - p.0).powi(2) + (q.1 - p.1).powi(2);
                    d(*a).total_cmp(&d(*b))
                })
                .unwrap_or(p),
        }
    }
}

/// A drop target: its frames, and the kinds of items it accepts.
#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    pub path: FramePath,
    pub accepts: Vec<String>,
}

/// The target a drop of an item of `kind` lands on at a point of the surface: the first, top
/// down, that contains the point and accepts the kind; `None` cancels the drop (the item goes
/// back).
pub fn drop_target<'a>(targets: &'a [Target], kind: &str, surface: (f32, f32)) -> Option<&'a Target> {
    targets.iter().find(|t| {
        t.accepts.iter().any(|k| k == kind)
            && t.path.0.last().is_some_and(|f| f.contains(t.path.to_inner(surface)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gesture::Frame;

    #[test]
    fn bounds_keep_a_knob_on_its_track_and_snap_settles_it() {
        let track = Bounds::Rect { min: (0.0, 10.0), max: (200.0, 10.0) };
        assert_eq!(track.clamp((250.0, 40.0)), (200.0, 10.0));
        let stops = Snap::Step { origin: (0.0, 0.0), step: (10.0, 0.0) };
        assert_eq!(stops.settle((44.0, 3.0)), (40.0, 3.0));
        let points = Snap::Points(vec![(0.0, 0.0), (100.0, 0.0), (200.0, 0.0)]);
        assert_eq!(points.settle((140.0, 5.0)), (100.0, 0.0));
    }

    #[test]
    fn a_drop_lands_on_the_first_target_that_accepts_it() {
        let t = |name: &str, origin, accepts: &[&str]| Target {
            name: name.into(),
            path: FramePath(vec![Frame::new((100.0, 100.0), 1.0).at(origin)]),
            accepts: accepts.iter().map(|s| s.to_string()).collect(),
        };
        let targets = [t("queue", (0.0, 0.0), &["track"]), t("playlist", (0.0, 0.0), &["track", "album"])];
        assert_eq!(drop_target(&targets, "album", (50.0, 50.0)).map(|t| t.name.as_str()), Some("playlist"));
        assert_eq!(drop_target(&targets, "track", (50.0, 50.0)).map(|t| t.name.as_str()), Some("queue"));
        assert!(drop_target(&targets, "track", (150.0, 50.0)).is_none(), "outside: the item goes back");
    }
}
