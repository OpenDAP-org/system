//! Presence (ui-model §8: enter and exit): what appears animates in, and what is removed stays
//! drawn while it animates out, then goes (Framer's AnimatePresence, SwiftUI's transitions,
//! Compose's AnimatedVisibility). Slint destroys an element the moment its condition turns false,
//! so the list of what to draw comes from here: the present keys and the leaving ones, each with
//! its progress. Leaving items are drawn but no longer reachable (input events §3.7).

use std::collections::HashMap;
use std::hash::Hash;

use crate::animator::{Animator, Kind};
use crate::easing::Bezier;

/// Enter and exit durations (motion tokens: what appears takes `standard`, what leaves `fast`).
pub const ENTER_S: f32 = 0.2;
pub const LEAVE_S: f32 = 0.12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Entering,
    Present,
    Leaving,
}

#[derive(Debug)]
pub struct Presence<K: Eq + Hash + Clone> {
    items: Vec<(K, Animator, bool)>,
    pub reduce_motion: bool,
}

impl<K: Eq + Hash + Clone> Default for Presence<K> {
    fn default() -> Self {
        Presence { items: Vec::new(), reduce_motion: false }
    }
}

impl<K: Eq + Hash + Clone> Presence<K> {
    /// The keys that should be present now, in order: new ones enter, missing ones start leaving
    /// (and keep their place among the others while they do).
    pub fn update(&mut self, keys: &[K], now: f32) {
        let wanted: HashMap<&K, usize> = keys.iter().enumerate().map(|(i, k)| (k, i)).collect();
        for (k, anim, leaving) in &mut self.items {
            let want = wanted.contains_key(k);
            if want && *leaving {
                // Back before it was gone: it comes in again from where it is.
                *leaving = false;
                anim.retarget(1.0, now, None);
            } else if !want && !*leaving {
                *leaving = true;
                let at = anim.sample(now).0;
                *anim = Animator::new(at, Kind::Timed { duration: LEAVE_S, curve: Bezier::STANDARD });
                anim.reduce_motion = self.reduce_motion;
                anim.retarget(0.0, now, None);
            }
        }
        for k in keys {
            if !self.items.iter().any(|(x, _, _)| x == k) {
                let mut a = Animator::new(0.0, Kind::Timed { duration: ENTER_S, curve: Bezier::STANDARD });
                a.reduce_motion = self.reduce_motion;
                a.retarget(1.0, now, None);
                // In its place among the wanted keys: after the item before it.
                let at = keys.iter().position(|x| x == k).and_then(|i| i.checked_sub(1)).and_then(|i| self.items.iter().position(|(x, _, _)| *x == keys[i])).map_or(0, |p| p + 1);
                self.items.insert(at, (k.clone(), a, false));
            }
        }
        self.prune(now);
    }

    fn prune(&mut self, now: f32) {
        self.items.retain(|(_, a, leaving)| !*leaving || a.running(now));
    }

    /// What to draw at `now`: each key, its phase and its progress (0 absent, 1 fully present).
    pub fn frame(&mut self, now: f32) -> Vec<(K, Phase, f32)> {
        self.prune(now);
        self.items
            .iter()
            .map(|(k, a, leaving)| {
                let p = a.sample(now).0;
                let phase = if *leaving {
                    Phase::Leaving
                } else if a.running(now) {
                    Phase::Entering
                } else {
                    Phase::Present
                };
                (k.clone(), phase, p)
            })
            .collect()
    }

    /// Whether frames are needed.
    pub fn running(&self, now: f32) -> bool {
        self.items.iter().any(|(_, a, _)| a.running(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_leaves_stays_drawn_until_it_is_gone() {
        let mut p: Presence<&str> = Presence::default();
        p.update(&["a", "b", "c"], 0.0);
        assert_eq!(p.frame(1.0).iter().map(|x| x.1).collect::<Vec<_>>(), vec![Phase::Present; 3]);
        p.update(&["a", "c"], 1.0);
        let f = p.frame(1.05);
        assert_eq!(f[1].0, "b", "it keeps its place");
        assert_eq!(f[1].1, Phase::Leaving);
        assert!(f[1].2 > 0.0 && f[1].2 < 1.0);
        assert_eq!(p.frame(1.2).len(), 2, "gone once its exit ends");
        assert!(!p.running(1.2));
    }

    #[test]
    fn what_comes_back_while_leaving_returns_from_where_it_is() {
        let mut p: Presence<u32> = Presence::default();
        p.update(&[1], 0.0);
        p.update(&[], 1.0);
        let at = p.frame(1.05)[0].2;
        p.update(&[1], 1.05);
        let f = p.frame(1.05);
        assert_eq!((f.len(), f[0].1), (1, Phase::Entering));
        assert!((f[0].2 - at).abs() < 1e-4, "no jump");
    }

    #[test]
    fn new_items_enter_in_their_place() {
        let mut p: Presence<u32> = Presence::default();
        p.update(&[1, 3], 0.0);
        p.update(&[1, 2, 3], 1.0);
        let keys: Vec<u32> = p.frame(1.0).iter().map(|x| x.0).collect();
        assert_eq!(keys, vec![1, 2, 3]);
        let mut r: Presence<u32> = Presence { reduce_motion: true, ..Default::default() };
        r.update(&[7], 0.0);
        assert_eq!(r.frame(0.0)[0].2, 1.0, "reduce motion: there at once");
    }
}
