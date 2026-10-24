//! The touch indicator, a developer option in the manner of Android's "show taps": a bubble where
//! the pointer is and where each finger touches. The pointer's bubble is a ring that follows it;
//! a contact shrinks it and fills it (`motion.duration.fast`), and a release ripples it out and
//! fades it (`motion.duration.standard`), so a recording or a person watching the screen can follow
//! every touch. Fingers show the same bubble while they touch, and it goes with the finger. The
//! pointer's fades out once the pointer rests, which also covers it leaving the screen.
//!
//! On when the board's support package sets `ODAP_SHOW_TOUCHES=1` for the composer (the virtual
//! board does), or while `/run/opendap/show-touches` exists, so it can be turned on and off on a
//! running device for debugging. Off, it draws nothing; idle, it asks for no frames.

use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            element::{
                Kind,
                memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            },
            gles::GlesRenderer,
        },
    },
    utils::{Logical, Point, Transform},
};
use tracing::warn;

/// The runtime switch; the environment's default is `ODAP_SHOW_TOUCHES`.
const SWITCH_PATH: &str = "/run/opendap/show-touches";

/// Diameters, in logical pixels: at rest, in contact, at the end of the ripple.
const REST: f64 = 32.0;
const CONTACT: f64 = 26.0;
const RIPPLE: f64 = 48.0;
/// Design system motion tokens (§11).
const FAST: Duration = Duration::from_millis(120);
const STANDARD: Duration = Duration::from_millis(200);
/// How long the pointer's bubble stays once the pointer stops moving.
const HIDE_AFTER: Duration = Duration::from_millis(1500);
/// Opacity of the ring at rest and of the fill in contact.
const RING_ALPHA: f32 = 0.85;
const FILL_ALPHA: f32 = 0.35;

/// The ring's white band and its dark outer edge (for light content), in logical pixels.
const RING_WIDTH: f64 = 2.0;
const EDGE_WIDTH: f64 = 1.0;

/// The two shapes, drawn at each diameter they are shown at (in physical pixels) and kept.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Shape {
    Ring,
    Disc,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Pointer,
    Finger(smithay::backend::input::TouchSlot),
}

struct Spot {
    at: Point<f64, Logical>,
    down: Option<Instant>,
    up: Option<Instant>,
    /// The last event from this source.
    active: Instant,
}

impl Spot {
    fn new(at: Point<f64, Logical>, down: Option<Instant>) -> Self {
        Self {
            at,
            down,
            up: None,
            active: Instant::now(),
        }
    }

    fn in_contact(&self) -> bool {
        self.down.is_some() && self.up.is_none()
    }

    /// The pointer's resting ring: shown while it moves, faded out once it rests. The virtual
    /// board's window reports no pointer leaving it, so this also takes the bubble away when the
    /// pointer leaves the screen.
    fn rest_opacity(&self, now: Instant) -> f32 {
        if self.in_contact() {
            return 1.0;
        }
        let idle = now - self.active;
        if idle < HIDE_AFTER {
            1.0
        } else {
            1.0 - ease(progress(idle - HIDE_AFTER, STANDARD)) as f32
        }
    }
}

pub struct Touches {
    default_on: bool,
    textures: HashMap<(Shape, i32), MemoryRenderBuffer>,
    spots: HashMap<Source, Spot>,
}

impl Touches {
    pub fn new() -> Self {
        Self {
            default_on: std::env::var("ODAP_SHOW_TOUCHES").is_ok_and(|v| v == "1"),
            textures: HashMap::new(),
            spots: HashMap::new(),
        }
    }

    /// Checked on input, never on a timer, so an idle screen stays idle.
    pub fn enabled(&self) -> bool {
        self.default_on || Path::new(SWITCH_PATH).exists()
    }

    pub fn moved(&mut self, source: Source, at: Point<f64, Logical>) {
        let spot = self
            .spots
            .entry(source)
            .or_insert_with(|| Spot::new(at, None));
        spot.at = at;
        spot.active = Instant::now();
    }

