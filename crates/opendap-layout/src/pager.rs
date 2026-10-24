//! A pager (ui-model §7): content in pages of one size (places side by side, a horizontal shelf of
//! screens, a sheet's detents), dragged by a hand and settled on a page. A fling moves one page at
//! most and a slow release settles on the nearest (iOS's paging, Android's ViewPager2); past the
//! first or the last page the content resists the finger rather than stopping dead, then springs
//! back (iOS's rubber band).

/// Where a pager released at `offset` (pixels, from page 0's start) with `velocity` (pixels a
/// second, positive toward later pages) settles: a page index. `from` is the page the drag began
/// on; a fling never goes further than its neighbour.
pub fn settle(from: usize, offset: f32, velocity: f32, page: f32, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    // Where a free fling would stop (iOS's normal deceleration, 0.998 per millisecond).
    let rate: f32 = 0.998;
    let projected = offset + velocity / 1000.0 * rate / (1.0 - rate);
    let nearest = (projected / page.max(1.0)).round() as i64;
    let lo = from.saturating_sub(1) as i64;
    let hi = (from + 1).min(count - 1) as i64;
    nearest.clamp(lo, hi).clamp(0, count as i64 - 1) as usize
}

/// How far content moves when the finger pulls it `over` pixels past an end, in a viewport of
/// `dimension`: less and less, never as far as the finger (iOS's rubber band, coefficient 0.55).
pub fn resist(over: f32, dimension: f32) -> f32 {
    let d = dimension.max(1.0);
    let sign = over.signum();
    sign * d * (1.0 - 1.0 / (over.abs() * 0.55 / d + 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 320.0;

    #[test]
    fn a_slow_release_settles_on_the_nearest_page() {
        assert_eq!(settle(0, 100.0, 0.0, W, 3), 0, "under half: back");
        assert_eq!(settle(0, 200.0, 0.0, W, 3), 1, "past half: on");
    }

    #[test]
    fn a_fling_moves_one_page_at_most() {
        assert_eq!(settle(0, 40.0, 900.0, W, 3), 1, "a short flick goes to the next");
        assert_eq!(settle(0, 40.0, 9000.0, W, 3), 1, "a hard one still only one");
        assert_eq!(settle(1, 300.0, -900.0, W, 3), 0, "back by a flick");
        assert_eq!(settle(2, 640.0, 2000.0, W, 3), 2, "the last page: stays");
    }

    #[test]
    fn past_an_end_it_resists() {
        let r = resist(100.0, W);
        assert!(r > 0.0 && r < 100.0, "{r}");
        assert!(resist(400.0, W) < W, "never a whole viewport");
        assert!(resist(-100.0, W) < 0.0);
        assert!(resist(200.0, W) - resist(100.0, W) < resist(100.0, W), "less and less");
    }
}
