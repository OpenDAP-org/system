# Compositor and UI runtime: benchmark

> Written before [RFC 0012](https://github.com/opendap-project/rfcs/blob/main/text/0012-composer-and-os-shell.md): "the shell" here means the compositor and the system's interface together, which are now the composer (`odap-composer`) and the OS Shell built on it. The report is kept as it was researched.

- **Date:** 2026-10-23
- **Status:** Research report, input to the [composer and UI runtime spec](../specs/composer-and-ui-runtime.md) and ADR 0012. Builds on ADR 0002, ADR 0005 and ADR 0006 without reopening them.

- **Date:** 2026-10-23
- **Status:** research. Nothing here is a decision. It builds on accepted ADRs 0002 to 0006 and 0009 to 0011, the system architecture research (§3.3, §3.4, §4.4, §5.2, §5.5, §5.7, §5.8, §5.13, §5.16), the toolkit and text-input research, the os-architecture spec (draft), design system §8, §11 to §13, §15, §17, §18, the experience budgets and the interface mockups.
- **Hardware assumed:** RK3566 (4× Cortex-A55, Mali-G52 on Panfrost, GLES 3.1), 320 × 480 MIPI DSI panel at 60 Hz (Focus LCDs E35GH, ILI9488, faceplate-touch ADR 0001), 4 GB RAM, battery; Debian trixie, kernel 7.2/7.3, systemd 257.
- **Evidence:** versions read on 2026-10-23 from crates.io, GitHub raw sources at tags, docs.rs, wayland.app, AOSP gitiles, Apple's and Android's developer docs, Debian man pages. **No benchmark was run** (machine under load, and the numbers that matter need the RK3566). The web-search budget was already exhausted, so only direct URLs were used. **UNVERIFIED** marks what was not confirmed against a primary source. Derived numbers say so and show their arithmetic.

## 1. Key findings

1. **Smithay is still 0.7.0 on crates.io (2025-06-24); master is active** (last commit 2026-10-20) and already has `presentation`, `fifo`, `commit_timing`, `drm_syncobj`, `security_context`, `input_method`, `text_input`, `session_lock`, `idle_notify` and `foreign_toplevel_list` modules ([src/wayland at master](https://github.com/Smithay/smithay/tree/master/src/wayland)). Pin a git revision, as cosmic-comp and niri do (text-input research §3.2 already argues this).
2. **Smithay's `DrmCompositor` already does plane assignment and direct scanout:** it "walks the stack of provided Elements from front to back while trying to assign each element to a drm overlay plane", falls back to rendering on the primary plane, and its `FrameFlags` gate primary, overlay and cursor scanout ([compositor module](https://docs.rs/smithay/0.7.0/smithay/backend/drm/compositor/index.html), [FrameFlags](https://docs.rs/smithay/0.7.0/smithay/backend/drm/compositor/struct.FrameFlags.html)). Only elements backed by client buffers exportable as DRM framebuffers qualify.
3. **On the RK3566, mainline VOP2 exposes only three usable hardware windows, not six.** Smart1, Esmart1 and Cluster1 "don't have an independent framebuffer. They can only share/mirror the framebuffer with smart0, esmart0 and cluster0", and the driver skips them ([rockchip_drm_vop2.c v7.2](https://github.com/torvalds/linux/blob/v7.2/drivers/gpu/drm/rockchip/rockchip_drm_vop2.c), `vop2_is_mirror_win`). The remaining Smart0, Esmart0 and Cluster0 have different abilities: Smart RGB only, linear; Esmart RGB plus NV12/YUV, linear; **Cluster AFBC-only** ("The cluster window on 3568 is AFBC-only"); **no cursor plane** ([rockchip_vop2_reg.c v7.2](https://github.com/torvalds/linux/blob/v7.2/drivers/gpu/drm/rockchip/rockchip_vop2_reg.c)). One display gets one primary and up to two overlays.
4. **The power rule every good compositor follows is "no damage, no frame".** niri keeps a per-output `RedrawState` (`Idle → Queued → WaitingForVBlank`, or `WaitingForEstimatedVBlank` when a redraw produced no damage) and throttles frame callbacks to one per refresh even without damage ([niri: redraw loop](https://github.com/YaLTeR/niri/blob/main/docs/wiki/Development:-Redraw-Loop.md)). This is the model to copy.
5. **Cross-process transitions are done by the window manager moving surfaces, not by the apps.** Android's WM Shell plays every transition on `SurfaceControl` "leashes" with a start and a finish transaction, serialised per track, mergeable, and delegable to a remote process ([Transitions.java](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/libs/WindowManager/Shell/src/com/android/wm/shell/transition/Transitions.java), [RemoteTransitionHandler.java](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/libs/WindowManager/Shell/src/com/android/wm/shell/transition/RemoteTransitionHandler.java)). niri applies the principle that input goes to the destination immediately while the animation plays ([design principles](https://github.com/YaLTeR/niri/blob/main/docs/wiki/Development:-Design-Principles.md)), which matches design system §11.1 ("the state changes at once, and the animation shows it").
6. **The T1's spatial model (Now Playing ↔ Library ↔ Status, with shared elements such as the playing strip) behaves like one app. One process per activity makes its main transitions cross-process.** Compositor-driven transitions over each activity's last buffer cover slides and fades. True shared-element morphs across processes have no clean precedent (**UNVERIFIED**). This is the main tension with ADR 0002 and is raised as a question in §9.8, not decided.
7. **Slint 1.18.1 (latest on crates.io):** `ListView` instantiates only visible elements ("guarantees stable performance with a practically unlimited number of items", [listview.mdx](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/reference/std-widgets/views/listview.mdx)). The Skia renderer has buffer-age partial rendering (`SLINT_SKIA_PARTIAL_RENDERING`, dirty regions per buffer age, [skia/lib.rs](https://github.com/slint-ui/slint/blob/v1.18.1/internal/renderers/skia/lib.rs)) but swaps with plain `swap_buffers`, so no damage reaches the compositor ([opengl_surface.rs](https://github.com/slint-ui/slint/blob/v1.18.1/internal/renderers/skia/opengl_surface.rs)). The FemtoVG renderer has no partial rendering at all ([femtovg/lib.rs](https://github.com/slint-ui/slint/blob/v1.18.1/internal/renderers/femtovg/lib.rs)). At 320 × 480 a full frame is 600 KiB, so full-surface damage matters less here than on a desktop (derived: 320 × 480 × 4 B = 614,400 B; × 60 Hz ≈ 36.9 MB/s).
8. **No measured GL-context memory figure for Panfrost was found** (**UNVERIFIED**). The only numbers on record are the toolkit research's CPU-rendered Slint idle RSS (10.4 to 16.4 MB, Docker on Apple Silicon) and a desktop COSMIC applet holding 108 MiB of GPU memory. Per-activity cost stays a device gate, as ADR 0006 requires.
9. **Unspoofable consent needs two things: an OS-owned surface and an input path apps cannot reach.** On Android the app "cannot customize the dialog" ([requesting permissions](https://developer.android.com/training/permissions/requesting)), and since Android 12 touches that pass through untrusted overlays are blocked, except for trusted windows such as IME and accessibility, with a 0.8 obscuring-opacity threshold ([Android 12 behaviour changes](https://developer.android.com/about/versions/12/behavior-changes-all)). On the T1 the compositor draws consent itself (ADR 0002). The physical second press of Main, already used for power-off, can be the trusted path for privileged confirmation (ADR 0009).
10. **Recovery UI:** Slint `linuxkms` with the software renderer draws through DRM dumb buffers "without a C library", with optional libseat and libinput ([backend_linuxkms.mdx](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/guide/backends-and-renderers/backend_linuxkms.mdx)). That makes recovery independent of Mesa. It has no AccessKit (architecture research finding 6).

## 2. What is already fixed (inputs, not re-decided)

| Fixed | Source |
|---|---|
| Smithay compositor that draws the product chrome in Slint; one activity toplevel visible, full screen, plus system overlays; no launcher, windows or shade | ADR 0002; research §5.7 |
| One process per activity (`dap-activity-host`), lifecycle on the cgroup freezer, hosts are bus clients only; fallback to a shared host is a research proposal | ADR 0002; research §5.2, §5.8 |
| Templates rendered by the host, data only, no code in the UI process; tier B when Slint stabilises | ADR 0002, 0005, 0010; spec §6.3, §12 |
| Slint under GPLv3, behind a renderer interface, measured on the device | ADR 0006 |
| Native aarch64, no WASM | ADR 0003 |
| Keyboard: separate `dap-keyboard` client on filtered layer-shell plus input-method-v2 (proposed, not decided) | text-input research §3.5 |
| Spatial model: Now Playing home, library below, queue from the trailing edge, More from the bottom, status bar unfolding; volume beside its keys, power beside Main | spec A1, §6.1; design system §8.1, §8.5; mockups |
| Touch to visual ≤ 50 ms ideal (hard 150 ms); 60 fps under any load; screen on to interactive ≤ 150 ms | experience budgets |
| Motion 120/200/320 ms, `cubic-bezier(0.2, 0, 0, 1)`, never between input and effect; reduced motion | design system §11 |
| Screen off: VOP2, panel and GPU powered down, activities frozen | image-composition research §8.5 |

## 3. Compositor benchmark

### 3.1 Smithay and the compositors built on it

| Project | Version (2026-10-23) | Relevant pattern | Lesson for the T1 |
|---|---|---|---|
| Smithay | 0.7.0 (crates.io); master 2026-10-20 | `DrmCompositor` (planes, direct scanout), `OutputDamageTracker`, `GlesRenderer`, `LibSeatSession`, presentation feedback helpers ([docs.rs 0.7.0](https://docs.rs/smithay/0.7.0/smithay/)) | Building blocks exist; the shell policy is ours |
| niri | (releases page not read this session; architecture research: v26.04) | Redraw state machine; frame-callback throttling without damage; animations "started" with duration 0 when disabled; clip-to-geometry still allows scanout when the surface is fully visible ([design principles](https://github.com/YaLTeR/niri/blob/main/docs/wiki/Development:-Design-Principles.md)) | Copy the redraw loop and the "reduced motion costs nothing" rule |
| cosmic-comp | epoch-1.11.0 | Renders Iced elements inside the compositor into `MemoryRenderBuffer`s with damage (toolkit research §5) | The in-compositor chrome technique, proven in production |
| Pinnacle | main (Cargo.toml depends on Smithay) | Configurable Smithay compositor (not studied further) | — |
| Jay | 1.15.0 | **Does not depend on Smithay**: no `smithay` entry in its [Cargo.toml](https://github.com/mahkoh/jay/blob/master/Cargo.toml). The brief listed it as Smithay-based; that is not correct | Not a Smithay reference |

DRM/KMS structure common to these: libseat session (logind backend, image-composition §4.2) → udev for the DRM node → GBM allocator → `GlesRenderer` on the EGL display → `DrmCompositor` per CRTC with a swapchain → libinput for touch. The T1 has one CRTC (DSI), one touch device (GT911) and the faceplate keys through `dap-input` semantic actions (research §5.7), so the multi-GPU and hotplug parts of these compositors are not needed.

### 3.2 Hardware planes on the RK3566 (verified in v7.2 source)

| Window | Usable on RK3566 | Formats | Modifiers | Scaling | Likely role |
|---|---|---|---|---|---|
| Smart0 | yes, primary | RGB/BGR 565/888/8888 | linear only | 8× up/down | Composited frame, or direct scanout of the foreground activity |
| Esmart0 | yes, overlay | RGB, ARGB1555, NV12/NV16/NV24, YUYV… | linear only | 8× | Chrome (status bar, system layers) or a video/artwork layer |
| Cluster0 | yes, overlay | RGB 8888/888/565, 2101010, YUV420 | **AFBC only** | 4×, rotation 90/270 | An activity buffer rendered with AFBC by Panfrost (**UNVERIFIED** that Panfrost-on-G52 and VOP2 agree on modifiers end to end) |
| Smart1, Esmart1, Cluster1 | **no** (mirror only) | — | — | — | — |

- Planes support zpos and blend-mode properties ([rockchip_drm_vop2.c](https://github.com/torvalds/linux/blob/v7.2/drivers/gpu/drm/rockchip/rockchip_drm_vop2.c), `vop2_plane_init`). Implicit modifiers are refused ("No support for implicit modifiers"), so clients must allocate with explicit modifiers through linux-dmabuf feedback.
- **Planes matter here for GPU idleness, not bandwidth.** A full XRGB8888 frame is 36.9 MB/s at 60 Hz (derived), trivial for LPDDR4. What a plane saves is waking the Mali to composite when only one layer changed: while a list scrolls under a static status bar, the compositor can flip the activity's buffer on one plane and leave the chrome on another.
- **Catch:** Smithay scans out only elements whose storage is a DRM framebuffer (finding 2). Chrome drawn into `MemoryRenderBuffer`s (the cosmic-comp technique) is composited on the primary plane. To put chrome on its own plane, the compositor would have to render it into its own GBM/dmabuf buffer and present that as a buffer element (**UNVERIFIED** API path in Smithay; measure before building it).
- The panel's DSI mode (video or command) decides whether VOP2 must scan out 60 times a second even when nothing changes. The ILI9488 has its own frame memory (GRAM), so a command-mode panel could hold a static image with VOP idle. The driver source could not be read this session (HTTP 404/429): **UNVERIFIED**, and a question for faceplate-touch.

### 3.3 Frame scheduling and presentation

| Mechanism | State | Use on the T1 |
|---|---|---|
| `wl_surface.frame` callbacks | Core | The only clock clients get. Send after the compositor's repaint, one per refresh, never to suspended or frozen surfaces |
| `wp_presentation` | **Stable**; flags `vsync`, `hw_clock`, `hw_completion`, `zero_copy` ("the buffer from the client was given to display hardware as is", [wayland.app](https://wayland.app/protocols/presentation-time)) | Time animations to real presentation; verification reads `zero_copy` to prove direct scanout; Smithay has the module |
| `fifo-v1` | Staging; implemented by cage, COSMIC, Jay, KWin, Mutter, niri, phoc, Sway, Weston and others ([wayland.app](https://wayland.app/protocols/fifo-v1)) | Lets a client queue one commit per refresh without blocking on frame callbacks (FIFO semantics); in Smithay master |
| `commit-timing-v1` | Staging; in Smithay master | Target-time commits; useful for a spectrogram tied to audio time (**UNVERIFIED** client support in winit/Slint) |
| Repaint window | Weston: `repaint-window=N`, "used to control and reduce the output latency for clients" ([weston.ini(5)](https://manpages.debian.org/trixie/weston/weston.ini.5.en.html)) | Composite late in the refresh so client commits from the same frame make it; a tunable, measured on the device |
| `xdg_toplevel` `suspended` | Since wayland-protocols 1.32 | Sent on `stopped`, before the freezer (research §5.8) |
| `fractional-scale` | Not needed: one token pixel is one panel pixel (design system §18.2) | Do not advertise |
| `wlr-layer-shell` | wlr-protocols; Smithay `new_with_filter` | Only `dap-keyboard` (text-input research §3.5) |
| `zwp_input_method_v2`, `zwp_text_input_v3`, `zwp_virtual_keyboard_v1` | Smithay has all three; one input method per seat | As text-input research §3.5 |
| `wp_security_context_v1` | In Smithay | Mark the sandboxed connections the orchestrator hands to activity hosts, so global filters can tell first-party from HAT-mode clients |
| `ext-session-lock-v1`, `ext-idle-notify-v1` | In Smithay | The lock is drawn by the compositor itself; idle notify is not needed when the compositor owns idle |

### 3.4 Other compositor families

| System | Model | Lesson |
|---|---|---|
| **cage** (wlroots 0.20; 0.2.0 in trixie, 0.3.1 in sid) | "A Wayland kiosk. A kiosk runs a single, maximized application" ([README](https://github.com/cage-kiosk/cage)); implements fifo and presentation | Bring-up only (image-composition §4.3). It has no layers, consent or transitions |
| **Weston kiosk-shell** | "makes all top-level application windows fullscreen", `app-ids=` per output ([kiosk-shell](https://wayland.pages.freedesktop.org/weston/toc/kiosk-shell.html)); libweston DRM backend assigns planes (**UNVERIFIED** this session) | Repaint window as a latency knob; a C codebase |
| **phoc + Phosh + Stevia/Squeekboard** | Shell is a privileged GTK client of a wlroots compositor; keyboard on layer-shell plus input-method-v2 (text-input research §3.4) | Shell outside the compositor costs a process and private protocols (research §3.3); the keyboard pattern is the one to keep |
| **Mutter (GNOME Mobile)** | Shell drawn inside the compositor (gnome-shell is the compositor) (**UNVERIFIED** current mobile branch state) | Same "the compositor is the shell" shape as ADR 0002 |
| **Mir + Lomiri** | Mir 2.30.0 implements input-method v1/v2 and text-input v1–v3; Lomiri pauses apps with SIGSTOP and models suspend states (research §3.3) | Closest lifecycle precedent; freezer instead of signals |
| **Qt Wayland Compositor + Qt Application Manager** | "becomes a subsystem inside a larger application manager process", supports XDG Shell and IVI Application ([Qt Wayland Compositor](https://doc.qt.io/qt-6/qtwaylandcompositor-index.html)); `WaylandHardwareLayer` "Makes a parent WaylandQuickItem use hardware layers", technology preview ([doc](https://doc.qt.io/qt-6/qml-qtwayland-compositor-waylandhardwarelayer.html)); single or multi process with one API (research §3.4) | The HMI precedent for a compositor drawing its own system UI, with planes for client content that updates continuously |
| **AGL compositor** | libweston-based, with a private `agl-shell` protocol for a privileged homescreen client that activates one application at a time (**UNVERIFIED**: sources unreachable this session) | Car HMIs also settle on "one active app, privileged shell protocol" |

### 3.5 Input to photon at 60 Hz (derived, to be measured)

Path for a touch handled by an activity: GT911 report (interval **UNVERIFIED**) → kernel → libinput in the compositor → `wl_touch` to the client → client waits for its frame callback (0 to 16.7 ms) → renders (budget below) → commits → compositor composites or flips at the next repaint (0 to 16.7 ms) → scanout from top to bottom (0 to 16.7 ms, so the middle of the panel is 8.3 ms after vblank).

- Typical case: about two to three refreshes, **33 to 50 ms**, before the touch report interval. The 50 ms ideal therefore leaves no room for a missed frame.
- What removes a refresh: the client renders in the same refresh it gets the input (Slint renders on the next event-loop turn after input); the compositor composites late in the refresh (repaint window); direct scanout means the compositor only flips.
- **Gestures the compositor handles itself skip the client hop.** It moves existing buffers on the next refresh, about 17 to 33 ms. That is an argument for the compositor to own edge gestures and cross-activity transitions (§4.3).
- Faceplate keys: `dap-input` → bus → compositor (system layer such as volume) or activity. The volume indicator is compositor-drawn, so it is one hop plus one refresh.

### 3.6 Power rules seen across the benchmark

1. No damage, no repaint, no frame callbacks beyond one per refresh (niri).
2. Never send frame callbacks to suspended or frozen surfaces. The freezer guarantees zero CPU anyway (research §5.13).
3. Direct scanout or a plane flip when only one buffer changed (Smithay `FrameFlags`).
4. Eye candy that is off is absent from the render tree (niri; design system §12.4 reduced effects).
5. Screen off: DPMS off, VOP2, panel and GPU powered down (image-composition §8.5).

## 4. Process model

### 4.1 Single-process shells versus one process per activity

| System | Model | Why | Cost |
|---|---|---|---|
| Android | SystemUI (status bar, shade, volume, keyguard) is one process; each app its own process; WM Shell animates across them ([Transitions.java](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/libs/WindowManager/Shell/src/com/android/wm/shell/transition/Transitions.java)) | Isolation, per-app freezing ([cached apps freezer](https://source.android.com/docs/core/perf/cached-apps-freezer)) | A whole transition system (leashes, tracks, merges, remote handlers) |
| iOS | SpringBoard as the home and system UI; apps in their own processes; system alerts drawn out of the app's process (**UNVERIFIED** internals; no primary source read) | Same | Not documented publicly |
| reMarkable | One product application, Xochitl, started by systemd ([Xochitl](https://developer.remarkable.com/documentation/xochitl)) | A dedicated product with a fixed IA | A crash takes the whole UI down |
| Kindle | Single product UI (**UNVERIFIED**) | — | — |
| Qt Application Manager | Single or multi process behind one API; single process "terminates the entire program" on a crash (research §3.4) | Choice after measurement | — |
| AGL | One active app, systemd template units (research §3.4) | — | — |

The T1 sits between reMarkable (one product, one IA) and Android (third parties that must not break it). ADR 0002 picks isolation. Under ADR 0010, third parties do not run code in the UI process, so isolation buys protection mainly against **HAT-mode activities** and against failures of first-party hosts.

### 4.2 Cost per activity host (what is known)

| Item | Known | Source / status |
|---|---|---|
| Slint idle RSS, CPU rendering, Weston headless | 10.4 MB (FemtoVG-only build) to 16.4 MB (defaults) | Toolkit research §11 (Docker, not the device) |
| Swapchain per activity surface | 3 × 600 KiB ≈ 1.8 MiB at XRGB8888 (derived) | Arithmetic; buffer count **UNVERIFIED** for Mesa's Wayland EGL |
| Mesa/Panfrost per-process driver overhead (context, shader cache, tiler heap) | **No measured figure found** | UNVERIFIED; measure PSS plus DRM fdinfo memory per client (Panfrost fdinfo support **UNVERIFIED** on 7.2) |
| Font and glyph caches | Per process; CJK font 8.33 MB on disk, mmapped and shared through the page cache (text-input research §7.4) | Glyph atlases are per GL context (**UNVERIFIED** size) |
| EGL context sharing | Only within one process (EGL share groups); across processes buffers are shared as dmabuf, never contexts | Standard EGL; Slint winit: one context per window (**UNVERIFIED**) |
| Desktop warning | COSMIC applet holding 108 MiB of GPU memory at idle | Toolkit research §10 |

Resident set to plan for: Now Playing, Library, Status, This T1, Sound, `dap-keyboard`, at most one HAT mode. That is six to seven Slint processes with GL contexts, most of them frozen. Even at 50 MiB each (an assumption, **not a measurement**) that is about 350 MiB of 4 GB. The real risk is GPU memory per context and thaw latency, not RAM.

### 4.3 Transitions across processes

- **Android:** each change gets a leash. The Shell sets position, crop, matrix and alpha on leashes in a start transaction, animates, then applies a finish transaction ([Transitions.java](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/libs/WindowManager/Shell/src/com/android/wm/shell/transition/Transitions.java) lines around `setupStartState`). Transitions queue per track and can merge, and a remote process (the launcher) can be handed the animation ([RemoteTransitionHandler.java](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/libs/WindowManager/Shell/src/com/android/wm/shell/transition/RemoteTransitionHandler.java)). Embedding another process's UI uses `SurfaceControlViewHost` (toolkit research §6.4).
- **niri:** animations are compositor-side over client buffers. Input goes to the destination immediately ([design principles](https://github.com/YaLTeR/niri/blob/main/docs/wiki/Development:-Design-Principles.md)), and springs or easings are configurable ([animations](https://github.com/YaLTeR/niri/blob/main/docs/wiki/Configuration:-Animations.md)).
- **The enabling fact for the T1:** a frozen client's last committed `wl_buffer` stays with the compositor. The compositor can therefore start sliding the Library in at once from its last frame, thaw the host in parallel, and switch to fresh frames as they arrive. Thaw time is part of "screen on to interactive" (≤ 150 ms) and of cross-activity transitions: measure it.
- **Shared elements across processes** (the player artwork shrinking into the playing strip on "swipe down to minimise"): the compositor can animate a cropped sub-rectangle of the outgoing buffer towards a rectangle the incoming activity declares, then cross-fade. No primary source shows a production system doing this across processes; Android's shared-element transitions are within one app (**UNVERIFIED**). Within one process Slint animates it natively.

### 4.4 Mapping onto the T1's screens

| Movement (design system §8.5, spec §6.1) | Processes involved | Cleanest mechanism |
|---|---|---|
| Queue from the trailing edge; More from the bottom | Inside Now Playing | Slint animation in the host. The compositor forwards edge-gesture progress (§8.c) |
| Back (leading edge) within an activity | Inside the host | Host |
| Now Playing ↔ Library (down/up; minimise the player) | Two hosts | Compositor slide over both buffers; shared element only if both live in one host |
| Status bar unfolding into the Status screen | Compositor chrome → Status host | Either the compositor draws the unfolded panel too (one process, the item-by-item unfold is easy), or it animates its own bar and hands over to the Status host's buffer |
| Volume, power, toasts, consent, pairing | Compositor | Slint inside the compositor (ADR 0002) |
| Keyboard | `dap-keyboard` (layer-shell) | Compositor slides the panel and resizes the activity (text-input research §3.5) |

## 5. UI runtime inside an activity host

| Topic | Finding | Consequence |
|---|---|---|
| Version | Slint 1.18.1 (2026-09-21) on winit 0.30; winit 0.31 upgrade open (#11243); AccessKit 0.25.1; parley 0.12.0 exists, Slint uses 0.11 | Track 1.19; the winit 0.31 move also unlocks text-input purposes |
| Renderers | FemtoVG (GLES 2, default): full repaint. Skia GL: partial rendering by buffer age, opt-in, no damage passed to Wayland. Software: partial repaint into a buffer | Measure all three (toolkit research gate). Full repaint of 320 × 480 may be cheap enough; partial rendering mainly saves GPU energy on small updates (clock, seek bar) |
| Swap behaviour | Skia GL sets `SwapInterval::Wait(1)` ([opengl_surface.rs](https://github.com/slint-ui/slint/blob/v1.18.1/internal/renderers/skia/opengl_surface.rs)) | On Wayland, eglSwapBuffers then paces on frame callbacks (Mesa behaviour, **UNVERIFIED** here): the compositor's callback timing sets client latency |
| Retained vs immediate | Slint is retained, with fine-grained property dirtiness; Iced rebuilds `view()` (toolkit research §1) | Only changed properties cost work. Bus `Watch` bindings (research §5.5) write single properties |
| Lists at 100k to 1M items | `ListView` instantiates only visible items. The model is the `Model` trait (`row_count`, `row_data`) | Implement a paging `Model` over `dap-library` cursors (research §5.5 diffing model); never a `VecModel` of the whole library. Variable row heights are **UNVERIFIED**: keep 52/60 px fixed rows (design system §8.3) |
| Artwork | `SharedPixelBuffer` is `Send` ([docs.rs](https://docs.rs/slint/1.18.1/slint/struct.SharedPixelBuffer.html)); `Image` is built on the UI thread | Decode and scale on a worker thread to the exact display size (138 px covers; OS-029 artwork at display size), hand the buffer to the UI thread, and the texture upload happens at the next render. An LRU keyed by (album, size) bounded in bytes (T1-047) |
| Text | parley + HarfRust shaping, fontique, swash/skrifa (text-input research §7.4) | Shaping cache per process. Prewarm the type roles at host start; measure shaping time for 52 px rows at fling speed |
| Animation timing | Slint animations tick from its event loop clock (**UNVERIFIED** whether it uses presentation time) | Gate: frame-time histogram from `wp_presentation` during scroll and transitions; 0 dropped frames at 60 Hz |
| Frame budget | 16.6 ms per refresh; the compositor needs its share | Proposal: client ≤ 8 ms render at p99 on the A55 + G52, compositor ≤ 4 ms, the rest as margin. Engineering proposal, calibrate on hardware |
| Accessibility | AccessKit on winit only | Activity hosts on winit (ADR 0002). Compositor chrome needs its own AT-SPI tree: Slint inside the compositor has no winit, so **AccessKit for the chrome is UNVERIFIED** and a real gap for `dap-voice` (SYS-062) |
| Dynamic components | `ComponentContainer`, named slots, `interface` experimental | Tier B waits (spec §12) |

## 6. Templates and data-only contributions

| Precedent | How the host keeps control | What applies |
|---|---|---|
| Android Car App Library | At most **five templates per task**; refreshes of the same type and main content don't count; exhausting the quota closes the app with an error ([template restrictions](https://developer.android.com/training/cars/apps/library/template-restrictions)) | Quotas per slot (spec §6.2) and a cap on refresh rate per contribution |
| Apple CarPlay | `CPInterfaceController` is created by CarPlay; the app pushes templates onto a navigation hierarchy; `CPListTemplate.maximumItemCount` and `maximumSectionCount` are set by the system ([CarPlay docs](https://developer.apple.com/documentation/carplay/cplisttemplate)) | Limits are published by the host and readable by the SDK |
| Android media controls | Host owns 5 slots; app gives command, icon, label (research finding 1) | `nowplaying/more-actions` |
| Pebble timeline, VS Code contribution points | System layouts; manifest-declared contributions rendered without running code (research §3.5) | Already the model |
| Garmin Connect IQ glances | Small, budgeted summary views (**UNVERIFIED**: page not readable this session) | — |

What the host must cache, so templates cost nothing at runtime:

1. The **compiled Slint definition per (template id, version, input profile, locale direction)**: the toolkit research's generated-`.slint` route makes compile time the cold cost (**UNVERIFIED** on the A55).
2. The **resolved slot list** from `org.opendap.system.Slots`, invalidated by watch events, never polled.
3. **Last-known contribution data** with its schema version, so a slot renders at once from cache while the provider is lazily started (spec §5.3 "lazy by default").
4. **Stock icons and text shaping** for template labels per locale.
5. Bounded sizes everywhere (A6, T1-047): per-slot byte caps, LRU eviction, and an endurance test.

## 7. Overlays and system layers

| Layer | Drawn by | Why |
|---|---|---|
| Status bar, volume beside the keys, power beside Main, toasts, consent, pairing, lock | **Compositor** (Slint in-process, ADR 0002) | One hop for faceplate keys (§3.5); trusted path; no client can cover it |
| On-screen keyboard | `dap-keyboard` client on filtered layer-shell | Crash isolation for composing engines (text-input research §3.5) |
| HAT-mode or third-party overlays | **None** | No layer-shell for capabilities (filtered global); spec §6.1 gives system layers no slots |

Trusted-path rules taken from the precedents:

- Only the compositor draws consent. Requests come from `dap-orchestrator` over the bus, never from a Wayland client (Android: the app "cannot customize the dialog").
- While a consent or pairing layer is up, the compositor stops forwarding touch to clients and never lets a client surface sit above the system layer (Android 12 untrusted-touch blocking, made total here).
- The prompt shows the principal's verified identity from the signed manifest, and the purpose string (spec §8.3).
- Privileged confirmation uses the physical Main button, a channel no software can synthesise. Synthetic input (`zwp_virtual_keyboard_v1`, `dap-agent` injection in developer mode) must never reach system layers (proposal; Apple's hardware-button confirmation is a similar idea, **UNVERIFIED** source).
- Globals are filtered per client: layer-shell and input-method only for `dap-keyboard`; screencopy only for `dap-agent` in developer mode; HAT-mode clients tagged through `wp_security_context_v1`.

## 8. Recovery, update UI and boot continuity

| Path | Facts | Assessment |
|---|---|---|
| Slint `linuxkms` + software renderer | Dumb buffers, no Mesa or GBM needed; optional libseat and libinput | Recovery works even if Mesa or Panfrost is what broke. No accessibility tree (finding 10) |
| Plymouth 24.004.60 (trixie) | `quit --retain-splash` "Don't explicitly hide boot splash on exit"; `deactivate`/`reactivate` ([plymouth(1)](https://manpages.debian.org/trixie/plymouth/plymouth.1.en.html)) | One more DRM master and an initramfs integration. The handover to a non-GNOME compositor is not documented |
| Direct KMS (no Plymouth) | The shell compositor starts early (image-composition: systemd to first Slint frame 0.5 to 1.5 s, proposal) and draws the same splash itself | Fewer moving parts; continuity depends on what U-Boot and the kernel show before it (mainline U-Boot DSI bring-up on RK3566 **UNVERIFIED**) |

## 9. Implications for Blackbird OS (proposal)

### 9.a Compositor responsibilities (`dap-shell`, ADR 0002)

- Session through libseat on logind; one `DrmCompositor` on the DSI CRTC; `GlesRenderer`; libinput touch; faceplate keys as semantic actions from `dap-input`.
- Owns: the spatial layout of activities, cross-activity transitions, edge-gesture recognition, system layers, focus, lock, DPMS, idle and the lifecycle hints to the orchestrator (`dap-activity-v1`).
- Redraw loop as niri's: idle with no damage, at most one frame callback per refresh, none to suspended surfaces.
- Plane policy: try direct scanout of the foreground activity on Smart0 (linear) or Cluster0 (AFBC). Keep chrome composited at first, and move it to Esmart0 only if measurements show GPU wake-ups from compositing matter.

### 9.b Protocols

Core plus `xdg-shell` (with `suspended`), `linux-dmabuf` with feedback (explicit modifiers; VOP2 refuses implicit ones), `wp_presentation`, `fifo-v1`, `viewporter`, `single-pixel-buffer`, `text-input-v3`, `input-method-v2`, `virtual-keyboard-v1` (filtered), `wlr-layer-shell` (filtered to `dap-keyboard`), `wp_security_context_v1`, the private `dap-activity-v1` (lifecycle; the gesture and element-rect additions of 9.d), plus `drm-syncobj` if Panfrost explicit sync proves useful (**UNVERIFIED**). Not advertised: `fractional-scale`, `xdg-decoration`, foreign-toplevel management, idle-inhibit to clients.

### 9.c Edge gestures

- The compositor recognises gestures that start in the 20 px edge zones (design system §8.5), minus the exclusion rectangles each activity declares (seek bar, A–Z index; Android has a comparable exclusion API, **UNVERIFIED** source here).
- **Cross-activity gestures** (down to the library, status pull) are tracked by the compositor, which moves buffers itself.
- **Intra-activity gestures** (queue, More, Back inside a stack) are sent to the host as `gesture(kind, phase, progress, velocity)` over `dap-activity-v1`. The host animates in Slint, and the finger never waits on a round trip of rendered frames for recognition.
- Every gesture still has a tap alternative (WCAG 2.5.1/2.5.7, design system §8.5).

### 9.d Transitions across processes

- Activities sit in a fixed spatial arrangement known to the compositor: Library below Now Playing, Status above.
- On navigation the compositor (1) sends input focus to the destination at once, (2) thaws the destination and asks it to resume, (3) animates both buffers with the motion tokens, starting from the destination's last buffer if it has no fresh frame yet, (4) swaps in fresh frames as they arrive, and (5) sends `suspended` to the source when it leaves, then lets the freezer act after the grace period (research §5.8).
- Reduced motion uses duration 0 with the same code path (niri).
- Optional shared-element morph: hosts declare named element rectangles (artwork, title, playing strip) and the compositor crops and moves them. Build only if usability shows the plain slide is not enough.

### 9.e Render loop and power rules (gates)

1. Static screen: 0 wake-ups per second from the compositor and the foreground host over 60 s; 0 from frozen hosts.
2. Scroll a 1,000,000-item list: 60 fps with no dropped-frame bursts (experience budgets), p99 client render ≤ 8 ms (proposal).
3. Touch to photon ≤ 50 ms measured on the device with a camera or photodiode (L3). Cross-activity transition start ≤ 1 refresh after input.
4. Direct scanout seen: `zero_copy` in `wp_presentation` and plane state in `/sys/kernel/debug/dri/*/state` during full-screen scroll.
5. Screen on to interactive ≤ 150 ms, including the thaw of the foreground host.
6. 72 h endurance: flat RSS, GPU memory and frame times across hosts and the compositor (T1-047).

### 9.f What the host caches (bounded)

Compiled template definitions per (id, version, profile, direction); resolved slots; last contribution data; artwork textures at display size (LRU by bytes); shaped text for visible and nearby rows; the paging window of library cursors. Every cache has a byte cap in the host manifest (A6).

### 9.g Process numbers to measure (ADR 0006 gate)

Per host: RSS, PSS, GPU memory (DRM fdinfo or debugfs), thread count, cold start to first frame, thaw to first fresh frame, idle wake-ups. Do this for FemtoVG, Skia GL (with and without partial rendering) and software, with 1, 4 and 7 resident hosts. Compare the same screens in a single shared first-party host.

### 9.h Verification additions (to research §5.16)

| Check | Level |
|---|---|
| Redraw idleness and frame-callback throttling (scripted, wake-up counter per unit) | L2 ratio, L3 absolute |
| Plane assignment and `zero_copy` with the VOP2 3-window limit | L3 only (QEMU `virtio-gpu` has no VOP2) |
| Consent spoofing: a HAT-mode client tries to draw above, inject input to, or obscure a consent layer; synthetic input never confirms a privileged prompt | L2 |
| Cross-activity transition with the destination frozen: first frame of motion within one refresh; no blank frame | L2 (screenshots per frame), L3 timing |
| Recovery UI boots with Mesa removed from the image | L2 |

## 10. Questions for the maintainers (would touch accepted decisions)

1. **One process per activity versus the spatial model (ADR 0002).** Now Playing, Library and Status behave like one app with shared elements. Should first-party activities share one host, which research §5.2 proposes as a fallback, while HAT modes keep their own process? Or should the transitions stay compositor slides? The answer depends on the 9.g measurements, and changing it means amending ADR 0002.
2. **Where is the unfolded status panel drawn?** Spec §6.1 lists Status as an activity, while ADR 0002 has the compositor draw the status line. Drawing the panel in the compositor gives the cleanest unfold, but puts `status/lines` template rendering inside the compositor process (data only, so ADR 0010 holds, but a rendering failure there is more serious).
3. **Accessibility of compositor-drawn chrome and of recovery.** Neither in-compositor Slint nor `linuxkms` has AccessKit, so `dap-voice` (SYS-062) cannot read the volume, consent and power layers or the recovery UI through AT-SPI. Should the compositor publish its own tree (for example to `dap-voice` over the bus), and what level is required in recovery? This is ADR 0002's open accessibility point.
4. **Panel mode.** Answered after this report: the mainline driver (`panel-ilitek-ili9488.c`, compatible `focuslcds,e35gh-i-mw800cb`) drives the panel in **DSI video mode** (`MIPI_DSI_MODE_VIDEO | MIPI_DSI_MODE_VIDEO_SYNC_PULSE | MIPI_DSI_MODE_LPM | MIPI_DSI_CLOCK_NON_CONTINUOUS`, RGB666, one lane). Command mode with the panel's own memory would be a driver change.
5. **Boot splash:** Plymouth or the compositor's own first frame? This depends on the boot-chain research and on U-Boot DSI support.

## 11. Sources (accessed 2026-10-23)

- crates.io API: [smithay](https://crates.io/crates/smithay), [slint](https://crates.io/crates/slint), [winit](https://crates.io/crates/winit), [accesskit](https://crates.io/crates/accesskit), [parley](https://crates.io/crates/parley)
- Smithay: [master tree](https://github.com/Smithay/smithay/tree/master/src/wayland) · [DrmCompositor](https://docs.rs/smithay/0.7.0/smithay/backend/drm/compositor/index.html) · [struct](https://docs.rs/smithay/0.7.0/smithay/backend/drm/compositor/struct.DrmCompositor.html) · [FrameFlags](https://docs.rs/smithay/0.7.0/smithay/backend/drm/compositor/struct.FrameFlags.html) · [OutputDamageTracker](https://docs.rs/smithay/0.7.0/smithay/backend/renderer/damage/struct.OutputDamageTracker.html) · [libseat session](https://docs.rs/smithay/0.7.0/smithay/backend/session/libseat/index.html) · [presentation](https://docs.rs/smithay/0.7.0/smithay/wayland/presentation/index.html)
- Kernel v7.2: [rockchip_drm_vop2.c](https://github.com/torvalds/linux/blob/v7.2/drivers/gpu/drm/rockchip/rockchip_drm_vop2.c) · [rockchip_vop2_reg.c](https://github.com/torvalds/linux/blob/v7.2/drivers/gpu/drm/rockchip/rockchip_vop2_reg.c)
- Compositors: [niri redraw loop](https://github.com/YaLTeR/niri/blob/main/docs/wiki/Development:-Redraw-Loop.md) · [niri design principles](https://github.com/YaLTeR/niri/blob/main/docs/wiki/Development:-Design-Principles.md) · [niri animations](https://github.com/YaLTeR/niri/blob/main/docs/wiki/Configuration:-Animations.md) · [Jay Cargo.toml](https://github.com/mahkoh/jay/blob/master/Cargo.toml) · [Pinnacle](https://github.com/pinnacle-comp/pinnacle) · [cage](https://github.com/cage-kiosk/cage) · [Weston kiosk-shell](https://wayland.pages.freedesktop.org/weston/toc/kiosk-shell.html) · [weston.ini(5)](https://manpages.debian.org/trixie/weston/weston.ini.5.en.html) · [Qt Wayland Compositor](https://doc.qt.io/qt-6/qtwaylandcompositor-index.html) · [WaylandHardwareLayer](https://doc.qt.io/qt-6/qml-qtwayland-compositor-waylandhardwarelayer.html)
- Protocols: [presentation-time](https://wayland.app/protocols/presentation-time) · [fifo-v1](https://wayland.app/protocols/fifo-v1)
- Android: [WM Shell Transitions.java](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/libs/WindowManager/Shell/src/com/android/wm/shell/transition/Transitions.java) · [RemoteTransitionHandler.java](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/libs/WindowManager/Shell/src/com/android/wm/shell/transition/RemoteTransitionHandler.java) · [Android 12 untrusted touches](https://developer.android.com/about/versions/12/behavior-changes-all) · [requesting permissions](https://developer.android.com/training/permissions/requesting) · [Car App template restrictions](https://developer.android.com/training/cars/apps/library/template-restrictions)
- Apple: [CPListTemplate](https://developer.apple.com/documentation/carplay/cplisttemplate) · [CPInterfaceController](https://developer.apple.com/documentation/carplay/cpinterfacecontroller)
- Slint 1.18.1: [ListView](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/reference/std-widgets/views/listview.mdx) · [linuxkms backend](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/guide/backends-and-renderers/backend_linuxkms.mdx) · [Skia renderer](https://github.com/slint-ui/slint/blob/v1.18.1/internal/renderers/skia/lib.rs) · [Skia GL surface](https://github.com/slint-ui/slint/blob/v1.18.1/internal/renderers/skia/opengl_surface.rs) · [FemtoVG renderer](https://github.com/slint-ui/slint/blob/v1.18.1/internal/renderers/femtovg/lib.rs) · [SharedPixelBuffer](https://docs.rs/slint/1.18.1/slint/struct.SharedPixelBuffer.html)
- Boot: [plymouth(1), trixie](https://manpages.debian.org/trixie/plymouth/plymouth.1.en.html)
- Repository context: system ADRs 0002 to 0006 and 0009 to 0011; system research (architecture, toolkit, text input); os-architecture spec; design system; experience budgets; interface mockups; blackbird-os image-composition research; faceplate-touch ADR 0001.
