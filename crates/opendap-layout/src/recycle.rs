//! Recycling (RecyclerView's view pool, a lazy list's reuse): a fixed set of slots, each showing
//! one item by key. When the window moves, items that stay keep their slot, items that leave
//! free theirs, and items that come take a free one: a slot is rebound to new data, never
//! rebuilt, and the number of slots is bounded by the window, not the list.

use std::collections::HashMap;
use std::hash::Hash;

/// The slots and what each shows.
#[derive(Debug, Clone, Default)]
pub struct Pool<K: Eq + Hash + Clone> {
    slots: Vec<Option<K>>,
    by_key: HashMap<K, usize>,
}

/// What moving the window did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Changes {
    /// Slots rebound to a new item: (slot, item index in the window's keys).
    pub bound: Vec<(usize, usize)>,
    /// Slots whose item stayed: no work.
    pub kept: usize,
    /// Slots created because none was free (only while the window grows).
    pub created: usize,
}

impl<K: Eq + Hash + Clone> Pool<K> {
    pub fn new() -> Self {
        Pool { slots: Vec::new(), by_key: HashMap::new() }
    }

    pub fn slots(&self) -> usize {
        self.slots.len()
    }

    /// The slot showing `key`, if any.
    pub fn slot_of(&self, key: &K) -> Option<usize> {
        self.by_key.get(key).copied()
    }

    /// The window now holds `keys`, in order.
    pub fn update(&mut self, keys: &[K]) -> Changes {
        let mut changes = Changes::default();
        let wanted: std::collections::HashSet<&K> = keys.iter().collect();
        // Free the slots of items that left.
        for slot in self.slots.iter_mut() {
            if let Some(k) = slot
                && !wanted.contains(k)
            {
                self.by_key.remove(k);
                *slot = None;
            }
        }
        let mut free: Vec<usize> = (0..self.slots.len()).filter(|&i| self.slots[i].is_none()).rev().collect();
        for (i, k) in keys.iter().enumerate() {
            if self.by_key.contains_key(k) {
                changes.kept += 1;
                continue;
            }
            let slot = match free.pop() {
                Some(s) => s,
                None => {
                    self.slots.push(None);
                    changes.created += 1;
                    self.slots.len() - 1
                }
            };
            self.slots[slot] = Some(k.clone());
            self.by_key.insert(k.clone(), slot);
            changes.bound.push((slot, i));
        }
        changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_the_window_rebinds_only_what_came_in() {
        let mut p: Pool<u32> = Pool::new();
        let c = p.update(&(0..10).collect::<Vec<_>>());
        assert_eq!((c.created, c.kept, c.bound.len()), (10, 0, 10));
        // Scrolled by three items.
        let c = p.update(&(3..13).collect::<Vec<_>>());
        assert_eq!((c.created, c.kept, c.bound.len()), (0, 7, 3));
        assert_eq!(p.slots(), 10, "bounded by the window");
        // Items 0..3's slots now show 10..13.
        let reused: Vec<usize> = (10..13).map(|k| p.slot_of(&k).unwrap()).collect();
        assert!(reused.iter().all(|s| *s < 10));
    }

    #[test]
    fn a_long_scroll_never_grows_the_pool() {
        let mut p: Pool<u64> = Pool::new();
        for start in (0..100_000u64).step_by(7) {
            p.update(&(start..start + 12).collect::<Vec<_>>());
        }
        assert_eq!(p.slots(), 12);
    }
}
