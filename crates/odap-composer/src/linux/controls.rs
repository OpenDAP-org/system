//! The system combinations, on `main` (T1 controls and status LED spec §1.3): OS Shell policy,
//! drafted in the composer until the OS Shell exists (RFC 0012). The composer owns them,
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
//! gesture (spec §7). Other controls pass through as roles.

use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use opendap_device::Role;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use tracing::info;

use super::{layers::Request, state::Composer};

/// Short against long (spec §1.1, Android's long-press timeout; a proposed value).
const LONG: Duration = Duration::from_millis(opendap_input::thresholds::LONG_MS);
/// Held, for deliberate system actions (spec §1.1; a proposed value).
const HOLD: Duration = Duration::from_millis(opendap_input::thresholds::HOLD_MS);

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
pub fn control(composer: &mut Composer, role: Role, pressed: bool) -> bool {
    // The debug UI mode lights the control's mark while it is down, whatever it does.
    if let Some(layers) = composer.layers.as_mut() {
        layers.refresh_debug();
        if super::layers::debug_ui() {
            layers.control(&super::backend::kebab(&format!("{role:?}")), pressed);
            if let Some(backend) = composer.backend.as_mut() {
                backend.schedule();
            }
            super::backend::render_if_idle(composer);
        }
    }
    if pressed {
        composer.controls.down.insert(role);
    } else {
        composer.controls.down.remove(&role);
    }
    let down = &composer.controls.down;
    let combo = down.contains(&Role::Main) && down.contains(&Role::VolumeDown);
    match (pressed, role) {
        // `main` pressed, or `volume-down` joining it: a press starts, or becomes the combination.
        (true, Role::Main) | (true, Role::VolumeDown) if down.contains(&Role::Main) => {
            let kind = if combo {
                Kind::MainVolumeDown
            } else {
                Kind::Main
            };
            if composer
                .controls
                .press
                .as_ref()
                .is_some_and(|p| p.kind == kind)
            {
                return true;
            }
            start(composer, kind);
            true
        }
        // With the power menu up, Volume + and − move the focus and Action takes it (SYS-039).
        (true, Role::VolumeUp | Role::VolumeDown | Role::Action) if menu_open(composer) => {
            if let Some(layers) = composer.layers.as_mut() {
                match role {
                    Role::VolumeUp => layers.move_focus(-1),
                    Role::VolumeDown => layers.move_focus(1),
                    _ => layers.activate_focus(),
                }
            }
            layer_requests(composer);
            if let Some(backend) = composer.backend.as_mut() {
                backend.schedule();
            }
            super::backend::render_if_idle(composer);
            true
        }
        (false, Role::VolumeUp | Role::Action) if menu_open(composer) => true,
        // A key of the press released: it ends.
        (false, Role::Main) | (false, Role::VolumeDown) if composer.controls.press.is_some() => {
            let press = composer.controls.press.take().expect("a press");
            if !press.held {
                short(composer, press.kind);
            }
            true
        }
        _ => false,
    }
}

fn start(composer: &mut Composer, kind: Kind) {
    composer.controls.presses += 1;
    let id = composer.controls.presses;
    composer.controls.press = Some(Press {
        id,
        kind,
        held: false,
    });
    let after = match kind {
        Kind::Main => LONG,
        Kind::MainVolumeDown => HOLD,
    };
    let _ = composer.handle.insert_source(
        Timer::from_deadline(Instant::now() + after),
        move |_, _, composer| {
            if let Some(press) = composer.controls.press.as_mut().filter(|p| p.id == id) {
                press.held = true;
                info!(combination = name(press.kind), "power menu");
                open_power_menu(composer);
            }
            TimeoutAction::Drop
        },
    );
}

fn short(composer: &mut Composer, kind: Kind) {
    let menu = composer.layers.as_ref().is_some_and(|l| l.power_open());
    match kind {
        // The second press of `main` confirms: power off.
        Kind::Main if menu => {
            info!("power off, confirmed with main");
            super::power::poweroff();
        }
        Kind::Main => {
            let on = composer.backend.as_ref().is_some_and(|b| b.display_on());
            info!(display = if on { "off" } else { "on" }, "main pressed");
            if on {
                super::input::system_takeover(composer);
            }
            set_display(composer, !on);
        }
        Kind::MainVolumeDown => {
            if let Some(backend) = composer.backend.as_mut() {
                backend.request_screenshot();
            }
            super::backend::render_if_idle(composer);
        }
    }
}

fn name(kind: Kind) -> &'static str {
    match kind {
        Kind::Main => "main",
        Kind::MainVolumeDown => "main + volume-down",
    }
}

/// Opens the power menu beside `main`, with the display on; recovery only if the device has it.
fn open_power_menu(composer: &mut Composer) {
    // A system view takes over: the activity's keys and touches end here.
    super::input::system_takeover(composer);
    let can_recovery = super::power::modes().iter().any(|m| m == "recovery");
    if let Some(backend) = composer.backend.as_mut() {
        backend.set_display(true);
        backend.schedule();
    }
    // Opened with the display off: the activity gets its keyboard back for when the menu closes.
    super::input::activity_keyboard(composer, true);
    if let Some(layers) = composer.layers.as_mut() {
        layers.open_power(can_recovery);
    }
    super::backend::render_if_idle(composer);
}

/// What the system layers asked for (layers.rs), after their input.
pub fn layer_requests(composer: &mut Composer) {
    let Some(layers) = composer.layers.as_mut() else {
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

fn menu_open(composer: &Composer) -> bool {
    composer.layers.as_ref().is_some_and(|l| l.power_open())
}

/// The display off or on again, and the activity's keyboard with it (composer spec §2).
pub fn set_display(composer: &mut Composer, on: bool) {
    if let Some(backend) = composer.backend.as_mut() {
        backend.set_display(on);
    }
    super::input::activity_keyboard(composer, on);
    super::backend::render_if_idle(composer);
}
