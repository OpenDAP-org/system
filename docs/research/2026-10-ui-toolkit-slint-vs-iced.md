# UI toolkit for DAP OS: Slint versus Iced (and the alternatives)

- **Date:** 2026-10-22
- **Status:** Research report. Input to the toolkit decision left open by [ADR 0005](../adr/0005-sdk-defined-services-components-and-schemas.md) (ADR 0002 named Slint; ADR 0005 makes the toolkit "an implementation detail of the host")
- **Question from the maintainers:** COSMIC (System76) uses Iced. Is Slint the right toolkit? Since ADR 0005: which toolkit is the better **host renderer** for the SDK-defined component model, plus the shell chrome?
- **Inputs read:** [ADR 0002](../adr/0002-system-architecture.md), [ADR 0003](../adr/0003-native-binaries-no-wasm.md), [ADR 0004](../adr/0004-hat-mode-activities-use-the-product-ui-system.md), [ADR 0005](../adr/0005-sdk-defined-services-components-and-schemas.md), [system architecture research](2026-10-system-architecture.md), [system requirements](../requirements/README.md).
- **Conventions:** versions and dates are those published on 2026-10-22. Claims not checked against a primary source are marked **UNVERIFIED**. Numbers measured for this report say where they were measured; **none were measured on the RK3566**. The design-system repository is now called `ui-components`.

## 1. Summary

