//! Input: touch goes to the surface under the finger; buttons become roles; an absolute pointer
//! (a simulator window, a USB tablet or mouse on any board) acts as one finger: the press is the
//! contact. Clients get no cursor; the touch indicator (touches.rs), when on, shows the pointer
//! and every contact.
//!
//! The device description maps Linux codes to roles (hardware support spec §4, "semantic
//! input"). This module only knows the Linux names of codes, the kernel's own table
//! (`linux/input-event-codes.h`), never which device has which button.

use smithay::{
    backend::{
        input::{
            AbsolutePositionEvent, ButtonState, Event, InputEvent, KeyState, KeyboardKeyEvent,
            PointerButtonEvent, TouchEvent, TouchSlot,
        },
        libinput::LibinputInputBackend,
    },
    input::touch::{DownEvent, MotionEvent, UpEvent},
    utils::{Logical, Point, SERIAL_COUNTER},
};
use tracing::{debug, info};

use super::{layers::Contact as LayerContact, state::Shell, touches::Source};

pub fn process(shell: &mut Shell, event: InputEvent<LibinputInputBackend>) {
    // With the display off, the screen takes no touch: only the buttons act (spec §1.3).
    let display_off = shell.backend.as_ref().is_some_and(|b| !b.display_on());
    if display_off && !matches!(event, InputEvent::Keyboard { .. }) {
        return;
    }
    match event {
        InputEvent::Keyboard { event } => {
            // xkb keycodes are Linux codes plus 8.
            let code = event.key_code().raw().saturating_sub(8);
            let pressed = event.state() == KeyState::Pressed;
            match linux_key_name(code).and_then(|name| shell.description.role_of(name)) {
                // System combinations first (controls.rs); every other control is a role.
                Some(role) => {
                    if !super::controls::control(shell, role, pressed) {
                        info!(?role, pressed, "control");
                    }
                }
                None => debug!(code, "code without a role on this device"),
            }
        }
        InputEvent::TouchDown { event } => {
            let Some(location) = touch_location(shell, &event) else {
                return;
            };
            indicate(shell, |t| t.down(Source::Finger(event.slot()), location));
            if to_layers(shell, location, LayerContact::Down) {
                return;
            }
            let under = shell.surface_under(location);
            let serial = SERIAL_COUNTER.next_serial();
            if let Some(touch) = shell.seat.get_touch() {
                touch.down(
                    shell,
                    under,
                    &DownEvent {
                        slot: event.slot(),
                        location,
                        serial,
                        time: event.time(),
                    },
                );
            }
        }
        InputEvent::TouchMotion { event } => {
            let Some(location) = touch_location(shell, &event) else {
                return;
            };
            indicate(shell, |t| t.moved(Source::Finger(event.slot()), location));
            if to_layers(shell, location, LayerContact::Moved) {
                return;
            }
            let under = shell.surface_under(location);
            if let Some(touch) = shell.seat.get_touch() {
                touch.motion(
                    shell,
                    under,
                    &MotionEvent {
                        slot: event.slot(),
                        location,
                        time: event.time(),
                    },
                );
            }
        }
        InputEvent::TouchUp { event } => {
            indicate(shell, |t| t.up(Source::Finger(event.slot())));
            let at = shell.last_touch;
            if to_layers(shell, at, LayerContact::Up) {
                return;
            }
            let serial = SERIAL_COUNTER.next_serial();
            if let Some(touch) = shell.seat.get_touch() {
                touch.up(
                    shell,
                    &UpEvent {
                        slot: event.slot(),
                        serial,
                        time: event.time(),
                    },
                );
            }
        }
        InputEvent::TouchFrame { .. } => {
            if let Some(touch) = shell.seat.get_touch() {
                touch.frame(shell);
            }
        }
        InputEvent::TouchCancel { .. } => {
            if let Some(touch) = shell.seat.get_touch() {
                touch.cancel(shell);
            }
        }
        InputEvent::PointerMotionAbsolute { event } => {
            let Some(location) = touch_location(shell, &event) else {
                return;
            };
            shell.pointer_location = location;
            indicate(shell, |t| t.moved(Source::Pointer, location));
            if shell.pointer_contact {
                pointer_touch(shell, Contact::Motion, event.time());
            } else {
                // A pointer hovers: the shell's layers, or else the client under it, show where it
                // is. Contact itself is always touch (a finger never hovers).
                if !to_layers(shell, location, LayerContact::Moved) {
                    hover(shell, location, event.time());
                }
            }
        }
        InputEvent::PointerButton { event } => {
            // The primary button is the finger; others mean nothing on a touch interface.
            const BTN_LEFT: u32 = 0x110;
            if event.button_code() != BTN_LEFT {
                return;
            }
            match event.state() {
                ButtonState::Pressed if !shell.pointer_contact => {
                    shell.pointer_contact = true;
                    let at = shell.pointer_location;
                    indicate(shell, |t| t.down(Source::Pointer, at));
                    pointer_touch(shell, Contact::Down, event.time());
                }
                ButtonState::Released if shell.pointer_contact => {
                    shell.pointer_contact = false;
                    indicate(shell, |t| t.up(Source::Pointer));
                    pointer_touch(shell, Contact::Up, event.time());
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// The pointer over a client, without contact: `wl_pointer` motion, for hover states only.
fn hover(
    shell: &mut Shell,
    location: Point<f64, Logical>,
    time: smithay::backend::input::InputTime,
) {
    let under = shell.surface_under(location);
    let Some(pointer) = shell.seat.get_pointer() else {
        return;
    };
    pointer.motion(
        shell,
        under,
        &smithay::input::pointer::MotionEvent {
            location,
            serial: SERIAL_COUNTER.next_serial(),
            time,
        },
    );
    pointer.frame(shell);
}

/// A contact for the system layers while one takes input (spec §7: then no client gets it).
/// Returns whether the layers took it.
fn to_layers(shell: &mut Shell, at: Point<f64, Logical>, contact: LayerContact) -> bool {
    shell.last_touch = at;
    let Some(layers) = shell.layers.as_mut().filter(|l| l.takes_input()) else {
        return false;
    };
    layers.contact(at, contact);
    super::controls::layer_requests(shell);
    if let Some(backend) = shell.backend.as_mut() {
        backend.schedule();
    }
    super::backend::render_if_idle(shell);
    true
}

/// Tells the touch indicator, when it is on, and draws its change.
fn indicate(shell: &mut Shell, change: impl FnOnce(&mut super::touches::Touches)) {
    if !shell.touches.enabled() {
        return;
    }
    change(&mut shell.touches);
    if let Some(backend) = shell.backend.as_mut() {
        backend.schedule();
    }
    super::backend::render_if_idle(shell);
}

enum Contact {
    Down,
    Motion,
    Up,
}

/// The pointer's contact as a touch point, in a slot of its own so it never collides with fingers.
fn pointer_touch(shell: &mut Shell, contact: Contact, time: smithay::backend::input::InputTime) {
    let slot = TouchSlot::from(Some(u32::MAX - 1));
    let location: Point<f64, Logical> = shell.pointer_location;
    let layer_contact = match contact {
        Contact::Down => LayerContact::Down,
        Contact::Motion => LayerContact::Moved,
        Contact::Up => LayerContact::Up,
    };
    if to_layers(shell, location, layer_contact) {
        return;
    }
    let Some(touch) = shell.seat.get_touch() else {
        return;
    };
    match contact {
        Contact::Down => {
            let under = shell.surface_under(location);
            touch.down(
                shell,
                under,
                &DownEvent {
                    slot,
                    location,
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                },
            );
        }
        Contact::Motion => {
            let under = shell.surface_under(location);
            touch.motion(
                shell,
                under,
                &MotionEvent {
                    slot,
                    location,
                    time,
                },
            );
        }
        Contact::Up => touch.up(
            shell,
            &UpEvent {
                slot,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        ),
    }
    touch.frame(shell);
}

fn touch_location<E: AbsolutePositionEvent<LibinputInputBackend>>(
    shell: &Shell,
    event: &E,
) -> Option<smithay::utils::Point<f64, smithay::utils::Logical>> {
    let output = shell.output.as_ref()?;
    let geometry = shell.space.output_geometry(output)?;
    Some(event.position_transformed(geometry.size) + geometry.loc.to_f64())
}

/// The Linux name of a key code (`linux/input-event-codes.h`), as device descriptions write it.
pub fn linux_key_name(code: u32) -> Option<&'static str> {
    super::key_names::KEY_NAMES
        .binary_search_by_key(&code, |&(c, _)| c)
        .ok()
        .map(|i| super::key_names::KEY_NAMES[i].1)
}

#[cfg(test)]
mod tests {
    use super::linux_key_name;

    #[test]
    fn names_match_the_kernel_table() {
        assert_eq!(linux_key_name(164), Some("KEY_PLAYPAUSE"));
        assert_eq!(linux_key_name(115), Some("KEY_VOLUMEUP"));
        assert_eq!(linux_key_name(114), Some("KEY_VOLUMEDOWN"));
        assert_eq!(linux_key_name(116), Some("KEY_POWER"));
        assert_eq!(linux_key_name(57), Some("KEY_SPACE"));
        assert_eq!(linux_key_name(25), Some("KEY_P"));
        assert_eq!(linux_key_name(0), None);
    }
}