    pub fn down(&mut self, source: Source, at: Point<f64, Logical>) {
        self.spots
            .insert(source, Spot::new(at, Some(Instant::now())));
    }

    pub fn up(&mut self, source: Source) {
        if let Some(spot) = self.spots.get_mut(&source) {
            spot.up = Some(Instant::now());
            spot.active = Instant::now();
        }
    }

    /// Whether frames must keep coming: while a press, a ripple or the pointer's fade is in motion.
    pub fn animating(&self) -> bool {
        let now = Instant::now();
        self.spots.iter().any(|(source, s)| {
            let fading = *source == Source::Pointer
                && !s.in_contact()
                && (HIDE_AFTER..HIDE_AFTER + STANDARD).contains(&(now - s.active));
            fading
                || match (s.down, s.up) {
                    (_, Some(up)) => now - up < STANDARD,
                    (Some(down), None) => now - down < FAST,
                    _ => false,
                }
        })
    }

    /// When the composer must draw again with no input: when the resting pointer starts to fade.
    pub fn wake_at(&self) -> Option<Instant> {
        let now = Instant::now();
        self.spots
            .get(&Source::Pointer)
            .filter(|s| !s.in_contact() && now - s.active < HIDE_AFTER)
            .map(|s| s.active + HIDE_AFTER)
    }

    /// The bubbles to draw above everything else, at `scale` physical pixels per logical pixel.
    pub fn elements(
        &mut self,
        renderer: &mut GlesRenderer,
        scale: f64,
    ) -> Vec<MemoryRenderBufferRenderElement<GlesRenderer>> {
        if !self.enabled() {
            self.spots.clear();
            return Vec::new();
        }
        let now = Instant::now();
        // A finger's bubble is gone once its ripple ends, like the finger; the pointer's once it
        // has rested and faded out. Neither stays on screen.
        self.spots.retain(|source, s| {
            let rippling = s.up.is_some_and(|up| now - up < STANDARD);
            match source {
                Source::Pointer => rippling || s.rest_opacity(now) > 0.0,
                Source::Finger(_) => s.up.is_none() || rippling,
            }
        });
        // What to draw, in logical pixels: shape, centre, diameter, opacity.
        let mut marks: Vec<(Shape, Point<f64, Logical>, f64, f32)> = Vec::new();
        for (source, s) in &self.spots {
            let mut mark = |shape, diameter, alpha: f32| {
                if alpha > 0.0 {
                    marks.push((shape, s.at, diameter, alpha));
                }
            };
            match (s.down, s.up) {
                // In contact: shrink and fill.
                (Some(down), None) => {
                    let t = ease(progress(now - down, FAST));
                    let d = lerp(REST, CONTACT, t);
                    mark(Shape::Ring, d, RING_ALPHA);
                    mark(Shape::Disc, d, FILL_ALPHA * t as f32);
                }
                // Released: ripple out and fade; the pointer's ring is back at rest beneath it.
                (_, Some(up)) => {
                    let t = ease(progress(now - up, STANDARD));
                    if *source == Source::Pointer {
                        mark(Shape::Ring, REST, RING_ALPHA * s.rest_opacity(now));
                    }
                    if t < 1.0 {
                        let fade = 1.0 - t as f32;
                        let d = lerp(CONTACT, RIPPLE, t);
                        mark(Shape::Ring, d, RING_ALPHA * fade);
                        mark(Shape::Disc, d, FILL_ALPHA * fade);
                    }
                }
                // The pointer, hovering.
                (None, None) => mark(Shape::Ring, REST, RING_ALPHA * s.rest_opacity(now)),
            }
        }
        let mut out = Vec::with_capacity(marks.len());
        for (shape, at, diameter, alpha) in marks {
            // Drawn at its size on screen, never scaled, so the edges stay sharp.
            let px = (diameter * scale).round().max(2.0) as i32;
            let texture = self
                .textures
                .entry((shape, px))
                .or_insert_with(|| texture(shape, px, scale));
            let loc = Point::<f64, Logical>::from((at.x, at.y)).to_physical(scale)
                - Point::from((px as f64 / 2.0, px as f64 / 2.0));
            match MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                loc,
                texture,
                Some(alpha),
                None,
                None,
                Kind::Unspecified,
            ) {
                Ok(e) => out.push(e),
                Err(err) => warn!(?err, "touch indicator"),
            }
        }
        out
    }
}

