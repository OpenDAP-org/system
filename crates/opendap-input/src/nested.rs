//! Nested scrolling (input events §6.5, ui-model §7): a scroll area inside another takes the
//! motion it can, and hands what is left to its parent; a fling does the same with its velocity
//! (Compose's nestedScroll: pre-scroll, scroll, post-scroll; CSS overscroll-behavior). A parent
//! may take first (a collapsing header that shrinks before the list scrolls).

/// One scroll area along one axis: where it is and how far it can go.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scroller {
    pub offset: f32,
    /// The largest offset: content length minus viewport length, at least 0.
    pub max: f32,
    /// Takes the motion before its child does (a collapsing header), up to its range.
    pub takes_first: bool,
}

impl Scroller {
    pub fn new(content: f32, viewport: f32) -> Self {
        Scroller { offset: 0.0, max: (content - viewport).max(0.0), takes_first: false }
    }

    /// Moves by `delta` (content moves opposite to a finger: a finger moving up scrolls down,
    /// positive), within its range; returns what it could not take.
    pub fn take(&mut self, delta: f32) -> f32 {
        let to = (self.offset + delta).clamp(0.0, self.max);
        let used = to - self.offset;
        self.offset = to;
        delta - used
    }
}

/// A chain of nested scroll areas, the innermost first: a finger's scroll delta goes through it
/// and comes out as what nobody took (an edge effect's stretch, ui-model §7).
pub fn scroll(chain: &mut [Scroller], delta: f32) -> f32 {
    let mut left = delta;
    // Pre-scroll: the outer areas that take first, outermost first.
    for s in chain.iter_mut().rev().filter(|s| s.takes_first) {
        left = s.take(left);
    }
    // Scroll and post-scroll: the innermost first, then outward.
    for s in chain.iter_mut().filter(|s| !s.takes_first) {
        left = s.take(left);
    }
    left
}

/// A fling's velocity going through the chain: the innermost area that can still move in its
/// direction takes it; none can, it is left over (for the edge effect).
pub fn fling(chain: &[Scroller], velocity: f32) -> Option<usize> {
    chain.iter().position(|s| {
        if velocity > 0.0 { s.offset < s.max } else { s.offset > 0.0 }
    })
}

/// One scroll area that the focus is revealed in: its scroller, its viewport's length, where the
/// viewport sits in its parent's content, and what covers its edges (a sticky header pinned at
/// the start, a playing strip at the end).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RevealArea {
    pub scroller: Scroller,
    pub viewport: f32,
    pub at: f32,
    pub inset_start: f32,
    pub inset_end: f32,
}

/// The focus moved to the span `start..end` (in the innermost area's content): each area,
/// innermost first, scrolls just enough to show it whole below what covers its edges (or its
/// start, when it is taller than the room), then the part it shows is revealed in the parent
/// (the browser's nested `scrollIntoView`, `block: nearest`). Returns the new offsets.
pub fn reveal(chain: &mut [RevealArea], mut start: f32, mut end: f32) -> Vec<f32> {
    for a in chain.iter_mut() {
        let room_start = a.scroller.offset + a.inset_start;
        let room_end = a.scroller.offset + a.viewport - a.inset_end;
        let to = if start < room_start || end - start > room_end - room_start {
            start - a.inset_start
        } else if end > room_end {
            end - a.viewport + a.inset_end
        } else {
            a.scroller.offset
        };
        a.scroller.offset = to.clamp(0.0, a.scroller.max);
        let (s, e) = ((start - a.scroller.offset).max(0.0), (end - a.scroller.offset).min(a.viewport));
        start = a.at + s;
        end = a.at + e;
    }
    chain.iter().map(|a| a.scroller.offset).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inner_list_scrolls_first_then_hands_the_rest_to_the_page() {
        // A row of covers 600 long in a 300 viewport, inside a page 1000 long in a 400 viewport.
        let mut chain = [Scroller::new(600.0, 300.0), Scroller::new(1000.0, 400.0)];
        assert_eq!(scroll(&mut chain, 200.0), 0.0);
        assert_eq!((chain[0].offset, chain[1].offset), (200.0, 0.0));
        assert_eq!(scroll(&mut chain, 200.0), 0.0);
        assert_eq!((chain[0].offset, chain[1].offset), (300.0, 100.0), "the end of the inner one");
        assert_eq!(scroll(&mut chain, 900.0), 400.0, "what nobody took: the edge effect");
        assert_eq!(fling(&chain, 1.0), None);
        assert_eq!(fling(&chain, -1.0), Some(0));
    }

    #[test]
    fn a_collapsing_header_takes_first() {
        let mut header = Scroller::new(120.0, 64.0);
        header.takes_first = true;
        let mut chain = [Scroller::new(2000.0, 400.0), header];
        scroll(&mut chain, 40.0);
        assert_eq!((chain[0].offset, chain[1].offset), (0.0, 40.0), "the header shrinks first");
        scroll(&mut chain, 40.0);
        assert_eq!((chain[0].offset, chain[1].offset), (24.0, 56.0), "then the list");
    }

    #[test]
    fn the_focus_is_revealed_innermost_first_and_below_a_sticky_header() {
        // A 300 list (48 rows) at 200 in a page whose viewport is 400; the list has a 28 sticky
        // header; the page a 64 playing strip at its end.
        let list = RevealArea { scroller: Scroller::new(4800.0, 300.0), viewport: 300.0, at: 200.0, inset_start: 28.0, inset_end: 0.0 };
        let page = RevealArea { scroller: Scroller::new(1200.0, 400.0), viewport: 400.0, at: 0.0, inset_start: 0.0, inset_end: 64.0 };
        let mut chain = [list, page];
        // Row 7 (336..384): the list scrolls it to its bottom; the page brings the list's bottom
        // above the strip.
        assert_eq!(reveal(&mut chain, 336.0, 384.0), vec![84.0, 164.0]);
        // Back up to row 2 (96..144): under the sticky header, not behind it.
        assert_eq!(reveal(&mut chain, 96.0, 144.0)[0], 68.0);
        // Already in view: nothing moves.
        assert_eq!(reveal(&mut chain, 120.0, 140.0)[0], 68.0);
    }
}
