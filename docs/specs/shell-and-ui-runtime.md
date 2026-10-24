# Shell and UI runtime

- **Status:** Draft, 2026-10-23. Decided by Claude under the maintainers' delegation; the decisions that refine ADR 0002 are in [ADR 0012](../adr/0012-shell-process-model-and-system-layers.md). Values are gates to measure on the T1.
- **Builds on:**
  - [ADR 0002](../adr/0002-system-architecture.md): `odap-shell` on Smithay, activities, the freezer;
  - [ADR 0005](../adr/0005-sdk-defined-services-components-and-schemas.md) and [ADR 0006](../adr/0006-slint-as-host-renderer.md): Slint behind a renderer interface;
  - [ADR 0010](../adr/0010-data-before-ui.md): data before UI;
  - the [system architecture spec](os-architecture.md);
  - the [design system](https://github.com/opendap-project/blackbird-ui/blob/main/docs/specs/design-system.md) and the [interface mockups](https://github.com/opendap-project/blackbird-ui/blob/main/renders/web/ui-mockups.html).
- **Hardware:** nothing here assumes one board (SYS-067, SYS-068). Display controllers, planes, panels and controls come from the merged hardware record ([hardware support spec](hardware-support.md)). The T1 reference board's values appear only as examples.
- **Benchmark:** [compositor and UI runtime research](../research/2026-10-compositor-ui-runtime.md). Facts it relies on, for the T1 reference board:
  - Smithay 0.7.0, with master carrying the presentation, fifo and input-method protocols;
  - only three usable VOP2 windows on the RK3566 in mainline (Smart0, Esmart0, Cluster0);
  - the panel in DSI video mode in the mainline ILI9488 driver;
  - Slint's FemtoVG renderer redraws the whole window, and its Skia renderer can redraw partially.

## 1. Summary

1. **`odap-shell` is the instrument's frame.** It owns the screen's spatial model, the transitions between places, edge gestures, every system layer, focus, lock and power of the display. It draws the status bar and the status panel that unfolds from it, the volume beside its keys, power beside Main, consent, pairing, toasts and the keyboard's placement.
2. **One product host, separate HAT hosts** (ADR 0012).
   - Now Playing, Library, Sound and This T1 run in one first-party process, `odap-product`, so moving between them is one app moving, with shared elements (the playing strip, the artwork).
   - Each HAT mode activity keeps its own process.
   - Third-party code never runs in either, because contributions are data (ADR 0010).
3. **Nothing changes, nothing is drawn.** No damage means no frame. At most one frame callback per refresh, none to suspended or frozen surfaces, and the foreground surface scanned out directly when it covers the screen.
4. **Consent cannot be spoofed.** Consent and pairing are drawn only by the shell, above every client, with input to them only from real touches and the physical buttons. The second press of Main confirms privileged actions.
5. **What the shell draws is accessible too.** The shell publishes the semantic tree of its layers to `odap-voice`, the same node model as AT-SPI. Recovery speaks its few states from the buttons.

## 2. `odap-shell`

| Owns | How |
|---|---|
| Display | Smithay `DrmCompositor` on the board's display CRTC, `GlesRenderer` on its GPU (Panfrost on the T1 reference board), session through libseat on logind. Every display the hardware record lists is driven; no resolution, density, shape or orientation is assumed |
| Spatial model | The places and how they relate: Now Playing in the centre, Library below, Queue to the trailing side, More below the player, Status above. The arrangement adapts to the layout class (portrait, landscape, round, no display: design system §18.2). The product host draws inside a place; the shell moves between places |
| Gestures | Recognises swipes that start in the 20 px edge zones (design system §8.5), minus the exclusion rectangles each surface declares (seek bar, A–Z index). Between places it moves buffers itself; within a place it forwards `gesture(kind, phase, progress, velocity)` to the host |
| System layers | Status bar and status panel; the volume level beside the volume controls and power beside the `main` control, wherever the hardware record places them (on screen when a role has no physical control); consent; pairing code and confirmation; toasts with Undo; the keyboard's layer (`odap-keyboard`, text-input and input-method protocols) |
| Input | Touch through libinput; buttons and faceplate HID as semantic actions from `odap-input`; adjustable press, hold and repeat timings (SYS-066) |
| Lifecycle hints | `odap-activity-v1`: activate, background and suspended, plus gestures and element rectangles, to hosts and to the orchestrator |
| Display power | Screen off and on, DPMS, idle, lock; the screen-off playback path of research §5.13 |

**Protocols.** The core, plus:

- `xdg-shell` with `suspended`;
- `linux-dmabuf` with feedback and explicit modifiers, which some display controllers require (the T1 reference board's VOP2 does);
- `wp_presentation`, `fifo-v1`, `viewporter`, `single-pixel-buffer`;
- `text-input-v3`, `input-method-v2`, a filtered `virtual-keyboard-v1`;
- `wlr-layer-shell`, filtered to `odap-keyboard` only;
- `wp_security_context_v1`;
- the private `odap-activity-v1`.

Not offered: fractional scale, decorations, foreign-toplevel management, idle inhibition for clients.

**The system control, `main`** (T1 controls spec §1.3). The shell owns it, and logind handles none of its presses:

- **Short press:** the display off (DPMS off, no planes, no frame callbacks, so clients stop drawing) or on again with a full frame. With the display off the screen takes no touch; the buttons still act.
- **Long press (500 ms):** the power menu.
- **Main + Volume −:** short, a screenshot; held 2 s, the power menu, also with the display off.
- **The power menu** is the first system layer: Slint (`ui/layers.slint`, Blackbird UI's tokens and fonts) drawn by the software renderer into a buffer the compositor places above every client, with glyphs embedded at build time. It follows the mockup "Power, beside Main":
  - the choices beside `main`, at the height the device description gives it;
  - Power off, then Restart, then Restart to recovery only if the device has that mode;
  - Cancel at the bottom.
  - The choices run the full width, without dividers, their labels at the right, beside Main.
  - It is a system view (SYS-069): it precedes the shell and takes the whole screen, with no status bar or other shell layer showing.
- **Buttons alone** (SYS-039, as recovery menus work). Volume + and − move a visible focus (design system: a `focus` border) through the choices and Cancel, wrapping around. Action takes the focused choice. Main powers off.
- **Confirmation.** Pressing `main` again powers off. Touching "Power off" only points at that press (§7: power requires it). Restart and recovery are touch choices. While the layer is up, no client receives touch or the pointer.
- **The power service.** The shell's user cannot power off or pick a boot mode, so it asks blackbird-os's `odap-power.socket` (root, the `odap` group only, `odap-reboot --serve`). It needs no polkit, and stands in until the orchestrator's `org.opendap.system.Power`.
- **Screenshot** (Main + Volume −, short). The shell draws what the display shows into an offscreen buffer, without the touch indicator, and saves it as `~/Pictures/Screenshots/Screenshot_YYYYMMDD-HHMMSS.png`, Android's path and name (UTC).
  - A toast says "Screenshot saved": a short bar that leaves by itself after 2 s (user-facing conventions: Material's snackbar). The design system forbids a full-screen flash (§11).
  - With the display off, it is taken when the display is next on.
- Every other control passes through as a role.
- Restarting into a boot mode and powering off are direct requests too, not imitations of these combinations: blackbird-os `odap-reboot [MODE]`, used by the `opendap` CLI (`device reboot --mode`, `device poweroff`) and by `odap-vdev reboot --mode`. The combinations stay testable with `odap-vdev input press main volume-down --hold MS`.
- **Forced power-off:** the PMIC's, in hardware, whatever the OS does. On the virtual board, `odap-vdev input press main --hold` past the board's PMIC time (8 s) cuts the power.

Verified on the virtual T1: off and on by both of `main`'s codes; a tap with the display off reaches no client; a long press leaves the display on; 1 wakeup in 70 s with the display off (the product's minute timer); after the PMIC's cut, the next boot recovers the file system from its journal.

**Debug UI mode** (SYS-070). The real interface, drawn as a blueprint at run time, off by default.

- **On and off.** `ODAP_DEBUG_UI=1` for the shell (the virtual board sets it), or while `/run/opendap/debug-ui` exists, read again on each input. Applications read the same flag.
- **Two parts.** The shell draws its references in a layer above everything that takes no input: the regions (status bar 28, side margins 16, edge zones 20, the thumb's lower half), a grid, the physical controls where the description places them, a crosshair with coordinates while a finger is down, and the gesture recognised (`opendap-gesture`) in a callout for 2 s. Applications draw their own elements as blueprints through Blackbird UI's components (blackbird-ui ADR 0002).
- **One semantics.** Structure in ink hairlines; interactive areas, physical controls included, in `state-success`; live input (pressed, held) in `brand-base`; the shell's regions in `state-info`; margins dotted in `state-warning`; edge zones in `state-processing`. A physical control is an interactive area for the OS: held, its mark turns live, its geometry unchanged.
- **Calibration.** Every measure the specs give in design pixels is drawn at its physical size: multiplied by the hardware's exact scale (density and reading distance, design system §18.2) over the applied one.
- **Cost.** Off, nothing. On, the layer redraws only on input; a static screen keeps 0 wakeups.

**UI primitives activity** (SYS-071, system `odap-testbed`). A developer activity included in the OS, separate from the debug UI mode: calibration, targets, a gesture pad, controls (slider, toggles, a button group), motion, opacity and the host's failures, one screen each, built from Blackbird UI's components. Until the orchestrator's activity launcher, the shell starts it in place of the product with its activity flag: `ODAP_ACTIVITY=ui-primitives`, or that name in `/run/opendap/activity`, then a restart of `odap-shell.service`.

**Touch indicator** (a developer option, in the manner of Android's "show taps"). The shell draws a bubble above everything, so a person or a recording can follow each touch:

- **Pointer.** An absolute pointer (the simulator window, a USB tablet or mouse) acts as one finger, and clients get no cursor. The bubble is a ring that follows the pointer.
  - A contact shrinks and fills it (`motion.duration.fast`).
  - A release ripples it out and fades it (`motion.duration.standard`).
  - Once the pointer rests 1.5 s, the bubble fades out, and it returns on the next movement. The virtual board's window reports no pointer leaving it, so this also takes the bubble away when the pointer leaves the screen.
- **Fingers.** Each finger shows the same bubble while it touches. The bubble goes with the finger once its ripple ends.
- **On and off.** It is on when the board support package sets `ODAP_SHOW_TOUCHES=1` for the shell (the virtual board does), or while `/run/opendap/show-touches` exists: a running device can be switched for debugging.
- **Cost.** Off, it draws nothing. Idle, it asks for no frames: the fade uses one timer after the last movement, and a static screen keeps 0 wakeups (§5.1).

**Smithay version.** A pinned revision of master until a release carries these protocols, listed in the build's lock and upgraded deliberately.

## 3. Process model

| Process | Draws | Why separate |
|---|---|---|
| `odap-shell` | The frame and system layers, `status/lines` contributions through the fixed `status-line` template only | Trust: consent and the power path must not depend on any client |
| `odap-product` | Now Playing, Library, Sound, This T1, with their slots' templates | One product: shared elements and instant moves between places; no third-party code inside (ADR 0010) |
| HAT mode hosts | One per attached HAT's mode activity | Isolation of a HAT's own screen; frozen or destroyed when not shown |
| `odap-keyboard` | The on-screen keyboard (SYS-021) | As in the text input research |
| Recovery | The recovery and update UI on Slint `linuxkms` with the software renderer, no compositor, no Mesa | Works when the main system does not |

**Lifecycle inside `odap-product`.** Places that are not visible stop rendering and release their caches down to their budgets. The process is frozen only when the screen is off or a HAT mode is in front, so returning is instant. Background work belongs to services, never to the host (ADR 0002).

**Gate.** Measure the product host against separate hosts on the same screens (research §9.g):

- RSS, PSS and GPU memory;
- cold start to first frame, and thaw to first fresh frame;
- idle wakeups.

ADR 0012 records the switch back to one process per place if the shared host ever costs more than it saves.

## 4. Moving between places

1. Input goes to the destination at once, so the finger never waits for frames.
2. A frozen destination is thawed and resumed.
3. The shell animates both buffers with the motion tokens (design system §11), starting from the destination's last buffer if it has no fresh frame yet, and swaps fresh frames in as they come.
4. The source is told it is suspended when it leaves.

Within `odap-product`, moves between places are ordinary in-process animations, so shared elements (artwork, title, the playing strip) move as one. Reduced motion uses the same path with zero duration.

## 5. Render loop and power

- **No damage, no frame.** A static screen costs no GPU work and no wakeups from the shell or the host.
- **One frame callback per refresh** to the visible surface; none to suspended or frozen ones; animations are paced by presentation feedback.
- **Planes.** The full-screen product surface goes to direct scanout when nothing covers it, on whichever plane the display controller offers for its format. Shell chrome is composited at first, and moves to an overlay plane only if measurements show compositing wakes the GPU often. Plane counts and formats come from the hardware: on the T1 reference board, three VOP2 windows are usable (Smart0 linear, Cluster0 AFBC, Esmart0 also YUV).
- **Panel.** The panel's mode is its driver's. On the T1 reference board it is DSI video mode, as the mainline ILI9488 driver sets it: the controller scans the panel at 60 Hz whatever the content.
  - Command mode with the panel's own memory, and the TE line the faceplate already routes, could let the display pipeline idle on static screens.
  - It is a candidate optimisation, carried as a driver patch only if power measurements justify it (OS-023).
- **Renderer.** FemtoVG redraws the full window; Skia can redraw only what changed. Both are measured (ADR 0006 gate). The cost of a full redraw at 320 × 480 is small, but its GPU wakeups are what matter on battery.
- **Lists.** Slint's `ListView` instantiates only visible rows. Library lists use paged models over `odap-library`'s cursors (SYS-033).

### 5.1 Gates

| Gate | Target |
|---|---|
| Static screen | 0 wakeups per second from the shell and the foreground host over 60 s; 0 from frozen hosts |
| Scrolling a 1,000,000-item list | 60 fps without dropped-frame bursts; p99 client render ≤ 8 ms |
| Touch to photon | ≤ 50 ms, measured with a camera or photodiode (experience budgets, T1-057); a move between places starts within one refresh |
| Screen on to interactive | ≤ 150 ms, including the thaw of the product host |
| Direct scanout | Seen as zero-copy in `wp_presentation` and in the plane state during full-screen scrolling |
| Endurance | 72 h: flat RSS, GPU memory and frame times in every process (T1-047) |

## 6. What hosts cache

Bounded, each with a byte cap in its manifest (A6 in the system architecture spec):

- compiled template definitions per id, version, input profile and text direction;
- resolved slots and the last data of each contribution;
- artwork textures at display size, LRU by bytes, decoded off the UI thread;
- shaped text for visible and nearby rows;
- the paging window of library cursors.

## 7. Trusted layers

- Consent, pairing, power and the volume overlay are drawn by `odap-shell` only, always above every client. No client gets the layer-shell protocol except `odap-keyboard`.
- While a consent or pairing layer is up, input to it comes only from real touches and the physical buttons. Synthetic input (virtual keyboard, injected events) never confirms. Privileged confirmations also accept, and for power require, a second press of Main (SYS-050, ADR 0009).
- No surface may cover or obscure these layers, as Android blocks touches through untrusted overlays.

## 8. Accessibility of what the shell draws

- `odap-product` and HAT hosts expose their trees through AccessKit on winit to AT-SPI (research §5.7).
- `odap-shell` publishes the semantic tree of its own layers (status bar and panel, volume, power, consent, pairing, toasts) on the bus, as `org.opendap.a11y.Tree`. It uses the same node model (role, name, state, value, actions), and `odap-voice` merges it with AT-SPI. One voice reads the whole screen (SYS-062).
- **Recovery** speaks its few states (recovery mode, flashing, done, failed, the button to press next) with fixed phrases through the same speech engine, if the recovery image carries it. Otherwise earcons with a documented meaning. Its buttons need no touch.

## 9. Boot

- **No Plymouth:** it has no documented handover to a compositor like `odap-shell`.
- No consoles or text at boot (blackbird-os OS-035): the panel shows only the splash screen, then the UI that boot needs. Consoles exist only in the debug boot mode (`blackbird.debug`).
- U-Boot shows the static logo on the panel if its DSI support allows (**UNVERIFIED** for this panel in U-Boot).
- `odap-boot` is the boot UI: started by udev as soon as the panel's DRM device appears, it draws the splash screen (Blackbird UI's vertical lockup, design system §15.4) into a dumb buffer. It is also where the boot-mode menu and updates that block normal use (those needing a reboot) belong; ordinary OTA updates are checked at start and installed in the background, without it.
- Handover: `odap-shell.service` runs `odap-boot handoff` before it starts; the boot UI drops DRM master without blanking, and the shell's first frame draws the same splash in the same place. The shell keeps it until 5 s after it first appeared (`opendap-splash::MINIMUM_MS`, a deliberate minimum so the listener sees it), then fades over 320 ms to Now Playing, resumed where the listener left it (T1-051).
- A restart of the shell (an update, a crash) is covered by the boot UI too, without restarting the minimum (the time it first showed stays in `/run/opendap/splash-shown`), and never during a shutdown.
  - Orderly stop, with no blank frame: before systemd asks the shell to stop, `odap-boot cover` starts `odap-boot-cover.service`, a unit with no ordering against the shell, so it starts at once. It waits until that boot UI is armed (`/run/opendap/boot-armed`: display device open, splash screen ready). On SIGTERM the shell drops DRM master with its last frame still on the display, waits until the splash screen replaces it (`/run/opendap/boot-on-screen`, at most 1 s), then exits. The next shell takes over through the usual hand-over.
  - After a crash, the same command lets the boot UI take the display the crashed shell left.
- Until the orchestrator starts activities (ADR 0002), the shell supervises the product host. When the host exits, the shell restarts it: 250 ms after the first exit, doubling up to 8 s on repeated failures, back to 250 ms once a host has stayed up 30 s. Meanwhile the display keeps the last frame; it does not go black.
- Open: what the screen shows if the product host keeps failing. Today the last frame stays; it should become the recovery UI's state.
- Verified on the virtual T1:
  - Cold boot: black until the splash screen at 1.96 s, the UI once the 5 s minimum has passed.
  - `kill -9` of the product host: the last frame stays and the new host draws within 0.5 s, with the same shell process.
  - `systemctl restart odap-shell`: the last frame, then the splash screen with no blank frame in between (31 captures back to back), then the UI, with no second minimum.
  - `kill -9` of the shell: the boot UI's splash screen, then the next shell.
- Screen on to interactive stays within the gate of §5.1.

## 10. Verification additions

Added to research §5.16:

| Check | Level |
|---|---|
| Redraw idleness and frame-callback throttling, with a wakeup counter per unit | L2 ratio, L3 absolute |
| Plane assignment and zero-copy within the board's plane limits (three VOP2 windows on the T1 reference board) | L3 on each board (QEMU's virtio-gpu has no hardware planes) |
| The same flows on the test fixtures of the hardware support spec §8 (no display, round, landscape, monochrome) | L2 |
| Consent spoofing: a HAT host tries to draw above, inject into or obscure a consent layer; synthetic input never confirms | L2 |
| A move to a frozen place: first frame of motion within one refresh, no blank frame | L2 per-frame screenshots, L3 timing |
| `odap-voice` reads the status panel, the volume overlay and a consent layer from the shell's tree | L2 |
| Recovery boots and speaks or signals its states with Mesa removed from the image | L2 |
