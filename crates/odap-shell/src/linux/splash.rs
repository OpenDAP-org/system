//! The splash screen (opendap-splash; shell and UI runtime spec §9): the shell's first frames show
//! the same splash screen the boot UI (`odap-boot`) put on the display while the OS loaded, keep it at least
//! `MINIMUM_MS` from when it first showed, then fade it out once the product host has drawn.
//! Nothing else is drawn at boot (OS-035).

use std::time::{Duration, Instant};

use opendap_splash::{FADE_MS, MINIMUM_MS, SHOWN_AT_PATH};
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
    reexports::rustix::time::{ClockId, clock_gettime},
    utils::{Size, Transform},
};
use tracing::warn;

enum Phase {
    /// On screen until `until`, and longer if the product host has not drawn yet.
    Showing { until: Instant, client_ready: bool },
    Fading(Instant),
    Done,
}

pub struct Splash {
    phase: Phase,
    buffer: MemoryRenderBuffer,
}

fn monotonic_ms() -> u64 {
    let t = clock_gettime(ClockId::Monotonic);
    t.tv_sec as u64 * 1000 + t.tv_nsec as u64 / 1_000_000
}

impl Splash {
    pub fn new(display: Size<i32, smithay::utils::Physical>) -> Self {
        let (w, h) = (display.w.max(1) as u32, display.h.max(1) as u32);
        let pixels = opendap_splash::screen_xrgb(w, h, w as usize * 4);
        // B, G, R, 0xff bytes: ARGB8888 on little-endian hosts, opaque.
        let buffer = MemoryRenderBuffer::from_slice(&pixels, Fourcc::Argb8888, (w as i32, h as i32), 1, Transform::Normal, None);
        // The minimum counts from when odap-boot first showed it, or from now without it.
        let now = monotonic_ms();
        let shown = std::fs::read_to_string(SHOWN_AT_PATH)
            .ok()
            .and_then(|t| t.trim().parse::<u64>().ok())
            .filter(|&t| t <= now)
            .unwrap_or(now);
        let left = Duration::from_millis((shown + MINIMUM_MS).saturating_sub(now));
        Self { phase: Phase::Showing { until: Instant::now() + left, client_ready: false }, buffer }
    }

    /// The product host drew its first frame.
    pub fn client_ready(&mut self) {
        if let Phase::Showing { client_ready, .. } = &mut self.phase {
            *client_ready = true;
        }
    }

    /// Whether frames must keep coming: only while fading. A static splash screen draws nothing.
    pub fn animating(&self) -> bool {
        matches!(self.phase, Phase::Fading(_))
    }

    /// When the shell must look again: the end of the minimum, once the product host is ready.
    pub fn wake_at(&self) -> Option<Instant> {
        match self.phase {
            Phase::Showing { until, client_ready: true } if Instant::now() < until => Some(until),
            _ => None,
        }
    }

    /// The splash screen's element above the client, if it is still on screen.
    pub fn element(&mut self, renderer: &mut GlesRenderer) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        if let Phase::Showing { until, client_ready: true } = self.phase
            && Instant::now() >= until
        {
            self.phase = Phase::Fading(Instant::now());
        }
        let alpha = match self.phase {
            Phase::Showing { .. } => 1.0,
            Phase::Fading(start) => {
                let t = start.elapsed().as_secs_f32() * 1000.0 / FADE_MS as f32;
                if t >= 1.0 {
                    self.phase = Phase::Done;
                    return None;
                }
                1.0 - t
            }
            Phase::Done => return None,
        };
        MemoryRenderBufferRenderElement::from_buffer(renderer, (0.0, 0.0), &self.buffer, Some(alpha), None, None, Kind::Unspecified)
            .map_err(|err| warn!(?err, "splash screen"))
            .ok()
    }
}
