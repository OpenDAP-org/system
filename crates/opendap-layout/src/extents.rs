//! Item sizes along the scroll axis. Fixed sizes need no memory; measured sizes keep only the
//! measured items' differences from an estimate, in a Fenwick tree, so offsets and lookups stay
//! O(log n) for any count and memory grows with what was measured, not with the list.

/// What a windowed list needs to know about its items' sizes.
pub trait Extents {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Where item `i` starts (the sum of the sizes before it).
    fn offset(&self, i: usize) -> f32;
    /// Item `i`'s size.
    fn size(&self, i: usize) -> f32;
    /// The item at a content offset (the last one past the end).
    fn index_at(&self, offset: f32) -> usize;
    /// The whole content's length.
    fn total(&self) -> f32 {
        self.offset(self.len())
    }
}

/// Every item the same size (a list of lines).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fixed {
    pub count: usize,
    pub size: f32,
}

impl Extents for Fixed {
    fn len(&self) -> usize {
        self.count
    }
    fn offset(&self, i: usize) -> f32 {
        i.min(self.count) as f32 * self.size
    }
    fn size(&self, _i: usize) -> f32 {
        self.size
    }
    fn index_at(&self, offset: f32) -> usize {
        if self.count == 0 || self.size <= 0.0 {
            return 0;
        }
        ((offset.max(0.0) / self.size) as usize).min(self.count - 1)
    }
}

/// Items of their own sizes, measured as they are built; the others at an estimate. Memory: one
/// f32 per item for the deltas (a Fenwick tree), no per-item allocation.
#[derive(Debug, Clone)]
pub struct Measured {
    estimate: f32,
    /// Fenwick tree over each item's (size − estimate), 1-based.
    tree: Vec<f32>,
    sizes: Vec<f32>,
}

impl Measured {
    pub fn new(count: usize, estimate: f32) -> Self {
        Measured { estimate, tree: vec![0.0; count + 1], sizes: vec![estimate; count] }
    }

    /// Item `i` was built and measured at `size`.
    pub fn measure(&mut self, i: usize, size: f32) {
        let delta = size - self.sizes[i];
        if delta == 0.0 {
            return;
        }
        self.sizes[i] = size;
        let mut k = i + 1;
        while k < self.tree.len() {
            self.tree[k] += delta;
            k += k.isolate_lowest_one();
        }
    }

    /// The sum of the deltas of items `0..i`.
    fn delta_before(&self, i: usize) -> f32 {
        let mut k = i.min(self.sizes.len());
        let mut sum = 0.0;
        while k > 0 {
            sum += self.tree[k];
            k -= k.isolate_lowest_one();
        }
        sum
    }
}

impl Extents for Measured {
    fn len(&self) -> usize {
        self.sizes.len()
    }
    fn offset(&self, i: usize) -> f32 {
        let i = i.min(self.sizes.len());
        i as f32 * self.estimate + self.delta_before(i)
    }
    fn size(&self, i: usize) -> f32 {
        self.sizes[i]
    }
    fn index_at(&self, offset: f32) -> usize {
        // Binary search on offsets: O(log² n), fine at any count a device holds.
        let n = self.sizes.len();
        if n == 0 {
            return 0;
        }
        let (mut lo, mut hi) = (0usize, n - 1);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if self.offset(mid) <= offset { lo = mid } else { hi = mid - 1 }
        }
        lo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_sizes_need_no_memory() {
        let e = Fixed { count: 1_000_000, size: 56.0 };
        assert_eq!(e.total(), 56_000_000.0);
        assert_eq!(e.index_at(56.0 * 500_000.0 + 10.0), 500_000);
        assert_eq!(e.index_at(-5.0), 0);
        assert_eq!(e.index_at(1e12), 999_999);
    }

    #[test]
    fn measured_sizes_correct_the_estimate_where_measured() {
        let mut e = Measured::new(10, 50.0);
        assert_eq!(e.offset(4), 200.0);
        e.measure(1, 80.0);
        e.measure(3, 20.0);
        assert_eq!(e.offset(2), 130.0);
        assert_eq!(e.offset(4), 200.0, "+30 −30");
        assert_eq!(e.offset(10), 500.0);
        assert_eq!(e.index_at(129.0), 1);
        assert_eq!(e.index_at(130.0), 2);
        assert_eq!(e.index_at(185.0), 3);
        e.measure(1, 80.0);
        assert_eq!(e.offset(2), 130.0, "measuring the same size again changes nothing");
    }

    #[test]
    fn a_million_measured_items_stay_fast() {
        let mut e = Measured::new(1_000_000, 56.0);
        for i in (0..1_000_000).step_by(1000) {
            e.measure(i, 72.0);
        }
        assert_eq!(e.offset(1_000_000), 56_000_000.0 + 1000.0 * 16.0);
        let i = e.index_at(28_000_000.0);
        assert!(e.offset(i) <= 28_000_000.0 && e.offset(i + 1) > 28_000_000.0);
    }
}