fn progress(elapsed: Duration, total: Duration) -> f64 {
    (elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// `motion.easing.standard`, cubic-bezier(0.2, 0, 0, 1): solve x(u) = t, return y(u).
fn ease(t: f64) -> f64 {
    let (x1, y1, x2, y2) = (0.2, 0.0, 0.0, 1.0);
    let bez = |u: f64, p1: f64, p2: f64| {
        3.0 * (1.0 - u).powi(2) * u * p1 + 3.0 * (1.0 - u) * u * u * p2 + u.powi(3)
    };
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if bez(mid, x1, x2) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez((lo + hi) / 2.0, y1, y2)
}

/// The shape's premultiplied (B, G, R, A) at distance `r` from the centre, for a texture `px`
/// physical pixels across at `scale`.
fn shade(shape: Shape, r: f64, px: i32, scale: f64) -> [f64; 4] {
    let outer = px as f64 / 2.0;
    let edge = EDGE_WIDTH * scale;
    match shape {
        // A white band with a dark outer edge, so it shows on light content too.
        Shape::Ring => {
            let white = band(r, outer - edge - RING_WIDTH * scale, outer - edge);
            let dark = band(r, outer - edge, outer) * 0.5 * (1.0 - white);
            // Graphite 950 (#111315) for the edge.
            let c = |d: u8| white + dark * d as f64 / 255.0;
            [c(0x15), c(0x13), c(0x11), white + dark]
        }
        // A white disc inside the ring's edge.
        Shape::Disc => {
            let a = band(r, -1.0, outer - edge);
            [a, a, a, a]
        }
    }
}

/// Anti-aliased coverage of the annulus between `inner` and `outer` (one pixel of smoothing).
fn band(r: f64, inner: f64, outer: f64) -> f64 {
    ((r - inner + 0.5).clamp(0.0, 1.0)).min((outer - r + 0.5).clamp(0.0, 1.0))
}

fn texture(shape: Shape, px: i32, scale: f64) -> MemoryRenderBuffer {
    let n = px as usize;
    let c = px as f64 / 2.0;
    let mut pixels = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let r = ((x as f64 + 0.5 - c).powi(2) + (y as f64 + 0.5 - c).powi(2)).sqrt();
            let v = shade(shape, r, px, scale);
            let i = (y * n + x) * 4;
            for k in 0..4 {
                pixels[i + k] = (v[k].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (px, px),
        1,
        Transform::Normal,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_easing_runs_from_0_to_1_and_front_loads() {
        assert!(ease(0.0).abs() < 1e-6);
        assert!((ease(1.0) - 1.0).abs() < 1e-6);
        assert!(ease(0.5) > 0.5);
    }

    #[test]
    fn the_ring_is_hollow_and_the_disc_is_full() {
        let (px, scale) = (32, 1.0);
        assert_eq!(shade(Shape::Ring, 0.0, px, scale)[3], 0.0);
        assert!(shade(Shape::Ring, 16.0 - EDGE_WIDTH - RING_WIDTH / 2.0, px, scale)[3] > 0.99);
        assert!(shade(Shape::Disc, 0.0, px, scale)[3] > 0.99);
        assert_eq!(shade(Shape::Disc, 17.0, px, scale)[3], 0.0);
    }
}
