//! The splash screen of Blackbird OS (design system, "Splash screen"; shell and UI runtime spec §9):
//! the OpenDAP vertical lockup, light, centred on the background, shown while the OS loads. The
//! boot UI (`odap-boot`) and the shell draw it from here, so the screen shows one image
//! from the first frame to the interface.

/// The official light vertical lockup (Blackbird UI brand/svg), never redrawn.
const LOCKUP_SVG: &[u8] = include_bytes!(env!("OPENDAP_LOCKUP_SVG"));
/// The lockup's width as a share of the display's short side.
pub const LOCKUP_SHARE: f32 = 0.40;
/// The background, `semantic.bg` (graphite 950), as 8-bit sRGB.
pub const BACKGROUND: [u8; 3] = [0x11, 0x13, 0x15];
/// The least time the splash screen stays on screen, so it is seen as intended, not as a flash.
pub const MINIMUM_MS: u64 = 5000;
/// The fade from the splash screen to the interface (`motion.duration.slow`).
pub const FADE_MS: u64 = 320;
/// Where `odap-boot` records, in CLOCK_MONOTONIC milliseconds, when the splash screen first showed.
pub const SHOWN_AT_PATH: &str = "/run/opendap/splash-shown";

/// The lockup rasterized as premultiplied RGBA, with its top-left corner on the display.
pub struct Lockup {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub rgba: Vec<u8>,
}

/// The lockup at its size for a display of `width` × `height` pixels, centred.
pub fn lockup(width: u32, height: u32) -> Option<Lockup> {
    let tree = resvg::usvg::Tree::from_data(LOCKUP_SVG, &resvg::usvg::Options::default()).ok()?;
    let source = tree.size();
    let w = (width.min(height) as f32 * LOCKUP_SHARE).round().max(1.0);
    let scale = w / source.width();
    let h = (source.height() * scale).round().max(1.0);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w as u32, h as u32)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(Lockup {
        width: w as u32,
        height: h as u32,
        x: (width as i32 - w as i32) / 2,
        y: (height as i32 - h as i32) / 2,
        rgba: pixmap.take(),
    })
}

/// The whole screen as XRGB8888 (little-endian B, G, R, X bytes): the lockup over the background.
pub fn screen_xrgb(width: u32, height: u32, stride: usize) -> Vec<u8> {
    let mut out = vec![0u8; stride * height as usize];
    for row in out.chunks_exact_mut(stride) {
        for px in row[..width as usize * 4].as_chunks_mut::<4>().0 {
            px.copy_from_slice(&[BACKGROUND[2], BACKGROUND[1], BACKGROUND[0], 0xff]);
        }
    }
    if let Some(l) = lockup(width, height) {
        for y in 0..l.height as usize {
            for x in 0..l.width as usize {
                let s = &l.rgba[(y * l.width as usize + x) * 4..][..4];
                let (dx, dy) = (l.x as usize + x, l.y as usize + y);
                let d = &mut out[dy * stride + dx * 4..][..4];
                // Premultiplied RGBA over the opaque background, into B, G, R order.
                let a = s[3] as u32;
                for (dst, src) in [(2, 0), (1, 1), (0, 2)] {
                    d[dst] = (s[src] as u32 + d[dst] as u32 * (255 - a) / 255).min(255) as u8;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockup_is_centred_and_sized_by_the_short_side() {
        let l = lockup(320, 480).unwrap();
        assert_eq!(l.width, 128);
        assert_eq!(l.x, (320 - 128) / 2);
        assert!(l.height > 0 && l.y > 0);
        assert!(
            l.rgba.iter().skip(3).step_by(4).any(|&a| a == 255),
            "the lockup has opaque pixels"
        );
    }

    #[test]
    fn screen_is_background_outside_the_lockup() {
        let s = screen_xrgb(320, 480, 320 * 4);
        assert_eq!(
            &s[..4],
            &[BACKGROUND[2], BACKGROUND[1], BACKGROUND[0], 0xff]
        );
    }
}
