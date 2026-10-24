//! A grid over a list (a cover grid): columns by the space, auto-fit (CSS
//! `repeat(auto-fit, minmax(min, 1fr))`, Compose's `GridCells.Adaptive`), rows windowed as a
//! list of rows, so a grid of a hundred thousand covers builds only the rows on screen.

use crate::extents::Fixed;

/// Columns that fit `width` with cells of at least `min` and `gap` between them, and the cell
/// width that fills the row.
pub fn auto_fit(width: f32, min: f32, gap: f32) -> (usize, f32) {
    let cols = (((width + gap) / (min + gap)).floor() as usize).max(1);
    let cell = (width - gap * (cols - 1) as f32) / cols as f32;
    (cols, cell)
}

/// The rows of `count` items in `cols` columns, each `row_size` long plus `gap`: a list to
/// window like any other.
pub fn rows(count: usize, cols: usize, row_size: f32, gap: f32) -> Fixed {
    Fixed { count: count.div_ceil(cols.max(1)), size: row_size + gap }
}

/// The items of row `r`.
pub fn row_items(r: usize, cols: usize, count: usize) -> std::ops::Range<usize> {
    let start = r * cols;
    start.min(count)..(start + cols).min(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::{Overscan, window};

    #[test]
    fn columns_by_the_space() {
        assert_eq!(auto_fit(288.0, 88.0, 8.0).0, 3);
        let (cols, cell) = auto_fit(288.0, 88.0, 8.0);
        assert!((cell * cols as f32 + 8.0 * (cols - 1) as f32 - 288.0).abs() < 1e-3, "fills the row");
        assert_eq!(auto_fit(900.0, 88.0, 8.0).0, 9, "OpenDAP Desktop's wider window");
        assert_eq!(auto_fit(40.0, 88.0, 8.0).0, 1, "never none");
    }

    #[test]
    fn a_hundred_thousand_covers_build_a_few_rows() {
        let (cols, cell) = auto_fit(288.0, 88.0, 8.0);
        let r = rows(100_000, cols, cell, 8.0);
        assert_eq!(r.count, 33_334);
        let w = window(&r, 5000.0 * (cell + 8.0), 452.0, 0.0, Overscan { around: 0.0, lead_s: 0.0, max_ahead: 0.0 });
        assert!(w.len() <= 6);
        assert_eq!(row_items(33_333, cols, 100_000), 99_999..100_000, "the last row holds one");
    }
}
