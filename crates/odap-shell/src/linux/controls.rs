//! The system control, `main` (T1 controls and status LED spec §1.3): the shell owns it, and logind
//! handles none of its presses (blackbird-os's logind drop-in). Forced power-off is the PMIC's,
//! in hardware, whatever the OS does.
//!
//! - Short press (released before `LONG`): the display off, or on again. Playback goes on.
//! - Long press (held `LONG`): the power menu. Its layer is not drawn yet; the request is logged.
//!
//! Combinations with Volume − (screenshot, the power menu with the screen off) come with the
//! shell's system layers.

use std::time::{Duration, Instant};

use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use tracing::info;

use super::state::Shell;

/// Short against long (spec §1.1, Android's long-press timeout; a proposed value).
const LONG: Duration = Duration::from_millis(500);

/// The press of `main` in progress.
pub struct MainPress {
    id: u64,
    long: bool,
}

#[derive(Default)]
pub struct Controls {
    main: Option<MainPress>,
    presses: u64,
}

pub fn main_pressed(shell: &mut Shell) {
    if shell.controls.main.is_some() {
        return;
    }
    shell.controls.presses += 1;
    let id = shell.controls.presses;
    shell.controls.main = Some(MainPress { id, long: false });
    let _ = shell.handle.insert_source(
        Timer::from_deadline(Instant::now() + LONG),
        move |_, _, shell| {
            if let Some(press) = shell.controls.main.as_mut().filter(|p| p.id == id) {
                press.long = true;
                // The power menu is a system layer of the shell, not drawn yet.
                info!("main held: power menu");
            }
            TimeoutAction::Drop
        },
    );
}

pub fn main_released(shell: &mut Shell) {
    let Some(press) = shell.controls.main.take() else {
        return;
    };
    if press.long {
        return;
    }
    let on = shell.backend.as_ref().is_some_and(|b| b.display_on());
    info!(display = if on { "off" } else { "on" }, "main pressed");
    if let Some(backend) = shell.backend.as_mut() {
        backend.set_display(!on);
    }
    super::backend::render_if_idle(shell);
}
