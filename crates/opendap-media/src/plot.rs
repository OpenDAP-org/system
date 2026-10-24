//! Data to pixels: a waveform or a spectrum of millions of samples drawn in a few hundred
//! columns, keeping every peak (min and max per column: never averaged away, so a clip shows),
//! and a ring buffer of the latest values for live meters (levels, a scope), fixed in memory.

/// For each of `columns` columns, the min and max of the samples that fall in it.
pub fn min_max(samples: &[f32], columns: usize) -> Vec<(f32, f32)> {
    if columns == 0 || samples.is_empty() {
        return Vec::new();
    }
    let n = samples.len();
    (0..columns)
        .map(|c| {
            let a = c * n / columns;
            let b = ((c + 1) * n / columns).max(a + 1).min(n);
            samples[a..b].iter().fold((f32::MAX, f32::MIN), |(lo, hi), s| (lo.min(*s), hi.max(*s)))
        })
        .collect()
}

/// The latest `capacity` values, oldest first when read; pushing never allocates.
#[derive(Debug, Clone)]
pub struct Ring {
    data: Vec<f32>,
    head: usize,
    full: bool,
}

impl Ring {
    pub fn new(capacity: usize) -> Self {
        Ring { data: vec![0.0; capacity.max(1)], head: 0, full: false }
    }

    pub fn push(&mut self, v: f32) {
        self.data[self.head] = v;
        self.head = (self.head + 1) % self.data.len();
        self.full |= self.head == 0;
    }

    pub fn values(&self) -> Vec<f32> {
        if self.full {
            self.data[self.head..].iter().chain(&self.data[..self.head]).copied().collect()
        } else {
            self.data[..self.head].to_vec()
        }
    }

    /// The peak of what it holds (a level meter's hold).
    pub fn peak(&self) -> f32 {
        self.values().into_iter().fold(0.0, |m, v| m.max(v.abs()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_million_samples_to_columns_keeping_the_peaks() {
        let mut s = vec![0.0f32; 1_000_000];
        s[123_456] = 0.99;
        s[876_543] = -1.0;
        let cols = min_max(&s, 300);
        assert_eq!(cols.len(), 300);
        assert!(cols.iter().any(|c| c.1 == 0.99), "the peak survives");
        assert!(cols.iter().any(|c| c.0 == -1.0), "the clip shows");
        assert!(min_max(&s, 0).is_empty());
    }

    #[test]
    fn a_ring_keeps_the_latest() {
        let mut r = Ring::new(3);
        for v in [0.1, 0.5, 0.2, 0.3] {
            r.push(v);
        }
        assert_eq!(r.values(), vec![0.5, 0.2, 0.3]);
        assert_eq!(r.peak(), 0.5);
    }
}
