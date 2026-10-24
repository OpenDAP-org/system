//! A frosted backdrop (blackbird-ui ui-model §9, design system §12.3): what is under a dialog,
//! made small and blurred, so the layer above reads as frosted glass rather than a darkened sheet.
//!
//! One blur when the layer opens, never per frame (design system §17: no stacked blur filters, no
//! cost while it stays): the picture is shrunk first (the blur's radius shrinks with it, so a
//! wide blur costs little), then blurred by three box passes each way, which approach a Gaussian
//! (the central limit; what CSS filters and Android's RenderEffect approximate too). The renderer
//! scales the small result up smoothly, which blurs it further for free.

/// The frosted picture of `rgba` (`w` × `h`, 4 bytes a pixel): `factor` times smaller on each
/// side, blurred with a radius of `radius` pixels of the original. Returns the small picture and
/// its size.
pub fn frosted(rgba: &[u8], w: usize, h: usize, factor: usize, radius: usize) -> (Vec<u8>, usize, usize) {
    let factor = factor.max(1);
    let (sw, sh) = (w.div_ceil(factor), h.div_ceil(factor));
    let mut small = shrink(rgba, w, h, factor, sw, sh);
    let r = (radius / factor).max(1);
    for _ in 0..3 {
        box_pass(&mut small, sw, sh, r, true);
        box_pass(&mut small, sw, sh, r, false);
    }
    (small, sw, sh)
}

/// Each small pixel is the mean of the `factor` × `factor` block it covers.
fn shrink(rgba: &[u8], w: usize, h: usize, factor: usize, sw: usize, sh: usize) -> Vec<u8> {
    let mut out = vec![0u8; sw * sh * 4];
    for sy in 0..sh {
        for sx in 0..sw {
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for y in sy * factor..((sy + 1) * factor).min(h) {
                for x in sx * factor..((sx + 1) * factor).min(w) {
                    let i = (y * w + x) * 4;
                    for c in 0..4 {
                        acc[c] += rgba[i + c] as u32;
                    }
                    n += 1;
                }
            }
            let o = (sy * sw + sx) * 4;
            for c in 0..4 {
                out[o + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    out
}

/// One box blur of radius `r` along rows (`horizontal`) or columns, edges clamped: a running sum,
/// so its cost does not grow with the radius.
fn box_pass(img: &mut [u8], w: usize, h: usize, r: usize, horizontal: bool) {
    let (len, lines) = if horizontal { (w, h) } else { (h, w) };
    let at = |line: usize, k: usize| if horizontal { (line * w + k) * 4 } else { (k * w + line) * 4 };
    let window = (2 * r + 1) as u32;
    let mut line_buf = vec![0u8; len * 4];
    for line in 0..lines {
        for c in 0..4 {
            let px = |k: isize| img[at(line, k.clamp(0, len as isize - 1) as usize) + c] as u32;
            let mut sum: u32 = (-(r as isize)..=r as isize).map(px).sum();
            for k in 0..len {
                line_buf[k * 4 + c] = (sum / window) as u8;
                sum += px(k as isize + r as isize + 1);
                sum -= px(k as isize - r as isize);
            }
        }
        for k in 0..len {
            let i = at(line, k);
            img[i..i + 4].copy_from_slice(&line_buf[k * 4..k * 4 + 4]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 4]) -> Vec<u8> {
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                out.extend_from_slice(&f(x, y));
            }
        }
        out
    }

    #[test]
    fn a_flat_picture_stays_flat_and_shrinks() {
        let p = picture(320, 480, |_, _| [20, 30, 40, 255]);
        let (s, w, h) = frosted(&p, 320, 480, 4, 24);
        assert_eq!((w, h), (80, 120));
        assert!(s.chunks(4).all(|px| px == [20, 30, 40, 255]));
    }

    #[test]
    fn a_hard_edge_becomes_a_soft_symmetric_ramp() {
        // Black left half, white right half.
        let p = picture(64, 8, |x, _| if x < 32 { [0, 0, 0, 255] } else { [255, 255, 255, 255] });
        let (s, w, _) = frosted(&p, 64, 8, 1, 4);
        let row: Vec<u8> = (0..w).map(|x| s[x * 4]).collect();
        assert!(row.windows(2).all(|p| p[0] <= p[1]), "monotonic: {row:?}");
        assert!(row[31] > 30 && row[32] < 225, "softened at the edge: {row:?}");
        assert_eq!(row[0], 0, "far from the edge, unchanged");
        assert_eq!(row[63], 255);
        let mid = row[31] as i32 + row[32] as i32;
        assert!((mid - 255).abs() <= 3, "symmetric about the edge: {} {}", row[31], row[32]);
    }

    #[test]
    fn a_full_screen_frost_is_cheap() {
        let p = picture(320, 480, |x, y| [(x % 256) as u8, (y % 256) as u8, 0, 255]);
        let t = std::time::Instant::now();
        let _ = frosted(&p, 320, 480, 4, 24);
        // Generous for a debug build on any host; the baseline benchmark measures it properly.
        assert!(t.elapsed().as_millis() < 200, "{:?}", t.elapsed());
    }
}
