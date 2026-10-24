//! Scroll anchoring (CSS `overflow-anchor`, Android's and iOS's list behaviour): when items
//! above what the user is looking at change size, appear or go (a cover loads, a track is added
//! at the top), the scroll offset moves by the same amount, so nothing on screen jumps.

use crate::extents::Extents;

/// What the user is looking at: an item by key, and how far into the viewport its start is.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor<K> {
    pub key: K,
    /// The item's start minus the scroll offset (negative when it starts above the viewport).
    pub inset: f32,
}

/// The anchor at a scroll offset: the first item visible, by its key.
pub fn take<K: Clone>(extents: &impl Extents, keys: &[K], offset: f32) -> Option<Anchor<K>> {
    if extents.is_empty() {
        return None;
    }
    let i = extents.index_at(offset);
    Some(Anchor { key: keys.get(i)?.clone(), inset: extents.offset(i) - offset })
}

/// The scroll offset that keeps `anchor` where it was after the list changed (new extents and
/// keys); `None` when the anchored item is gone (keep the offset, clamped by the caller).
pub fn restore<K: PartialEq>(extents: &impl Extents, keys: &[K], anchor: &Anchor<K>) -> Option<f32> {
    let i = keys.iter().position(|k| *k == anchor.key)?;
    Some((extents.offset(i) - anchor.inset).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extents::{Fixed, Measured};

    #[test]
    fn items_added_above_do_not_move_what_is_on_screen() {
        let keys: Vec<u32> = (100..200).collect();
        let e = Fixed { count: keys.len(), size: 50.0 };
        // Looking at item 120, which starts 10 px into the viewport.
        let a = take(&e, &keys, 50.0 * 20.0 - 10.0).unwrap();
        assert_eq!((a.key, a.inset), (119, -40.0));
        // Three tracks inserted at the top.
        let keys2: Vec<u32> = [1, 2, 3].into_iter().chain(100..200).collect();
        let e2 = Fixed { count: keys2.len(), size: 50.0 };
        let o = restore(&e2, &keys2, &a).unwrap();
        assert_eq!(o, 50.0 * 22.0 + 40.0, "moved by the 150 px inserted");
        assert_eq!(take(&e2, &keys2, o).unwrap().key, 119);
    }

    #[test]
    fn a_cover_that_grows_above_moves_the_offset_by_its_growth() {
        let keys: Vec<u32> = (0..50).collect();
        let mut e = Measured::new(50, 60.0);
        let a = take(&e, &keys, 600.0).unwrap();
        e.measure(2, 160.0);
        assert_eq!(restore(&e, &keys, &a), Some(700.0));
        assert_eq!(restore(&e, &[7u32, 8], &a), None, "gone");
    }
}
