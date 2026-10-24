//! Content in a box of another shape (ui-model §6.4, CSS `object-fit`): where it lands, which
//! part of the source shows, and the size to decode it at: the box's physical pixels, never more
//! than the source has, so a 3000 px cover in a 96 px tile decodes at 96 px (memory and time).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Contain,
    Cover,
    Fill,
    None,
    ScaleDown,
}

/// A rectangle in some frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Where content of `source` size lands in `bounds` (centred), and the part of the source shown.
pub fn place(source: (f32, f32), bounds: Rect, fit: Fit) -> (Rect, Rect) {
    let (sw, sh) = (source.0.max(1e-3), source.1.max(1e-3));
    let full = Rect { x: 0.0, y: 0.0, w: sw, h: sh };
    let centred = |w: f32, h: f32| Rect { x: bounds.x + (bounds.w - w) / 2.0, y: bounds.y + (bounds.h - h) / 2.0, w, h };
    match fit {
        Fit::Fill => (bounds, full),
        Fit::Contain => {
            let s = (bounds.w / sw).min(bounds.h / sh);
            (centred(sw * s, sh * s), full)
        }
        Fit::ScaleDown => {
            let s = (bounds.w / sw).min(bounds.h / sh).min(1.0);
            (centred(sw * s, sh * s), full)
        }
        Fit::Cover => {
            let s = (bounds.w / sw).max(bounds.h / sh);
            // The source part that fills the box, centred (the focal point).
            let (cw, ch) = (bounds.w / s, bounds.h / s);
            (bounds, Rect { x: (sw - cw) / 2.0, y: (sh - ch) / 2.0, w: cw, h: ch })
        }
        Fit::None => {
            let (w, h) = (sw.min(bounds.w), sh.min(bounds.h));
            (centred(w, h), Rect { x: (sw - w) / 2.0, y: (sh - h) / 2.0, w, h })
        }
    }
}

/// The size to decode `source` at for a box of `bounds` logical pixels at `scale` physical
/// pixels each: what lands on screen, in physical pixels, never more than the source.
pub fn decode_size(source: (u32, u32), bounds: (f32, f32), scale: f32, fit: Fit) -> (u32, u32) {
    let (sw, sh) = (source.0 as f32, source.1 as f32);
    let (dest, part) = place((sw, sh), Rect { x: 0.0, y: 0.0, w: bounds.0 * scale, h: bounds.1 * scale }, fit);
    // The whole source scaled by how the shown part maps to its destination.
    let s = (dest.w / part.w.max(1e-3)).min(dest.h / part.h.max(1e-3)).min(1.0);
    (((sw * s).round() as u32).max(1), ((sh * s).round() as u32).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TILE: Rect = Rect { x: 0.0, y: 0.0, w: 96.0, h: 96.0 };

    #[test]
    fn contain_and_cover() {
        let (d, s) = place((200.0, 100.0), TILE, Fit::Contain);
        assert_eq!((d.w, d.h, d.y), (96.0, 48.0, 24.0));
        assert_eq!(s.w, 200.0);
        let (d, s) = place((200.0, 100.0), TILE, Fit::Cover);
        assert_eq!((d.w, d.h), (96.0, 96.0));
        assert_eq!((s.x, s.w, s.h), (50.0, 100.0, 100.0), "the centre square");
        let (d, _) = place((40.0, 40.0), TILE, Fit::ScaleDown);
        assert_eq!(d.w, 40.0, "never enlarged");
    }

    #[test]
    fn a_large_cover_decodes_at_the_tile_size() {
        assert_eq!(decode_size((3000, 3000), (96.0, 96.0), 1.0, Fit::Cover), (96, 96));
        assert_eq!(decode_size((3000, 3000), (96.0, 96.0), 2.0, Fit::Cover), (192, 192));
        assert_eq!(decode_size((3000, 1500), (96.0, 96.0), 1.0, Fit::Cover), (192, 96), "cover crops the width");
        assert_eq!(decode_size((64, 64), (96.0, 96.0), 1.0, Fit::Cover), (64, 64), "never more than the source");
    }
}
