//! Items in pages: what the window needs is loaded a page at a time, the visible pages first,
//! then the prefetch in the scroll's direction; pages being loaded that went far are cancelled,
//! and loaded pages beyond a budget are evicted, farthest first (Android's Paging, iOS's
//! prefetching data sources). An item not loaded yet is a placeholder of the right size, so the
//! list never jumps when it arrives.

use std::collections::BTreeMap;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageState {
    Loading,
    Ready,
    Failed,
}

/// What an item shows now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    /// Not loaded: a placeholder of the item's size (ADR 0003's loading: the structure at once).
    Placeholder,
    Ready,
    /// Its page failed: an error in place, with retry.
    Failed,
}

/// What to do now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// Pages to request, most needed first.
    pub load: Vec<usize>,
    /// Pages in flight no longer needed: cancel them.
    pub cancel: Vec<usize>,
    /// Loaded pages to let go (memory).
    pub evict: Vec<usize>,
}

/// The directives of a paged list: how big a page is, how many to keep ahead, how many to keep
/// at most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paging {
    pub page_size: usize,
    /// Pages to prefetch beyond the window in the scroll's direction (one behind always).
    pub ahead: usize,
    /// Loaded pages kept at most; the farthest from the window go first.
    pub keep: usize,
}

impl Default for Paging {
    fn default() -> Self {
        // Proposed, to measure on the T1: pages of 50 items, two ahead, twelve kept (600 items).
        Paging { page_size: 50, ahead: 2, keep: 12 }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Pages {
    pub paging: Paging,
    /// The item count, when known (a cursor's end not reached yet: unknown).
    pub count: Option<usize>,
    states: BTreeMap<usize, PageState>,
}

impl Pages {
    pub fn new(paging: Paging, count: Option<usize>) -> Self {
        Pages { paging, count, states: BTreeMap::new() }
    }

    pub fn page_of(&self, item: usize) -> usize {
        item / self.paging.page_size.max(1)
    }

    pub fn state(&self, page: usize) -> Option<PageState> {
        self.states.get(&page).copied()
    }

    pub fn item(&self, item: usize) -> ItemState {
        match self.state(self.page_of(item)) {
            Some(PageState::Ready) => ItemState::Ready,
            Some(PageState::Failed) => ItemState::Failed,
            _ => ItemState::Placeholder,
        }
    }

    pub fn loaded(&self) -> usize {
        self.states.values().filter(|s| **s == PageState::Ready).count()
    }

    pub fn in_flight(&self) -> usize {
        self.states.values().filter(|s| **s == PageState::Loading).count()
    }

    fn last_page(&self) -> Option<usize> {
        self.count.map(|c| c.div_ceil(self.paging.page_size.max(1)).saturating_sub(1))
    }

    /// The plan for a window of items, moving forward (`forward`) or back. Marks what it asks to
    /// load as loading and what it cancels or evicts as gone.
    pub fn plan(&mut self, window: Range<usize>, forward: bool) -> Plan {
        let mut plan = Plan::default();
        if window.is_empty() {
            return plan;
        }
        let (first, last) = (self.page_of(window.start), self.page_of(window.end - 1));
        let clamp = |p: usize| self.last_page().map_or(p, |l| p.min(l));
        let (lo, hi) = if forward {
            (first.saturating_sub(1), clamp(last + self.paging.ahead))
        } else {
            (first.saturating_sub(self.paging.ahead), clamp(last + 1))
        };
        // The window's pages first, nearest the middle out; then the prefetch, nearest first.
        let mut wanted: Vec<usize> = (first..=clamp(last)).collect();
        let mid = (first + last) / 2;
        wanted.sort_by_key(|p| p.abs_diff(mid));
        let prefetch: Vec<usize> = if forward {
            (clamp(last) + 1..=hi).chain((lo..first).rev()).collect()
        } else {
            (lo..first).rev().chain(clamp(last) + 1..=hi).collect()
        };
        wanted.extend(prefetch);
        for p in &wanted {
            if !self.states.contains_key(p) {
                self.states.insert(*p, PageState::Loading);
                plan.load.push(*p);
            }
        }
        // Cancel what is loading outside the keep range.
        let keep = lo..=hi;
        for (p, s) in self.states.iter() {
            if *s == PageState::Loading && !keep.contains(p) {
                plan.cancel.push(*p);
            }
        }
        for p in &plan.cancel {
            self.states.remove(p);
        }
        // Evict ready pages beyond the budget, the farthest from the window first.
        let mut ready: Vec<usize> = self.states.iter().filter(|(_, s)| **s == PageState::Ready).map(|(p, _)| *p).collect();
        if ready.len() > self.paging.keep {
            ready.sort_by_key(|p| std::cmp::Reverse(if *p < first { first - p } else { p.saturating_sub(last) }));
            for p in ready {
                if self.states.values().filter(|s| **s == PageState::Ready).count() <= self.paging.keep {
                    break;
                }
                if keep.contains(&p) && p >= first && p <= last {
                    continue;
                }
                self.states.remove(&p);
                plan.evict.push(p);
            }
        }
        plan
    }

    /// A page arrived. `last` when a cursor says there is nothing after it (the count becomes
    /// known). Ignored if the page was cancelled meanwhile.
    pub fn arrived(&mut self, page: usize, items: usize, last: bool) {
        if self.states.get(&page) != Some(&PageState::Loading) {
            return;
        }
        self.states.insert(page, PageState::Ready);
        if last {
            self.count = Some(page * self.paging.page_size + items);
        }
    }

    pub fn failed(&mut self, page: usize) {
        if self.states.get(&page) == Some(&PageState::Loading) {
            self.states.insert(page, PageState::Failed);
        }
    }

    /// Retry a failed page: it is planned again.
    pub fn retry(&mut self, page: usize) {
        if self.states.get(&page) == Some(&PageState::Failed) {
            self.states.remove(&page);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_pages_first_then_prefetch_ahead() {
        let mut p = Pages::new(Paging { page_size: 50, ahead: 2, keep: 12 }, Some(100_000));
        let plan = p.plan(1000..1030, true);
        assert_eq!(plan.load, vec![20, 21, 22, 19]);
        assert_eq!(p.item(1010), ItemState::Placeholder);
        p.arrived(20, 50, false);
        assert_eq!(p.item(1010), ItemState::Ready);
        // Moving on: only the new pages are asked.
        let plan = p.plan(1050..1080, true);
        assert_eq!(plan.load, vec![23]);
    }

    #[test]
    fn a_long_fling_cancels_what_went_far_and_memory_stays_bounded() {
        let mut p = Pages::new(Paging { page_size: 50, ahead: 2, keep: 6 }, Some(100_000));
        p.plan(0..30, true);
        let plan = p.plan(50_000..50_030, true);
        assert_eq!(plan.cancel, vec![0, 1, 2], "the old pages, still loading, went far");
        for page in 999..=1002 {
            p.arrived(page, 50, false);
        }
        for start in (50_000..60_000).step_by(500) {
            let plan = p.plan(start..start + 30, true);
            for page in plan.load {
                p.arrived(page, 50, false);
            }
            assert!(p.loaded() <= 6 + 4, "bounded");
        }
    }

    #[test]
    fn a_cursor_learns_its_end_and_a_late_answer_is_ignored() {
        let mut p = Pages::new(Paging::default(), None);
        p.plan(0..30, true);
        p.arrived(0, 50, false);
        p.arrived(1, 12, true);
        assert_eq!(p.count, Some(62));
        assert!(p.plan(40..62, true).load.is_empty(), "nothing past the end");
        p.arrived(7, 50, false);
        assert_eq!(p.state(7), None, "never asked, or cancelled: ignored");
        p.plan(0..10, true);
        p.failed(2);
        p.retry(2);
        assert_eq!(p.state(2), None);
    }
}
