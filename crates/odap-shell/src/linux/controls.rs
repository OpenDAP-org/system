//! The system combinations, on `main` (T1 controls and status LED spec §1.3). The shell owns them,
//! and logind handles none of `main`'s presses (blackbird-os's logind drop-in). Forced power-off
//! is the PMIC's, in hardware, whatever the OS does.
//!
//! | Press | Result |
//! |---|---|
//! | `main`, short (released before `LONG`) | The display off, or on again; playback goes on |
//! | `main`, long (held `LONG`) | The power menu |
//! | `main` + `volume-down`, short | A screenshot |
//! | `main` + `volume-down`, held `HOLD` | The power menu, also with the display off |
//!
//! With the power menu up, a press of `main` powers off: the confirmation is the same physical
//! gesture (spec §7). The screenshot is not built yet: its request is logged. Other controls pass
//! through as roles.

use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use opendap_device::Role;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use tracing::info;

use super::{layers::Request, state::Shell};

/// Short against long (spec §1.1, Android's long-press timeout; a proposed value).
const LONG: Duration = Duration::from_millis(500);
/// Held, for deliberate system actions (spec §1.1; a proposed value).
const HOLD: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `main` alone.
    Main,
    /// `main` + `volume-down`.
    MainVolumeDown,
}

/// The system press in progress.
struct Press {
    id: u64,
    kind: Kind,
    /// Its held action has run; the release does nothing more.
    held: bool,
}

#[derive(Default)]
pub struct Controls {
    down: HashSet<Role>,
    press: Option<Press>,
    presses: u64,
}

/// A control changed. Returns whether it was part of a system combination.
pub fn control(shell: &mut Shell, role: Role, pressed: bool) -> bool {
    if pressed {
        shell.controls.down.insert(role);
    } else {
        shell.controls.down.remove(&role);
    }
    let down = &shell.controls.down;
    let combo = down.contains(&Role::Main) && down.contains(&Role::VolumeDown);
    match (pressed, role) {
        // `main` pressed, or `volume-down` joining it: a press starts, or becomes the combination.
        (true, Role::Main) | (true, Role::VolumeDown) if down.contains(&Role::Main) => {
            let kind = if combo {
                Kind::MainVolumeDown
            } else {
                Kind::Main
            };
            if shell
                .controls
                .press
                .as_ref()
                .is_some_and(|p| p.kind == kind)
            {
                return true;
            }
            start(shell, kind);
            true
        }
        // With the power menu up, Volume + and − move the focus and Action takes it (SYS-039).
        (true, Role::VolumeUp | Role::VolumeDown | Role::Action) if menu_open(shell) => {
            if let Some(layers) = shell.layers.as_mut() {
                match role {
                    Role::VolumeUp => layers.move_focus(-1),
                    Role::VolumeDown => layers.move_focus(1),
                    _ => layers.activate_focus(),
                }
            }
            layer_requests(shell);
            if let Some(backend) = shell.backend.as_mut() {
                backend.schedule();
            }
            super::backend::render_if_idle(shell);
            true
        }
        (false, Role::VolumeUp | Role::Action) if menu_open(shell) => true,
        // A key of the press released: it ends.
        (false, Role::Main) | (false, Role::VolumeDown) if shell.controls.press.is_some() => {
            let press = shell.controls.press.take().expect("a press");
            if !press.held {
                short(shell, press.kind);
            }
            true
        }
        _ => false,
    }
}

fn start(shell: &mut Shell, kind: Kind) {
    shell.controls.presses += 1;
    let id = shell.controls.presses;
    shell.controls.press = Some(Press {
        id,
        kind,
        held: false,
    });
    let after = match kind {
        Kind::Main => LONG,
        Kind::MainVolumeDown => HOLD,
    };
    let _ = shell.handle.insert_source(
        Timer::from_deadline(Instant::now() + after),
        move |_, _, shell| {
            if let Some(press) = shell.controls.press.as_mut().filter(|p| p.id == id) {
                press.held = true;
                info!(combination = name(press.kind), "power menu");
                open_power_menu(shell);
            }
            TimeoutAction::Drop
        },
    );
}

fn short(shell: &mut Shell, kind: Kind) {
    let menu = shell.layers.as_ref().is_some_and(|l| l.power_open());
    match kind {
        // The second press of `main` confirms: power off.
        Kind::Main if menu => {
            info!("power off, confirmed with main");
            super::power::poweroff();
        }
        Kind::Main => {
            let on = shell.backend.as_ref().is_some_and(|b| b.display_on());
            info!(display = if on { "off" } else { "on" }, "main pressed");
            if let Some(backend) = shell.backend.as_mut() {
                backend.set_display(!on);
            }
            super::backend::render_if_idle(shell);
        }
        // The screenshot is a system layer of the shell, not built yet.
        Kind::MainVolumeDown => info!(combination = name(kind), "screenshot"),
    }
}

fn name(kind: Kind) -> &'static str {
    match kind {
        Kind::Main => "main",
        Kind::MainVolumeDown => "main + volume-down",
    }
}

/// Opens the power menu beside `main`, with the display on; recovery only if the device has it.
fn open_power_menu(shell: &mut Shell) {
    let can_recovery = super::power::modes().iter().any(|m| m == "recovery");
    if let Some(backend) = shell.backend.as_mut() {
        backend.set_display(true);
        backend.schedule();
    }
    if let Some(layers) = shell.layers.as_mut() {
        layers.open_power(can_recovery);
    }
    super::backend::render_if_idle(shell);
}

/// What the system layers asked for (layers.rs), after their input.
pub fn layer_requests(shell: &mut Shell) {
    let Some(layers) = shell.layers.as_mut() else {
        return;
    };
    for request in layers.take_requests() {
        match request {
            Request::Restart => {
                info!("restart, from the power menu");
                super::power::reboot("normal");
            }
            Request::RestartToRecovery => {
                info!("restart to recovery, from the power menu");
                super::power::reboot("recovery");
            }
            Request::Cancel => layers.close(),
        }
    }
}

fn menu_open(shell: &Shell) -> bool {
    shell.layers.as_ref().is_some_and(|l| l.power_open())
}
