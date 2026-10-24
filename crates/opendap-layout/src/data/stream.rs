//! Results that arrive in parts (a search over a large library, a scan of a card): appended at
//! the end without moving what is shown, deduplicated by key, and dropped when the query that
//! asked for them is no longer the current one.

use std::collections::HashSet;
use std::hash::Hash;

#[derive(Debug, Clone)]
pub struct Stream<K: Eq + Hash + Clone> {
    query: u64,
    seen: HashSet<K>,
    pub items: Vec<K>,
    pub complete: bool,
}

impl<K: Eq + Hash + Clone> Default for Stream<K> {
    fn default() -> Self {
        Stream { query: 0, seen: HashSet::new(), items: Vec::new(), complete: false }
    }
}

impl<K: Eq + Hash + Clone> Stream<K> {
    /// A new query: what came before is gone; returns the query's number for its parts.
    pub fn restart(&mut self) -> u64 {
        self.query += 1;
        self.seen.clear();
        self.items.clear();
        self.complete = false;
        self.query
    }

    /// A part of `query`'s results: appended, new keys only; ignored when `query` is stale.
    /// Returns how many were added.
    pub fn part(&mut self, query: u64, keys: impl IntoIterator<Item = K>, last: bool) -> usize {
        if query != self.query {
            return 0;
        }
        let before = self.items.len();
        for k in keys {
            if self.seen.insert(k.clone()) {
                self.items.push(k);
            }
        }
        self.complete |= last;
        self.items.len() - before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_append_dedupe_and_stale_queries_drop() {
        let mut s: Stream<u32> = Stream::default();
        let q1 = s.restart();
        assert_eq!(s.part(q1, [1, 2, 3], false), 3);
        assert_eq!(s.part(q1, [3, 4], false), 1, "deduplicated");
        let q2 = s.restart();
        assert_eq!(s.part(q1, [9], true), 0, "the old query's part: dropped");
        assert_eq!(s.part(q2, [7], true), 1);
        assert_eq!(s.items, vec![7]);
        assert!(s.complete);
    }
}
