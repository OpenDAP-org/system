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
    input::{
        keyboard::{FilterResult, Keycode},
        touch::{DownEvent, MotionEvent, UpEvent},
    },
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
                    // A release first lets go of what the press sent the activity, whatever the
                    // system does with it next, so no key stays down and no repeat goes on.
                    if !pressed {
                        release_forwarded(shell, role, event.time());
                    }
                    let taken = super::controls::control(shell, role, pressed);
                    if taken && pressed {
                        // The system takes this press: nothing the activity holds goes on.
                        release_all_forwarded(shell);
                    } else if !taken && pressed {
                        info!(?role, "control");
                        forward(shell, role, event.time());
                    }
                }
                // Not a control of this device: a keyboard attached to it (it is a general-purpose
                // Linux device; the simulator's arrows too). The key reaches the activity as is.
                None => {
                    let state = if pressed { KeyState::Pressed } else { KeyState::Released };
                    send_key(shell, code, state, event.time());
                    keyboard_repeat(shell, code, pressed);
                }
            }
        }
        InputEvent::TouchDown { event } => {
            let Some(location) = touch_location(shell, &event) else {
                return;
            };
            indicate(shell, |t| t.down(Source::Finger(event.slot()), location));
            debug(shell, location, LayerContact::Down);
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
            debug(shell, location, LayerContact::Moved);
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
            debug(shell, at, LayerContact::Up);
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
                debug(shell, location, LayerContact::Moved);
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
                    debug(shell, at, LayerContact::Down);
                    pointer_touch(shell, Contact::Down, event.time());
                }
                ButtonState::Released if shell.pointer_contact => {
                    shell.pointer_contact = false;
                    indicate(shell, |t| t.up(Source::Pointer));
                    let at = shell.pointer_location;
                    debug(shell, at, LayerContact::Up);
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

// Linux key codes (linux/input-event-codes.h) the activity receives.
const KEY_NEXTSONG: u32 = 163;
const KEY_PREVIOUSSONG: u32 = 165;
/// Short against long (T1 controls spec §1.1; proposed).
const LONG_PRESS: std::time::Duration = std::time::Duration::from_millis(opendap_input::thresholds::LONG_MS);
const KEY_TAB: u32 = 15;
const KEY_ENTER: u32 = 28;
const KEY_LEFTSHIFT: u32 = 42;

/// A held focus step's repeat accelerates exponentially and stops where it is released, with no
/// inertia (maintainers, 2026-10-23): after Android's
/// key-repeat delay, the first step comes after REPEAT_FIRST, and each next interval is
/// REPEAT_FACTOR of the last, down to REPEAT_FASTEST. Precise at first, fast through long lists.
/// SYS-066 asks for these to be adjustable.
const REPEAT_DELAY: std::time::Duration =
    std::time::Duration::from_millis(opendap_input::thresholds::REPEAT_DELAY_MS);
const REPEAT_FIRST: f64 = opendap_input::thresholds::REPEAT_FIRST_MS;
const REPEAT_FACTOR: f64 = opendap_input::thresholds::REPEAT_FACTOR;
const REPEAT_FASTEST: f64 = opendap_input::thresholds::REPEAT_FASTEST_MS;

/// One more Tab while a volume button is held in focus navigation: Tab released and pressed
/// again, Shift (for Volume +) still down.
fn step_again(shell: &mut Shell) {
    let Some(keyboard) = shell.seat.get_keyboard() else {
        return;
    };
    for state in [KeyState::Released, KeyState::Pressed] {
        keyboard.input::<(), _>(
            shell,
            Keycode::new(KEY_TAB + 8),
            state,
            SERIAL_COUNTER.next_serial(),
            smithay::backend::input::InputTime::now(),
            |_, _, _| FilterResult::Forward,
        );
    }
}

/// Whether sound is playing: any ALSA playback stream running (the cards' `status` files).
fn sound_playing() -> bool {
    let Ok(cards) = std::fs::read_dir("/proc/asound") else {
        return false;
    };
    cards
        .flatten()
        .filter(|c| c.file_name().to_string_lossy().starts_with("card"))
        .flat_map(|c| std::fs::read_dir(c.path()).into_iter().flatten().flatten())
        .filter(|p| p.file_name().to_string_lossy().ends_with('p'))
        .flat_map(|p| std::fs::read_dir(p.path()).into_iter().flatten().flatten())
        .any(|s| {
            std::fs::read_to_string(s.path().join("status")).is_ok_and(|t| t.contains("RUNNING"))
        })
}

/// A role the system combinations left to the activity, pressed (T1 controls spec §1.3).
/// Volume ± is the volume while sound plays or the activity asks for it, and otherwise moves the
/// focus: Volume − to the next element, Volume + to the previous (Tab, Shift+Tab). Action takes
/// the focused element (Enter), or the screen's primary action where it declares one (Blackbird
/// UI's components). With the display off nothing reaches the focus: Action is play/pause and
/// Volume the volume (§1.3, T1-060). Main stays the system's. The keys chosen at the press are
/// the ones released ([`release_forwarded`]).
fn forward(shell: &mut Shell, role: opendap_device::Role, time: smithay::backend::input::InputTime) {
    use opendap_device::Role;
    use opendap_input::semantic::{self, Semantic};
    let display_off = shell.backend.as_ref().is_some_and(|b| !b.display_on());
    let Some(input_role) = (match role {
        Role::Action => Some(semantic::Role::Action),
        Role::VolumeUp => Some(semantic::Role::VolumeUp),
        Role::VolumeDown => Some(semantic::Role::VolumeDown),
        _ => None,
    }) else {
        return;
    };
    // What the press means here (input events §3.4). The shell does not see the activity's focus
    // or locks: the components refine Activate into the primary action and Next/Previous into a
    // value's steps (Blackbird UI).
    let ctx = semantic::Context {
        display_on: !display_off,
        volume_wanted: !display_off && sound_playing(),
        ..Default::default()
    };
    let meaning = semantic::press(input_role, ctx);
    let keys: &'static [u32] = match meaning {
        // No audio engine yet to take these (system ADR 0002 odap-audiod).
        Semantic::PlayPause => {
            info!("play/pause, display off");
            &[]
        }
        // The volume: a step when released short; held to the long-press time, the track
        // (Volume + next, Volume − previous: T1-062, controls spec §1.3), as media keys to the
        // activity until the audio engine takes them. One press means one thing.
        Semantic::VolumeUp | Semantic::VolumeDown => {
            shell.repeat_gen += 1;
            let generation = shell.repeat_gen;
            let _ = shell.handle.insert_source(
                calloop::timer::Timer::from_duration(LONG_PRESS),
                move |_, _, shell| {
                    if shell.repeat_gen == generation && shell.forwarded.contains_key(&role) {
                        let key = match semantic::long_press(input_role, ctx) {
                            Semantic::NextTrack => KEY_NEXTSONG,
                            _ => KEY_PREVIOUSSONG,
                        };
                        info!(?role, "track, volume held");
                        let now = smithay::backend::input::InputTime::now();
                        send_key(shell, key, KeyState::Pressed, now);
                        send_key(shell, key, KeyState::Released, now);
                        // Its release is the long press's: no step after.
                        shell.forwarded.insert(role, &[]);
                        shell.volume_long = Some(role);
                    }
                    calloop::timer::TimeoutAction::Drop
                },
            );
            shell.volume_long = None;
            &[]
        }
        Semantic::Next => &[KEY_TAB],
        Semantic::Previous => &[KEY_LEFTSHIFT, KEY_TAB],
        Semantic::Activate | Semantic::Primary => &[KEY_ENTER],
        _ => &[],
    };
    shell.forwarded.insert(role, keys);
    // Held in focus navigation, a volume button keeps moving: after REPEAT_DELAY, accelerating
    // down to REPEAT_FASTEST, until it is released (maintainers, 2026-10-23). Action never
    // repeats.
    if semantic::repeats(input_role, ctx) {
        shell.repeat_gen += 1;
        let generation = shell.repeat_gen;
        let mut interval = REPEAT_FIRST;
        let _ = shell.handle.insert_source(
            calloop::timer::Timer::from_duration(REPEAT_DELAY),
            move |_, _, shell| {
                if shell.repeat_gen != generation || !shell.forwarded.contains_key(&role) {
                    return calloop::timer::TimeoutAction::Drop;
                }
                step_again(shell);
                debug!(interval_ms = interval as u64, "focus step, held");
                let next = std::time::Duration::from_millis(interval as u64);
                interval = (interval * REPEAT_FACTOR).max(REPEAT_FASTEST);
                calloop::timer::TimeoutAction::ToDuration(next)
            },
        );
    }
    for &code in keys {
        send_key(shell, code, KeyState::Pressed, time);
    }
}

