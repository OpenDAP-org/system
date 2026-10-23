//! `dap-product`: the product host (ADR 0012). Bring-up: the Now Playing place.
//!
//! What it shows comes through ports, so the same screen runs on the device, on the virtual T1
//! and on a developer host:
//! - the track comes from a fixture until the audio engine and library services exist;
//! - the battery comes from the kernel's power-supply class where there is one, and is absent
//!   (not drawn) where there is none, as on a desk host;
//! - the time is UTC until the time-zone setting exists.

mod power;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use slint::{ComponentHandle, Timer, TimerMode};

slint::include_modules!();

/// The track to show, until `dap-library` and the audio engine provide it over the bus.
fn fixture_track() -> TrackView {
    TrackView {
        title: "You Only Live Once".into(),
        artist: "The Strokes".into(),
        album: "First Impressions of Earth".into(),
        format: "FLAC 16/44.1 · 4.4 mm".into(),
        path: "Direct".into(),
        position: "1:19".into(),
        remaining: "−1:50".into(),
        progress: 0.42,
    }
}

fn clock_text(now: SystemTime) -> (slint::SharedString, Duration) {
    let secs = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let (h, m) = ((secs / 3600) % 24, (secs / 60) % 60);
    let until_next_minute = Duration::from_secs(60 - secs % 60);
    (format!("{h:02}:{m:02}").into(), until_next_minute)
}

fn status() -> (StatusView, Duration) {
    let (time, next) = clock_text(SystemTime::now());
    let battery = power::battery();
    (
        StatusView {
            time,
            volume: 38,
            microsd: false,
            bluetooth: false,
            wifi: false,
            battery_present: battery.is_some(),
            battery: battery.unwrap_or(0) as i32,
        },
        next,
    )
}

/// Refreshes the status bar once a minute, on the minute: one wakeup per minute while the
/// screen is on, and none in between (shell and UI runtime spec §5).
fn tick(ui: slint::Weak<Product>, timer: std::rc::Rc<Timer>) {
    let Some(product) = ui.upgrade() else { return };
    let (view, next) = status();
    product.set_status(view);
    let again = timer.clone();
    timer.start(TimerMode::SingleShot, next, move || tick(ui.clone(), again.clone()));
}

fn main() -> Result<(), slint::PlatformError> {
    let product = Product::new()?;
    product.set_track(fixture_track());
    product.set_status(status().0);

    let weak = product.as_weak();
    product.on_toggle_play(move || {
        if let Some(p) = weak.upgrade() {
            p.set_playing(!p.get_playing());
        }
    });

    let timer = std::rc::Rc::new(Timer::default());
    tick(product.as_weak(), timer.clone());

    product.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_waits_for_the_next_minute() {
        let t = UNIX_EPOCH + Duration::from_secs(10 * 3600 + 42 * 60 + 15);
        let (text, next) = clock_text(t);
        assert_eq!(text.as_str(), "10:42");
        assert_eq!(next, Duration::from_secs(45));
    }
}
