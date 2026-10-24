//! Anchored placement (input events §6.5, ui-model §9 `Overlay`): an overlay placed at another
//! element, on the side it asks for, flipped to the other side when it does not fit, and kept
//! inside its bounds (CSS anchor positioning with fallbacks; Flutter's CompositedTransform
//! follower recomputes it as the anchor moves). Logical sides: start and end mirror right to
//! left.

/// A rectangle: origin and size, in one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub origin: (f32, f32),
    pub size: (f32, f32),
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { origin: (x, y), size: (w, h) }
    }
    fn right(&self) -> f32 {
        self.origin.0 + self.size.0
    }
    fn bottom(&self) -> f32 {
        self.origin.1 + self.size.1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Below,
    Above,
    Start,
    End,
}

impl Side {
    fn flipped(self) -> Side {
        match self {
            Side::Below => Side::Above,
            Side::Above => Side::Below,
            Side::Start => Side::End,
            Side::End => Side::Start,
        }
    }
}

/// Where an overlay of `size` goes next to `anchor`, asked on `side` with `gap`, inside
/// `bounds`: the side used (flipped when the asked one does not fit and the other does) and the
/// overlay's origin, slid along to stay inside.
pub fn place(anchor: Rect, size: (f32, f32), side: Side, gap: f32, bounds: Rect, rtl: bool) -> (Side, (f32, f32)) {
    let physical = |s: Side| match (s, rtl) {
        (Side::Start, false) | (Side::End, true) => Side::Start, // left
        (Side::End, false) | (Side::Start, true) => Side::End,   // right
        (s, _) => s,
    };
    let at = |s: Side| -> (f32, f32) {
        match physical(s) {
            Side::Below => (anchor.origin.0, anchor.bottom() + gap),
            Side::Above => (anchor.origin.0, anchor.origin.1 - gap - size.1),
            Side::Start => (anchor.origin.0 - gap - size.0, anchor.origin.1),
            Side::End => (anchor.right() + gap, anchor.origin.1),
        }
    };
    let fits = |p: (f32, f32)| {
        p.0 >= bounds.origin.0 && p.1 >= bounds.origin.1 && p.0 + size.0 <= bounds.right() && p.1 + size.1 <= bounds.bottom()
    };
    let fits_main = |s: Side, p: (f32, f32)| match physical(s) {
        Side::Below | Side::Above => p.1 >= bounds.origin.1 && p.1 + size.1 <= bounds.bottom(),
        Side::Start | Side::End => p.0 >= bounds.origin.0 && p.0 + size.0 <= bounds.right(),
    };
    let mut used = side;
    let mut p = at(side);
    if !fits(p) && !fits_main(side, p) && fits_main(side.flipped(), at(side.flipped())) {
        used = side.flipped();
        p = at(used);
    }
    // Slide inside the bounds.
    let x = p.0.clamp(bounds.origin.0, (bounds.right() - size.0).max(bounds.origin.0));
    let y = p.1.clamp(bounds.origin.1, (bounds.bottom() - size.1).max(bounds.origin.1));
    (used, (x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { origin: (0.0, 28.0), size: (320.0, 452.0) };

    #[test]
    fn below_when_it_fits_above_when_it_does_not_and_always_inside() {
        let button = Rect::new(16.0, 100.0, 288.0, 48.0);
        assert_eq!(place(button, (288.0, 112.0), Side::Below, 4.0, SCREEN, false), (Side::Below, (16.0, 152.0)));
        let low = Rect::new(16.0, 400.0, 288.0, 48.0);
        assert_eq!(place(low, (288.0, 112.0), Side::Below, 4.0, SCREEN, false), (Side::Above, (16.0, 284.0)));
        // Wider than the room on its side: slid back inside.
        let edge = Rect::new(250.0, 100.0, 60.0, 40.0);
        let (_, p) = place(edge, (200.0, 80.0), Side::Below, 4.0, SCREEN, false);
        assert_eq!(p.0, 120.0);
    }

    #[test]
    fn start_and_end_mirror_right_to_left() {
        let a = Rect::new(150.0, 200.0, 20.0, 20.0);
        let (_, ltr) = place(a, (60.0, 20.0), Side::End, 4.0, SCREEN, false);
        let (_, rtl) = place(a, (60.0, 20.0), Side::End, 4.0, SCREEN, true);
        assert_eq!(ltr.0, 174.0, "end is right");
        assert_eq!(rtl.0, 86.0, "end is left, right to left");
    }
}