1. **ADR 0005 changes the question.** Capabilities no longer ship toolkit code or toolkit markup: components are authored in Rust with the SDK and compile to versioned, schema-validated **data** that the host renders with `ui-components`. The toolkit only has to be a good **host renderer of a runtime-defined tree**, plus the compositor chrome. Licence exposure to third parties disappears, and Slint's main exclusive feature (a runtime loader for its own language) becomes an implementation option rather than a requirement (§6).
2. **Rendering a data-defined tree is more natural in Iced** (`view()` is already a function from data to widgets, with unlimited depth and no compile step). **In Slint** it works by generating `.slint` source from the SDK data and compiling it with `slint-interpreter` 1.18.1, whose API is stable; compiled Slint components alone cannot render arbitrary trees because Slint has no recursive components ([#10391](https://github.com/slint-ui/slint/issues/10391), open) (§6.2).
3. **Everything around the tree favours Slint:** fine-grained property updates instead of rebuilding the view on every message (battery, high-rate bindings), declarative animations and transitions, AccessKit accessibility (upstream Iced has none: draft PR [#3111](https://github.com/iced-rs/iced/pull/3111); issue [#552](https://github.com/iced-rs/iced/issues/552) open since 2020), gestures and focus, GLES-first rendering, a stable 1.x API with a minor release every 2 to 4 months, and an embedded-focused vendor (§7 to §12).
4. **Mali-G52 favours a GLES-first toolkit.** Mesa's PanVK refuses to load on Bifrost v7 (Mali-G52) unless `PAN_I_WANT_A_BROKEN_VULKAN_DRIVER=1` is set ([panvk_physical_device.c](https://gitlab.freedesktop.org/mesa/mesa/-/blob/main/src/panfrost/vulkan/panvk_physical_device.c)); GLES 3.1 is conformant ([Mesa Panfrost docs](https://docs.mesa3d.org/drivers/panfrost.html)). Slint's default FemtoVG renderer is plain GLES. Iced renders through wgpu, whose GLES backend is "Downlevel/Best Effort" ([wgpu README](https://github.com/gfx-rs/wgpu/blob/trunk/README.md)), or on the CPU with tiny-skia (§4).
5. **COSMIC is evidence that Iced works for a desktop on desktop GPUs, not for a low-power ARM product.** It runs on a heavily patched fork (`pop-os/iced`, which adds accessibility and layer-shell), its compositor renders only with GLES, Raspberry Pi users hit wgpu limit crashes, and idle-CPU and heat complaints are open (§10).
6. **Licence:** Slint's royalty-free licence excludes embedded systems, so the host uses Slint under GPLv3 (the system is GPL anyway) or a commercial licence from USD 1 per device ([LICENSE.md](https://github.com/slint-ui/slint/blob/v1.18.1/LICENSE.md), [pricing](https://slint.dev/pricing)). With ADR 0005 no third party links the toolkit, so this is acceptable. Iced is MIT (§3).
7. **Hands-on (Docker, aarch64 Linux, Debian trixie, on an Apple Silicon Mac):** a minimal counter app is 18.8 MB stripped with 334 crates (Slint defaults), 13.6 MB with 306 crates (Slint, Wayland and FemtoVG only) and 14.5 MB with 230 crates (Iced defaults). On a headless Weston with CPU rendering, idle RSS was 16.4 MB for Slint and 14.0 MB for Iced, and **both used no measurable CPU in 20 s of idle** (§11).
8. **Recommendation (§13):** use **Slint** as the host renderer and for the chrome, with SDK component data translated to generated `.slint` and compiled by `slint-interpreter`, and keep the renderer behind an OpenDAP trait with a conformance suite so Iced stays a viable fallback. Gate it on device measurements and a spike.

## 2. Requirements that drive the choice

| Driver | Source | What it asks of the toolkit |
|---|---|---|
| SDK-defined components rendered by the host from versioned data | ADR 0005, SYS-005, SYS-007, SYS-018 | Build and update a UI tree whose shape is known only at runtime |
| Dynamic two-way data binding | SYS-008, ADR 0005 | Properties settable from the bus at runtime, user edits flowing back |
| One design system (`ui-components`) | SYS-009 | Tokens, styles, components with full visual control |
| Heavy visuals (spectrograms, oscilloscope, waterfalls) as host primitives fed by `memfd` | ADR 0005 | Efficient image or custom-geometry primitives updated at display rate |
| Product, not OS | SYS-017 | No native look; no windows |
| One process per activity; Smithay compositor drawing the chrome | ADR 0002 | Wayland client support, and a way to draw inside the compositor |
| Battery life and audio first | T1-009, OS-020 | Redraw on demand, no idle wake-ups, small memory, cheap rendering on Mali-G52 |
| System GPL-3.0-or-later; SDK and `ui-components` MPL-2.0; closed third-party capabilities allowed | Owner brief | Licence compatible with all three |
| Native aarch64, no WASM | ADR 0003 | — |
| Inputs: 3.5" 640×480 IPS touch faceplate; small OLED/AMOLED faceplates with buttons, rotary dials and HID | Faceplate specs | Touch, gestures, focus navigation, key and dial input |
| Verification in the real product, functional and visual | Project DoD | Headless rendering, screenshot tests, introspection for automation |

## 3. Licence fit

| Toolkit | Licence (2026-10-22) | Host in `system` (GPL-3.0-or-later) | `ui-components` and SDK (MPL-2.0) | Closed third-party capability |
|---|---|---|---|---|
| Slint 1.18.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` ([crates.io](https://crates.io/crates/slint), [LICENSE.md](https://github.com/slint-ui/slint/blob/v1.18.1/LICENSE.md)) | Yes, under GPLv3. The combined host binary is effectively GPL-3.0-only | MPL-2.0 files may be combined into a GPL binary (MPL-2.0 §3.3) | Under ADR 0005 a capability ships data and a service that does not link the toolkit: no exposure. A closed binary that **links** Slint would need a commercial licence; royalty-free excludes embedded ("a computer system designed to perform a specific task within a larger mechanical or electrical system"); commercial embedded "from $1.00 per device" ([pricing](https://slint.dev/pricing)) |
| Iced 0.14.0 | MIT ([crates.io](https://crates.io/crates/iced)) | Yes | Yes | Yes |
| libcosmic | MPL-2.0 ([repo](https://github.com/pop-os/libcosmic)) | Yes | Yes | Yes |
| Freya 0.4.3, Floem 0.2.0, Xilem/Masonry 0.4.0, Blitz 0.2.3, gpui 0.2.2 | MIT; MIT; Apache-2.0; MIT OR Apache-2.0; Apache-2.0 ([crates.io](https://crates.io/)) | Yes | Yes | Yes |

Remaining Slint licence points under ADR 0005:

- `ui-components` implemented in `.slint` is MPL-2.0 source, but it is only usable with Slint; an OEM wanting a proprietary build of the host would need a Slint commercial licence. The host is GPL anyway, so this only matters if the licensing strategy changes.
- Before ADR 0005 the open question was whether a third-party `.slint` file interpreted in a GPL host is "data" ([GPL FAQ: IfInterpreterIsGPL](https://www.gnu.org/licenses/gpl-faq.html#IfInterpreterIsGPL)). With ADR 0005 the host generates the `.slint` itself from schema-validated data, so third-party material is plainly data (**UNVERIFIED** legally; worth one confirmation).
- Whether DAP OS is an "embedded system" under the royalty-free terms is moot while the host uses GPLv3.

## 4. Rendering on Mali-G52 (Panfrost / PanVK)

### 4.1 Driver status

| Item | Status | Source |
|---|---|---|
| GLES on Mali-G52 (Bifrost v7) | OpenGL ES 3.1, Khronos-conformant (submission 949); desktop GL 3.1 | [Mesa Panfrost docs](https://docs.mesa3d.org/drivers/panfrost.html) |
| Vulkan on Mali-G52 (PanVK) | Bifrost listed as Vulkan 1.3, but architectures v6, v7, v11 and v14 are **gated as experimental**: "panvk is not well-tested on v%d, pass PAN_I_WANT_A_BROKEN_VULKAN_DRIVER=1". Only Valhall v10 (for example Mali-G610) loads by default, and it is Vulkan 1.4 conformant | [panvk_physical_device.c (main, Mesa 27.0-devel)](https://gitlab.freedesktop.org/mesa/mesa/-/blob/main/src/panfrost/vulkan/panvk_physical_device.c), [Mesa Panfrost docs](https://docs.mesa3d.org/drivers/panfrost.html) |
| Mesa in Debian trixie | 25.0.7-2+deb13u1; trixie-backports 26.1.6-1~bpo13+1 | [sources.debian.org](https://sources.debian.org/src/mesa/) |

Consequence: on the RK3566 every toolkit renders with **GLES** or on the **CPU**. Toolkits whose primary path is Vulkan (wgpu's first-class backends, Vello's compute pipeline, gpui on wgpu) run on their fallback. Whether the same gate is in Mesa 25.0.7 is **UNVERIFIED**; check on the device with `vulkaninfo`.

### 4.2 Toolkit renderers

| Toolkit | Renderers | Path on Mali-G52 | Notes |
|---|---|---|---|
| Slint 1.18.1 | FemtoVG on OpenGL ES 2.0+ via `glow` (default), FemtoVG on wgpu, Skia (GL or Vulkan), software, Vello (experimental) ([Cargo features](https://github.com/slint-ui/slint/blob/v1.18.1/api/rs/slint/Cargo.toml)) | **FemtoVG on GLES**; software renderer as fallback; Skia GL to measure | Software renderer: partial repaint (`RepaintBufferType`), rotation, RGB565 ([docs](https://docs.rs/slint/1.18.1/slint/platform/software_renderer/index.html)). A Mali EGL/GBM fix landed in 2024 ([#5719](https://github.com/slint-ui/slint/pull/5719)) |
| Iced 0.14.0 | `iced_wgpu` (wgpu 27), `iced_tiny_skia` (CPU), runtime fallback between them ([Cargo.toml 0.14.0](https://github.com/iced-rs/iced/blob/0.14.0/Cargo.toml)) | **wgpu's GL backend** (best-effort tier) or **tiny-skia** on the Cortex-A55 | Low-end GPUs hit wgpu limits: "Dimension X value 4096 exceeds the limit of 2048" on a Raspberry Pi 4 ([libcosmic #1084](https://github.com/pop-os/libcosmic/issues/1084)), surface-size panics on a Raspberry Pi 5 ([iced #2552](https://github.com/iced-rs/iced/issues/2552)). Mali-G52 limits are higher, so these exact crashes are unlikely at 640×480 (**UNVERIFIED**) |
| libcosmic / COSMIC | Iced fork on wgpu 28 | — | cosmic-comp "only has an OpenGL ES backend and cosmic apps also have a software rendering fallback" (Drakulix, [cosmic-epoch #3831](https://github.com/pop-os/cosmic-epoch/issues/3831)) |
| Freya 0.4.3 | Skia (`skia-safe`) GL/Vulkan | Skia GL | Skia is a large C++ dependency |
| Xilem / Masonry 0.4.0 | Vello on wgpu; `vello_hybrid`, `vello_cpu` options ([masonry Cargo.toml](https://github.com/linebender/xilem/blob/main/masonry/Cargo.toml)) | Poor fit: compute shaders on wgpu-GL; hybrid and CPU paths are young | — |
| gpui (Zed) 0.2.2 | `gpui_wgpu` on Linux ([gpui_linux Cargo.toml](https://github.com/zed-industries/zed/blob/main/crates/gpui_linux/Cargo.toml)) | wgpu GL | Desktop editor toolkit |
| Dioxus native / Blitz 0.2.3 | Vello through `anyrender` (**UNVERIFIED**) | As Vello | HTML/CSS engine |

### 4.3 Host primitives for heavy visuals (plot, canvas)

| Need | Slint | Iced |
|---|---|---|
| Waterfall or spectrogram from a `memfd` ring | Write rows into a `SharedPixelBuffer` and show it as an `Image` (CPU upload each frame); or draw with raw GL under or over the scene with `Window::set_rendering_notifier` on the FemtoVG path | `image` handle updated per frame; or the `shader` widget with a custom wgpu pipeline (on the GL backend on Mali-G52) |
| Oscilloscope or plot lines | `Path` elements with commands from data; or the rendering notifier | `canvas` with a geometry cache (`lyon` tessellation) |
| Verdict | Workable; GL notifier gives full control on GLES | Workable; `shader` is more flexible but sits on the best-effort GL backend |

## 5. Wayland and Smithay integration

| Topic | Slint | Iced |
|---|---|---|
| Wayland client (activity hosts) | winit 0.30; Wayland-only build via `backend-winit-wayland` | winit 0.30 (`wayland` feature); the System76 fork adds an SCTK shell with layer-shell |
| Drawing inside the compositor (product chrome) | Custom `Platform` with `SoftwareRenderer` into a buffer with damage, or `FemtoVGRenderer` with an `OpenGLInterface` on the compositor's EGL context ([platform module](https://docs.rs/slint/1.18.1/slint/platform/index.html)). No production precedent: the only Smithay plus Slint project found is a "vibe coded" desktop by Slint's co-founder ([ogoffart/slint-compositor](https://github.com/ogoffart/slint-compositor), created 2026-06-03, no licence file) | **Proven in production:** cosmic-comp renders Iced elements in-compositor with `iced_tiny_skia` into Smithay `MemoryRenderBuffer`s with damage ([src/utils/iced/mod.rs](https://github.com/pop-os/cosmic-comp/blob/master/src/utils/iced/mod.rs)), on Smithay 0.7.0 with `renderer_glow` ([Cargo.toml](https://github.com/pop-os/cosmic-comp/blob/master/Cargo.toml)) |
| Bare KMS (recovery UI) | `backend-linuxkms` (EGL or Vulkan, libinput, libseat) | None upstream (**UNVERIFIED** third-party crates) |

This is Iced's strongest point: COSMIC already solved "toolkit widgets inside a Smithay compositor". The Slint version is the same technique with the software renderer, but OpenDAP writes it.

## 6. Dynamic UI components

### 6.1 Mechanisms considered

| Mechanism | Description | Status |
|---|---|---|
| (a) Toolkit markup loaded at runtime | Third parties ship `.slint` files that the host loads with `slint-interpreter` (Iced has no equivalent) | Superseded by (d). Slint's embedding pieces (`ComponentContainer`, named slots, `interface`) are still experimental and Rust-only in 1.18.1 ([experimental docs](https://github.com/slint-ui/slint/tree/v1.18.1/docs/astro/src/content/docs/guide/experimental), tracking [#2390](https://github.com/slint-ui/slint/issues/2390)) |
| (b) Out-of-process surfaces | The capability renders its own Wayland surface; the compositor composes it into the slot | Escape hatch only (§6.4) |
| (c) Native plugins (`dlopen`) | Host loads third-party machine code | Rejected: no stable Rust ABI, and third-party code inside the host defeats the sandbox and the "no capability code in the UI process" rule (architecture research §5) |
| **(d) SDK-defined components as data** | Rust SDK types and macros compile to versioned, schema-validated data; the host interprets them with `ui-components`; events and bindings over Varlink; heavy visuals through host primitives | **Accepted** ([ADR 0005](../adr/0005-sdk-defined-services-components-and-schemas.md)) |

### 6.2 Which toolkit renders a runtime-defined tree better

| Aspect | Iced | Slint |
|---|---|---|
| Mapping data to widgets | Direct: a recursive `fn render(node) -> Element` matching on the node type; any depth; no compile step. Structural changes are free (the next `view()` reflects them) | Two routes. **Route 1 (recommended):** translate the component data into generated `.slint` source that imports `ui-components`, compile it with `slint_interpreter::Compiler`, instantiate it, and bind properties by name. Depth is known at generation time, so the lack of recursive components does not matter. **Route 2:** a fixed set of compiled Slint components driven by models (`for` and `if` over typed rows). Works for fixed-shape templates only, because recursive components are not supported ([#10391](https://github.com/slint-ui/slint/issues/10391)) |
| API stability of the mechanism | Stable public API (pre-1.0 crate, breaking changes each minor) | Interpreter API is stable 1.x; only its `internal` feature is unstable ([docs.rs](https://docs.rs/slint-interpreter/1.18.1/slint_interpreter/)). Route 1 can avoid `ComponentContainer` by generating the **whole activity screen** (chrome slots plus contributed components) as one interpreted component; `ComponentContainer` becomes an optimisation, not a dependency |
| Cost of a structural change (component installed, updated or removed) | None beyond the next `view()` | Recompile the generated screen (milliseconds to tens of milliseconds, **UNVERIFIED** on a Cortex-A55; measure). Rare event; cache compiled definitions per component hash in memory |
| Cost of a value update (bound property changes) | `update()` then a full `view()` rebuild and diff of the widget tree; layout of rebuilt nodes. `lazy` widgets can memoise subtrees. A position bound at display rate means a view rebuild per frame unless isolated in a `canvas` | Set one property; Slint's dependency tracking re-evaluates only dependent bindings and repaints only dirty items. Better fit for high-rate bindings and for battery |
| Two-way binding | Emulated: widget emits a message, host maps it to `SetX` on the bus | Native: generated source uses `<=>` and change callbacks; host maps callbacks to `SetX` |
| Animations and transitions declared in component data | Host must implement interpolation per animated property with the `Animation` API ([#2757](https://github.com/iced-rs/iced/pull/2757)) and request frames | Map directly to `animate`, `states` and transitions, including spring and path animations ([1.18 blog](https://slint.dev/blog/slint-1.18-released)); runs without host code |
| Safety of the generation step | Not applicable | String values must never be spliced into source: generated code declares typed properties and the host sets values through the API; identifiers come from the schema, not from capability data |
| Accessibility of rendered components | None upstream | AccessKit roles and names from `ui-components`, automatically for every generated component |

Reading: Iced is the more natural *interpreter*; Slint is the better *runtime* once the tree exists. With Route 1, the extra cost for Slint is a code generator plus a compile step on install, which is small next to writing accessibility, animation and gesture support for Iced.

### 6.3 Prior art for SDK-defined, host-rendered components

| System | Model | Lesson for OpenDAP |
|---|---|---|
| Android `RemoteViews` and Jetpack Glance | A view hierarchy described in one process and inflated in another, limited to an allow-list of layouts and widgets; Glance compiles Compose-style Kotlin into `RemoteViews` ([RemoteViews](https://developer.android.com/reference/android/widget/RemoteViews), [Glance](https://developer.android.com/develop/ui/compose/glance)) | The closest match to ADR 0005: author in the platform language, ship a restricted tree. The allow-list is what keeps it safe and consistent |
| Apple WidgetKit and Live Activities | SwiftUI views rendered into timelines of archived snapshots by the system; limited interactivity through intents ([WidgetKit](https://developer.apple.com/documentation/widgetkit)) | Precomputed states plus system-driven transitions save power; interactivity goes back to the maintainers process as intents |
| Microsoft Adaptive Cards | JSON schema rendered natively by each host; "Purely declarative - No code is needed or allowed"; "Card Authors own the content, Host App owns the look and feel" ([overview](https://learn.microsoft.com/en-us/adaptive-cards/)) | Versioned schema, host-owned styling, fallbacks for unknown elements, several native renderers for one schema: proof that the renderer can stay an implementation detail |
| Server-driven UI (Airbnb Ghost Platform and similar) | Backend sends a typed section tree; clients render with their design system (**UNVERIFIED** details; no primary source fetched) | Typed sections and actions, versioned per client capability |
| Pebble timeline pins, Home Assistant cards, Rockbox WPS | System layouts filled with data; actions back to the maintainers (architecture research §3) | Templates first; components as data |

### 6.4 Out-of-process surfaces (escape hatch)

Kept for HAT modes whose visuals cannot be expressed with host primitives. Costs: Wayland subsurfaces work only within one client ([wl_subcompositor](https://wayland.app/protocols/wayland#wl_subcompositor)) and there is no standard cross-client embedding (GTK 4 removed `GtkSocket`/`GtkPlug`, [migration guide](https://docs.gtk.org/gtk4/migrating-3to4.html)), so OpenDAP would add a private Smithay protocol; one more process, font cache and EGL context per visible component; frame synchronisation across processes (Android's `SurfaceControlViewHost` is the precedent, [reference](https://developer.android.com/reference/android/view/SurfaceControlViewHost)); input, focus and accessibility-tree stitching; and either a second `ui-components` implementation or a Slint licence question for closed code.

### 6.5 How ADR 0005 changes the verdict

- It removes Slint's biggest risk for third parties (licence) and its biggest exclusive advantage (third-party markup loading).
- It adds a small advantage for Iced (direct data-to-widget mapping).
- It does not change the platform criteria (GLES on Mali-G52, accessibility, animations, fine-grained updates, input, maturity), where Slint leads.
- Net: Slint still leads, by a smaller margin, and the toolkit choice becomes **reversible** if the host renderer sits behind an OpenDAP trait with a conformance suite of component data and expected screenshots.

## 7. Declarative model and data binding (SYS-008)

| Topic | Slint | Iced |
|---|---|---|
| Model | Declarative `.slint` DSL, compiled to Rust or interpreted; reactive properties with dependency tracking; `in`, `out`, `in-out`; callbacks; globals; models (`ModelRc`) | Elm architecture: `State`, `Message`, `update`, `view`; the view tree is rebuilt from state |
| Two-way binding | `<=>` between properties | By convention through messages |
| Bus binding layer (`dap-ui-bind`, architecture research §5.5) | Bus values set properties or models; change callbacks become `SetX(value, expected_seq)` | Bus `Watch` streams become `Subscription`s; edits become `Task`s |

## 8. Theming and `ui-components`

- **Slint:** tokens as `global` singletons; component libraries resolved through `@library` paths (`set_library_paths` in the interpreter); `Palette` and `StyleMetrics` globals ([style docs](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/reference/std-widgets/style.mdx)). `ui-components` becomes a `.slint` library plus a Rust crate that maps SDK node types to it.
- **Iced:** `Theme` and `Catalog` traits and closure-based styles (0.13+). libcosmic proves a full design system on Iced, as Rust code.
- Both allow full visual control (SYS-017).

## 9. Text, i18n and accessibility

| Topic | Slint 1.18.1 | Iced 0.14.0 |
|---|---|---|
| Text stack | Linebender `parley` 0.11 with `complex-scripts`, `fontique`, `skrifa` and `swash` ([i-slint-core Cargo.toml](https://github.com/slint-ui/slint/blob/v1.18.1/internal/core/Cargo.toml)) | `cosmic-text` 0.15 (shaping, bidi, fallback) ([Cargo.toml](https://github.com/iced-rs/iced/blob/0.14.0/Cargo.toml)) |
| RTL layout mirroring | Not supported: [#2294](https://github.com/slint-ui/slint/issues/2294) open since 2023; RTL ellipsis bug [#11750](https://github.com/slint-ui/slint/issues/11750) | No layout mirroring (**UNVERIFIED**) |
| Translations | `@tr()`, gettext or bundled translations, `tr-extractor` tool ([translations guide](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/guide/development/translations.mdx)). Under ADR 0005 the SDK's string catalogues feed properties, so either works | None built in; `fluent` or gettext in Rust |
| IME | Yes (winit) | Yes since 0.14 ([#2777](https://github.com/iced-rs/iced/pull/2777)) |
| Accessibility | AccessKit in the winit backend (`accessibility`, on by default); 1.18 exposes text-input content and selection ([1.18 blog](https://slint.dev/blog/slint-1.18-released)). Not in `linuxkms` | **None upstream.** Draft PR [#3111](https://github.com/iced-rs/iced/pull/3111) (Button and Text only, updated 2026-09-18); PR [#3281](https://github.com/iced-rs/iced/pull/3281) closed by the maintainer: "Thanks! But I'll work on this myself." (2026-03). The `pop-os/iced` fork enables `a11y` by default ([fork Cargo.toml](https://github.com/pop-os/iced/blob/master/Cargo.toml)) |

## 10. Input, animation, power and COSMIC's record

### 10.1 Input

| Input | Slint | Iced |
|---|---|---|
| Touch | `TouchArea`, `Flickable`, `SwipeGestureHandler`, `ScaleRotateGestureHandler` ([native-gestures example](https://github.com/slint-ui/slint/tree/v1.18.1/examples/native-gestures)) | `touch::Event`; no gesture recognisers; `scrollable` handles touch drags |
| Keyboard, focus, D-pad | `FocusScope`, `forward-focus`, focus chain ([focus guide](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/guide/development/focus.mdx)) | Focus operations exist; the accessibility PR author notes the "keyboard navigation and focus system" needs more work ([#3281](https://github.com/iced-rs/iced/pull/3281)) |
| Rotary dials (`REL_DIAL`) | Neither maps dials; `dap-input` in the compositor translates them into key or scroll events per input profile | Same |

### 10.2 Animation

- **Slint:** declarative `animate`, states and transitions, spring and path animations (1.18). Frames run only while an animation runs.
- **Iced:** `Animation` API since 0.14; animations are state in `update`, with redraw requests per frame.

### 10.3 Power behaviour

| Topic | Slint | Iced |
|---|---|---|
| Redraw on demand | Yes: only when a property that affects rendering changes or an animation runs | Yes since 0.14: "Reactive rendering" ([#2662](https://github.com/iced-rs/iced/pull/2662)) |
| Damage | Software renderer: partial repaint. FemtoVG: full-window repaint per frame (**UNVERIFIED** whether damage reaches Wayland on the GL path) | tiny-skia: incremental since 0.10; wgpu: full frame |
| Known idle issues | Wayland busy loop only with the Node.js/Deno event-loop integration ([#5780](https://github.com/slint-ui/slint/issues/5780)) | See §10.4 for COSMIC |

### 10.4 COSMIC since its stable release (real-world record)

| Topic | Evidence |
|---|---|
| Release and cadence | Stable with Pop!_OS 24.04 LTS on 2025-12-11 ([System76](https://system76.com/blog/post/pop-os-letter-from-our-founder)); roadmap for Epochs 2 and 3 published 2026-02-04 ([System76](https://system76.com/blog/post/cosmic-epoch-2-and-3-roadmap)); point releases Epoch 1.1 (2026-06-23) to 1.11 (2026-10-20), roughly every one to two weeks ([Phoronix 1.11](https://www.phoronix.com/news/COSMIC-Epoch-1.11), [Phoronix 1.1](https://www.phoronix.com/news/COSMIC-Epoch-1.1)) |
| Stack | cosmic-comp 1.0.0 on Smithay 0.7.0, GPL-3.0 ([Cargo.toml](https://github.com/pop-os/cosmic-comp/blob/master/Cargo.toml)); apps on libcosmic (MPL-2.0) over the `pop-os/iced` fork (wgpu 28, `a11y`, SCTK) |
| Renderer lesson | Epoch 1.10 moved the launcher from the software renderer to wgpu because scrolling became "unbearably slow (approx 1fps)" ([Phoronix](https://www.phoronix.com/news/COSMIC-Epoch-1.10)), on desktop CPUs. A warning for tiny-skia on Cortex-A55 |
| Idle CPU and heat | cosmic-comp at 2% idle on a laptop ([#972](https://github.com/pop-os/cosmic-comp/issues/972), open since 2024-11, 20 to 80% reports in 2026 on NVIDIA and DisplayLink setups); 60% idle on a Framework 13 AMD ([#2351](https://github.com/pop-os/cosmic-comp/issues/2351)); higher idle temperature than GNOME on a ThinkPad T440 ([#2692](https://github.com/pop-os/cosmic-comp/issues/2692), later partly traced to RustDesk). Early 2024 alpha bug: time applet at 60% CPU ([cosmic-epoch #772](https://github.com/pop-os/cosmic-epoch/issues/772)) |
| Memory | `cosmic-files-applet` holding 108 MiB of GPU memory at idle ([cosmic-epoch #2557](https://github.com/pop-os/cosmic-epoch/issues/2557)); a launcher OOM case at >5 GB ([#3486](https://github.com/pop-os/cosmic-epoch/issues/3486), duplicate of a launcher bug) |
| ARM and low-end GPUs | Raspberry Pi 4: wgpu atlas exceeds the 2048 limit ([libcosmic #1084](https://github.com/pop-os/libcosmic/issues/1084), open); early Pi attempt failed ([cosmic-epoch #164](https://github.com/pop-os/cosmic-epoch/issues/164)); Iced on Pi 5 and Pi with wgpu: open crashes ([#2552](https://github.com/iced-rs/iced/issues/2552), [#2337](https://github.com/iced-rs/iced/issues/2337)); non-Vulkan GPUs: apps fall back to GL or software ([#3831](https://github.com/pop-os/cosmic-epoch/issues/3831)). No reports found for Mali/Panfrost or Asahi beyond one Asahi colour-inversion bug ([#2750](https://github.com/pop-os/cosmic-epoch/issues/2750)) |
| Benchmarks | Phoronix benchmarked Pop!_OS 24.04 beta on a Threadripper workstation ([review](https://www.phoronix.com/review/popos-2404-beta)); no published COSMIC memory, idle-power or ARM measurements were found (**UNVERIFIED**: the web search budget was exhausted during this research, so the search covered issue trackers, System76's blog and Phoronix only) |

Reading: COSMIC shows that Iced can carry a full desktop and that System76 invests heavily, but in a fork, on desktop GPUs, with power problems mostly in its compositor. It is not evidence for or against Iced on Mali-G52.

### 10.5 Published Slint numbers

- "fits in less than 300 KiB RAM" on MCUs ([slint.dev/embedded](https://slint.dev/embedded)); no Linux-class footprint numbers from SixtyFPS (**UNVERIFIED**).
- Third-party report: a desktop app at ~10 MB (1.13.1), >100 MB (1.14.1) and ~50 MB (1.15.1) on Windows ([#10837](https://github.com/slint-ui/slint/issues/10837), open; maintainers could not reproduce on Linux, 2026-10-18). On Windows 1.18 moved Skia to wgpu/D3D12 and a user saw 200 MB instead of ~30 MB; the maintainer explains the old default was software rendering, trading "lower memory consumption with a higher CPU usage and energy consumption" ([#13713](https://github.com/slint-ui/slint/issues/13713)). Renderer choice dominates memory.
- No third-party Slint measurements on i.MX, Toradex or Raspberry Pi with RSS, idle CPU or frame times were found (**UNVERIFIED**).

## 11. Hands-on measurements (this report)

Test app, identical in both toolkits: a 640×480 window with a text label showing a counter and a button that increments it.

### 11.1 Build size and dependency count (aarch64 Linux)

`cargo build --release`, `strip = true`, no LTO, 4 CPUs.

| Build | Toolkit features | Stripped binary | Crates (unique, `cargo tree -e normal`) | `NEEDED` libraries | Clean build |
|---|---|---|---|---|---|
| `slint_hello` | Slint 1.18.1 defaults (winit X11 + Wayland, FemtoVG, software, accessibility, system tray) | 18.8 MB | 334 | `libfontconfig.so.1`, libc, libm, libgcc_s | 116 s |
| `slint_min` | Slint 1.18.1 `std`, `compat-1-2`, `backend-winit-wayland`, `renderer-femtovg` | 13.6 MB | 306 | `libfontconfig.so.1`, libc, libm, libgcc_s | 109 s |
| `iced_hello` | Iced 0.14.0 defaults (wgpu, tiny-skia, X11 + Wayland; no accessibility) | 14.5 MB | 230 | libc, libm, libgcc_s | 97 s |
| `iced_min` | Iced 0.14.0 `wgpu`, `wayland`, `thread-pool` | not measured | — | — | cancelled (load above 8) |

Both toolkits are in the same range. Slint carries more crates (zbus for AT-SPI and the tray, parley and fontique); Iced carries wgpu and naga. EGL and GL are loaded at runtime by both. LTO, `opt-level = "s"` and `panic = "abort"` were not tried.

### 11.2 Idle footprint (aarch64 Linux, headless Weston 14.0.2, CPU rendering)

Setup: Weston `--backend=headless --renderer=pixman` at 640×480 in the container; each app started, left 6 s, then sampled over 20 s of idle (`utime + stime` ticks at `CLK_TCK=100`, `VmRSS`, `Pss` from `smaps_rollup`). Mesa 25.0.7's GL driver could not be installed (a mirror kept dropping the LLVM package), so **GPU paths were not measured**; these are CPU-rendering numbers in a VM, useful only to compare the two toolkits with each other.

| Run | Renderer actually used | RSS | PSS | Threads | CPU over 20 s idle |
|---|---|---|---|---|---|
| Slint defaults, `SLINT_BACKEND=winit-software` | Slint software renderer | 16.4 MB | 13.2 MB | 2 | 0 ticks |
| Iced defaults, `ICED_BACKEND=tiny-skia` | tiny-skia | 14.0 MB | 11.3 MB | 7 | 1 tick (10 ms) |
| Slint defaults, no override (EGL present, no Mesa driver) | Not confirmed: `libEGL` mapped, no Mesa driver; no frame was checked | 12.9 MB | 10.7 MB | 2 | 0 ticks |
| Slint FemtoVG-only build, no override | Not confirmed, as above | 10.4 MB | 8.4 MB | 2 | 0 ticks |
| Iced defaults, no override | Not confirmed, as above | 15.6 MB | 12.9 MB | 7 | 0 ticks |

Reading: on CPU rendering both idle at zero or near-zero CPU and use 11 to 16 MB, so neither has an idle-loop problem in this setup; Iced starts more threads (its executor and thread pool). Startup time and frame times were not measured. Device numbers (GLES on Panfrost, GPU memory, energy) remain the decisive missing data.

## 12. Maturity, backing, tooling and testing

| Topic | Slint | Iced |
|---|---|---|
| Latest release | 1.18.1, 2026-09-21 ([release](https://github.com/slint-ui/slint/releases/tag/v1.18.1)) | 0.14.0, 2025-12-07 ([CHANGELOG](https://github.com/iced-rs/iced/blob/master/CHANGELOG.md)); `iced_winit` 0.14.1 on 2026-09-13; `[Unreleased]` empty on 2026-10-22 |
| Cadence | Minor every 2 to 4 months (1.13 2025-09, 1.14 2025-10, 1.15 2026-02, 1.16 2026-04, 1.17 2026-06, 1.18 2026-09) | Minor every 7 to 15 months (0.12 2024-02, 0.13 2024-09, 0.14 2025-12) |
| API stability | 1.x semver since 2023 with `compat-1-x` features; experimental features gated; third-party-version APIs suffixed (`unstable-wgpu-30`) | Pre-1.0; breaking changes each minor (0.13 replaced `Command` with `Task`) |
| Backing | SixtyFPS GmbH, revenue from commercial licences, embedded focus; vendor logos for NXP, Raspberry Pi, TI, Renesas, ST, NVIDIA ([slint.dev/embedded](https://slint.dev/embedded)) | Héctor Ramón (hecrj) leads; System76 funds work but ships its own fork |
| GitHub (2026-10-22) | 24.2k stars, 747 open issues and PRs | 31.8k stars, 512 open issues and PRs |
| Embedded Linux track record | linuxkms backend, MCU support, embedded customers (**UNVERIFIED** beyond logos) | Little upstream; COSMIC on desktops, Raspberry Pi with issues |
| Tooling | `slint-lsp`, live preview, `slint-viewer`, SlintPad, Figma inspector plugin and `figma_import` ([tools](https://github.com/slint-ui/slint/tree/v1.18.1/tools)) | Rust tooling; `comet` debugger with time travel and metrics; hot reloading (0.14, [#3000](https://github.com/iced-rs/iced/pull/3000)) |
| Testing and automation | Testing backend (headless, mock time, element search), marked "Preliminary" and pinned to `=x.y.z` ([README](https://github.com/slint-ui/slint/blob/v1.18.1/internal/backends/testing/README.md)); `system-testing` feature for remote introspection and control; `mcp` feature exposes the running UI to MCP clients ([Cargo features](https://github.com/slint-ui/slint/blob/v1.18.1/api/rs/slint/Cargo.toml)); screenshots with the software renderer | `iced_test`: headless `Simulator` (`click`, `typewrite`, selectors), `snapshot` with `matches_hash` and `matches_image`, and the `Ice` end-to-end format ([docs.rs](https://docs.rs/iced_test/0.14.0/iced_test/)): cleaner and documented |

Under ADR 0005 the most valuable test layer is toolkit-independent: component data in, screenshot and accessibility tree out, run against the host renderer.

### 12.1 Other Rust toolkits (brief)

| Toolkit | Version (2026-10-22) | Verdict |
|---|---|---|
| Freya | 0.4.3 (0.5.0-rc.10 on 2026-10-18), MIT | Dioxus-style components on Skia; desktop focus; heavy C++ dependency. No |
| Floem | 0.2.0 (2024-11-14), MIT | Fine-grained reactivity fits binding, but no release in two years. No |
| Xilem / Masonry | 0.4.0 (2025-10-29), Apache-2.0; Vello 0.11.0 | Strong accessibility design; alpha; Vello's compute path is wrong for Mali-G52. Watch |
| Dioxus native / Blitz | 0.7.10 / 0.2.3 (0.8 alpha), MIT OR Apache-2.0 | HTML/CSS engine; heavy and young. No |
| gpui | 0.2.2 on crates.io (2025-10-22), Apache-2.0 | Zed's editor toolkit on wgpu; no embedded intent. No |

## 13. Recommendation

**Recommendation:** use **Slint** as the host renderer for SDK-defined components and for the shell chrome, under GPLv3, with these rules:

1. **Host renderer behind a trait.** `ui-components` defines the node types (from the SDK schema) and a `HostRenderer` trait; the Slint implementation is the first one. A conformance suite (component data in; screenshot, accessibility tree and bus calls out) keeps an Iced implementation possible, as ADR 0005 intends.
2. **Route 1 for components:** translate component data into generated `.slint` that imports the `ui-components` Slint library, compile it with `slint-interpreter` when the component is installed or updated, cache compiled definitions per hash, and bind values only through properties and callbacks (never by splicing data into source). Generate the whole activity screen so that `ComponentContainer` is not required; adopt it when it becomes stable.
3. **Fixed templates** (tier A) can be compiled Slint components driven by models.
4. **Chrome:** Slint in the compositor through a custom platform with the software renderer and partial repaint into Smithay buffers (the cosmic-comp technique), or FemtoVG on the compositor's EGL context if measurements favour it.
5. **Heavy visuals:** host primitives with `SharedPixelBuffer` images first, GL through the rendering notifier if needed.
6. **Out-of-process surfaces** only for HAT modes that host primitives cannot serve; **no `dlopen`**.

Gates before the decision is frozen:

- **Device gate:** on the RK3566 with trixie Mesa (and backports), measure per-activity RSS and PSS, GPU memory, idle wake-ups (zero on static screens), list-scroll and now-playing frame times, compile time of a generated screen, and energy over 10 minutes idle and 10 minutes scrolling, for FemtoVG, Skia GL and the software renderer, with an Iced prototype of the same screens (wgpu-GL and tiny-skia) as control.
- **Spike gate:** one SDK component (for example the lyrics tab) authored in Rust, compiled to data, rendered by both a Slint and a minimal Iced `HostRenderer`, with two-way binding over Varlink and one declared transition.

Why not Iced, even though COSMIC uses it: under ADR 0005 Iced's natural data-to-view mapping is a real advantage, but the host would still need accessibility (absent upstream), declarative animations, gestures and focus, and a GLES-first path on Mali-G52; COSMIC solved the first by forking and runs on Vulkan-class desktop GPUs. Slint has these today, with a stable API and an embedded vendor.

### 13.1 What would change this recommendation

- **Device gate results:** Slint on Panfrost clearly worse than Iced in idle wake-ups, memory per activity, energy, or compile time of generated screens (for example, seconds on the A55).
- **Upstream Iced gains accessibility and a focus system** (watch [#3111](https://github.com/iced-rs/iced/pull/3111)) and wgpu's GL backend proves solid on Panfrost: the gap shrinks to animations and fine-grained updates, and Iced's MIT licence and in-compositor precedent could tip it.
- **SixtyFPS changes the GPL option or its business falters:** switch the `HostRenderer` implementation.
- **Out-of-process components become the main mechanism:** then the toolkit inside each third-party process matters for licensing, and an MIT toolkit gains weight.

## 14. Open questions

1. How long does `slint-interpreter` take to compile a generated activity screen on a Cortex-A55, and is an in-memory cache per component hash enough?
2. Real memory, GPU memory and energy per activity host on the RK3566 with FemtoVG versus Skia GL versus software (device gate).
3. Is PanVK's Bifrost gate the same in Mesa 25.0.7 (trixie) and 26.1 (backports)? Does any component need Vulkan at all?
4. Does Slint's FemtoVG path send damage to the compositor, and does Smithay use it to avoid full recomposition on Panfrost?
5. Which node types and primitives does the first SDK component schema include (list, text, synced text, form controls, image, plot, canvas), and how are transitions expressed in data?
6. RTL layout mirroring (Slint [#2294](https://github.com/slint-ui/slint/issues/2294)): which launch languages need it?
7. One-line legal confirmation that schema-validated component data from third parties is not a work combined with the GPL host.
8. If out-of-process surfaces are ever allowed, how are accessibility trees stitched across processes (AccessKit cross-process subtrees, **UNVERIFIED**)?
