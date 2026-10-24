//! Requests in flight: one per key (asking twice joins the first), and a generation per key, so
//! an answer to a request superseded (the data changed, a new query was typed) is dropped
//! instead of overwriting the newer one.

use std::collections::HashMap;
use std::hash::Hash;

/// A request's ticket: the key and the generation it was asked under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket<K> {
    pub key: K,
    pub generation: u64,
}

#[derive(Debug, Clone)]
pub struct Requests<K: Eq + Hash + Clone> {
    generation: HashMap<K, u64>,
    in_flight: HashMap<K, u64>,
}

impl<K: Eq + Hash + Clone> Default for Requests<K> {
    fn default() -> Self {
        Requests { generation: HashMap::new(), in_flight: HashMap::new() }
    }
}

impl<K: Eq + Hash + Clone> Requests<K> {
    /// Asks for `key`: a ticket to run the request, or `None` when the same is already in flight.
    pub fn begin(&mut self, key: K) -> Option<Ticket<K>> {
        let g = *self.generation.get(&key).unwrap_or(&0);
        if self.in_flight.get(&key) == Some(&g) {
            return None;
        }
        self.in_flight.insert(key.clone(), g);
        Some(Ticket { key, generation: g })
    }

    /// The data behind `key` changed (or a newer query replaced it): answers in flight are
    /// stale.
    pub fn invalidate(&mut self, key: &K) {
        *self.generation.entry(key.clone()).or_insert(0) += 1;
        self.in_flight.remove(key);
    }

    /// An answer arrived: whether to use it (it is the current generation's).
    pub fn finish(&mut self, ticket: &Ticket<K>) -> bool {
        let current = *self.generation.get(&ticket.key).unwrap_or(&0);
        if self.in_flight.get(&ticket.key) == Some(&ticket.generation) {
            self.in_flight.remove(&ticket.key);
        }
        ticket.generation == current
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_twice_joins_and_a_stale_answer_is_dropped() {
        let mut r: Requests<&str> = Requests::default();
        let t = r.begin("cover-12").unwrap();
        assert!(r.begin("cover-12").is_none(), "already in flight");
        r.invalidate(&"cover-12");
        let t2 = r.begin("cover-12").unwrap();
        assert!(!r.finish(&t), "the old answer arrives late: dropped");
        assert!(r.finish(&t2));
        assert_eq!(r.in_flight(), 0);
    }
}
