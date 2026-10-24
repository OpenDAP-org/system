//! Sticky headers (CSS `position: sticky`, Compose's `stickyHeader`): a section's header stays
//! at the viewport's top while its section scrolls under it, and the next section's header
//! pushes it out. A position, computed; the header is drawn in the raised band (ui-model §9).

/// Where the header of the section that starts at `section_start` (content offset) and ends at
/// `section_end` sits in the viewport at `offset`, for a header `height` long: its natural place
/// when below the top, pinned at 0 while its section passes, and pushed up as the section ends.
pub fn header_y(section_start: f32, section_end: f32, height: f32, offset: f32) -> f32 {
    let natural = section_start - offset;
    if natural >= 0.0 {
        return natural;
    }
    // Pinned, until the section's end pushes it out.
    (section_end - offset - height).min(0.0)
}

/// The section showing at the top of the viewport at `offset`, from the sections' starts in
/// order: the one whose header is pinned.
pub fn current(starts: &[f32], offset: f32) -> Option<usize> {
    match starts.partition_point(|s| *s <= offset) {
        0 => None,
        i => Some(i - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_then_pinned_then_pushed() {
        // Section A from 0 to 500, a 28 px header.
        assert_eq!(header_y(100.0, 500.0, 28.0, 40.0), 60.0, "natural");
        assert_eq!(header_y(100.0, 500.0, 28.0, 300.0), 0.0, "pinned");
        assert_eq!(header_y(100.0, 500.0, 28.0, 482.0), -10.0, "pushed by the next");
        assert_eq!(current(&[0.0, 500.0, 900.0], 650.0), Some(1));
        assert_eq!(current(&[100.0], 50.0), None);
    }
}
