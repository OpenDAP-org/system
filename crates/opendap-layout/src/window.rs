//! The window of a virtual list: which items to build for a viewport at a scroll offset, with
//! overscan on both sides and prefetch ahead in the direction it moves (RecyclerView, Compose's
//! LazyColumn, Flutter's slivers). Everything outside it is not built.

use std::ops::Range;

use crate::extents::Extents;

/// How much beyond the viewport to keep built.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Overscan {
    /// Logical pixels on each side, always.
    pub around: f32,
    /// Seconds of motion to build ahead in the scroll's direction (velocity × lead).
    pub lead_s: f32,
    /// The most the prefetch may add, in logical pixels.
    pub max_ahead: f32,
}

impl Default for Overscan {
    fn default() -> Self {
        // One screen's worth around, and up to two screens ahead while flinging (proposed: to
        // measure on the T1).
        Overscan { around: 480.0, lead_s: 0.25, max_ahead: 960.0 }
    }
}

/// The items to build for a viewport of `viewport` length at `offset`, moving at `velocity`
/// (logical px/s, positive when the offset grows).
pub fn window(extents: &impl Extents, offset: f32, viewport: f32, velocity: f32, overscan: Overscan) -> Range<usize> {
    let n = extents.len();
    if n == 0 {
        return 0..0;
    }
    let ahead = (velocity.abs() * overscan.lead_s).min(overscan.max_ahead);
    let (before, after) = if velocity >= 0.0 {
        (overscan.around, overscan.around + ahead)
    } else {
        (overscan.around + ahead, overscan.around)
    };
    let first = extents.index_at(offset - before);
    let last = extents.index_at(offset + viewport + after);
    first..(last + 1).min(n)
}

/// Only the items actually on screen (for accessibility: what a screen reader reads first, and
/// for "3 of 100 000").
pub fn visible(extents: &impl Extents, offset: f32, viewport: f32) -> Range<usize> {
    window(extents, offset, viewport, 0.0, Overscan { around: 0.0, lead_s: 0.0, max_ahead: 0.0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extents::Fixed;

    #[test]
    fn a_window_holds_the_viewport_and_its_overscan() {
        let e = Fixed { count: 100_000, size: 48.0 };
        let o = Overscan { around: 96.0, lead_s: 0.0, max_ahead: 0.0 };
        let w = window(&e, 48.0 * 1000.0, 452.0, 0.0, o);
        assert_eq!(w, 998..1012);
        assert_eq!(visible(&e, 48.0 * 1000.0, 452.0), 1000..1010);
        assert!(w.len() < 20, "bounded whatever the count");
    }

    #[test]
    fn a_fling_builds_ahead_in_its_direction() {
        let e = Fixed { count: 100_000, size: 48.0 };
        let o = Overscan { around: 0.0, lead_s: 0.25, max_ahead: 960.0 };
        let still = window(&e, 48_000.0, 452.0, 0.0, o);
        let down = window(&e, 48_000.0, 452.0, 2000.0, o);
        let up = window(&e, 48_000.0, 452.0, -2000.0, o);
        assert_eq!(down.start, still.start);
        assert!(down.end > still.end + 9, "about 500 px ahead");
        assert_eq!(up.end, still.end);
        assert!(up.start < still.start);
        let fast = window(&e, 48_000.0, 452.0, 100_000.0, o);
        assert!(fast.len() <= still.len() + 21, "capped");
    }

    #[test]
    fn edges_and_empty() {
        let e = Fixed { count: 3, size: 48.0 };
        assert_eq!(window(&e, 0.0, 452.0, 0.0, Overscan::default()), 0..3);
        assert_eq!(window(&Fixed { count: 0, size: 48.0 }, 0.0, 452.0, 0.0, Overscan::default()), 0..0);
    }
}
