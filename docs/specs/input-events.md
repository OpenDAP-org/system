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

## 1. Raw and domain events

**Raw events** are what a device reports, normalized only in units and time: a key went down, a contact moved, a dial turned one detent. They carry the hardware's facts (code, slot, axis) and are for the shell, drivers, diagnostics and the debug UI mode.

**Domain events** are what the input means, independent of the hardware that produced it: a pointer (touch, mouse or pen alike), a gesture, a control used by its role, a semantic action. Activities handle domain events only, so an activity written for the T1 runs unchanged on a faceplate with a dial and no touch (SYS-040, SYS-068).

```text
hardware → raw events → [shell: pointer unification, recognizers, roles, system combinations]
        → domain events → [semantic mapping by context] → activity
```

## 2. Raw events

| Raw event | From | Fields |
|---|---|---|
| `key` | evdev `EV_KEY` (buttons, keyboards, a faceplate's HID controls) | `code` (Linux name), `state`: down, up (libinput never delivers the kernel's value 2: repeats are made by the shell, §3.3) |
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

Recognized from pointer streams (`opendap-input`), with the thresholds of §7. Every gesture carries the **region** it started in: the content, one of the four edge zones, or one of the four corners where two edge zones meet. A corner is a region of its own, so a contact there is never claimed by two edges (design system §8.5). The top zone is the status bar's height, since the system's top gesture is "swipe down from the status bar" (SYS-039); the side and bottom zones are the edge zone's width. Leading and trailing follow the layout's direction (right to left mirrors them, design system §9.5).

**Discrete gestures** fire once:

