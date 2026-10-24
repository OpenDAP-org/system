//! Matched elements (input events §6.4: scoped anchors): two elements in different places of the
//! tree, or on two screens, that are the same thing (an album's tile and its cover on the next
//! screen) declare one id in one namespace; a transition pairs them and moves the one from the
//! other's frame (SwiftUI's `@Namespace` with `matchedGeometryEffect`, Android's shared element
//! names). Namespaces keep two lists' ids apart, so nothing is global. Frames are in one shared
//! space (`@screen`), so the pair is found wherever each side sits.

use crate::anchor::Rect;

/// Which side of a transition an element is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Where the element is now (the tile).
    Source,
    /// Where it goes (the cover on the next screen).
    Destination,
}

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    namespace: String,
    id: String,
    side: Side,
    frame: Rect,
}

/// The matched elements declared on screen.
#[derive(Debug, Clone, Default)]
pub struct Matched {
    entries: Vec<Entry>,
}

impl Matched {
    /// An element says where it is; the same namespace, id and side again replaces it (it moved).
    pub fn declare(&mut self, namespace: &str, id: &str, side: Side, frame: Rect) {
        self.entries.retain(|e| !(e.namespace == namespace && e.id == id && e.side == side));
        self.entries.push(Entry { namespace: namespace.into(), id: id.into(), side, frame });
    }

    /// An element went away (its screen closed, its row left the window).
    pub fn withdraw(&mut self, namespace: &str, id: &str, side: Side) {
        self.entries.retain(|e| !(e.namespace == namespace && e.id == id && e.side == side));
    }

    /// The pairs of a namespace: each id with both sides declared, from its source's frame to its
    /// destination's. An id with one side only moves nothing (it simply appears or leaves).
    pub fn pairs(&self, namespace: &str) -> Vec<(String, Rect, Rect)> {
        self.entries
            .iter()
            .filter(|e| e.namespace == namespace && e.side == Side::Source)
            .filter_map(|s| {
                self.entries
                    .iter()
                    .find(|d| d.namespace == namespace && d.id == s.id && d.side == Side::Destination)
                    .map(|d| (s.id.clone(), s.frame, d.frame))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tile_pairs_with_its_cover_and_only_in_its_namespace() {
        let mut m = Matched::default();
        m.declare("albums", "kind-of-blue", Side::Source, Rect::new(16.0, 300.0, 72.0, 72.0));
        m.declare("albums", "a-love-supreme", Side::Source, Rect::new(96.0, 300.0, 72.0, 72.0));
        m.declare("queue", "kind-of-blue", Side::Source, Rect::new(16.0, 60.0, 40.0, 40.0));
        m.declare("albums", "kind-of-blue", Side::Destination, Rect::new(16.0, 76.0, 288.0, 288.0));
        let p = m.pairs("albums");
        assert_eq!(p.len(), 1, "only the album with both sides");
        assert_eq!(p[0], ("kind-of-blue".into(), Rect::new(16.0, 300.0, 72.0, 72.0), Rect::new(16.0, 76.0, 288.0, 288.0)));
        assert!(m.pairs("queue").is_empty(), "the queue's tile is another thing");
    }

    #[test]
    fn a_moved_element_replaces_itself_and_a_withdrawn_one_pairs_no_more() {
        let mut m = Matched::default();
        m.declare("a", "x", Side::Source, Rect::new(0.0, 0.0, 10.0, 10.0));
        m.declare("a", "x", Side::Source, Rect::new(0.0, 50.0, 10.0, 10.0));
        m.declare("a", "x", Side::Destination, Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(m.pairs("a")[0].1, Rect::new(0.0, 50.0, 10.0, 10.0));
        m.withdraw("a", "x", Side::Destination);
        assert!(m.pairs("a").is_empty());
    }
}