/// A role released: the keys its press sent are released, in reverse, and its repeat stops.
/// The focus stays where it is, with no step after (maintainers, 2026-10-23).
fn release_forwarded(
    shell: &mut Shell,
    role: opendap_device::Role,
    time: smithay::backend::input::InputTime,
) {
    let Some(keys) = shell.forwarded.remove(&role) else {
        return;
    };
    shell.repeat_gen += 1;
    // A volume press released short, in the volume context: one step (no audio engine yet,
    // system ADR 0002 odap-audiod).
    if keys.is_empty()
        && matches!(role, opendap_device::Role::VolumeUp | opendap_device::Role::VolumeDown)
        && shell.volume_long.take() != Some(role)
    {
        info!(?role, "volume step");
    }
    for &code in keys.iter().rev() {
        send_key(shell, code, KeyState::Released, time);
    }
}

/// The system takes over (a combination, the power menu, the display going off): every key the
/// activity holds is released, every repeat stops, and its touches are cancelled (input events
/// §3.1: `pointercancel`; Wayland's `wl_touch.cancel`), so nothing that started acts.
pub fn release_all_forwarded(shell: &mut Shell) {
    let roles: Vec<_> = shell.forwarded.keys().copied().collect();
    for role in roles {
        release_forwarded(shell, role, smithay::backend::input::InputTime::now());
    }
    shell.key_repeat_gen += 1;
}