| Gesture | Is | Payload |
|---|---|---|
| `tap` | Down and up within the slop, before the long-press time. A contact that went past the slop is never a tap, even if it comes back (Android's `GestureDetector`) | position |
| `long-press` | Held within the slop past the long-press time; fires while still down, and nothing follows on release (no tap) | position |
| `double-tap` | Two taps within the double-tap time; opt-in per element only, because it delays every single tap there (a single press never waits, T1-063) | position |
| `swipe` | A pan released faster than the fling velocity (measured over the last 100 ms, Android's `VelocityTracker`); it follows the pan's `end` | direction, distance, velocity, duration |
| `edge-swipe` | A swipe that starts in an edge zone (not a corner) and moves away from that edge; reserved for the system's navigation (back, the companion screen, More, status; design system §8.5) | edge, direction |

**Composite gestures** are continuous: one stream with phases `start`, `move`, `end`, `cancel` (W3C's pointer stream), and nothing that started acts after a `cancel`:

| Gesture | Is | Payload |
|---|---|---|
| `pan` | Moved past the slop, with no drag: scrolling, paging, an edge gesture in progress | position, translation, delta, velocity |
| `drag` | Drag and drop: an element follows the finger from `start` to `end` (the **drop**, with the target under it) or `cancel` | position, translation, delta, the drop target |
| `pinch` | Two contacts whose distance changes | scale, centre |
| `rotate` | Two contacts whose angle changes | angle, angle delta (unwrapped across ±180°), centre |
| `scroll` | From a pan in a scroll area, or from an `axis` | delta |

**Drag and drop: the free primitive and its directives.** A drag with no directive is free: both axes, and it starts after a long press then a move (Android's and iOS's drag and drop), so a plain move over draggable content still scrolls. An element changes only what it needs (§5):

| Directive | Values | Default | Example |
|---|---|---|---|
| `axis` | `free`, `horizontal`, `vertical`, `dominant` (locked to the axis of the first move past the slop) | `free` | A slider's knob: `horizontal`; a list reordered: `vertical`; a card in a pager: `dominant` |
| `start` | `long-press`, `immediate` (at the first move past the slop; no pan) | `long-press` | A knob or a panel's handle: `immediate` |
| `bounds` | the element's container, a rectangle, none | none | A knob kept on its track |
| `snap` | positions or a step, applied at the drop | none | Stops on an axis |
| `targets` | which elements accept a drop, by kind | any that declares `drop` | A queue accepting tracks |

A drag's position is already constrained to its axis and bounds; the raw positions stay in the pointer events. The pan follows the same `axis` directive in a scroll area (Compose's `draggable(orientation)`, iOS's axis lock).

### 3.2.1 Levels and ownership

The same contact means different things at different levels, and every level has its own box, its own coordinates and its own regions (maintainers, 2026-10-23). A swipe that starts at the screen's edge is the system's back; the same swipe in the middle of a card is the card's. Each level recognizes only within its own box, and its regions are its own: the screen's edges are not the activity's, and an element's edges are not the screen's.

| Level | Box | Recognizes (examples) | Its regions |
|---|---|---|---|
| **Screen** | The whole display | The system's gestures: edge swipes, the status bar's swipe down, corners (design system §8.5) | Edge zones, the status bar, corners |
| **Shell content area** | What the shell gives activities: the screen minus the system's insets (the status bar, a docked keyboard), Android's window insets and iOS's safe area | Nothing of its own; it places the activity and says its insets | Its edges are where the activity's begin |
| **Activity surface** | The activity's surface, in the content area | The activity's gestures: a pager's swipe, its own edge gestures inside the content area | The surface's edges and corners, with zones the activity declares |
| **Container** | A scroll area, a sheet, a pager | Scrolling, paging, a sheet's drag to dismiss | Its edges (a sheet's top handle), and positions in its **content** (scrolled) as well as in its box |
| **Group or region** | A group, a named region of a screen | Whatever its handlers declare | Its edges and corners, by the zone width it declares |
| **Element** | One control | Tap, long press, drag of a knob | `regionOf(element, zone)` (§6.1) |

**Ownership: one owner per contact** (Flutter's gesture arena, iOS's `require(toFail:)`, Android's `onInterceptTouchEvent`):

1. **The system first.** A contact the screen level recognizes (an edge swipe from a screen edge zone, a corner) belongs to the system; the activity gets `cancel` for it as soon as the system claims it (§5.1), and never a gesture of its own from it. A contact that starts in a screen edge zone is held by the shell until it is decided (past the slop, or lifted); one that starts outside every screen zone goes to the activity at once, with no delay.
2. **Then outward in, in the capture phase:** a container that recognizes a gesture its children do not want (a scroll area's vertical pan over a horizontal slider's knob) claims it at the slop, and the child gets `cancel`. A child keeps what it declared first: a knob's `immediate` horizontal drag wins the horizontal axis, and the scroll keeps the vertical one (`axis` directives, §3.2).
3. **Then the target**, and bubbling: what no level claimed is the target's, as a tap or a long press.

A level never sees coordinates or regions of the levels above it (an activity never learns screen coordinates, §6.1), and a level never claims what starts outside its box.

### 3.3 Controls, by role

Physical controls are used through their role (`action`, `volume-up`, `main`…), never their code (SYS-067). Every kind of control (hardware-support §5) produces the same family:

| Event | Is (Android `KeyEvent`, evdev) |
|---|---|
| `down`, `up` | The raw edges, by role |
| `press` | Down and up before the long-press time: fires on up (Android's `KeyEvent` up; the press is decided only when it ends) |
| `long-press` | Held past the long-press time: fires while down, once |
| `hold` | Held past the hold time (2 s): fires while down, once (the power menu) |
| `repeat` | While held, where the meaning repeats (focus, a value): `repeatCount`, at accelerating intervals |
| `chord` | Two or more held together (Main + Volume −): the system's combinations first |
| `step` | A dial or wheel turned: `steps` (± detents, from `value120`), `velocity` |
| `toggle` | A switch moved: its `position` |

A `press` never waits for a second press: there is no double or triple press on controls (T1-063). Where a control both repeats and long-presses, its context picks one meaning, never both: a volume button held in focus navigation repeats the focus step; held while it is the volume (sound playing, the display off), its long press changes the track (T1 controls spec §1.3).

### 3.4 Semantic actions

What most handlers want, whatever produced it. The shell maps controls, gestures and keys to these by context (the [T1 controls spec](https://github.com/opendap-project/t1/blob/main/docs/specs/controls-and-status-led.md), design system §8.4):

| Action | From (T1) | From a keyboard | From other hardware |
|---|---|---|---|
| `activate` | Action press with the focus shown; a tap | Enter, Space | A dial's push, a select button |
| `primary` | Action press with the focus hidden, where the screen has a primary action | — | A faceplate's primary button |
| `next`, `previous` | Volume − / + when no sound plays and nothing asks for the volume | Tab, Shift+Tab | A dial's step, without a lock |
| `increase`, `decrease` | Volume + / − on a locked control | → / ← on a focused slider or axis; ↑ / ↓ only while it is locked (they are the volume buttons by place, §3.5) | A dial's step on a locked control |
| `volume-up`, `volume-down` | Volume + / − while sound plays or the activity asks | Volume keys | A volume dial |
| `back` | The leading-edge swipe; the header's back | Escape | A back button |
| `cancel` | Leaving a lock or a system view without choosing | Escape | — |

A semantic action keeps its source in its payload, so a handler that must know (a slider showing whether a finger or a button moves it) can.

**Action, by the focus** (T1-061). The focus is always somewhere, and lands on the screen's primary action when the screen arrives, else on its first element (focus on entry, design system §8.4). So Action is the primary action by default. Once the buttons have shown the focus and moved it, Action takes the element it is on: the user chose it. With the focus hidden (after a touch), Action is the primary action where the screen declares one, and otherwise only shows where the focus is. A navigation key with the focus hidden only shows it, and moves nothing.

**With the display off** nothing reaches the focus, which no one sees: Action is play/pause and Volume ± the volume (T1 controls spec §1.3, T1-060).

### 3.5 Keyboards

A keyboard attached to the device is not a control: its keys are `key` domain events by name (`left`, `enter`) and text input goes through the input-method protocols (the on-screen keyboard included). ↑ ↓ on the simulator are the volume buttons by their place; ← → are a keyboard's arrows (design system §8.4).

### 3.6 What is not an input event

Sensors (ambient light, battery, temperature) are states with change notifications, read through their own interfaces (hardware-support §4), not input events. A microphone is an audio source.

### 3.7 Navigation without touch

The buttons give one axis: Volume moves, Action takes. One axis cannot walk a long page of nested content item by item, so navigation has **levels**, as a slider already has (design system §8.4): the focus moves among the stops of the current level; Action **engages** a stop to work inside it, one level down; leaving goes back up. Decided by Claude under the maintainers' delegation, best UX (2026-10-24), extending the slider's lock, which the maintainers decided, to every composite (tvOS focus sections, Android TV rows, ARIA composites with one tab stop, the Apple Watch's crown on a focused control).

**Roles.** Every node is one of these to navigation (`opendap-input::nav`):

| Role | Volume | Action | Escape | Examples |
|---|---|---|---|---|
| **Leaf** | moves | activates | back (or releases the level it is in) | button, line, toggle, tab, check line |
| **Adjustable** | moves; engaged: more (+) and less (−), held with acceleration | engages; engaged: releases | releases, then back | slider, a range's knob, an axis of stops, the seek bar |
| **Stop** | moves; engaged: walks its children | engages: the focus enters at its primary child, else its first | releases (the focus on the stop) | a horizontal shelf in a page, a player's transport, a card with its own actions |
| **Transparent** | — (its children join the level it is in) | — | — | stack, row, wrap, grid, adaptive pair, aspect box, group, a virtual list over its items |
| **Layer** | moves inside it; a modal wraps | — | closes it: the focus back to its opener | dialog, power menu (trap); popover, menu (no trap) |
| **None** | never focused | — | — | label, section label, sticky header, image, toast, the status bar |

**Rules.**

- **One meaning per stop.** Action on a stop does one thing: a line that opens something is a leaf, and its secondary actions come as a menu (long press), never as stops inside the line. What holds several controls is a stop, or transparent.
- **Out by the buttons alone.** Past either end of an engaged stop the focus comes back to the stop itself, released, and the next press continues outward: no Escape needed, since the T1 has none on its body. An adjustable is released by Action again.
- **← → at any time.** A keyboard's arrows change a focused adjustable without engaging it, and walk a stop's children from the side they point to (← enters at the far end); they mirror right to left, Volume never does.
- **What releases.** Action again, Escape, moving the focus away, a touch outside the engaged node (a touch inside keeps the levels it is in), the focus hiding, the display going off, a layer opening over it, the engaged node leaving the tree. The focus stays where the buttons can continue: on the stop, when it was inside one.
- **Layers.** Opening one releases the levels below, puts the focus on its primary action (else its first stop) and remembers the opener; closing it gives the focus back to the opener. A modal traps (Volume wraps, nothing outside is reachable); a popover does not wrap; toasts and the status bar never take the focus. The status panel is a modal.
- **Disabled is skipped** (maintainers, 2026-10-24, amending ADR 0003): it keeps its place and its reason shows, but neither the buttons nor a touch give it the focus, and a disabled container's children are skipped with it; a disabled primary action is not landed on at entry. An element disabled while it has the focus keeps it (Action does nothing) until the buttons move on from its place, as Android does.
- **Ends.** Volume past the first or the last element of a screen stays there (a boundary cue may show it); only a modal wraps. A list that takes the focus before any item has it shows the ring on its first item at the first press.
- **Order** is reading order through transparent containers: a row's children start to end, right to left mirrored; a grid row by row; Volume + is always previous and Volume − next, by place.
- **Revealing.** The focus moved is scrolled into view, innermost scroll area first, each one just enough, below what covers its edges (a sticky header at its start, the playing strip at its end), and the part shown is then revealed in its parent (`nested::reveal`; the browser's nested `scrollIntoView`). An element taller than the room shows its start.
- **Large and lazy content.** A virtual list is transparent over its items by key, never by slot: the buttons walk the whole list, not just what is built, and the focus follows its item when items arrive, move or go (`opendap-layout::focus`). A placeholder (data not arrived) takes the focus; Action on it waits for its data, with the loading shown, and moving the focus away gives the wait up (`opendap-layout::focus::Activation`).
- **Motion.** The focus moves at once (logically), the ring follows the element's settled place, and the reveal's scroll is animated and retargeted, never queued: a held repeat moves the focus at the repeat's rate and the scroll follows it; with reduced motion it jumps. An element on its way out is no longer reachable; one coming in is reachable at once.
- **Touch and the focus** (design system §8.4). A touch gives the touched element the focus, hidden, and keeps only the levels it is inside; the next button press shows the ring there.

**Combinations** (each one tried in the UI primitives activity, Navigation):

| Inside → | Leaf | Adjustable | Stop | Transparent | Virtual list | Layer |
|---|---|---|---|---|---|---|
| **Screen** | a stop of the screen | engage to adjust | engage to walk it | its children join | its items join, by key | opens over; focus back on close |
| **Stop** | a child | second level: engage inside the engaged stop | nested stop: a further level | children join the stop's level | a horizontal virtual shelf: its items, by key | opening releases the stop |
| **Transparent** | joins the level | joins | one stop of the level | flattens | items join | — |
| **Virtual list (row)** | the row is a leaf (one meaning) | a row of settings: the control is the stop | a row with its own actions is a stop | its children join | nested virtual lists are stops | opening releases |
| **Modal** | wraps | engage inside; Escape releases, then closes | engage inside | joins | items join, wrapping at the ends | stacked: the inner one closes first |
| **Disabled group** | skipped | skipped | skipped, not entered | skipped | skipped | — |

## 4. Handling

### 4.1 Dispatch order

1. **The system** first: the shell's system combinations, system views (the power menu), the debug UI mode's references (which observe and never consume). What the system takes, no activity sees; a system view's taking over sends `pointercancel` to the activity.
2. **The activity on screen**: focus-targeted events (controls, keys, semantic actions) go to the focused element; pointer events to the element under the pointer, or to the element that captured it.
3. **Inside the activity**, W3C phases: **capture** from the root down to the target, **target**, then **bubble** back up. A handler returns handled or not; an unhandled event continues.

### 4.2 Default actions

Every event that has a default does it unless a handler prevents it: Action's default follows the focus (§3.4: the primary action by default, the chosen element once the focus is shown and moved); Volume's default is the volume or the focus (§3.4); a tap's default is the element's activation; an edge swipe's default is the system's navigation (which an activity can never prevent).

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
| `axis`, `start`, `bounds`, `snap`, `targets` | A drag's or a pan's shape (§3.2) |

### 5.1 Implicit defaults

Good UX needs no directive written; these are the defaults, and a directive only changes one:

- **Activation is guarded.** An `activate` or `primary` whose handler starts work (a request, a change of hardware, a navigation) does not fire again until that work ends: no double submission, no repeated reboot. Fast repeated taps on an element that starts no work (a skip, a stepper, a key of the keyboard) all count, as on every platform.
- **A long press is not also a tap,** and a contact that scrolled or dragged is not a tap either (§3.2).
- **The system taking over cancels.** A system view opening, a system combination starting or the display going off sends `cancel` to every contact and releases every key the activity holds; repeats stop (W3C `pointercancel`, Wayland `wl_touch.cancel`).
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

### 6.1 Coordinate spaces

**Every coordinate is relative** (maintainers, 2026-10-23): there is no absolute position, only frames, each placed in its parent by an offset, a scale and a direction. Even the screen is one frame among them, the outermost that software sees: below it are the touch digitizer's raw axes (0 to 32767 on the virtual board), mapped to the display by calibration and rotation. A position always says which frame it is in, and converting between two frames walks the tree through their common ancestor (iOS's `UICoordinateSpace`, Compose's `LayoutCoordinates`, Flutter's `PointerEvent.transform`); `opendap-input`'s `Frame` and `FramePath` implement it.

```text
digitizer (raw axes) → display (calibration, rotation) → screen → content-area → surface
        → content (a scroll area) → named frames (groups, regions) → local (the element)
```

Each frame also carries its own regions (§3.2.1): edges and corners by the zone widths it declares, mirrored right to left where its direction says so.

Positions are `f32` logical pixels, never device pixels, and never rounded before they are drawn. A design-pixel size converts through the display's `unit` (design system §18.2). An activity never receives screen coordinates: as in Wayland, its positions are relative to its own surface (which also keeps where other things are on screen to the system).

| Space | Origin | Who sees it |
|---|---|---|
| `digitizer`, `display` | The hardware's axes; the panel after calibration and rotation | The input stack and the board description only |
| `screen` | The display's top left, the full display: the outermost frame software uses | The shell and the debug UI mode only |
| `content-area` | The shell's viewport for activities: from the status bar's bottom line to the screen's bottom edge, the screen minus the system's insets (§3.2.1) | The shell; an activity gets only its insets, as sizes |
| `surface` | The activity's surface, from the status bar's bottom line down; its content may be taller than the content area and scroll inside it, as its own viewport and content (§6.2). Today the surface still covers the whole screen and leaves the status bar's 28 px clear itself, and a contact that starts in the status bar is the system's and never reaches it; once the shell places it, its origin is the status bar's bottom line | Every event the activity receives |
| `content` | A scroll container's content, scroll offset included | Handlers inside a scroll area: a position that stays with the content when it scrolls |
| `local` | The element handling the event, after its container's offset, scale, scroll and right-to-left mirroring | Every handler: re-based at each step of capture and bubble (Flutter's `PointerEvent.transform`, Android's `MotionEvent.transform`) |
| A named container | An ancestor that names its space (SwiftUI's `coordinateSpace(name:)`): a group, a region | Handlers that position relative to a group |

Every pointer and gesture event carries, eagerly:

| Field | Space | Is |
|---|---|---|
| `position` | local | Where the contact is now |
| `surfacePosition` | surface | The same point, stable through the dispatch (to keep across a scroll, then convert again) |
| `pressPosition` | local | Where the contact went down (Qt), so a handler needs no state for "since the start" |
| `delta` | local | `position` − the previous position: derived from positions, never summed, so it cannot drift (Compose) |
| `translation` | local | `position` − `pressPosition` (SwiftUI) |
| `axisDelta` | the locked axis | A pan's or drag's delta along its axis once it is known (Flutter's `primaryDelta`) |
| `velocity` | local, px/s | Fitted over the last 100 ms of samples, not the last delta (Android's `VelocityTracker`) |
| `predictedEnd` | local | Where a fling would stop (SwiftUI, iOS): for snapping and dismissing |
| `region` | the recognizing level's zones | The region it started in, in the box of the level that recognized it (§3.2.1): the screen's for the system, the surface's or an element's for the activity |
| `coalesced` | as above | The samples between two frames, for smooth scrubbing (W3C `getCoalescedEvents`) |

And on demand, in the space the caller names (iOS's `location(in:)`, Compose's `localPositionOf`):

| Helper | Gives |
|---|---|
| `convert(point, from, to)` | A point in another space |
| `contains(element)` | Whether the point is inside an element |
| `regionOf(element, zone)` | The element's own region of the point: its start, end, top or bottom edge zone, a corner, or its content (a sheet's edge, a resize handle) |
| `normalised(element)` | 0 to 1 along the element, start to end (mirrored right to left; transport, the seek bar and the volume stay physical, design system §9.5) |
| `polar(element, centre)` | Angle (0 up, clockwise), radius and angle delta unwrapped across ±180°, for dials |

### 6.2 Scrolling: the viewport and the content

A scroll container is two frames, and with scrolling the difference matters most (maintainers, 2026-10-23): its **viewport** (its box, which stays with the screen) and its **content** (placed at minus the scroll offset, which moves under the viewport, vertically, horizontally or both). The same contact has a position in each, and they drift apart by exactly the scroll that happens during it.

| Use | Frame | Why |
|---|---|---|
| A pan that drives the scroll itself | viewport | In the content, the finger stands still while the content follows it: its delta would be zero, or feed back into the scroll |
| Where a tap, a long press or a drop lands (which row, which slot) | content | The row is the same whether the list is scrolled or not |
| A drag that carries an item while the list auto-scrolls under it (reordering) | both: the item follows the finger in the viewport; its slot is computed in the content | The list moves under a still finger, and the slot changes with it |
| A point kept for later (an anchor, a selection's start, a pending drop) | content | A viewport point means something else after any scroll; convert again from `surfacePosition` when needed |
| Regions: a scroll area's edges (auto-scroll zones, a fade, an edge gesture inside it) | viewport | The content's edges are off screen most of the time |
| The focused element kept in view (`FocusReveal`) | content to viewport | Scroll just enough to bring its content box inside the viewport |

- **Nested scrolling** (horizontal inside vertical: a row of covers in a page). Each container has its own viewport and content frames; the `dominant` axis directive (§3.2) gives the contact to one of them at the slop, and an inner container that reaches its end hands the rest of the motion to the outer one (ui-model §7).
- **Payloads in a scroll area** carry `contentPosition` beside `position`, and `scrollOffset` at the time of the event, so a handler never mixes the two.
- **During momentum** (after a fling) the content keeps moving with no contact: positions computed earlier in the content stay right; those computed in the viewport do not.
- **Right to left:** a horizontal scroll's offset counts from the start edge, so it mirrors with the layout (design system §9.5).

**Avoid:** summing deltas into positions; keeping a local point after the content scrolled; mixing spaces in one computation; device pixels in a payload; DOM's legacy `layerX` and `movementX` (units that differ by browser).

### 6.3 Utilities for developers

Primitives first: what any handler can ask of a pointer, as plain functions over frames, with no activity or component runtime behind them (`opendap-input`'s `space`). The activity system and the components will use the same primitives when they come.

| Utility | Answers | In |
|---|---|---|
| `Track::position(path)`, `press_position(path)` | Where the contact is, where it went down | Any frame |
| `Track::delta(before, now)`, `translation(at_press, now)` | The last move, the whole move; with a frame's path at each time, so motion relative to a frame that moves itself (a list scrolling under a still finger) is right | Any frame, moving or not |
| `Track::velocity(path)`, `predicted_end(path, deceleration)` | Speed over the last 100 ms; where a fling stops | Any frame |
| `Track::region(path)`, `inside(path)` | The frame's own region of the contact (its edges and corners, never the screen's); whether it is inside | The innermost frame of a path |
| `normalised(frame, point)` | 0 to 1 along the frame, start to end, mirrored right to left (the physical form for transport, the seek bar, the volume) | One frame |
| `polar(centre, point)`, `angle_delta(from, to)` | Angle (0 up, clockwise) and radius; turns unwrapped across 0°/360° | One frame |
| `Spaces::path(space)` | A shared space by name: `Surface`, `Viewport`, `Content` (and `Screen`, `ContentArea` for the shell only) | Singletons, not nodes |
| `FramePath::convert`, `NodePath::convert` | A point of one frame or node, in another, through their common ancestor | Any two of one tree |
| `Gesture::describe_at(level)` | A gesture's name at its level: the system's meanings only at the screen level (§3.2.1) | Debugging, tests |

### 6.4 Identity and paths

Every node an event goes through has a kind and a name, and an address from the outermost node down to it, so a handler, a test, a tool or the debug UI mode can name it, select it and convert between it and any other node (`opendap-input`'s `node`). W3C's `composedPath`, the accessibility tree's ancestry and test identifiers do the same.

- **Kinds:** `activity` (the outermost), `surface`, `layer`, `group`, `region` (a named area that is not an element), `container` (a viewport over content), `element`.
- **Names:** stable across frames and data changes (an item's own key, never its index), unique among siblings: `element:track-12`.
- **Paths:** `activity:music/layer:content/container:queue/element:track-12`. A path carries each node's frame, so a point converts down and up it, and two paths meet at their common ancestor.
- **Selectors:** `kind:name`, `kind:*`, a name prefix `element:track-*`, `*` for any node, `**` for any depth: `activity:music/**/element:track-*` is every track.
- **Shared spaces** are singletons with reserved names, never nodes: `@screen`, `@content-area`, `@surface`, `@viewport`, `@content`.
- An event's payload carries its `target` path and the path it travels (capture down it, bubble up it, §4.1).
- **Extensible.** These are the primitives; whoever builds on them names their own instances and singletons. The shell, built on this compositor, will register its parts (`@status-bar`, `@status-panel`, `@power-menu`), and its events then come in that domain's language ("a swipe down from `@status-bar`", not "from the top zone"); an activity names its own nodes the same way. Names follow the user-facing conventions (§9 there).

### 6.5 The catalogue, by layer

Every primitive, directive and modifier the compositor's domain needs, layer by layer, so the shell and then each activity built with the SDK extend it rather than invent beside it (maintainers, 2026-10-24: primitives first, across every layer). **Built** is in `opendap-input` with tests; **Specified** is defined here and comes with the layer that needs it. The lessons from SwiftUI, Flutter and Compose are named where they apply.

| Layer | Primitives | Directives and modifiers | State |
|---|---|---|---|
| Raw (§2) | `key`, `contact`, `motion`, `axis`, `switch` | Normalized units and time only | Shell (libinput) |
| Pointer (§3.1) | `pointerdown/move/up/cancel`, capture | One stream per pointer; exactly one `up` or `cancel` (every framework's guarantee: no stuck state) | Shell; `Contact` |
| Gestures (§3.2) | tap, long press, swipe, edge swipe, pan, drag and drop | `axis` (free, horizontal, vertical, dominant), `start` (long press, immediate) | Built |
| | double tap, pinch, rotate | `count(n)` (double tap, opt-in); `bounds`, `snap`, `targets` | Built (`arena`, `multi`, `drag`) |
| Composition of gestures | sequenced (a long press, then a drag), simultaneous, exclusive (first listed wins) | priority: normal, simultaneous (SwiftUI); an arena where competing recognizers resolve one owner (Flutter, §3.2.1) | Built (`arena`), tried in the UI primitives activity (Arena) |
| Gesture state | a value scoped to one gesture, reset on its end and on cancel (SwiftUI's `@GestureState`) | none | Built (`arena::GestureState`) |
| Hit testing | a frame's region, containment | behaviour per area: defer to children, opaque, translucent (Flutter's `HitTestBehavior`); `hit: self, through, none` (ui-model §9) | Built (`hit`) |
| Nested scrolling | a container hands the motion it leaves to its parent | pre-scroll, post-scroll, pre-fling, post-fling (Compose's `nestedScroll`); an inner control that declared its drag keeps it | Built (`nested`), tried in the UI primitives activity (Arena); Slint's `Flickable` takes any drag past its threshold and no control can keep it (a gap, ui-model §6.3), so a `Scroll` component drives its content with these primitives |
| Controls (§3.3) | press, long press, hold, repeat | kind: press only, long press, repeat | Built (`control`) |
| Semantics (§3.4) | activate, primary, next, previous, increase, decrease, volume, play/pause, tracks | context: display, volume wanted, locked, focus shown, primary declared | Built (`semantic`) |
| Intents and actions | an intent (what is meant) and the actions that handle it, found up the path from the focus; a disabled action lets it pass on (Flutter's `Actions`/`Intents`) | bindings: system, then activity, then layer; the same actions serve the screen reader (Compose) | Resolution built (`dispatch::resolve`); bindings come with the activity runtime |
| Frames and spaces (§6.1–6.3) | `Frame`, `FramePath`, `Track`, shared spaces | position, delta, translation (frames that move included), velocity, predicted end, region, normalised, polar | Built (`gesture`, `space`) |
| | named spaces (`Space::named("queue")`, SwiftUI's `coordinateSpace`), any node as a space, opaque anchors resolved in the current layout (SwiftUI's `Anchor`), a follower that tracks a moving target (Flutter's `LayerLink`) | `place(anchor, size, side, gap, bounds)` with a flip and a slide inside (CSS anchor positioning) | Named spaces and anchored placement built (`space`, `anchor`); opaque anchors and followers specified |
| Identity (§6.4) | `NodeId` (unique among siblings, from the data, never an index), `NodePath`, selectors, reserved singletons | `test_id` apart from the accessible label; scoped anchors (`@Namespace`) for pairs across the tree, never global keys | Ids, paths, selectors, `test_id` built; scoped anchors specified |
| Dispatch (§4) | capture, target, bubble along the composed path | `once`, `stop`, `prevent`, `capture`, `self`, `passive`, `throttle`, `debounce`, `repeat(off)`, `hold(ms)` | Routing built (`dispatch`: phases and every directive); the protocol to activities comes with their runtime |
| Focus (design system §8.4) | focus on entry (the primary action, else the first element), focus shown or hidden, tap moves it | a focus scope: default focus, restore the last focused (falling back to the default when it is gone), trap (modals), traversal order (Flutter's `FocusScopeNode`, Compose's `focusRestorer`) | Scopes built (`focus`); entry and visibility built in the components |
| Navigation (§3.7) | levels: move, engage, release; roles leaf, adjustable, stop, transparent, layer, none | trap, opener restore, release on touch, focus hidden, display off, layer open or node gone; reveal innermost first with insets; ← → walk without engaging | Built (`nav`, `nested::reveal`, `opendap-layout::focus::Activation`); in the components: the slider's lock, modals, the screen's ends, disabled skipped; stops (a virtual shelf of 10 000 albums, nav for its levels and opendap-layout for its items; a player with a seek inside) tried in the UI primitives activity (Navigation); the reveal's scroll animated and retargeted, below a pinned section header in the virtual list (Virtual) |
| Testing | a virtual clock (every primitive takes time as an input), synthetic contacts | a harness with finders by path, role and label; a recorder whose recordings replay through the same code as live input, so a device's bug becomes a test | Built (`harness`: clock, scripts, readable recordings, `play`); finders by path come with the runtime |
| Debugging | gestures named at their level, the debug UI mode | none | Built |

**Extension, layer by layer.** The compositor's domain is these primitives. The shell registers its own nodes and singletons on them (`@status-bar`, `@status-panel`, `@power-menu`) and speaks of its events in that language; each activity, through the SDK, names its own nodes, binds its intents and declares its directives. No layer adds a parallel mechanism.

## 7. Thresholds

Defaults, in design pixels (px, as the design system writes them) and milliseconds; all adjustable (SYS-066), and lengthened, never shortened, by accessibility settings:

| Threshold | Default | From |
|---|---|---|
| Touch slop | 10 px | Android's 8 dp, at the T1's reading distance |
| Long press | 500 ms | T1 controls spec §1.1; proposed (Android's default has been 400 ms since Android 12) |
| Hold | 2 s | T1 controls spec §1.1 |
| Double-tap | 300 ms | Android `DOUBLE_TAP_TIMEOUT` |
| Fling velocity | 50 px/s minimum, over the last 100 ms | Android `MINIMUM_FLING_VELOCITY`, `VelocityTracker` |
| Edge zone | 20 px at the sides and the bottom; the status bar's 28 px at the top | Design system §8.5 |
| Keyboard repeat | 600 ms, then 25 per second; modifiers never repeat | X11's and GNOME's defaults |
| Repeat | 400 ms, then 250 ms × 0.8 down to 50 ms; it stops on release, with no inertia | Shell spec §2 (maintainers) |

## 8. Where it lives

- **`opendap-input`** (the crate, formerly `opendap-gesture`): `gesture` (the recognizers, regions, frames and composite gestures, §3.2, §6.1), `control` (press, long press, hold and repeat as a pure state machine, §3.3), `semantic` (roles and context to semantic actions, §3.4), `nav` (navigation without touch: levels, stops, layers, §3.7) and `thresholds` (§7), platform-agnostic, used by the shell, the UI primitives activity, and later the CLI and OpenDAP Desktop. The device service `odap-input` (OS architecture) will run it; the crate and the service follow the workspace's naming (`opendap-*` crates, `odap-*` binaries). A drag's `bounds`, `snap` and `targets` belong to the element that declares them (Blackbird UI).
- **The shell** turns raw events into domain events and dispatches them (§4).
- **Activities** receive them today through Wayland's standard input (touch, pointer, and the keys of focus navigation, shell spec §2); the full domain events, semantic actions and directives need a protocol of their own, opened when the activity runtime is (ADR 0002).
- **The CLI and the simulator** inject raw events (`opendap input send`), so they test the same pipeline a hand does.

## Open questions

- The protocol that carries domain events and directives to activities.
- Whether `double-tap` is available to third-party activities, or first-party only.
- Pen and pressure: no OpenDAP hardware has them yet; kept for any that will.
