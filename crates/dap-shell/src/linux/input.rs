//! Input: touch goes to the surface under the finger; buttons become roles.
//!
//! The device description maps Linux codes to roles (hardware support spec §4, "semantic
//! input"). This module only knows the Linux names of codes, the kernel's own table
//! (`linux/input-event-codes.h`), never which device has which button.

use smithay::{
    backend::{
        input::{AbsolutePositionEvent, Event, InputEvent, KeyState, KeyboardKeyEvent, TouchEvent},
        libinput::LibinputInputBackend,
    },
    input::touch::{DownEvent, MotionEvent, UpEvent},
    utils::SERIAL_COUNTER,
};
use tracing::{debug, info};

use super::state::Shell;

pub fn process(shell: &mut Shell, event: InputEvent<LibinputInputBackend>) {
    match event {
        InputEvent::Keyboard { event } => {
            // xkb keycodes are Linux codes plus 8.
            let code = event.key_code().raw().saturating_sub(8);
            let pressed = event.state() == KeyState::Pressed;
            match linux_key_name(code).and_then(|name| shell.description.role_of(name)) {
                Some(role) => info!(?role, pressed, "control"),
                None => debug!(code, "code without a role on this device"),
            }
        }
        InputEvent::TouchDown { event } => {
            let Some(location) = touch_location(shell, &event) else { return };
            let under = shell.surface_under(location);
            let serial = SERIAL_COUNTER.next_serial();
            if let Some(touch) = shell.seat.get_touch() {
                touch.down(shell, under, &DownEvent { slot: event.slot(), location, serial, time: event.time() });
            }
        }
        InputEvent::TouchMotion { event } => {
            let Some(location) = touch_location(shell, &event) else { return };
            let under = shell.surface_under(location);
            if let Some(touch) = shell.seat.get_touch() {
                touch.motion(shell, under, &MotionEvent { slot: event.slot(), location, time: event.time() });
            }
        }
        InputEvent::TouchUp { event } => {
            let serial = SERIAL_COUNTER.next_serial();
            if let Some(touch) = shell.seat.get_touch() {
                touch.up(shell, &UpEvent { slot: event.slot(), serial, time: event.time() });
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
        _ => {}
    }
}

fn touch_location<E: AbsolutePositionEvent<LibinputInputBackend>>(
    shell: &Shell,
    event: &E,
) -> Option<smithay::utils::Point<f64, smithay::utils::Logical>> {
    let output = shell.output.as_ref()?;
    let geometry = shell.space.output_geometry(output)?;
    Some(event.position_transformed(geometry.size) + geometry.loc.to_f64())
}

/// Linux names of the key codes a control can produce (`linux/input-event-codes.h`).
pub fn linux_key_name(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "KEY_ESC",
        28 => "KEY_ENTER",
        102 => "KEY_HOME",
        103 => "KEY_UP",
        105 => "KEY_LEFT",
        106 => "KEY_RIGHT",
        108 => "KEY_DOWN",
        113 => "KEY_MUTE",
        114 => "KEY_VOLUMEDOWN",
        115 => "KEY_VOLUMEUP",
        116 => "KEY_POWER",
        139 => "KEY_MENU",
        142 => "KEY_SLEEP",
        158 => "KEY_BACK",
        163 => "KEY_NEXTSONG",
        164 => "KEY_PLAYPAUSE",
        165 => "KEY_PREVIOUSSONG",
        166 => "KEY_STOPCD",
        200 => "KEY_PLAYCD",
        201 => "KEY_PAUSECD",
        207 => "KEY_PLAY",
        226 => "KEY_MEDIA",
        353 => "KEY_SELECT",
        _ => return None,
    })
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
        assert_eq!(linux_key_name(0), None);
    }
}
