//! A placeholder chain: something right away, better as it arrives (the dominant colour, stored
//! with the metadata; a thumbnail; the full image at its decode size), and stopped early while
//! the content flies past, so a fling never decodes what nobody sees (ADR 0003: the structure at
//! once).

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// Nothing yet: the box's own surface.
    Empty,
    /// The dominant colour (from the metadata: free).
    Colour,
    /// A small thumbnail (cheap).
    Thumbnail,
    /// The image at its decode size (costly).
    Full,
}

/// The next stage to ask for, from what is shown, for content `moving_fast` or not: the colour
/// always, the thumbnail always, the full image only when it is not flying past.
pub fn next(shown: Stage, has_colour: bool, moving_fast: bool) -> Option<Stage> {
    match shown {
        Stage::Empty if has_colour => Some(Stage::Colour),
        Stage::Empty | Stage::Colour => Some(Stage::Thumbnail),
        Stage::Thumbnail if !moving_fast => Some(Stage::Full),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_then_thumbnail_then_full_unless_flying_past() {
        assert_eq!(next(Stage::Empty, true, true), Some(Stage::Colour));
        assert_eq!(next(Stage::Empty, false, true), Some(Stage::Thumbnail));
        assert_eq!(next(Stage::Thumbnail, true, true), None, "a fling stops at the thumbnail");
        assert_eq!(next(Stage::Thumbnail, true, false), Some(Stage::Full));
        assert_eq!(next(Stage::Full, true, false), None);
    }
}
