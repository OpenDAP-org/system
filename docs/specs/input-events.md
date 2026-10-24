# Input events

- **Status:** Draft
- **Decided by:** Claude, under the maintainers' delegation (they asked to standardize every event of the T1 and of any faceplate or hardware, following known standards, with raw and domain events, directives with good implicit defaults, and the handling model and payloads); for their review
- **Requirements:** SYS-039 (gestures), SYS-066 (adjustable timings), SYS-067 and SYS-068 (hardware by description), T1-060 to T1-063 (buttons); the T1 [controls spec](https://github.com/opendap-project/t1/blob/main/docs/specs/controls-and-status-led.md); the design system §8.3 to §8.5

One model of input for every device Blackbird OS runs on: the T1's touch faceplate and buttons, other faceplates (buttons, dials, wheels, sliders, switches), HATs, a mouse, a keyboard, the simulator window. Nothing in it is invented where a standard already says it.

| Concern | Follows |
|---|---|
| Touch, mouse and pen as one pointer | W3C Pointer Events (Level 3): `pointerdown`, `pointermove`, `pointerup`, `pointercancel`, `pointerType`, `pointerId`, `isPrimary`, pointer capture |
| Propagation and default actions | W3C DOM Events: capture, target and bubble phases; `stopPropagation`, `preventDefault` |
| Keys and buttons at the device | Linux evdev: `EV_KEY` value 0 up, 1 down, 2 repeat; names from `input-event-codes.h` |
| Press, long press, repeat | Android `KeyEvent` (`ACTION_DOWN`/`ACTION_UP`, `repeatCount`, long press) |
| Gestures and their thresholds | Android `GestureDetector` and `ViewConfiguration`; iOS gesture recognizers' names |
| Wheels, dials, detents | Wayland `wl_pointer.axis` with `axis_value120` (120 per detent); Android rotary input |
| Handler directives | Vue event modifiers (`.once`, `.stop`, `.prevent`, `.capture`, `.self`, `.passive`) and DOM listener options |

## 1. Two levels

**Raw events** are what a device reports, normalized only in units and time: a key went down, a contact moved, a dial turned one detent. They carry the hardware's facts (code, slot, axis) and are for the shell, drivers, diagnostics and the debug UI mode.

**Domain events** are what the input means, independent of the hardware that produced it: a pointer (touch, mouse or pen alike), a gesture, a control used by its role, a semantic action. Activities handle domain events only, so an activity written for the T1 runs unchanged on a faceplate with a dial and no touch (SYS-040, SYS-068).

```text
hardware → raw events → [shell: pointer unification, recognizers, roles, system combinations]
        → domain events → [semantic mapping by context] → activity
```

## 2. Raw events

| Raw event | From | Fields |
|---|---|---|
| `key` | evdev `EV_KEY` (buttons, keyboards, a faceplate's HID controls) | `code` (Linux name), `state`: down, up, repeat |
| `contact` | evdev multitouch, `wl_touch` | `slot`, `phase`: down, move, up, cancel; `x`, `y`; `major`, `minor`, `pressure` when reported |
| `motion` | a mouse, a tablet, the simulator's pointer | `x`, `y` absolute or `dx`, `dy` relative; `buttons` |
| `axis` | a wheel, a dial, an encoder, a touch ring | `axis`: vertical, horizontal, rotary; `value120` (120 per detent, fractions for high resolution) |
| `switch` | evdev `EV_SW`, a faceplate's switch | `code`, `position` |

Every raw event carries the envelope of §6 with `level: raw`.

## 3. Domain events

### 3.1 Pointer: touch, mouse and pen as one

A pointer event is the same whatever made it (W3C): `pointerType` says touch, mouse or pen; `isPrimary` marks the first contact. The rules that make them homologous:

- **Contact is the press.** A finger touching, a mouse's primary button down and a pen touching are all `pointerdown`; lifting is `pointerup`. Every other mouse button means nothing on a touch interface.
- **Hover exists only without contact,** and only for mouse and pen (`pointerType` says which). A touch interface never depends on it; it only adds feedback where a pointer can hover (the simulator window, a mouse).
- **Capture.** The element that received `pointerdown` receives the rest of that pointer's stream until `pointerup` or `pointercancel`, even outside its bounds (W3C pointer capture). A slider's drag is one such stream.
- **Cancel.** A system gesture or a system view taking over sends `pointercancel`; nothing that started acts.

### 3.2 Gestures

Recognized from pointer streams (`opendap-gesture`), with the thresholds of §7:

| Gesture | Is | Payload |
|---|---|---|
| `tap` | Down and up within the slop, before the long-press time | position |
| `long-press` | Held within the slop past the long-press time; fires while still down | position |
| `double-tap` | Two taps within the double-tap time; opt-in per element only, because it delays every single tap there (a single press never waits, T1-063) | position |
| `pan` | Moved past the slop: `start`, `move`, `end` | delta, velocity |
| `swipe` | A pan that ends faster than the fling velocity | direction, distance, velocity, duration |
| `edge-swipe` | A swipe that starts in an edge zone; reserved for the system's navigation (back, the companion screen, More, status; design system §8.5) | edge, direction |
| `pinch` | Two contacts whose distance changes: `start`, `change`, `end` | scale, centre |
| `rotate` | Two contacts whose angle changes | angle, centre |
| `scroll` | From a pan in a scroll area, or from an `axis` | delta |

### 3.3 Controls, by role

Physical controls are used through their role (`action`, `volume-up`, `main`…), never their code (SYS-067). Every kind of control (hardware-support §5) produces the same family:

| Event | Is (Android `KeyEvent`, evdev) |
|---|---|
| `down`, `up` | The raw edges, by role |
| `press` | Down and up before the long-press time: fires on up |
| `long-press` | Held past the long-press time: fires while down, once |
| `hold` | Held past the hold time (2 s): fires while down, once (the power menu) |
| `repeat` | While held, where the meaning repeats (focus, a value): `repeatCount`, at accelerating intervals |
| `chord` | Two or more held together (Main + Volume −): the system's combinations first |
| `step` | A dial or wheel turned: `steps` (± detents, from `value120`), `velocity` |
| `toggle` | A switch moved: its `position` |

A `press` never waits for a second press: there is no double or triple press on controls (T1-063).

### 3.4 Semantic actions

What most handlers want, whatever produced it. The shell maps controls, gestures and keys to these by context (the [T1 controls spec](https://github.com/opendap-project/t1/blob/main/docs/specs/controls-and-status-led.md), design system §8.4):

| Action | From (T1) | From a keyboard | From other hardware |
|---|---|---|---|
| `activate` | Action press on a focused element; a tap | Enter, Space | A dial's push, a select button |
| `primary` | Action press where the screen has a primary action | — | A faceplate's primary button |
| `next`, `previous` | Volume − / + when no sound plays and nothing asks for the volume | Tab, Shift+Tab | A dial's step, without a lock |
| `increase`, `decrease` | Volume + / − on a locked control | ↑ → / ↓ ← on a focused control | A dial's step on a locked control |
| `volume-up`, `volume-down` | Volume + / − while sound plays or the activity asks | Volume keys | A volume dial |
| `back` | The leading-edge swipe; the header's back | Escape | A back button |
| `cancel` | Leaving a lock or a system view without choosing | Escape | — |

A semantic action keeps its source in its payload, so a handler that must know (a slider showing whether a finger or a button moves it) can.

### 3.5 Keyboards

A keyboard attached to the device is not a control: its keys are `key` domain events by name (`left`, `enter`) and text input goes through the input-method protocols (the on-screen keyboard included). ↑ ↓ on the simulator are the volume buttons by their place; ← → are a keyboard's arrows (design system §8.4).

### 3.6 What is not an input event

Sensors (ambient light, battery, temperature) are states with change notifications, read through their own interfaces (hardware-support §4), not input events. A microphone is an audio source.

## 4. Handling

### 4.1 Dispatch order

1. **The system** first: the shell's system combinations, system views (the power menu), the debug UI mode's references (which observe and never consume). What the system takes, no activity sees; a system view's taking over sends `pointercancel` to the activity.
2. **The activity on screen**: focus-targeted events (controls, keys, semantic actions) go to the focused element; pointer events to the element under the pointer, or to the element that captured it.
3. **Inside the activity**, W3C phases: **capture** from the root down to the target, **target**, then **bubble** back up. A handler returns handled or not; an unhandled event continues.

### 4.2 Default actions

Every event that has a default does it unless a handler prevents it: Action's default is the screen's primary action, else activating the focused element; Volume's default is the volume or the focus (§3.4); a tap's default is the element's activation; an edge swipe's default is the system's navigation (which an activity can never prevent).

## 5. Directives

Declared on a handler, as Vue's modifiers and DOM's listener options name them:

| Directive | Does |
|---|---|
| `once` | The handler runs once, then is removed |
| `stop` | Stops propagation after this handler |
| `prevent` | Prevents the default action |
| `capture` | Runs in the capture phase |
| `self` | Runs only when the element is the target, not a descendant |
| `passive` | Promises not to prevent the default, so scrolling and dragging never wait for it |
| `throttle(ms)` | At most one call per interval, the latest kept |
| `debounce(ms)` | One call after the events stop for the interval |
| `repeat(off)` | Takes a held control's first event only |
| `hold(ms)` | Overrides the long-press or hold time for this element (an accessibility setting can still lengthen it) |

### 5.1 Implicit defaults

Good UX needs no directive written; these are the defaults, and a directive only changes one:

- **Activation is guarded.** An `activate` or `primary` whose handler starts work (a request, a change of hardware, a navigation) does not fire again until that work ends: no double submission, no repeated reboot. A second tap within the double-tap time on the same element is dropped unless the element opted into `double-tap`.
- **Continuous values coalesce to the frame.** `pointermove`, `pan.move`, `pinch.change`, `step` and `increase`/`decrease` from a held control are delivered at most once per frame, the latest values kept; a value sent to hardware is fire-and-forget, the latest winning.
- **Repeat only where it means something:** `next`/`previous` and `increase`/`decrease` repeat while held, accelerating; `activate`, `primary` and `back` never repeat.
- **Pointer and scroll handlers are passive by default;** an element that must prevent (a slider inside a scroll area) declares it by capturing its pointer.
- **No debounce on input itself.** Contact bounce is the driver's (the kernel's debounce for buttons), never added latency in handlers.

## 6. Payloads

Every event shares one envelope:

| Field | Meaning |
|---|---|
| `type` | The event's name (`pointerdown`, `tap`, `press`, `next`…) |
| `level` | `raw` or `domain` |
| `source` | `kind` (touch, mouse, pen, control, rotary, keyboard, switch), `device` (its description id), `role` for controls |
| `time` | Monotonic time of the hardware event, in microseconds |
| `target` | The element it is delivered to (domain events in an activity) |
| `phase` | capture, target or bubble, while it propagates |

Then by type:

| Type | Fields |
|---|---|
| Pointer | `pointerId`, `pointerType`, `isPrimary`, `x`, `y` (logical px), `pressure`, `width`, `height`, `tiltX`, `tiltY`, `buttons` |
| Gesture | `start` and `end` positions, `delta`, `velocity` (px/s), `duration` (ms), `direction`, `edge`, `scale`, `rotation`, `centre` |
| Control | `role`, `controlKind` (button, rocker-half, dial, slider, switch), `repeatCount`, `chord` (the roles held) |
| Rotary | `role`, `steps` (± detents), `value120`, `velocity` |
| Key | `key` (Linux name), `code`, `state`, `repeat`, `modifiers`, `text` |
| Semantic | `action`, `from` (the domain event that produced it) |

Positions are logical pixels of the activity's surface; sizes in design pixels convert through the display's scale (design system §18.2).

## 7. Thresholds

Defaults, in design pixels and milliseconds; all adjustable (SYS-066), and lengthened, never shortened, by accessibility settings:

| Threshold | Default | From |
|---|---|---|
| Touch slop | 10 dp | Android's 8 dp, at the T1's reading distance |
| Long press | 500 ms | T1 controls spec §1.1 |
| Hold | 2 s | T1 controls spec §1.1 |
| Double-tap | 300 ms | Android `DOUBLE_TAP_TIMEOUT` |
| Fling velocity | 50 dp/s minimum | Android `MINIMUM_FLING_VELOCITY` |
| Edge zone | 20 dp | Design system §8.5 |
| Repeat | 400 ms, then 250 ms × 0.8 down to 50 ms; coasting back after release | Shell spec §2 (maintainers) |

## 8. Where it lives

- **`opendap-gesture`** becomes **`opendap-input`**: the domain model (§3, §6), the recognizers (§3.2), the semantic mapping (§3.4) and the thresholds (§7), platform-agnostic, used by the shell, the CLI and OpenDAP Desktop.
- **The shell** turns raw events into domain events and dispatches them (§4).
- **Activities** receive them today through Wayland's standard input (touch, pointer, and the keys of focus navigation, shell spec §2); the full domain events, semantic actions and directives need a protocol of their own, opened when the activity runtime is (ADR 0002).
- **The CLI and the simulator** inject raw events (`opendap input send`), so they test the same pipeline a hand does.

## Open questions

- The protocol that carries domain events and directives to activities.
- Whether `double-tap` is available to third-party activities, or first-party only.
- Pen and pressure: no OpenDAP hardware has them yet; kept for any that will.
