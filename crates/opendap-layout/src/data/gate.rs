//! What loads while the content moves fast: the cheap (text, a page of rows) at once, the
//! costly (decoding a cover, drawing a waveform) only when the scroll slows or stops, so a fling
//! stays at 60 fps (T1-056) and nothing is decoded for rows that only fly past (iOS's and
//! Android's deferred image loading).

/// How costly a load is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cost {
    Cheap,
    Costly,
}

/// The gate's directives: above `fast` px/s only the cheap loads; at or below, everything.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gate {
    pub fast: f32,
}

impl Default for Gate {
    fn default() -> Self {
        // Proposed, to measure on the T1: about three screens a second.
        Gate { fast: 1500.0 }
    }
}

impl Gate {
    pub fn admits(&self, cost: Cost, velocity: f32) -> bool {
        cost == Cost::Cheap || velocity.abs() <= self.fast
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_wait_for_the_scroll_to_slow() {
        let g = Gate::default();
        assert!(g.admits(Cost::Cheap, 6000.0));
        assert!(!g.admits(Cost::Costly, 6000.0));
        assert!(g.admits(Cost::Costly, 200.0));
        assert!(g.admits(Cost::Costly, -1200.0));
    }
}
