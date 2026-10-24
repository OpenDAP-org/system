//! The pager's invariants over generated releases (ADR 0014, level L1). A failure is shrunk to the
//! smallest case that breaks one.

use opendap_layout::pager::{resist, settle};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig { cases: 10000, ..ProptestConfig::default() })]

    /// Wherever and however fast it is let go: a page that exists, at most one from where the drag
    /// began; a faster fling never settles further back; with no speed, the nearest page.
    #[test]
    fn a_release_settles_on_a_neighbour(count in 1usize..12, page in 100.0f32..800.0, from_seed in 0usize..12, drift in -1.5f32..1.5, v in -5000.0f32..5000.0, dv in 0.0f32..2000.0) {
        let from = from_seed % count;
        let offset = (from as f32 + drift) * page;
        let to = settle(from, offset, v, page, count);
        prop_assert!(to < count, "a page that exists");
        prop_assert!(to.abs_diff(from) <= 1, "one page at most: {from} → {to}");
        prop_assert!(settle(from, offset, v + dv, page, count) >= to, "faster forward never goes back");
        let still = settle(from, offset, 0.0, page, count);
        let nearest = ((offset / page).round().max(0.0) as usize).min(count - 1);
        if nearest.abs_diff(from) <= 1 && ((offset / page).fract().abs() - 0.5).abs() > 1e-3 {
            prop_assert_eq!(still, nearest);
        }
    }

    /// The rubber band: content follows the finger past an end less and less, in its direction,
    /// never as far as the finger and never past one viewport.
    #[test]
    fn the_rubber_band_resists(over in -5000.0f32..5000.0, more in 0.1f32..500.0, d in 50.0f32..1000.0) {
        let r = resist(over, d);
        prop_assert!(r.abs() <= over.abs() + 1e-3, "never further than the finger");
        prop_assert!(r.abs() < d, "never past one viewport");
        prop_assert!(r * over >= 0.0, "in the finger's direction");
        prop_assert!((resist(-over, d) + r).abs() < 1e-3, "the same both ways");
        let s = over.signum();
        if s != 0.0 {
            prop_assert!(resist(over + s * more, d).abs() >= r.abs(), "further pulls move it further");
        }
    }
}