pub fn system_takeover(shell: &mut Shell) {
    release_all_forwarded(shell);
    if let Some(touch) = shell.seat.get_touch() {
        touch.cancel(shell);
    }
    if let Some(layers) = shell.layers.as_mut() {
        let _ = layers.debug_contact(shell.last_touch, LayerContact::Cancel);
    }
}

/// One key to the activity (xkb keycodes are Linux codes plus 8).
fn send_key(shell: &mut Shell, code: u32, state: KeyState, time: smithay::backend::input::InputTime) {
    let Some(keyboard) = shell.seat.get_keyboard() else {
        return;
    };
    keyboard.input::<(), _>(
        shell,
        Keycode::new(code + 8),
        state,
        SERIAL_COUNTER.next_serial(),
        time,
        |_, _, _| FilterResult::Forward,
    );
}

/// An attached keyboard's key held: repeated by the shell (the seat's client-side repeat is off,
/// so the focus keys the shell sends never repeat twice), after the usual 600 ms at 25 per second
/// (X11's and GNOME's defaults). Modifiers never repeat, nor Escape, Enter and Space.
const KEY_REPEAT_DELAY: std::time::Duration =
    std::time::Duration::from_millis(opendap_input::thresholds::KEY_REPEAT_DELAY_MS);
const KEY_REPEAT_EVERY: std::time::Duration =
    std::time::Duration::from_millis(opendap_input::thresholds::KEY_REPEAT_EVERY_MS);
const MODIFIERS: &[u32] = &[29, 42, 54, 56, 97, 100, 125, 126, 58];
/// Keys whose meaning never repeats (input events §5.1: activate, primary and back): Escape,
/// Enter, Space, the keypad's Enter.
const NO_REPEAT: &[u32] = &[1, 28, 57, 96];

fn keyboard_repeat(shell: &mut Shell, code: u32, pressed: bool) {
    shell.key_repeat_gen += 1;
    if !pressed || MODIFIERS.contains(&code) || NO_REPEAT.contains(&code) {
        return;
    }
    let generation = shell.key_repeat_gen;
    let _ = shell.handle.insert_source(
        calloop::timer::Timer::from_duration(KEY_REPEAT_DELAY),
        move |_, _, shell| {
            if shell.key_repeat_gen != generation {
                return calloop::timer::TimeoutAction::Drop;
            }
            let now = smithay::backend::input::InputTime::now();
            send_key(shell, code, KeyState::Released, now);
            send_key(shell, code, KeyState::Pressed, now);
            calloop::timer::TimeoutAction::ToDuration(KEY_REPEAT_EVERY)
        },
    );
}

/// How long the debug UI mode shows a gesture after its contact ends: feedback for the eye, not a
/// message to read (maintainers, 2026-10-23). It stays while the contact lasts, and a newer
/// gesture replaces it at once.
const GESTURE_SHOWN: std::time::Duration = std::time::Duration::from_millis(800);

/// Tells the debug UI mode, when it is on, of a contact: its crosshair and region, then its
/// gesture, shown for a while.
fn debug(shell: &mut Shell, at: Point<f64, Logical>, contact: LayerContact) {
    // A long press is named at LONG_MS while the finger is still down.
    if matches!(contact, LayerContact::Down) && super::layers::debug_ui() {
        let _ = shell.handle.insert_source(
            calloop::timer::Timer::from_duration(std::time::Duration::from_millis(opendap_input::gesture::LONG_MS)),
            |_, _, shell| {
                if let Some(layers) = shell.layers.as_mut() {
                    // Shown while the finger stays down; its timeout starts when it lifts.
                    layers.debug_long_press();
                    if let Some(b) = shell.backend.as_mut() {
                        b.schedule();
                    }
                    super::backend::render_if_idle(shell);
                }
                calloop::timer::TimeoutAction::Drop
            },
        );
    }
    let Some(layers) = shell.layers.as_mut() else {
        return;
    };
    let changed = layers.refresh_debug();
    let Some(seq) = layers.debug_contact(at, contact).or(changed.then_some(0)) else {
        if super::layers::debug_ui() {
            if let Some(backend) = shell.backend.as_mut() {
                backend.schedule();
            }
            super::backend::render_if_idle(shell);
        }
        return;
    };
    if seq > 0 {
        clear_later(shell, seq);
    }
    if let Some(backend) = shell.backend.as_mut() {
        backend.schedule();
    }
    super::backend::render_if_idle(shell);
}

/// The gesture's callout leaves after GESTURE_SHOWN.
fn clear_later(shell: &mut Shell, seq: u64) {
    {
        let _ = shell.handle.insert_source(
            calloop::timer::Timer::from_duration(GESTURE_SHOWN),
            move |_, _, shell| {
                if let Some(l) = shell.layers.as_mut() {
                    l.clear_gesture(seq);
                }
                if let Some(b) = shell.backend.as_mut() {
                    b.schedule();
                }
                super::backend::render_if_idle(shell);
                calloop::timer::TimeoutAction::Drop
            },
        );
    }
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
