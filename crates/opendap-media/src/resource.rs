//! A resource and its variants (an image in several sizes, an icon at 16 and 24, a track in
//! several qualities): one identity, and the variant that serves a target (HTML's `srcset`,
//! Android's density buckets): the smallest at least as large, else the largest.

/// One rendition of a resource: its pixel size (0 for non-images) and a weight to break ties
/// (bytes, bitrate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub id: String,
    pub width: u32,
    pub height: u32,
}

/// The variant to use for a target of `width` × `height` physical pixels: the smallest that
/// covers it, else the largest there is (upscaled; never asked bigger than it is).
pub fn choose(variants: &[Variant], width: u32, height: u32) -> Option<&Variant> {
    let covers = |v: &&Variant| v.width >= width && v.height >= height;
    variants
        .iter()
        .filter(covers)
        .min_by_key(|v| u64::from(v.width) * u64::from(v.height))
        .or_else(|| variants.iter().max_by_key(|v| u64::from(v.width) * u64::from(v.height)))
}

/// A resource's cache key at a decode size: the same source decoded at two sizes is two
/// entries (a cover in a grid and on Now Playing).
pub fn key(source: &str, width: u32, height: u32) -> String {
    format!("{source}@{width}x{height}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: &str, w: u32) -> Variant {
        Variant { id: id.into(), width: w, height: w }
    }

    #[test]
    fn the_smallest_that_covers_else_the_largest() {
        let set = [v("thumb", 96), v("medium", 300), v("full", 1200)];
        assert_eq!(choose(&set, 88, 88).unwrap().id, "thumb");
        assert_eq!(choose(&set, 240, 240).unwrap().id, "medium");
        assert_eq!(choose(&set, 2000, 2000).unwrap().id, "full");
        assert!(choose(&[], 10, 10).is_none());
        assert_eq!(key("album/42/cover", 96, 96), "album/42/cover@96x96");
    }
}
