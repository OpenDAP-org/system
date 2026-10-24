//! A decode queue (images, waveforms, video thumbnails): jobs by priority, a bounded number at
//! once (the T1's four cores are shared with audio, which always comes first: T1-008), the same
//! key never twice, and jobs whose content left the screen cancelled before they start.

use std::collections::{BTreeMap, HashMap};

/// Most needed first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    /// On screen now.
    Visible,
    /// About to be (the window's prefetch).
    Prefetch,
    /// Whenever idle (a library's thumbnails generated ahead).
    Background,
}

#[derive(Debug, Clone, Default)]
pub struct Queue {
    pub parallel: usize,
    waiting: BTreeMap<(Priority, u64), String>,
    by_key: HashMap<String, (Priority, u64)>,
    running: Vec<String>,
    seq: u64,
}

impl Queue {
    pub fn new(parallel: usize) -> Self {
        Queue { parallel: parallel.max(1), ..Default::default() }
    }

    /// Asks for `key` at `priority`; asking again raises its priority if higher, never adds a
    /// second job.
    pub fn want(&mut self, key: &str, priority: Priority) {
        if self.running.iter().any(|k| k == key) {
            return;
        }
        if let Some(&(p, s)) = self.by_key.get(key) {
            if priority >= p {
                return;
            }
            self.waiting.remove(&(p, s));
        }
        self.seq += 1;
        self.waiting.insert((priority, self.seq), key.to_string());
        self.by_key.insert(key.to_string(), (priority, self.seq));
    }

    /// The content left: a waiting job for it is dropped (a running one finishes and is cached).
    pub fn cancel(&mut self, key: &str) -> bool {
        match self.by_key.remove(key) {
            Some(k) => self.waiting.remove(&k).is_some(),
            None => false,
        }
    }

    /// The jobs to start now, most needed first, up to the parallel bound.
    pub fn start(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        while self.running.len() < self.parallel {
            let Some((&k, _)) = self.waiting.iter().next() else { break };
            let key = self.waiting.remove(&k).expect("present");
            self.by_key.remove(&key);
            self.running.push(key.clone());
            out.push(key);
        }
        out
    }

    pub fn done(&mut self, key: &str) {
        self.running.retain(|k| k != key);
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_first_bounded_deduplicated_and_cancellable() {
        let mut q = Queue::new(2);
        q.want("a", Priority::Background);
        q.want("b", Priority::Prefetch);
        q.want("c", Priority::Visible);
        q.want("a", Priority::Visible);
        q.want("c", Priority::Prefetch);
        assert_eq!(q.start(), vec!["c", "a"], "visible first; a raised; c not lowered");
        assert_eq!(q.start(), Vec::<String>::new(), "bounded");
        assert!(q.cancel("b"));
        q.done("c");
        assert_eq!(q.start(), Vec::<String>::new(), "b was cancelled");
        q.want("a", Priority::Visible);
        assert_eq!(q.waiting(), 0, "running: not twice");
    }
}
