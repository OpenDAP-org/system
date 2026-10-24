//! A cache within a budget: least recently used goes first, by cost (bytes of a decoded cover,
//! a page's items), so memory stays bounded however long the device runs (T1-047).

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

#[derive(Debug, Clone)]
pub struct Cache<K: Eq + Hash + Clone, V> {
    budget: usize,
    used: usize,
    entries: HashMap<K, (V, usize)>,
    /// Least recently used at the front.
    order: VecDeque<K>,
}

impl<K: Eq + Hash + Clone, V> Cache<K, V> {
    pub fn new(budget: usize) -> Self {
        Cache { budget, used: 0, entries: HashMap::new(), order: VecDeque::new() }
    }

    pub fn used(&self) -> usize {
        self.used
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn touch(&mut self, key: &K) {
        if let Some(i) = self.order.iter().position(|k| k == key) {
            let k = self.order.remove(i).expect("present");
            self.order.push_back(k);
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.entries.contains_key(key) {
            self.touch(key);
        }
        self.entries.get(key).map(|(v, _)| v)
    }

    /// Stores `value` costing `cost`; returns the keys evicted to make room. A value costing more
    /// than the whole budget is not kept.
    pub fn put(&mut self, key: K, value: V, cost: usize) -> Vec<K> {
        let mut evicted = Vec::new();
        if cost > self.budget {
            return evicted;
        }
        if let Some((_, old)) = self.entries.remove(&key) {
            self.used -= old;
            self.order.retain(|k| *k != key);
        }
        while self.used + cost > self.budget {
            let Some(k) = self.order.pop_front() else { break };
            if let Some((_, c)) = self.entries.remove(&k) {
                self.used -= c;
            }
            evicted.push(k);
        }
        self.used += cost;
        self.entries.insert(key.clone(), (value, cost));
        self.order.push_back(key);
        evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_least_recently_used_goes_first_within_the_budget() {
        let mut c: Cache<u32, &str> = Cache::new(300);
        c.put(1, "a", 100);
        c.put(2, "b", 100);
        c.put(3, "c", 100);
        c.get(&1);
        assert_eq!(c.put(4, "d", 150), vec![2, 3]);
        assert_eq!(c.used(), 250);
        assert!(c.get(&1).is_some());
        assert!(c.put(5, "huge", 1000).is_empty() && c.get(&5).is_none(), "never kept");
    }
}
