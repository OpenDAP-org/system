//! What moves with a transition's progress (`transition`): numbers, rectangles (a shared element's
//! frame from its tile to its place on the next screen), corner radii, colours (mixed in linear
//! light, so a mix is never darker than both ends) and the opacities of a container transform's
//! two contents (Material's fade through: the old fades out quickly, then the new fades in).

/// `a` to `b` at `t` (0–1).
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// A part of the progress mapped to 0–1: before `start` 0, after `end` 1 (a property that moves in
/// only a part of the transition).
pub fn segment(t: f32, start: f32, end: f32) -> f32 {
    ((t - start) / (end - start).max(1e-6)).clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub radius: f32,
}

impl Frame {
    pub fn new(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Self {
        Frame { x, y, w, h, radius }
    }
}

/// A shared element's frame between its two places (SwiftUI's matched geometry, Android's shared
/// element transition): position and size move together, the corners with them.
pub fn frame(a: Frame, b: Frame, t: f32) -> Frame {
    Frame { x: lerp(a.x, b.x, t), y: lerp(a.y, b.y, t), w: lerp(a.w, b.w, t), h: lerp(a.h, b.h, t), radius: lerp(a.radius, b.radius, t) }
}

fn to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(l: f32) -> u8 {
    let c = if l <= 0.0031308 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Two sRGB colours (0xRRGGBB) mixed in linear light.
pub fn color(a: u32, b: u32, t: f32) -> u32 {
    let ch = |c: u32, s: u32| ((c >> s) & 0xFF) as u8;
    [16, 8, 0].iter().fold(0u32, |acc, s| {
        let v = to_srgb(lerp(to_linear(ch(a, *s)), to_linear(ch(b, *s)), t));
        acc | ((v as u32) << s)
    })
}

/// A container transform's contents (Material's fade through): the outgoing content's opacity
/// and the incoming one's at `t`. The old is gone by 0.3, the new appears from 0.3 to 0.8, so the
/// two never show half each.
pub fn fade_through(t: f32) -> (f32, f32) {
    (1.0 - segment(t, 0.0, 0.3), segment(t, 0.3, 0.8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shared_element_moves_and_grows_with_its_corners() {
        let tile = Frame::new(16.0, 300.0, 72.0, 72.0, 4.0);
        let cover = Frame::new(16.0, 76.0, 288.0, 288.0, 12.0);
        assert_eq!(frame(tile, cover, 0.0), tile);
        assert_eq!(frame(tile, cover, 1.0), cover);
        let mid = frame(tile, cover, 0.5);
        assert_eq!((mid.y, mid.w, mid.radius), (188.0, 180.0, 8.0));
    }

    #[test]
    fn colours_mix_in_linear_light() {
        // Black and white at half: linear 0.5 is sRGB 188, not the muddy 128 of a naive mix.
        assert_eq!(color(0x000000, 0xFFFFFF, 0.5), 0xBCBCBC);
        assert_eq!(color(0xFF6A2A, 0xFF6A2A, 0.7), 0xFF6A2A);
    }

    #[test]
    fn fade_through_never_shows_both_at_half() {
        assert_eq!(fade_through(0.0), (1.0, 0.0));
        assert_eq!(fade_through(0.3), (0.0, 0.0), "the gap between them");
        assert_eq!(fade_through(1.0), (0.0, 1.0));
        assert_eq!(segment(0.55, 0.3, 0.8), 0.5);
    }
}
