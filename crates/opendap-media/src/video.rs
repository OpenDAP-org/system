//! Video frames to display refreshes: at each refresh, the frame whose time has come (the latest
//! not later than the refresh), the ones skipped counted as dropped, and the same frame kept
//! (repeated) until the next is due. A 24 fps video on a 60 Hz panel shows each frame two or
//! three refreshes; a late decoder drops rather than drifts.

/// The frame to show at `refresh_ms` from frames' presentation times (ascending), and how many
/// before it were never shown (dropped); `None` before the first.
pub fn frame_at(pts_ms: &[u64], refresh_ms: u64, last_shown: Option<usize>) -> Option<(usize, usize)> {
    let i = pts_ms.partition_point(|t| *t <= refresh_ms).checked_sub(1)?;
    let dropped = match last_shown {
        Some(l) if i > l => i - l - 1,
        Some(_) => 0,
        None => i,
    };
    Some((i, dropped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_repeat_or_drop_to_stay_on_time() {
        // 24 fps: a frame every 41.67 ms; a 60 Hz refresh every 16.67 ms.
        let pts: Vec<u64> = (0..48).map(|i| (i as f64 * 1000.0 / 24.0) as u64).collect();
        let mut shown = Vec::new();
        let mut last = None;
        for r in 0..60u64 {
            let (i, d) = frame_at(&pts, r * 1000 / 60, last).unwrap();
            assert_eq!(d, 0, "on time: nothing dropped");
            shown.push(i);
            last = Some(i);
        }
        let counts: Vec<usize> = (0..6).map(|f| shown.iter().filter(|i| **i == f).count()).collect();
        assert_eq!(counts, vec![3, 2, 3, 2, 3, 2], "3:2 pulldown");
        // A late refresh skips frames: counted as dropped.
        assert_eq!(frame_at(&pts, 300, Some(2)), Some((7, 4)));
        assert_eq!(frame_at(&pts[1..], 10, None), None, "before the first frame");
    }
}
