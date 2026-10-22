# OpenDAP OS system software: architecture research and proposal

- **Date:** 2026-10-22
- **Status:** research. Everything under "Proposal" or "Recommendation" is a recommendation for the maintainers. None of it is a decision, and no ADR is implied.
- **Scope:** the OpenDAP "system" layer: the bus, the orchestrator, the UI runtime, the capability model and its manifest, schemas and the registry, the audio engine's placement, security, power, the SDK, and how each piece is verified on the virtual T1.
- **Inputs read:** `system/docs/requirements` and ADR 0001, `opendap-os` requirements, ADRs 0001–0005 and research (base distribution, image composition, OS lifecycle, boot chain), `t1` requirements, faceplate-interface spec, ADRs and the experiment review, `board` requirements and ADRs, `hat-standard` requirements, ADR and research, `toolkit` requirements, specs (`opendap-cli`, component export) and research (agentic CLI, verification instrumentation, software toolchain), `ui-components` and every `faceplates/*` requirements file.

## 0. How evidence is marked

- Versions and dates were read on 2026-10-21/22 from crates.io, GitHub/GitLab/Gerrit releases and tags, the projects' official docs, man pages (manpages.debian.org for trixie) and kernel sources at tag `v7.2`. Each claim links its source.
- **UNVERIFIED** marks anything not confirmed against a primary source in this session: claims from memory, pages that could not be fetched, or behaviour that needs hardware.
- No benchmark was run for this report. Every latency figure is a vendor number for other hardware. Section 4.1 says what must be measured on the target before any choice becomes final.
- The four themes were researched in parallel. The web-search budget ran out partway through, so some secondary items (listed in §10) stayed unverified.

---

## 1. Key findings (read this first)

1. **The maintainers' vision maps onto proven patterns almost point by point.** Fuchsia's components v2 is "capabilities + manifests + routing" ([capabilities](https://fuchsia.dev/fuchsia-src/concepts/components/v2/capabilities)). VS Code contribution points are "contributions into existing views, drawn from the manifest without running code" ([contribution points](https://code.visualstudio.com/api/references/contribution-points)). Android's Car App Library is "the host renders templates, the app supplies data and logic" ([cars/apps](https://developer.android.com/training/cars/apps)). Android's media controls are "the host owns 5 slots, the app supplies command + icon + label" ([media controls](https://developer.android.com/media/implement/surfaces/mobile)). Nothing in the vision is speculative. The novelty is combining these on standard Linux.
2. **Recommended bus: Varlink for the control plane, plus sealed `memfd` rings passed over it for the data plane.** Varlink is systemd's own IPC: brokerless, introspectable, Unix-socket based, with public `sd-varlink` since systemd 257, which is the version trixie ships ([NEWS](https://github.com/systemd/systemd/blob/main/NEWS)). `zlink` 0.7.1 is a Rust implementation by the zbus author ([zlink](https://github.com/z-galaxy/zlink)). `memfd` + `SCM_RIGHTS` is the zero-copy mechanism PipeWire and Wayland already use ([PipeWire native protocol](https://docs.pipewire.org/page_native_protocol.html), [memfd_create(2)](https://man7.org/linux/man-pages/man2/memfd_create.2.html)). iceoryx2 0.10.0 is faster (vendor figure: 0.92 µs on a Raspberry Pi 4 vs 9.2 µs for a Unix socket), but it has no access control yet and aarch64 Linux is only tier 2 for it ([roadmap](https://github.com/eclipse-iceoryx/iceoryx2/blob/main/ROADMAP.md)).
3. **"More modern than CBOR, highest performance" matters only on the data plane.** Control messages (play, like, set volume) are small and infrequent, so introspection, agent access (MCP is JSON) and cross-language reach matter more than encoding speed. Audio, spectrogram and level-meter frames should be **fixed-layout binary in shared memory**, with no serialization at all. **bincode is dead** (development ceased; 3.0.0 is a tombstone crate that fails to compile, [crates.io](https://crates.io/crates/bincode/3.0.0)). rkyv has no schema evolution ([feature matrix](https://github.com/rkyv/rkyv/blob/main/book_src/feature-comparison.md)), so it does not suit a public, versioned third-party API.
4. **Slint 1.18.1 (2026-09-21) can load capability UI at runtime, but only experimentally.** `slint-interpreter` plus `ComponentContainer`/`ComponentFactory` exist, are Rust-only and are gated behind `SLINT_ENABLE_EXPERIMENTAL_FEATURES`. Named slots and `interface`/`implement`, which map exactly onto the slot model, are also experimental ([component-container doc](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/guide/experimental/component-container.mdx)). **Proposal:** start with host-rendered **templates** (data-only contributions, as in Car App) and add declarative `.slint` contributions once the API stabilises.
5. **Slint licence: the royalty-free licence excludes embedded systems.** OpenDAP OS must use Slint under **GPLv3** (compatible with `system`'s GPL-3.0-or-later) or a commercial licence ([LICENSE.md](https://github.com/slint-ui/slint/blob/v1.18.1/LICENSE.md)). One question then follows for third parties: what licence applies to a third-party `.slint` file interpreted inside a GPL process. This is an owner and legal question (Q2).
6. **Accessibility requires Wayland and winit.** Slint's AccessKit integration exists only in the winit backend. `linuxkms` has no accessibility tree ([selector Cargo.toml](https://github.com/slint-ui/slint/blob/v1.18.1/internal/backends/selector/Cargo.toml)). That supports the Wayland architecture already chosen, with linuxkms kept only for recovery.
7. **The orchestrator fits systemd well, with one gap.** Transient units, slices, `FreezeUnit`/`ThawUnit` (since 246), socket activation, `Accept=yes` per-connection instances and the full sandbox set are all in trixie's 257 ([org.freedesktop.systemd1(5)](https://manpages.debian.org/trixie/systemd/org.freedesktop.systemd1.5.en.html)). **systemd has no uclamp setting**, so the orchestrator must write `cpu.uclamp.min/max` itself in a delegated subtree ([cgroup-v2](https://github.com/torvalds/linux/blob/v7.2/Documentation/admin-guide/cgroup-v2.rst)).
8. **Native DSD over the on-board I2S is blocked on the SoC side.** The mainline `cs43130` driver (which handles the CS43198) has DSD and DoP DAIs. But both Rockchip I2S drivers in v7.2 accept only S8/S16/S20_3/S24/S32, with no DSD formats ([rockchip_i2s_tdm.c](https://github.com/torvalds/linux/blob/v7.2/sound/soc/rockchip/rockchip_i2s_tdm.c)). The driver also limits DSD and DoP to 176.4 and 352.8 kHz, which means DSD64 and DSD128 only ([cs43130.c](https://github.com/torvalds/linux/blob/v7.2/sound/soc/codecs/cs43130.c)). **DoP over S24_LE should work with stock mainline. Native DSD on-board needs a kernel change** (tension with OS-002), while native DSD over USB DACs is unaffected. This is an inference from source code, not a hardware result (Q6).
9. **The proven background-pause pattern is the freezer, and blocking calls into frozen processes must be designed out.** Android freezes cached apps through the cgroup v2 freezer, and a synchronous binder call into a frozen process gets the server killed ([cached apps freezer](https://source.android.com/docs/core/perf/cached-apps-freezer)). Lomiri pauses apps with SIGSTOP over their cgroup ([jobs-base.cpp](https://gitlab.com/ubports/development/core/lomiri-app-launch/-/blob/main/liblomiri-app-launch/jobs-base.cpp)). **Proposal:** activities are bus *clients only*, never providers, so freezing one can never block a service.
10. **"Always through one package endpoint" has a clean precedent: Go's module proxy plus checksum log, and TUF metadata with expiry** ([go.dev/ref/mod](https://go.dev/ref/mod), [TUF](https://theupdateframework.io/docs/metadata/)). Recalls become a signed, expiring deny list. AWS's `tough` is a Rust TUF client ([tough](https://github.com/awslabs/tough)).
11. **The UI must feel like a dedicated product, not an OS, and the activity/slot model is what makes that compatible with extensibility.** This is a maintainers' principle (2026-10-22). Appliance UIs that are extensible all share one shape: the product owns navigation and layout, and extensions supply data or a few capped entries.
    - Rockbox themes declare the now-playing screen from data tags without code ([wps_tags.tex](https://raw.githubusercontent.com/Rockbox/rockbox/master/manual/appendix/wps_tags.tex)).
    - Playdate lets a game add at most **three** items to the system menu ([Inside Playdate](https://sdk.play.date/inside-playdate/)).
    - Pebble timeline pins are rendered by system layouts, and apps only attach actions ([pin structure](https://developer.rebble.io/guides/pebble-timeline/pin-structure/)).
    - reMarkable is Linux with one product application, Xochitl, started by systemd ([Xochitl](https://developer.remarkable.com/documentation/xochitl)).

    **Proposal:** no launcher, no app grid, no app icons, no windows, no notification shade and no per-capability settings app. Capabilities appear only as tasks inside product activities (§5.7, §3.7).
12. **The "bit-perfect yes/no" signal has two good references.** Roon's signal path classes each stage as Lossless, Enhanced, High Quality or Low Quality ([signal path](https://help.roonlabs.com/portal/en/kb/articles/signal-path)). Android 14's `MIXER_BEHAVIOR_BIT_PERFECT` sends media "as is down to the HAL" with no framework volume ([AudioMixerAttributes](https://developer.android.com/reference/android/media/AudioMixerAttributes)). **Proposal:** the engine publishes a typed `SignalPath`, and the verification harness asserts it against captured bits.

---

## 2. Requirements in scope

| Group | IDs | What they ask of the system layer |
|---|---|---|
| Extension model | SYS-001, SYS-002, SYS-005, SYS-007 | Capabilities with manifests (needs/provides), versioned schemas and UI components in a registry, contributions to existing activities |
| Services and bus | SYS-003, SYS-004 | Unix-philosophy services: ephemeral or persistent, foreground or background; a standardised bus API |
| UI | SYS-006, SYS-008, SYS-009, DS-001 | Activities with a lifecycle, dynamic two-way binding, one design system |
| Distribution | SYS-010, SYS-011, HCS-007…011, HCS-017 (Draft), opendap-os ADR 0004, ADR 0005 | Reverse-domain/well-known HTTP fetch, always through the package endpoint; signed bundles on the data partition; signed HAT manifests on the same mechanism; unlocked device, HATs trusted for now |
| Audio | OS-020, T1-008, BRD-022/023 (Draft), SYS-012 (Draft) | Bit-perfect PCM and DSD (native and DoP), audio quality first, hearing-safety dose |
| Power | T1-009, BRD-008, BRD-009 | Battery life first |
| Platform | T1-011, OS-002, OS-003, OS-005, OS-006, OS-021 | Standard, trick-less Linux, no custom kernel, minimal default image |
| Peripherals | OS-008, BRD-004/005/014/015, HCS-013, T1-023/024, ADR t1-0002 | OpenDAP OS hosts every chip; faceplate controls as HID over I2C |
| Tooling | T1-025, T1-026, TK-002, TK-009, OS-009, OS-016, OS-018, OS-019, TK-012/013 | Agentic CLI with MCP; everything verified in software on the virtual T1 |
| Language | system ADR 0001 | Rust for all custom software and the SDK |

Constraints that come from earlier research, not from requirements: systemd stays as PID 1 and the orchestrator is a service that uses transient units (image-composition §2.4). The product compositor is Smithay, with cage for bring-up (§4.3). PipeWire handles shared audio, and an exclusive ALSA engine handles bit-perfect playback (§5.3). Hardware: RK3566 (4× Cortex-A55, Mali-G52 on Panfrost, GLES 3.1), most likely 4 GB of RAM (BRD-037, Draft), a 480 × 800 touch display or an OLED with physical controls.

---

## 3. Benchmark: what to learn from each system

### 3.1 Android

| Mechanism | Facts (source) | Lesson for OpenDAP OS |
|---|---|---|
| Activity lifecycle | `onCreate → onStart → onResume → onPause → onStop → onDestroy`. Lifecycle state maps to how likely the process is to be killed ([activity-lifecycle](https://developer.android.com/guide/components/activities/activity-lifecycle)) | Take the states and the state-to-killability table directly (§5.8) |
| Low-memory killer | `onTrimMemory()` warnings first, then kills by `oom_adj_score` ([memory-management](https://developer.android.com/topic/performance/memory-management)) | The orchestrator sets `OOMScoreAdjust=` per state and sends a "trim" bus event before killing |
| Cached-apps freezer | Android 11+ on cgroup v2. Android 14 freezes cached apps about 10 s after they become cached. A synchronous binder call into a frozen process kills the server ([AOSP](https://source.android.com/docs/core/perf/cached-apps-freezer)) | Freeze stopped activities after a grace period. Activities must never serve bus calls |
| Foreground service types | Android 14 makes types mandatory (`mediaPlayback`, `connectedDevice`, `shortService` about 3 min, …), each with its own permission and time limits ([fgs types](https://developer.android.com/develop/background-work/services/fgs/service-types)) | Typed background classes in the manifest (`playback`, `transfer`, `device`, `short`), each with a budget |
| Intents and filters | Declarative "what I can handle". Services only through explicit intents. `exported` is required ([intents](https://developer.android.com/guide/components/intents-filters)) | "Handlers" in the manifest (open a URI or file type, share), resolved by the orchestrator |
| Stable AIDL | Frozen API dumps with a hash. Only appends allowed. A missing method fails predictably (`UNKNOWN_TRANSACTION`) ([stable AIDL](https://source.android.com/docs/core/architecture/aidl/stable-aidl)) | The same append-only rule and a schema hash per interface version (§5.6) |
| Permissions | Install-time (normal, signature), runtime (dangerous), special (appop) ([permissions](https://developer.android.com/guide/topics/permissions/overview)) | Three permission classes (§5.12) |
| Doze and wake locks | Doze ignores wake locks and batches work into maintenance windows. "You shouldn't use wake locks if there are any suitable alternatives" ([doze](https://developer.android.com/training/monitoring-device-state/doze-standby), [wakelock](https://developer.android.com/develop/background-work/background-tasks/awake/wakelock)) | Wake requests go through the orchestrator, are typed and time-limited, and map to logind inhibitors (§5.13) |
| Media controls | The host draws 5 slots (3 in compact mode). Apps fill them with `CommandButton`s (command + stock icon + label), e.g. "Save to favorites" ([media controls](https://developer.android.com/media/implement/surfaces/mobile)) | Almost exactly the "like action" requirement. Copy the slot cap and stock icons |
| Car App Library | Host-rendered templates. The app supplies data and logic (`CarAppService → Session → Screen`). Quota of 5 templates per task, where a same-content update counts as a free refresh ([template restrictions](https://developer.android.com/training/cars/apps/library/template-restrictions)) | **The closest model for contributed tabs:** templates first, with quotas |
| Slices | `androidx.slice` last released as 1.1.0-alpha02 on 2021-01-13, never stable ([releases](https://developer.android.com/jetpack/androidx/releases/slice)) | Warning: a remote-UI format that the host does not own and evolve dies |
| App Shortcuts | Static, dynamic and pinned. `shortcuts.xml` can also declare capabilities ([shortcuts](https://developer.android.com/develop/ui/views/launch/shortcuts)) | Static contributions read from the manifest, without starting the app |

### 3.2 Fuchsia (components v2)

- **Manifests:** `.cml` (JSON5) compiled to a binary `.cm` (FIDL `ComponentDecl`), with program, children, collections and capability routes ([component manifests](https://fuchsia.dev/fuchsia-src/concepts/components/v2/component_manifests)).
- **Seven capability types** (protocol, service, directory, storage, dictionary, resolver, runner), routed with `use` / `offer` / `expose` ([capabilities](https://fuchsia.dev/fuchsia-src/concepts/components/v2/capabilities)). A component sees only what was routed into its namespace.
- **Lifecycle:** Created → Resolved → Started → Stopped → Destroyed. Components start **lazily when something binds to an exposed capability** ([lifecycle](https://fuchsia.dev/fuchsia-src/concepts/components/v2/lifecycle)).
- **FIDL versioning:** `@available(added, deprecated, removed, replaced)` with `NEXT`/`HEAD`. It is "API versioning, not ABI versioning", with no runtime query ([versioning](https://fuchsia.dev/fuchsia-src/reference/fidl/language/versioning)).
- **Status:** active. F31 shipped 2026-07-22 ([F31](https://fuchsia.dev/whats-new/release-notes/f31)). The session framework and element manager could not be read this session (UNVERIFIED).
- **Lessons:**
  - **Routing by namespace** is the strongest idea to borrow. On Linux it maps to "bind-mount only the sockets the manifest declares into the unit's mount namespace" (§5.4).
  - Lazy start on first bind maps to systemd socket activation.
  - Add a runtime-queryable version, which FIDL lacks and stable AIDL has.

### 3.3 Linux mobile and embedded shells

| Shell | Version (date) | How it hosts the UI and the app lifecycle | Lesson |
|---|---|---|---|
| Phosh / phoc | 0.58.0 (2026-10-04) ([phosh](https://gitlab.gnome.org/World/Phosh/phosh/-/releases)) | A GTK shell as a privileged client of the wlroots-based phoc compositor | Splitting the shell from the compositor costs an extra process and protocol. A Smithay compositor can render its own shell |
| Plasma Mobile | 6.8.1 (2026-10-20) ([tags](https://invent.kde.org/plasma/plasma-mobile/-/tags)) | KWin plus a QML shell on Plasma and Kirigami ([plasma-mobile.org](https://plasma-mobile.org/)) | Desktop-derived. Too heavy as a model for 4 GB and a battery-first device |
| Ubuntu Touch / Lomiri | OTA 24.04-2.1 (2026-10-16); Lomiri 0.6.2 (2026-09-08) | Mir. Background apps are paused with **SIGSTOP over their cgroup**, their OOM score raised, and an `ApplicationPaused` signal sent. qtmir models `SuspendingWaitSession → SuspendingWaitProcess → Suspended`, with an exemption list ([jobs-base.cpp](https://gitlab.com/ubports/development/core/lomiri-app-launch/-/blob/main/liblomiri-app-launch/jobs-base.cpp), [qtmir](https://gitlab.com/ubports/development/core/qtmir/-/blob/main/src/modules/QtMir/Application/application.cpp)) | The closest Linux precedent for SYS-006. Use the cgroup v2 freezer (atomic, no signal races) instead of SIGSTOP. Keep the "wait for the session to settle" step and the exemption list |
| Sailfish OS | 5.0.0.71 (2025-10-30) ([releases](https://docs.sailfishos.org/Support/Releases/)) | lipstick compositor, Silica, ambiences. Details UNVERIFIED | Ambiences, where one theme drives the whole system, are a precedent for a design system driven by tokens |
| webOS OSE | 2.28.0 (2025-03-27), apparently stalled ([release](https://github.com/webosose/build-webos/releases/tag/v2.28.0)) | **LS2 "Luna Bus"**, **SAM** (app lifecycle manager), **LSM** (surface manager and compositor), `appinfo.json` ([architecture](https://www.webosose.org/docs/guides/core-topics/architecture/architecture-overview/)) | The same three-part split proposed here: bus, orchestrator and surface manager. Its own bus became a maintenance burden once the project stalled, which argues for a standard bus |
| Tizen | Current version UNVERIFIED (app docs moved to samsungtizenos.com, which returned 503) | — | No usable lesson this session |
| postmarketOS, now "Nura" | [nura.eco](https://nura.eco/) | Ships GNOME, KDE, Phosh, SXMO | Confirms that general-purpose mobile shells exist; none of them is activity-centric |

### 3.4 Car and embedded HMIs

- **Qt Application Manager 6.12.0** (2026-09-27, GPL-3.0 or commercial) ([intro](https://doc.qt.io/QtApplicationManager/introduction.html)):
  - A headless daemon with a QtWayland compositor, quick-launch, bubblewrap containers and signed TAR+YAML packages.
  - `info.yaml` manifests declare `capabilities` and **intents** (`handlingApplicationId`, `requiredCapabilities`, `parameterMatch`) ([manifest](https://doc.qt.io/QtApplicationManager/manifest.html)).
  - The same System UI runs **single-process or multi-process**. Single-process shares one QML engine, but a crash "terminates the entire program" and apps are "never really terminated" ([single vs multi](https://doc.qt.io/QtApplicationManager/singlevsmultiprocess.html)).
  - **Lesson:** keep the activity-host API identical across both process models, so the choice can be revisited after measuring memory (§5.7).
- **AGL Vimba 22.0.0** (2026-09-16): `applaunchd` exposes a gRPC `AppLauncher` service. Apps are **systemd template units** `agl-app*@*.service` ([applaunchd README](https://gerrit.automotivelinux.org/gerrit/plugins/gitiles/src/applaunchd/+/refs/heads/master/README.md)). **Lesson:** an automotive platform also landed on "apps are systemd units". Its D-Bus and gRPC interfaces "are not interoperable", a reminder to keep exactly one bus API.
- **COVESA:** vsomeip 3.7.6 is active (2026-09-17). The CommonAPI runtimes have been at 3.2.4 since 2024-10-09 and look dormant ([core](https://github.com/COVESA/capicxx-core-runtime/releases)). **Lesson:** IDL-plus-generated-bindings frameworks built only for one vertical go quiet. Prefer an IDL shared with the base OS (Varlink/systemd).
- **Slint embedded:** vendor logos for NXP, Raspberry Pi, TI, Renesas, ST and NVIDIA; "fits in less than 300 KiB RAM" on MCUs ([slint.dev/embedded](https://slint.dev/embedded)). There are no published Linux-class footprint figures (UNVERIFIED), so they must be measured.

### 3.5 Plugin and extension models

| Model | Facts | Lesson |
|---|---|---|
| **VS Code contribution points** | 38 points (commands, menus, views, viewsContainers, …). Menus refer to commands by ID with `when` clauses (`! && \|\| == =~ in`). Activation events load code lazily, and contributed commands activate implicitly since 1.74. "Only use webviews when absolutely necessary" ([contribution points](https://code.visualstudio.com/api/references/contribution-points), [when clauses](https://code.visualstudio.com/api/references/when-clause-contexts), [activation](https://code.visualstudio.com/api/references/activation-events)) | **The template for SYS-007.** The shell draws actions and tabs from manifests, and visibility from `when` predicates over context keys, **without starting the capability**. Code starts on first use |
| **WASM component model + WIT** | WASI 0.3.0 released 2026-06-11 with native async; 0.3.1 on 2026-08-11 ([roadmap](https://wasi.dev/roadmap)). Wasmtime 50.0.0 (2026-10-20). `aarch64-unknown-linux-gnu` is **tier 2**. Pulley is about 10× slower than native ([tiers](https://docs.wasmtime.dev/stability-tiers.html), [Pulley](https://docs.wasmtime.dev/examples-pulley.html)). WIT packages are `ns:name@semver` with `@since`/`@deprecated` gates. Semver matching at runtime is left to the host ([WIT.md](https://github.com/WebAssembly/component-model/blob/main/design/mvp/WIT.md)) | A credible future runtime for small, pure business-logic capabilities (Zed uses it for extensions, [zed](https://zed.dev/docs/extensions/developing-extensions)). Not needed for v1: native Rust plus systemd sandboxing covers it, and drivers must be native anyway. **Keep the bus IDL mappable to WIT** (§7) |
| wasmCloud | Stateless components plus long-lived native **capability providers**, linked over WIT ([concepts](https://wasmcloud.com/docs/concepts/)) | The same split as "business logic" vs "user-space drivers" |
| **Home Assistant** | `manifest.json`: `domain`, `dependencies` (hard) vs `after_dependencies` (soft), `iot_class`, `quality_scale` tiers ([manifest](https://contributors.home-assistant.io/core/integrations/manifest/)). Integrations add **typed entities** that the frontend renders generically | Hard vs soft needs. A quality scale for capabilities in the registry. Typed entities rendered by the host, the same idea as templates |
| **Flatpak portals** | Static `finish-args`. Portals are preferred to "blanket access". The frontend daemon has desktop-specific backends and a Permission Store ([sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html), [portals](https://flatpak.github.io/xdg-desktop-portal/docs/)) | Sensitive access goes through OS-owned services that show their own consent UI (§5.12) |
| Snap interfaces | Plugs (consumer) and slots (provider), auto-connect or manual (UNVERIFIED this session; docs login-walled) | The vocabulary of needs/provides |
| Apple App Intents / WidgetKit | Intent metadata extracted at build time. Widgets are timelines the system renders, with no live code (partly UNVERIFIED) | Static metadata plus host rendering again |

### 3.6 Music-player and DAP software

| System | Architecture (source) | Lesson |
|---|---|---|
| **MPD** 0.24.16 (2026-10-17) | A daemon controlled over a text protocol by separate clients. "Attempts to do bit-perfect playback" when using ALSA `hw:`, no software mixer, no ReplayGain and no forced format, and tells users to check `/proc/asound/.../hw_params`. ALSA output options include `dop`, `allowed_formats` (e.g. `dsd64:*=dop`), `stop_dsd_silence` and `close_on_pause`. 0.25 adds a systemd idle-inhibitor option ([user.rst](https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/doc/user.rst), [plugins](https://mpd.readthedocs.io/en/latest/plugins.html), [NEWS](https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/NEWS)) | Engine plus clients is the right split, and MPD's bit-perfect checklist becomes the engine's own `SignalPath` rules. MPD's per-format output policy (`allowed_formats`) is a good shape for the DSD policy |
| **Rockbox** v4.0 (2025-03-31) | One firmware with separate buffering, codec and playback-control threads; loadable codecs and plugins; no DSD codec ([playback.c](https://github.com/Rockbox/rockbox/blob/master/apps/playback.c), [codecs](https://github.com/Rockbox/rockbox/blob/master/lib/rbcodec/codecs/SOURCES)) | The buffer → decode → output pipeline with large buffers suits battery life. Gapless by pre-buffering the next track (UNVERIFIED in docs) |
| **Volumio 3** | A Node.js backend that drives MPD, with plugin kinds `audio_interface`, `music_service`, `system_hardware`, `system_controller`, `user_interface` ([plugins](https://developers.volumio.com/plugins/plugins-overview)) | Its plugin kinds are close to capability kinds (music source, output, hardware, controller, UI) |
| **moOde** 10.3.6 (2026-10-10) | Ships its own patched MPD build. Displays native-DSD vs DoP ([releases](https://github.com/moode-player/moode/releases)) | Users of this class want the DSD transport shown, which supports `SignalPath` |
| **HiBy / FiiO** | HiBy markets "Direct Transport Architecture (systemwide bit-perfect audio)". FiiO has "no global SRC" and an exclusive "Pure Music mode" ([HiBy RS6](https://store.hiby.com/products/hiby-rs6), [FiiO M15S](https://www.fiio.com/m15s)). Internals are proprietary (UNVERIFIED) | The competitors' marketing claims are exactly what OpenDAP must *verify* (T1-026) |
| **Android 14 bit-perfect** | `MIXER_BEHAVIOR_BIT_PERFECT`: per-app (uid), `USAGE_MEDIA` only, USB only, no framework volume ([AudioManager](https://developer.android.com/reference/android/media/AudioManager), [AOSP](https://source.android.com/docs/core/audio/preferred-mixer-attr)) | The exclusive-mode contract: one owner, media only, hardware volume only |
| **Roon** | Core ("brain"), Control and Output over RAAT. The signal path classifies each stage as Lossless, Enhanced, High Quality or Low Quality ([architecture](https://help.roonlabs.com/portal/en/kb/articles/architecture), [signal path](https://help.roonlabs.com/portal/en/kb/articles/signal-path)) | Core/remote split maps to engine and activity. **The signal-path UX is the model for the bit-perfect signal** |
| Lyrion (LMS) | The server tells the player what to fetch over SlimProto, and the player decodes ([SlimProto](https://lyrion.org/reference/slimproto-protocol/)) | Not needed on-device |
| **PipeWire** 1.6.9 / WirePlumber 0.5.18 | DSD through the ALSA plugin only when `alsa.formats` allows it (1.4.0). `node.force-rate`, `node.lock-rate`, `audio.allowed-rates`. WirePlumber implements the `ReserveDevice1` D-Bus reservation (`monitor.alsa.reserve-device`) ([props](https://github.com/PipeWire/pipewire/blob/master/doc/dox/config/pipewire-props.7.md), [features](https://github.com/PipeWire/wireplumber/blob/master/docs/rst/daemon/configuration/features.rst)). The graph is always F32 (image-composition §5.1) | Device reservation is the standard handoff to an exclusive engine |

### 3.7 Appliance-style product UIs (owner principle: no OS look)

**The maintainers' principle (2026-10-22):** the UI must not look or behave like an operating system. That rules out a launcher, an app grid, app icons, windows, a notification shade full of apps, and a settings-app paradigm. It should feel like the purpose-built UI of a consumer product with no visible OS: a classic DAP, an iPod, a synth, a camera, a hi-fi component. Capabilities extend that product UI and never appear as "apps".

| Product / system | How the UI is built | How it is extended without becoming "apps" | Lesson |
|---|---|---|---|
| **Rockbox** (v4.0) | One firmware; the product's own menu tree and a While Playing Screen (WPS) | **Themes declare the now-playing screen from data tags** (`%ia` artist, `%it` title, `%pv` volume), viewports `%V(x,y,w,h)` and conditionals `%?`, with **no code** ([wps_tags.tex](https://raw.githubusercontent.com/Rockbox/rockbox/master/manual/appendix/wps_tags.tex)). Code plugins exist ([apps/plugins](https://github.com/Rockbox/rockbox/tree/master/apps/plugins)), but they are grouped behind a single "Plugins" entry of the product menu, not shown as icons (menu placement from product knowledge, UNVERIFIED) | Data-tag templates are the oldest proof that a DAP's now-playing screen can be extended declaratively. Rockbox's one OS-like seam, a plugin list, is what OpenDAP's task model removes |
| **Playdate** | A single-purpose OS; the "System Menu" is product UI | A game may add **at most three** custom items to the System Menu (`addMenuItem`, `addCheckmarkMenuItem`, `addOptionsMenuItem`) ([Inside Playdate](https://sdk.play.date/inside-playdate/)) | **Hard slot caps** keep the product menu the product's. Same idea as Android's 5 media slots |
| **Pebble** (PebbleOS, now open source, maintained by the community at [pebble-dev/pebble-firmware](https://github.com/pebble-dev/pebble-firmware)) | Timeline-centred watch UI | **Timeline pins render with system layouts** (generic, calendar, sports, weather, reminder, notification). Apps supply data and **actions** (`openWatchApp`, `http`) ([pin structure](https://developer.rebble.io/guides/pebble-timeline/pin-structure/)) | Third parties contribute *content into the product's timeline*, in the product's visual language. A direct model for templates plus action slots. Pebble still had a watchapp launcher, the part *not* to copy |
| **reMarkable** | Linux; the single product application **Xochitl** (login, settings, documents, viewer and editor), launched by `systemd` ([Xochitl](https://developer.remarkable.com/documentation/xochitl)); third-party Qt Quick software only in Developer Mode ([docs](https://developer.remarkable.com/documentation)) | Extensibility sits outside the product UI (developer mode) | Proof that a Linux appliance reads as "a product" when one product shell owns the whole screen under systemd. Its weakness is the inverse of OpenDAP's goal: it is not extensible inside the product UI |
| **iPod (click wheel)** | Hierarchical product menus (Music, Videos, Extras, Settings) and a now-playing screen; no app icons (from product knowledge, UNVERIFIED against a primary source) | Features arrive as menu entries inside the product's own hierarchy | Navigation is a **product information architecture**, not a list of installed software |
| **Car HMIs** (Android Car App Library, QtAM, AGL) | The OEM System UI owns layout. Third-party UI comes only as host-rendered templates (Car App) or inside OEM-designed surfaces (§3.1, §3.4) | Templates with quotas | The strongest commercial precedent for "extensible, but it still looks like the car's UI" |
| **Teenage Engineering instruments, cameras, hi-fi components** | Purpose-built mode UIs tied to physical controls (UNVERIFIED: no primary source reached for TE internals) | Mostly not extensible by third parties | The feel to aim for: **modes, not apps**, and every control has one meaning per mode |
| **Kiosk shells** (cage) | One full-screen client ([cage](https://github.com/cage-kiosk/cage)) | None | The bring-up base. No product semantics |

**What the slot model has to guarantee:**

1. **The product owns navigation.** Top-level activities ("modes") are designed by OpenDAP as one information architecture (now playing, library, settings, and modes for official HATs). A capability can never add a top-level entry by itself.
2. **Capabilities add tasks, not destinations.** Actions, tabs, panels, settings sections, status chips and handlers go into slots that OpenDAP designed, with caps (Playdate's three, Android's five).
3. **Rendering is the product's.** Templates come from the design system. Components (tier B) may import only the design system. No foreign toolkit and no windows.
4. **No notification shade.** Capabilities report state through status chips and timed toasts in shell-designed places, rate-limited and capped. Persistent concerns go into the relevant product activity (e.g. a "scrobbling paused" chip on now-playing), never into a central inbox of app messages.
5. **No settings app per capability.** A capability contributes a `settings-section` (template `form`) into the product's settings, grouped by *what it does* (Lyrics, Library sources, Output), not by publisher. Install, permission and recall management is one product screen, "Extensions", not an app manager.
6. **Hardware modes are the one exception, and they are still product UI.** An official or third-party HAT that needs a full-screen tool (oscilloscope, VNA, radio) gets a **mode activity** that the product navigation shows *only while that HAT is attached*, built from templates and ui-components components. A mode is a product destination for a piece of hardware, not an app someone installed.

---

## 4. Technology evaluation

### 4.1 Bus and IPC

Criteria from the brief: latency and throughput on Cortex-A55, zero-copy, schema evolution, cross-language support, introspection (CLI, MCP, debugging), security (peer credentials, policy), and how standard and trick-less the option is.

| Option | Version (date) | Latency evidence | Zero-copy | Evolution | Cross-language | Introspection | Security | Standard Linux | Verdict |
|---|---|---|---|---|---|---|---|---|---|
| **Varlink via `zlink`** | zlink 0.7.1 (2026-09-09), MIT; systemd sd-varlink public since 257 | No ARM numbers found. One Unix-socket round trip plus JSON parsing | fd passing (zlink natively; systemd APIs since 258) | IDL; adding optional fields is compatible | Any language with JSON and a socket. C via libsystemd | **`org.varlink.service` GetInfo/GetInterfaceDescription built in**; `varlinkctl`; registry (systemd 260+) | `SO_PEERCRED` / pidfd on a plain socket; socket file permissions; no broker policy | **Systemd's own direction** (systemd 257–263 NEWS) | **Recommended control plane** |
| D-Bus via `zbus` + dbus-broker | zbus 6.0.0 (2026-10-16), MIT; dbus-broker 37 in trixie | Broker hop on every call. No ARM numbers found | fd passing (`UNIX_FDS`) | Interface versioning by name | Wide | XML introspection, busctl | `GetConnectionCredentials`, XML policy | Standard, already in the image for BlueZ, logind, RAUC | **Runner-up.** Kept for talking to system daemons |
| iceoryx2 | 0.10.0 (2026-09-18), MIT/Apache | Vendor figure on RPi 4: 0.92 µs flat from 64 B to 4 MB, vs 9.2 µs / 6.1 ms for a UDS ([data](https://github.com/eclipse-iceoryx/iceoryx2/blob/main/internal/plots/benchmark_mechanism_comparison_rp4.dat)) | True zero-copy (pub/sub, req/resp, blackboard) | FlatBuffers payloads since 0.10 | C, C++, Python, C# | `iox2` CLI basic | **No IAM yet**; shm file permissions (UNVERIFIED detail) | Not a distribution standard; aarch64 tier 2 | Candidate for the data plane later, if measurements demand it |
| Zenoh | 1.10.1 (2026-09-07) | Vendor x86 figures: P2P about 10 µs ([blog](https://zenoh.io/blog/2023-03-21-zenoh-vs-mqtt-kafka-dds/)) | SHM opt-in | — | Wide | — | Network-oriented | Network middleware, not local IPC | Rejected on-device. Possibly for device ↔ host or HAT-SoC links later |
| NNG | C 2.0.0 (2026-10-18); Rust `anng` pre-release | — | No | No | C | No | No | Library, not a platform convention | Rejected |
| Cap'n Proto RPC | capnp-rpc 0.27.1 (2026-10-20) | — | Arena reads | Strong | Several | Schema reflection | Object capabilities (UNVERIFIED detail) | Not a Linux convention | Rejected (§7): elegant, but a second ecosystem to teach |
| gRPC / tonic | 0.14.6 (2026-05-07) | HTTP/2 framing overhead | No | protobuf, strong | Widest | Reflection | TLS/mTLS-oriented | Server-world standard; AGL uses it | Rejected on-device: heavy for a handheld, no fd passing |
| tarpc | 0.38.0 (2026-08-12) | — | No | Rust-only | Rust-only | No | No | — | Rejected: no third-party language reach |

**No primary source publishes D-Bus vs Varlink vs UDS latency on a Cortex-A55.** Before the bus becomes an ADR, the maintainers should require a **measurement gate** on RK3566 hardware (or a Cortex-A55 SBC):

- round-trip p50/p99 for zlink, for zbus via dbus-broker and for a raw UDS, at 64 B and 4 KiB payloads;
- a `memfd` ring vs iceoryx2 at 4 KiB and 64 KiB frames;
- CPU time per 1,000 calls at minimum OPP.

**Zero-copy, the trick-less way:** `memfd_create` + `SCM_RIGHTS` is what PipeWire (`SPA_DATA_MemFd`, `SPA_DATA_DmaBuf`) and Wayland (`wl_shm`) do ([PipeWire buffers](https://docs.pipewire.org/page_native_protocol.html)). Seals (`F_SEAL_SHRINK`, `F_SEAL_GROW`, and `F_SEAL_FUTURE_WRITE` since Linux 5.1) let a producer keep writing while consumers can only read ([memfd_create(2)](https://man7.org/linux/man-pages/man2/memfd_create.2.html), [fcntl(2)](https://man7.org/linux/man-pages/man2/fcntl.2.html); the `F_SEAL_FUTURE_WRITE` semantics are recalled from fcntl(2), UNVERIFIED this session).

### 4.2 Serialization

Benchmark: [rust_serialization_benchmark](https://github.com/djkoloski/rust_serialization_benchmark), updated 2026-10-05, run on x86 Zen 4, **not ARM**.

| Format | Crate version (date) | `log` ser / de / size | `mesh` (bulk floats) ser / de | Schema evolution | Cross-language | Fit |
|---|---|---|---|---|---|---|
| JSON (Varlink) | serde_json | 3.63 ms / 6.20 ms / 1.83 MB | 86 ms / 103 ms | Additive, by field name | Universal | **Control plane**: small, infrequent messages; agents and MCP read it natively |
| postcard | 1.1.3 (2025-07-24) | 416 µs / 2.34 ms / 725 KB | 515 µs / 6.55 ms | None (positional) | Rust-centric | Good for **firmware links** (RP2350, HAT MCUs), where both ends are ours and versioned together |
| bitcode | 0.6.9 (2025-12-18) | 146 µs / 1.46 ms / 704 KB | 1.39 ms / 700 µs | None | Rust-only | Rejected for a public API |
| rkyv | 0.8.18 (2026-08-05) | 252 µs / 1.56 ms / 1.01 MB; access 1.2 ns | 199 µs / 183 µs; validated access 5.3 ns | **None** ([matrix](https://github.com/rkyv/rkyv/blob/main/book_src/feature-comparison.md)) | Rust-only | Internal caches only (e.g. the library index on disk) |
| bincode | 2.0.1 last; 3.0.0 tombstone | — | — | — | — | **Dead.** Do not adopt |
| wincode | 0.6.3 (2026-10-13) | — | — | None | Rust | Not needed |
| FlatBuffers | 25.12.19 (2025-12-19) | 1.05 ms / n/a | 470 µs | Tables evolve; reflection | Wide | Alternative for data-plane frames with variable layout |
| Cap'n Proto | capnp 0.27.2 (2026-09-08) | 614 µs | 9.5 ms | Strong | Wide | Rejected together with its RPC (§7) |
| CBOR (minicbor / ciborium) | minicbor 2.3.1 (2026-10-18); ciborium 0.2.2 (2024, stale) | 510 µs / 2.97 ms (minicbor) | — | Self-describing | Wide | The baseline the maintainers wanted to beat. Possible compact Varlink payload later (§5.5) |
| **Fixed-layout POD in shared memory** (`#[repr(C)]` + `zerocopy`/`bytemuck`) | — | No encoding at all | No encoding at all | Header `{schema_hash, version, len}` plus append-only struct growth | **C layout, so any language** | **Data plane**: PCM taps, spectrogram rows, levels, battery telemetry |

**Recommendation:**

- JSON over Varlink for control.
- Fixed-layout POD frames in sealed memfd rings for data.
- postcard on our own firmware links.
- No general-purpose binary format on the public bus.

This beats CBOR where speed matters (the data plane is zero-copy, with nothing to encode or decode) and does not pay for speed where it does not matter (the control plane).

### 4.3 Schemas and the registry

| Need | Option | Facts | Recommendation |
|---|---|---|---|
| Single source of truth | **Rust types with derive** | `schemars` 1.2.3 (2026-10-12) emits JSON Schema 2020-12. `zlink`'s `#[service]` emits Varlink IDL for introspection ([zlink book](https://z-galaxy.github.io/zlink/)) | Types live in Rust (`opendap-schema` crates). From them, generate: the Varlink IDL (bus), JSON Schema (manifests, models, template data) and docs. Generation runs one way only, Rust to IDL and schema |
| Third parties in other languages | Codegen from the published IDL and schema | `typify` 0.8 (JSON Schema → Rust); Varlink has generators in several languages (UNVERIFIED which are maintained) | Publish the IDL and schema in the registry. Rust is first-class (ADR 0001); other languages work through the IDL |
| Compatibility rules | Stable AIDL (append-only plus a hash); Buf breaking-change categories FILE/PACKAGE/WIRE_JSON/WIRE ([buf rules](https://buf.build/docs/breaking/rules/)) | — | **Minor version:** add optional fields, methods or enum values (receivers ignore unknown ones). **Major version:** anything else; it becomes a new interface name (`org.opendap.media.Player2`) and both are served during a deprecation window. A `dap schema check` tool runs the rules in the SDK and at the registry |
| Identity | Content-addressed IDs | postcard-rpc keys are FNV1a(path + schema); OCI uses sha256 digests | Each interface version carries `sha256(canonical IDL)`. Manifests name `org.opendap.media.Player@1.3`, and the registry resolves that to the hash. Peers exchange hashes at connect time, which provides the runtime version query that FIDL lacks |
| Registry storage | OCI artifacts / ORAS | OCI image and distribution spec 1.1 with `artifactType` and the Referrers API ([image-spec](https://github.com/opencontainers/image-spec/blob/main/manifest.md)); ORAS 1.3.5 | **Not on the device.** The device speaks only the static HTTP plus TUF-style index (§5.14). |
| Registry content | — | — | Interface IDLs, JSON Schemas, template definitions, UI component bundles, ui-components tokens, each with a version, a hash, a publisher and a quality tier (in the spirit of HA's quality scale) |

### 4.4 UI runtime

**Slint 1.18.1** (2026-09-21; `slint` and `slint-interpreter` released together) ([release](https://github.com/slint-ui/slint/releases/tag/v1.18.1)):

- **Licence:** GPLv3, royalty-free (excludes embedded) or commercial. OpenDAP OS needs GPLv3 or commercial (finding 5).
- **Interpreter:** `Compiler` (`build_from_path`/`build_from_source`, `set_file_loader`, `set_include_paths`, `set_library_paths`, `set_style`, translations), then `ComponentDefinition::create`/`create_embedded`. Properties, callbacks and globals are reachable dynamically through `Value` (including `Model(ModelRc<Value>)`). Values are `Rc` and stay on the UI thread ([api.rs](https://github.com/slint-ui/slint/blob/v1.18.1/internal/interpreter/api.rs)).
- **Two-way bindings:** `<=>` within a component tree. The interpreter received several `<=>` fixes in 1.17 and 1.18 (#13446, #12278). Cross-process binding is ours to build (§5.7).
- **Dynamic composition:** `ComponentContainer` with a `component-factory` property, plus named slots and `interface`/`implement`. **All experimental and Rust-only.** `slint::ComponentFactory` is marked "made public by mistake" (finding 4).
- **Backends:** winit (Wayland) and linuxkms (KMS, optional libseat and libinput).
- **Renderers:** FemtoVG asks for GLES 2.0, which Panfrost on G52 covers. Skia GL is the alternative, with a software fallback. Vello needs compute shaders (avoid it) ([backends doc](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/guide/backends-and-renderers/backends_and_renderers.mdx)).
- **Accessibility:** AccessKit (`accesskit_unix` → AT-SPI over zbus) on winit only (finding 6). Orca 51.1 exists. **No touch-oriented Linux screen reader was found** (UNVERIFIED). Assume OpenDAP builds its own screen-reader behaviour on AT-SPI, or adapts Orca (Q9).
- **i18n:** gettext-style `@tr()`, bundled translations, and runtime translation paths in the interpreter.

**Compositor options:**

| Option | Version | Fit |
|---|---|---|
| **Custom Smithay compositor that is the shell** | Smithay 0.7.0 (2025-06-24) on crates.io; master active, and cosmic-comp (epoch-1.11.0, 2026-10-20) and niri (v26.04) pin git revisions | **Recommended product path** (as image-composition §4.3 already found). Owns activities, focus, lifecycle, input routing and system overlays |
| cage | 0.3.1 (2026-06-30), wlroots 0.20 | Bring-up only: one full-screen client, no shell semantics |
| Slint as the only client of linuxkms | 1.18.1 | Recovery and update UI only. No multi-process isolation and no accessibility |

Relevant Wayland protocols (wayland-protocols 1.49, 2026-06-07):

- `xdg_toplevel` state **`suspended`** (added in 1.32, 2023-07-03): "not ordinarily being repainted… occluded… outputs switched off" ([xdg-shell.xml](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/stable/xdg-shell/xdg-shell.xml)).
- `xdg-activation`, `ext-foreign-toplevel-list`, `ext-session-lock`, `ext-idle-notify`, `security-context`, `fractional-scale` (staging).
- `wlr-layer-shell` lives in wlr-protocols. Not needed if the shell is rendered by the compositor itself.

### 4.5 The orchestrator's substrate

All mechanisms below are in trixie's systemd 257 unless noted ([systemd.exec(5)](https://manpages.debian.org/trixie/systemd/systemd.exec.5.en.html), [systemd.resource-control(5)](https://manpages.debian.org/trixie/systemd/systemd.resource-control.5.en.html), [NEWS](https://raw.githubusercontent.com/systemd/systemd/main/NEWS)).

| Need | Mechanism | Since |
|---|---|---|
| Ephemeral, CLI-like service | Transient `Type=oneshot`/`exec` unit through `StartTransientUnit`, or a `.socket` with **`Accept=yes`**: one service instance per connection that exits when done | Long-standing |
| Persistent engine | Static or transient unit, **socket-activated** (lazy start on first bind, as in Fuchsia) | Long-standing |
| Foreground / background | `Slice=` (`dap-fg.slice`, `dap-bg.slice`, `dap-activity.slice`), `CPUWeight=`, `MemoryHigh=`/`MemoryMax=` | 208–232 |
| Pause / resume an activity | `FreezeUnit` / `ThawUnit`, `FreezerState` on cgroup v2 `cgroup.freeze` | 246 |
| Energy hints | `cpu.uclamp.min/max`: **no systemd setting.** Written by the orchestrator in a `Delegate=yes` subtree | Kernel only |
| Memory pressure | PSI, `ManagedOOM*` (systemd-oomd), `OOMScoreAdjust=`, `MemoryPressureWatch=` | 247–257 |
| Sandboxing | `DynamicUser=`, `ProtectSystem=strict`, `PrivateDevices=`, `DeviceAllow=`, `RestrictAddressFamilies=`, `SystemCallFilter=`, `PrivateUsers=`, `PrivatePIDs=` (257), `RootImage=`, `ExtensionImages=`, `MountImages=`, `RootImagePolicy=`/`ExtensionImagePolicy=` (254), `BindReadOnlyPaths=` | 187–257 |
| Exec-from-verity only | `RestrictFileSystemAccess=` (BPF-LSM) in **261**, not in trixie; IPE in the kernel | 261 / kernel |
| Sleep / wake control | logind inhibitors `sleep`, `idle`, `handle-*`, modes `block`/`delay`/`block-weak` (257 made block locks always obeyed) ([systemd-inhibit(1)](https://manpages.debian.org/trixie/systemd/systemd-inhibit.1.en.html)) | 257 |
| Timer coalescing | `AccuracySec=` ("primarily a power saving option"), `WakeSystem=`, `TimerSlackNSec=` ([systemd.timer(5)](https://manpages.debian.org/trixie/systemd/systemd.timer.5.en.html)) | Long-standing |
| Updates | RAUC 1.15.2 upstream (trixie 1.13). `systemd-sysupdate` went back to experimental in 262 | — |
| Kernel sandbox | **Landlock ABI 10 on kernel 7.2** (UDP rules), ABI 4+ TCP, ABI 6 scoping ([syscalls.c v7.2](https://github.com/torvalds/linux/blob/v7.2/security/landlock/syscalls.c)); `landlock` crate 0.4.7 (support for ABI ≥ 8 UNVERIFIED) | Kernel 7.2 |

### 4.6 Capability packaging

| Option | Fit |
|---|---|
| **erofs image + dm-verity + Ed25519-signed manifest that pins the root hash, run with `RootImage=`/`ExtensionImages=` in a transient unit** | **Recommended**, already recommended by image-composition §7.4 and lifecycle §6.4, and consistent with opendap-os ADR 0004 ("signed bundles on the data partition") |
| `systemd-sysext` | First-party, system-wide add-ons only (developer tools, development deploys). It merges into `/usr` for everyone. Its default image policy accepts unsigned images unless `--image-policy` says otherwise ([systemd-sysext(8)](https://manpages.debian.org/trixie/systemd/systemd-sysext.8.en.html)) |
| Portable services (`portablectl`) | Close in spirit, but its own unit-file conventions and attach/detach model duplicate what the orchestrator does with transient units. It reuses the same `RootImage=` machinery underneath |
| `.deb` via apt | Rejected (lifecycle §3.3): root maintainer scripts, a writable `/usr`, no per-publisher trust |
| OCI images + a container runtime | Rejected on-device: a second runtime and image store |
| Flatpak | Rejected: desktop app model, OSTree, its own runtimes; it does not match "capabilities, not apps" |

### 4.7 Audio engine technology

| Item | Facts | Use |
|---|---|---|
| `alsa` crate | 0.12.1 (2026-07-31) | Exclusive `hw:` output, `hw_params` per track |
| `symphonia` | 0.6.1 (2026-08-13): FLAC, ALAC, WAV, AAC-LC, MP3, Vorbis. **No DSD**; Opus and WavPack not started ([README](https://github.com/pdeljanov/Symphonia)) | PCM decoding in pure Rust. Licence check needed (UNVERIFIED this session) |
| DSF/DFF | `dsd-reader` 0.4.0 and others, maturity UNVERIFIED | Write OpenDAP's own reader, shared with the toolkit's verification `dsd-src` reader (verification research §3.5). That gives one implementation, tested by the same vectors |
| PipeWire / WirePlumber | 1.6.9 / 0.5.18; `ReserveDevice1` reservation | Shared audio, Bluetooth, UI sounds; handoff to the exclusive engine |
| GStreamer | 1.28.8; DSD caps since 1.24 | Rejected as the engine: `playbin` inserts conversion and volume by default (UNVERIFIED), and auditing the path is harder than owning it |
| ALSA DSD formats | `DSD_U8`, `DSD_U16_LE/BE`, `DSD_U32_LE/BE` ([asound.h](https://github.com/torvalds/linux/blob/v7.2/include/uapi/sound/asound.h)) | Native DSD to USB DACs |
| cs43130 driver | `asp-pcm`, `asp-dop`, `xsp-dop`, `xsp-dsd`; PCM 32–384 kHz; DoP/DSD at 176.4 and 352.8 kHz only, so DSD64/DSD128; "Master DSD Playback Volume"; PCM/DSD mixing ([cs43130.c](https://github.com/torvalds/linux/blob/v7.2/sound/soc/codecs/cs43130.c)) | Defines the on-board limits (finding 8) |

---

## 5. Proposed architecture (proposal)

### 5.1 Principles

1. **Systemd is the substrate; the orchestrator is policy.** Use units, slices, the freezer, sockets and inhibitors. Re-implement none of them.
2. **One bus API, two planes.** Varlink for control, and sealed shared-memory streams (whose fds travel over Varlink) for data.
3. **Capabilities are routed, not ambient.** A capability's sandbox contains only the bus sockets its manifest `needs` and the system granted (Fuchsia routing on Linux mount namespaces).
4. **The host renders; capabilities supply data and logic.** Contributions to activities are declarative: templates first, declarative Slint components later. No capability code runs in the UI process.
5. **Manifests are read without running code.** The shell knows every action, tab and handler at install time, and capabilities start lazily on first use (VS Code activation, Fuchsia lazy start).
6. **A product, not an OS (owner principle).** No launcher, app grid, icons, windows, notification shade or settings-app paradigm. The product's information architecture is fixed by OpenDAP. Capabilities add tasks to it and never add destinations, except hardware mode activities while their HAT is attached (§3.7).
7. **Every promise has an observable.** Lifecycle states, the signal path, permissions and power budgets are bus properties that the CLI, MCP and the verification harness all read the same way.

### 5.2 Layers and processes

```text
┌──────────────────────────────────────────────── user-visible ────────────────────────────────────────────────┐
│  dap-shell  (Smithay compositor + Slint product chrome: status line, product navigation, chips, toasts,      │
│             consent and system dialogs — no launcher, no app grid, no windows, no notification shade)        │
│     │ Wayland (xdg-shell + dap-activity-v1 private protocol)                                                  │
│     ├── activity host: org.opendap.nowplaying   (Slint/winit client, first-party .slint + slots)  [frozen?]   │
│     ├── activity host: org.opendap.library      (…)                                                          │
│     └── activity host: com.vendor.scope.mode    (HAT mode activity, shown only while its HAT is attached)    │
└───────────────┬───────────────────────────────────────────────────────────────────────────────────────────────┘
                │ Varlink (control) · memfd rings (data)     ← every arrow below is one or the other
┌───────────────┴──────────────────────────── system services (Rust, systemd units) ────────────────────────────┐
│ dap-orchestrator  lifecycle · capability registry · permission broker · power policy · bus directory          │
│ dap-audiod        playback engine: decode → [DSP] → exclusive ALSA hw: | PipeWire client; SignalPath; taps   │
│ dap-library       index of music on SD/eMMC (OS-022 cache levels); search; artwork                            │
│ dap-dose          hearing-safety dose accounting (SYS-012)                                                    │
│ dap-input         faceplate HID (evdev + hidraw vendor page), buttons → semantic actions                      │
│ dap-power         battery/fuel gauge, thermal, wake policy (logind inhibitors), screen-off path               │
│ dap-fetch         capability/HAT/firmware fetch through the package endpoint; TUF-style metadata; recalls        │
│ dap-hatd          HAT EEPROM read + manifest verification → capability resolution                             │
│ dap-chipd         flash/operate/debug RP2350, HAT MCUs/FPGAs/SoCs (lifecycle research §7)                     │
│ dap-agent         device side of `opendap` CLI/MCP: introspection, input injection, screenshots (dev mode)    │
├───────────────────────────────────────── capability services (third-party & first-party) ─────────────────────┤
│ com.example.lyrics  (transient unit, RootImage=erofs+verity, DynamicUser, routed sockets only)                │
│ com.example.scrobble, com.vendor.hat-radio driver, …                                                         │
├─────────────────────────────────────────────── standard Linux ───────────────────────────────────────────────┤
│ systemd 257 (PID 1, logind, journald, udevd) · dbus-broker (BlueZ, iwd, RAUC, logind) · PipeWire/WirePlumber  │
│ RAUC · Mesa Panfrost · kernel 7.2 (ALSA, i2c-hid, Landlock, cgroup v2)                                        │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Process model for activities: **one host process per running activity** (a shared binary, `dap-activity-host`), in its own transient unit under `dap-activity.slice`.

- **Why:** isolation (a third-party template or component cannot take down the shell), per-activity freezing and memory accounting, and Android/Lomiri-style kill ordering.
- **Cost:** one Slint plus GLES context per running activity. Memory per host is **UNVERIFIED** and must be measured. Only the foreground activity and at most a few stopped ones exist at once, and destroyed ones release everything.
- **Fallback:** the host API is kept independent of the process model (the QtAM lesson). If measurements show the cost is too high, first-party activities can share one host process, while HAT mode activities keep their own.

### 5.3 Domain model

| Concept | Definition | Lives in | Identified by |
|---|---|---|---|
| **Capability** | The unit of distribution and trust: a signed bundle with one manifest that declares what it **needs** and **provides** | `/var/lib/dap/capabilities/<id>/<version>/` (data partition) | Reverse-domain ID + semver, e.g. `com.example.lyrics@1.4.0` |
| **Service** | A process a capability provides. Kind: `ephemeral` (one call or connection, then exit), `persistent` (an engine), or `driver` (bound to hardware). Class: `foreground` or one of the typed background classes (`playback`, `transfer`, `device`, `short`) | systemd unit (transient, or static for the base system) | `<capability-id>/<service-name>` |
| **Interface** | A versioned bus contract (a Varlink interface) that a service implements | Registry; IDL inside the bundle | `org.opendap.media.Player@1.3` + sha256 |
| **Schema** | A versioned data type used by interfaces, template data, settings and stored documents | Registry; generated from Rust types | `org.opendap.media.Track@1.2` + sha256 |
| **Activity** | A full-screen product surface (a "mode" of the device) with a lifecycle (§5.8). Part of the product's information architecture designed by OpenDAP (`org.opendap.*`). The only exception is a **HAT mode activity**, provided by a HAT's capability and listed by the product navigation only while that HAT is attached (§3.7). Never an "app" a user installs and launches | Activity host process; Wayland toplevel | `<capability-id>/<activity-name>` |
| **Slot** | A named extension point an activity declares, with a type (`action`, `tab`, `panel`, `settings-section`, `status-chip`), a quota and accepted templates | Activity manifest | `org.opendap.nowplaying/actions` |
| **Task** (contribution) | What a capability adds to a slot: an action (command + stock icon + label) or a tab or panel (a template or component bound to data) | The contributing capability's manifest | `<capability-id>/<task-name>` |
| **Template** | A host-owned, design-system-native layout that renders data only (`list`, `text-document`, `synced-text`, `grid`, `form`, `detail`, `toggle-row`) | Built into the activity host; versioned in the registry | `org.opendap.ui.template.SyncedText@1` |
| **Component** | A declarative `.slint` file with no code, bound to a model, used when no template fits (tier B, §5.7) | Capability bundle; registry | `com.example.lyrics/components/Karaoke@1` |
| **Manifest** | The signed declaration of everything above for one capability | Bundle root (`capability.toml`) + detached signature | Hash of its canonical form |
| **Permission** | A named right a capability `needs` and the system grants (install-time, runtime with consent, or privileged) | Manifest + grant store kept by the orchestrator | `audio.play`, `net.outbound`, `bus:org.opendap.library.Read`, … |
| **Event / stream** | Control-plane event = a Varlink streaming reply (`more`) to a `Watch*` call. Data-plane stream = a typed memfd ring | Bus | Interface method / stream schema ID |
| **Context key** | A typed fact the shell evaluates in `when` predicates (`player.state == "playing"`, `track.has_lyrics`, `net.online`, `faceplate.input == "touch"`) | Published by services, evaluated by the shell | Dotted name |
| **Handler** | An intent-like declaration ("I can open `audio/x-dsf`", "share a track", "handle URI scheme `podcast:`") | Manifest | Action + data match |
| **HAT** | Hardware whose EEPROM carries a reverse-domain ID, a manifest hash and a signature (HCS-017, Draft) that resolve to a capability | EEPROM → `dap-hatd` → `dap-fetch` | Reverse-domain ID |

### 5.4 Bus design

**Control plane: Varlink.**

- **Transport:** one Unix socket per interface implementation, at `/run/dap/bus/<interface>` (e.g. `/run/dap/bus/org.opendap.media.Player`). The base system's sockets are `.socket` units, so services start lazily.
  - The layout mirrors systemd's own `/run/varlink/registry` (systemd 260+), so moving to it later is mechanical. Trixie's 257 lacks it.
- **Implementation:** `zlink` (`#[service]`, `#[proxy]`, IDL codegen, built-in `org.varlink.service` introspection, fd passing). zbus is still used to talk to logind, BlueZ, iwd, RAUC and WirePlumber, through D-Bus.
- **Patterns:**
  - request/reply;
  - **streaming replies (`more`)** for subscriptions, e.g. `org.opendap.media.Player.Watch() → stream of State`;
  - one-shot `oneway` calls.
  - Fan-out is the provider's job (each `Watch` is one connection). Broadcast-style system events, such as `org.opendap.system.Events.Watch(filter)`, go through the orchestrator.
- **Routing and access (the Fuchsia idea on Linux):**
  - The orchestrator starts each capability service with `BindReadOnlyPaths=/run/dap/bus/<iface>` **only for interfaces in its granted `needs`**. Nothing else exists in its namespace, so a capability cannot even find an interface it was not granted.
  - Every provider, through the SDK middleware, also checks `SO_PEERCRED`/pidfd on each connection. It maps the peer's unit (`DynamicUser` UID → unit → capability ID, through the orchestrator) to its grants (defence in depth), and caches the result for the lifetime of the connection.
  - `provides` interfaces are served by sockets the orchestrator creates (the capability gets a writable bind of its own listening sockets only).
- **Discovery:** `org.opendap.bus.Directory.List()` on the orchestrator returns the interfaces visible *to the caller*, with versions and hashes. `varlinkctl info /run/dap/bus/<iface>` works on any socket for debugging.
- **Versioning:** the interface name carries the major version. The hash is exchanged as the first call (`GetInfo` plus an `org.opendap.bus.Version` method). Minor additions are optional fields that old peers ignore.
- **Errors:** typed Varlink errors (`org.opendap.media.Player.NotBitPerfect`, …) with the same dotted-code style as the CLI's error envelope (CLI spec §2), so the CLI and MCP pass them through unchanged.
- **Optional binary upgrade (later):** systemd 261 added Varlink protocol "upgrade" to a raw protocol. A hot control path could switch a connection to postcard or CBOR framing if measurements ever demand it. Not in trixie's 257 (§4.1).

**Data plane: typed streams.**

- **Setup:** a consumer calls e.g. `org.opendap.audio.Engine.OpenTap(kind: "spectrum", bins: 256, rate_hz: 30)`. The reply carries a **memfd** (sealed `F_SEAL_SHRINK | F_SEAL_GROW | F_SEAL_FUTURE_WRITE`) and an **eventfd**, passed over the Varlink connection.
- **Layout:** a header `{magic, schema_hash, version, frame_size, capacity, write_seq}`, then a ring of fixed-size `#[repr(C)]` frames (or a single-slot seqlock "blackboard" for latest-value data such as meters). Producers never block. Consumers that fall behind skip ahead using `write_seq`.
- **Lifetime:** closing the Varlink connection tears the stream down, so the producer stops computing. **This is the power rule:** no visible consumer means no FFT.
- **Media buffers:** where a GPU texture is involved (album art, video on a HAT), use `dmabuf` like PipeWire. Audio for playback never crosses the bus. It stays inside `dap-audiod` (§5.11).
- **Escalation:** if the measurement gate shows the memfd ring is too slow for a real use case, swap the transport for iceoryx2 behind the same `OpenTap` contract. The API does not change.

**Bus API standardisation (SYS-004):** every first-party interface lives in `system/schemas/` (MPL-2.0 per the README), generated from the Rust crates and published to the registry. A **bus style guide** fixes the conventions: naming (`org.opendap.<area>.<Noun>`), `Get`/`Set`/`Watch` for properties, `Open*` for streams, error naming, and pagination (cursor-based), so every interface looks alike to humans and agents.

### 5.5 How two-way binding crosses processes (SYS-008)

- A **bound property** is a Varlink triple: `GetX`, `SetX` and `WatchX` (a streaming reply), or an aggregated `Watch()` of a state struct.
- In the activity host, the SDK's `dap-ui-bind` layer:
  1. subscribes to `Watch` and writes incoming values into Slint properties or models (`set_property`/`set_global_property`, or a `ModelRc` it updates in place);
  2. installs change callbacks on two-way properties (sliders, toggles), so a user edit becomes `SetX(value, expected_seq)` (optimistic concurrency);
  3. shows the local value immediately and **reconciles** when the next `Watch` value arrives: if the provider rejected or clamped it (e.g. a volume limit from `dap-dose`), the UI snaps to the authoritative value.
- Lists use a **diffing model** (insert, remove and update by key) so long lists (the library) update incrementally, with cursor-based paging.
- Bindings are **declared in the activity or template manifest** (`volume <=> org.opendap.audio.Output.volume`), so the shell can audit them, the CLI can list them, and tests can stub them.

### 5.6 Schemas, versioning and the registry

- **Authoring:** Rust types with `#[derive(Schema)]` (an `opendap-schema` macro that combines serde, schemars and the zlink IDL derive) in versioned crates (`opendap-schema-media`, …).
- **Artifacts generated per version:** Varlink IDL, JSON Schema 2020-12, Markdown docs and a canonical-hash file (`interface.sha256`), as in stable AIDL's frozen dumps.
- **Rules:** the minor and major rules of §4.3, enforced by `dap schema check --against <previous>` in the SDK.
- **Registry layout** (static HTTP files, TUF-style metadata):

  ```text
  /registry/
    timestamp.json  snapshot.json  targets.json          # TUF-style, short expiry, signed by the index keys
    interfaces/org.opendap.media.Player/1.3/{idl,schema.json,sha256,docs.md}
    schemas/org.opendap.media.Track/1.2/{schema.json,sha256}
    templates/org.opendap.ui.template.SyncedText/1/{spec.json}
    components/com.example.lyrics/Karaoke/1.0.0/{component.slint,manifest.toml,sig}
    capabilities/com.example.lyrics/1.4.0/{capability.toml,capability.sig,payload.erofs,payload.verity}
    ui-components/tokens/2/{tokens.json}
    recalls.json                                          # signed deny list, expiring (SYS-011)
  ```

- **Publishers' own interfaces** (e.g. `com.example.lyrics.Provider@1`) are allowed, namespaced under their reverse domain, and validated by the same rules. Shared, cross-vendor interfaces (e.g. `org.opendap.lyrics.Source@1`, implemented by any lyrics capability) are proposed through the RFC process that the faceplate spec already uses for HID commands.

### 5.7 UI runtime

**dap-shell (compositor):**

- A Smithay compositor that renders the product's own chrome with Slint: a status line, the **product navigation** (a fixed information architecture designed by OpenDAP, not a list of installed software), status chips, timed toasts, consent dialogs, and volume and dose overlays.
- **What it deliberately does not have** (owner principle, §3.7):
  - a launcher, app grid or app icons;
  - a window switcher (the device has modes, and Back/Home move through the product hierarchy);
  - a notification shade (capability state goes into chips and toasts, rate-limited and capped, or into the relevant product activity);
  - a settings app per capability (capabilities contribute `settings-section` templates into the product's Settings, grouped by function; one "Extensions" screen manages installs, grants and recalls).
- Exactly one activity toplevel is visible at a time, full screen, plus system overlays.
- Transitions between activities are product animations designed with the design system, never window-manager behaviour.
- Owns focus, input routing (touch through libinput; faceplate keys through `dap-input` semantic actions; HID usages per the faceplate spec), screen on/off and DPMS, and lock.
- Tells the orchestrator which activity is foreground through a private protocol, `dap-activity-v1`: `activate`, `background`, plus a `suspended` hint mapped to the `xdg_toplevel` state.

**Activity host (`dap-activity-host`):**

1. Loads the activity's `.slint` (first-party code is compiled in; HAT mode activities go through `slint-interpreter` with `set_file_loader` restricted to the bundle and the ui-components library path).
2. Wires bindings (§5.5).
3. Asks the orchestrator for the **resolved contributions** to its slots (filtered by grants, `when` predicates, quota and user order).
4. Renders each contribution.

**Two contribution tiers:**

| Tier | What the capability ships | How it renders | When |
|---|---|---|---|
| **A: templates (v1)** | Manifest entries only: `action{command, icon: stock-id, label, when}` or `tab{template: SyncedText@1, data: <interface method>, when}` | The host's own ui-components components; the capability supplies data over the bus | From day one. Safe and consistent (SYS-009), works on both touch and OLED faceplates |
| **B: declarative components (later)** | A `.slint` file that imports only `@opendap/ui-components` and declares an `interface` of in/out properties and callbacks | `ComponentContainer` + `ComponentFactory` via the interpreter, inside the host process, with the file loader confined to the bundle | Once Slint stabilises `ComponentContainer`, named slots and `interface` (tracking [#2390](https://github.com/slint-ui/slint/issues/2390)). Gated by a review tier in the registry |

**Rules for both tiers:**

- No code runs in the UI process. Callbacks become bus calls to the contributing capability's service, made on its behalf by the host.
- Slot quotas, e.g. now-playing: **3 visible actions plus overflow**, in the spirit of Android media controls.
- Stock icons only in tier A.
- A contribution that fails to render or bind is dropped and logged, never fatal.

**Input profiles:** each activity, template and component declares the input profiles it supports (`touch`, `focus` for D-pad and select/back from the HID core set, `dial` for `REL_DIAL`/`REL_WHEEL`). Templates implement all three, which is why they come first: the classic and cassette faceplates (OLED, physical controls) get contributions for free.

**Accessibility:** activity hosts run Slint on winit/Wayland with AccessKit, so the AT-SPI tree comes for free. Templates carry accessible names and roles by construction. The screen-reader experience is open (Q9).

**Rendering:** FemtoVG on GLES (Panfrost), Skia GL as the alternative after measurement, the software renderer as fallback.

**Recovery:** Slint `linuxkms` is used for the recovery and update UI only (image-composition §4.3).

### 5.8 Lifecycle state machines

**Service (any unit started by the orchestrator):**

```text
 declared ──install+verify──► installed ──enable──► inactive ◄────────────────────────────┐
                                                        │ first bind (socket) / explicit start│
                                                        ▼                                     │
                                                   activating ──ready (sd_notify)──► active ─┤ idle timeout (ephemeral,
                                                        │                              │  │   persistent w/ IdleExitSec)
                                                        └──fail──► failed ──backoff──┘  │
                                                                     │ N failures       │ stop / budget exceeded
                                                                     ▼                  ▼
                                                                quarantined        stopping ──► inactive
 recall / uninstall from any state ──► disabled (unit removed, sockets unbound, data kept until purge)
```

- `ephemeral`: `Accept=yes` socket or a transient oneshot. It exits after its call(s), with no idle state.
- `persistent`: socket-activated. It may exit after an idle period, declared in the manifest (`idle_exit = "30s"`); the socket stays.
- `quarantined` after repeated failures (systemd `StartLimitBurst=`). The shell shows it in Settings, and the CLI shows it in `opendap capability get`.

**Activity (the Android lifecycle on the cgroup freezer):**

```text
          launch
            │
            ▼
        created ──surface mapped──► started (visible) ──focus──► resumed (interactive)
            ▲                          ▲   │                        │
            │                          │   │ overlay/dialog on top  │ another activity foreground,
            │                          │   ▼                        │ screen off
            │                          └─ paused (visible, no input)◄┘
            │                                  │ not visible
            │                                  ▼
            │                              stopped (not visible; UI work stops; xdg suspended)
            │                                  │ grace period (proposal: 5 s)
            │       relaunch / bring to front  ▼
            └───────────────────────────── frozen (cgroup.freeze = 1; 0 CPU; memory kept)
                                               │ memory pressure / LRU limit / explicit close
                                               ▼
                                           destroyed (unit stopped; state saved via SaveState())
```

- Each transition is a bus event on `org.opendap.system.Activities.Watch()` and a callback in the SDK (`on_start`, `on_resume`, `on_pause`, `on_stop`, `save_state`).
- **Before freezing,** the host gets `on_stop` and must flush. **Before destroying,** it gets `save_state`, which returns a small state blob (schema-typed, kept by the orchestrator) so a relaunch can restore it.
- **Activity hosts are bus clients only.** They never serve interfaces, so a frozen host can never block a service (the Android binder lesson). Their `Watch` subscriptions are simply not read while frozen. Providers drop subscribers whose socket buffers fill, and the host re-subscribes on thaw.
- **Kill order under pressure** (via `OOMScoreAdjust=`): destroyed < frozen third-party < frozen first-party < stopped < paused < resumed; system services and `dap-audiod` are protected.
- Background work belongs to **services**, never to activities. Now-playing being frozen does not stop music, because `dap-audiod` plays.

### 5.9 Manifest sketch

TOML, which suits a Rust-first SDK (Cargo-like). It is validated against `opendap.capability-manifest/v1` JSON Schema, and signed in canonical form. The same schema family covers HAT manifests (HCS-009).

```toml
schema = "opendap.capability-manifest/v1"

[capability]
id          = "com.example.lyrics"          # reverse domain, proved via https://example.com/.well-known/opendap/publisher.json
version     = "1.4.0"
name        = "Example Lyrics"
publisher   = "Example Ltd"
license     = "MPL-2.0"
requires-os = ">=2026.11"
sdk         = "opendap-sdk@1"

[payload]                                    # the erofs image this manifest pins
image       = "payload.erofs"
verity-root = "sha256:…"
sha256      = "sha256:…"

# ---- what it needs -------------------------------------------------------
[needs]
interfaces  = ["org.opendap.media.Player@1.3"]     # read now-playing state
permissions = ["net.outbound"]                     # install-time, shown to the user
optional-interfaces = ["org.opendap.library.Read@1"]   # soft need (HA's after_dependencies)

[needs.network]                                    # Landlock/IPAddressAllow material
hosts = ["api.example.com:443"]

# ---- what it provides ----------------------------------------------------
[[provides.service]]
name        = "lyricsd"
kind        = "persistent"                  # ephemeral | persistent | driver
class       = "short"                       # foreground | playback | transfer | device | short
exec        = "/bin/lyricsd"
idle-exit   = "60s"
memory-max  = "32M"
implements  = ["com.example.lyrics.Provider@1", "org.opendap.lyrics.Source@1"]

[[provides.task]]                           # a tab on now-playing (tier A: template)
slot        = "org.opendap.nowplaying/tabs"
name        = "lyrics"
label       = { en = "Lyrics", es = "Letra" }
template    = "org.opendap.ui.template.SyncedText@1"
data        = "org.opendap.lyrics.Source.WatchLyrics"   # bound to the current track by the host
when        = "player.track != null"

[[provides.handler]]
action      = "open"
mime        = ["text/x-lrc"]
service     = "lyricsd"
```

A second capability, contributing the "like" action:

```toml
[capability]
id = "com.example.favorites"
version = "0.3.0"

[needs]
interfaces = ["org.opendap.media.Player@1.3"]

[[provides.service]]
name = "favd"
kind = "ephemeral"                          # one call, then exit
implements = ["com.example.favorites.Store@1"]

[[provides.task]]
slot    = "org.opendap.nowplaying/actions"
name    = "like"
label   = { en = "Like" }
icon    = "stock:heart"                     # ui-components stock icon; toggled state uses "stock:heart-filled"
state   = "com.example.favorites.Store.WatchLiked"   # bool, bound to the current track
command = "com.example.favorites.Store.Toggle"
when    = "player.track != null"
```

**HAT EEPROM (HCS-017, Draft):** the EEPROM carries only the reverse-domain ID, the manifest hash and the Ed25519 signature. `dap-hatd` reads it at boot (runtime overlays were withdrawn in the experiment review), and `dap-fetch` resolves the ID through the package endpoint to a normal capability bundle, whose manifest adds a `[hardware]` table: the chips and debug topology from lifecycle §7.4, power class (HCS-015, Draft) and device nodes to grant (`DeviceAllow=`). One mechanism, one trust model.

### 5.10 End to end: a "like" action and a "lyrics" tab on now-playing

1. **Install.** `opendap capability install com.example.lyrics`, or the on-device Settings, calls `dap-orchestrator.Install`. `dap-fetch` resolves `com.example.lyrics` via `https://<package-endpoint>/c/com.example.lyrics/index.json`. It checks the TUF-style timestamp, snapshot and targets, the recall list and the publisher signature, then downloads `payload.erofs` and verifies sha256 and the verity root. The orchestrator validates the manifest schema, checks `requires-os` and the interface versions against the registry, then **shows the grants** (net.outbound to api.example.com) for consent.
2. **Index.** The orchestrator adds the task contributions to its slot index. **No code runs.** It creates the socket for `org.opendap.lyrics.Source` (listening fd held by systemd) and records the unit template for `lyricsd`.
3. **Now-playing resumes.** The activity host asks `ResolveSlots("org.opendap.nowplaying")` and gets: actions `[like (com.example.favorites)]`, tabs `[lyrics (com.example.lyrics)]`, each with its template, bindings and `when` predicate. The shell evaluates `player.track != null` from context keys and renders the heart button (from the design system) and a "Lyrics" tab header.
4. **The like state is displayed.** The host calls `com.example.favorites.Store.WatchLiked(track_id)` on the user's behalf. Socket activation starts `favd` as an ephemeral transient unit (`DynamicUser`, `RootImage=`, only `org.opendap.media.Player` bound in), which answers `false` and exits.
5. **The user taps the heart.** The host shows `heart-filled` optimistically and calls `Store.Toggle(track_id)`. `favd` starts, writes to its `StateDirectory=`, replies `true` and exits. If it fails, the host reverts and shows the ui-components error toast.
6. **The user opens the Lyrics tab.** The host renders the `SyncedText@1` template and calls `org.opendap.lyrics.Source.WatchLyrics(track)`. `lyricsd` starts (persistent, idle-exit 60 s), fetches from api.example.com (Landlock and `IPAddressAllow` limit it to that host) and streams `{lines[], timing[]}`. The template highlights the current line from `org.opendap.audio.Engine.WatchPosition` (a seqlock blackboard on the data plane, read at the display refresh rate only while the tab is visible).
7. **The user leaves now-playing.** The activity goes to stopped and then frozen after the grace period. Its subscriptions stop being read. `lyricsd` exits after its idle period. Playback continues in `dap-audiod`.
8. **Recall.** If `com.example.lyrics` 1.4.0 lands in `recalls.json`, `dap-fetch` notices at its next timer-coalesced refresh. The orchestrator stops and disables the unit, removes the contributions and notifies the user. Data is kept until the user purges it.
9. **Classic faceplate (no touch).** The same contributions render in `focus` profile: the heart is a focusable action reached with Menu Up/Down and Select, and the lyrics tab scrolls with `REL_WHEEL`. Nothing changes in either capability.

### 5.11 Audio engine placement

`dap-audiod` is a **first-party, persistent system service**:

- **Scheduling and protection:** RT priority via its unit (`CPUSchedulingPolicy=fifo`) or rtkit, `OOMScoreAdjust=-900`, in `dap-audio.slice` with `cpu.uclamp.min` written by the orchestrator while decoding.
- **Pipeline:**

  ```text
   source (file on SD/eMMC via dap-library; later: network, USB-in UAC2 gadget)
     → demux/decode (symphonia for PCM codecs; own DSF/DFF reader)
     → [optional DSP chain: EQ, crossfeed, ReplayGain, volume — only when the user enables it]
     → output policy
         ├─ Exclusive (default for wired): ReserveDevice1 → open hw:CARD,DEV at native rate/format
         │      PCM S16/S24_3LE/S24_LE/S32_LE · DoP over S24_LE (on-board) · native DSD_U* (USB DACs)
         ├─ Shared: PipeWire client, resample/channelmix disabled, node.force-rate = track rate
         └─ Bluetooth: PipeWire (codec chosen by WirePlumber) — never bit-perfect, says so
     → taps (data plane): levels, spectrum, PCM for visualizers; computed only while subscribed
  ```

- **Gapless:** the engine opens the next track's decoder before the current one ends, and keeps one ALSA stream across tracks of the same rate and format. When rate or format changes, it drains, closes and reopens the PCM (`hw_params` per track; verification research §3.1 item 4), so rate-family changes produce only silence.
- **DSD policy:**
  - On-board: **DoP over S24_LE**, DSD64/DSD128, from what the v7.2 sources show (finding 8).
  - USB DACs: native DSD when the ALSA device lists `DSD_U*`, otherwise DoP.
  - Above the output's limit (e.g. DSD256 on-board): the user policy chooses refuse or DSD→PCM, and the latter marks the path not bit-perfect.
  - Modelled on MPD's per-format `allowed_formats` policy.
- **SignalPath (the bit-perfect signal):** `org.opendap.audio.Engine.WatchSignalPath()` streams a typed struct:

  ```text
  SignalPath { bitperfect: bool, class: "lossless"|"enhanced"|"high_quality"|"low_quality",
               source: {codec, container, rate, bits, channels, dsd_rate?},
               stages: [{name, kind: decode|dsp|volume|resample|dop_pack|mix, lossless: bool, reason?}],
               output: {path: exclusive|shared|bluetooth|usb, device, hw_params: {format, rate, channels}},
               reasons_not_bitperfect: [...] }
  ```

  - `bitperfect` is true only if every stage is lossless *and* the opened `hw_params` equal the source's (or DoP-packed) format and rate.
  - The UI shows it Roon-style. The CLI and MCP read the same struct. The verification harness asserts that `bitperfect == true` exactly when the captured bits match (verification research §3.1, §3.6). That cross-check is what turns the label into a verified promise (T1-026).
- **Volume:** hardware volume on the DAC (the cs43130 exposes PCM and DSD volume controls) keeps the path bit-perfect. Digital volume marks the stage as not lossless. `dap-dose` reads the engine's level and the headphone profile to compute dose (SYS-012), and can clamp output through the bound `volume` property (§5.5).
- **System sounds during exclusive playback:** muted, or rendered by the engine itself between tracks, as image-composition §5.3 already proposes (Q-open there; repeated in Q7).
- **Screen-off playback:** see §5.13.

### 5.12 Security model

| Layer | Mechanism | What it stops |
|---|---|---|
| Image | Signed FIT + dm-verity erofs root (opendap-os ADR 0004) | Modified base OS |
| Bundle | Ed25519 manifest signature (publisher key) pinning the payload's verity root and sha256; TUF-style index from the package endpoint (timestamp, snapshot and targets with expiry); signed recall list | Tampered or rolled-back capabilities, replayed metadata, recalled versions |
| Identity | Reverse-domain ID proved by `https://<domain>/.well-known/opendap/publisher.json` (RFC 8615) | Squatting someone else's namespace |
| Process | Transient unit: `DynamicUser=yes`, `ProtectSystem=strict`, `ProtectHome=yes`, `PrivateDevices=yes` + `DeviceAllow=` per declared driver need, `PrivateUsers=`, `PrivatePIDs=`, `RestrictAddressFamilies=AF_UNIX` (+ `AF_INET*` only with `net.*`), `IPAddressDeny=any` + `IPAddressAllow=`, `SystemCallFilter=@system-service`, `NoNewPrivileges=`, `MemoryMax=`, `RootImage=` + `RootImagePolicy=` requiring verity | Ambient filesystem, device and network access; resource abuse |
| Kernel | Landlock (ABI 10 on 7.2: filesystem, TCP, UDP, scoping), applied by the launcher from the manifest; IPE "execute only from dm-verity" as a later hardening (`RestrictFileSystemAccess=` needs systemd 261) | Defence in depth if a unit setting is wrong |
| Bus | Socket routing by namespace (§5.4) + peer-credential checks in the SDK middleware | Talking to undeclared interfaces; confused-deputy calls |
| UI | Templates and declarative-only components; file loader confined to the bundle; quotas; the host performs callbacks *as* the contributing capability | Code injection into the UI; impersonating system UI; slot spam |
| Consent | Three classes (Android model): **install-time** (`net.outbound`, `bus:*.Read`), **runtime with an OS-owned prompt** (`microphone`, `location`, `bluetooth.scan`, `storage.music.write`), **privileged**, first-party or developer-mode only (`audio.exclusive`, `chip.flash`, `hid.vendor-raw`, `power.policy`) | Silent escalation |

- **Trust posture per ADR 0005:** devices are unlocked and every HAT is trusted for now. The architecture keeps verification on the default path, so locking later is a policy switch (keyring contents and image policy), not a redesign.
- **Developer mode** (also needed by OS-009 and the dev loop) accepts locally signed or unsigned capabilities and shows a persistent indicator.

### 5.13 Power model

| Mechanism | Owner | Effect |
|---|---|---|
| Freeze stopped activities (cgroup freezer) | Orchestrator | Zero CPU from background UI (Android cached-app freezer, Lomiri pause) |
| Destroy activities under pressure or LRU limit | Orchestrator | RAM back for page cache (music files) |
| Lazy start + idle exit (socket activation, `idle-exit`, `Accept=yes`) | systemd + orchestrator | Nothing runs that is not in use |
| Typed background classes with budgets (`playback`, `transfer`, `device`, `short` ≤ N s) | Orchestrator | No unbounded background work (Android FGS types) |
| Wake requests | Services call `org.opendap.power.Wake.Acquire(reason, class, timeout)`; the orchestrator holds the matching **logind inhibitor** (`sleep`, `delay` or `block`) | One auditable place for "why is the device awake"; time limits by default |
| Timer coalescing | Capability timers become orchestrator-owned systemd timers with a generous `AccuracySec=`; batched in maintenance windows when the screen is off (Doze model) | Fewer wakeups |
| Data-plane streams computed only while subscribed | SDK contract | No FFT, meters or artwork decode with the screen off |
| Energy hints | `cpu.uclamp.max` capped on background and activity slices, `uclamp.min` raised briefly for UI animation and decode bursts (written by the orchestrator; systemd has no setting) | schedutil stays low without starving audio |
| Screen-off playback path | Shell → DPMS off, activities frozen; `dap-audiod` large ALSA buffers (100–500 ms, image-composition §5.6); radios rfkilled unless needed; suspend-to-RAM only when paused and idle after a policy timeout | The target behaviour of image-composition §8.5 |
| Power observability | `org.opendap.power.Stats.Watch()`: wakeups/s per unit (from `/proc` and cgroup stats), inhibitors held, CPU time per slice | Feeds the verification "power proxies" (verification research §5.2) |

### 5.14 Distribution through the package endpoint

- **Names:** reverse domain, following Flatpak's rule that the first part must be a domain the publisher controls ([conventions](https://docs.flatpak.org/en/latest/conventions.html)).
- **Publisher origin:** `https://<domain>/.well-known/opendap/capabilities/<id>/index.json` (RFC 8615). **Devices never fetch it directly.** They fetch `https://<package-endpoint>/c/<id>/…`, the endpoint configured in the image (SYS-011). In the virtual T1, egress except to the endpoint is blocked with nftables to prove this (verification research §5.16).
- **Metadata:** TUF roles (`root` key in the image, `timestamp` and `snapshot` signed often with short expiry, `targets` delegating `<id>` paths to the publisher's key). Implementation: `tough` (Rust) on the device side (version UNVERIFIED), or a deliberately smaller subset if it proves heavy.
- **Recalls:** an entry in the signed, expiring `recalls.json` (or a TUF `targets` removal). The device refuses to start a recalled version, offers the latest allowed one, and fails safe when metadata has expired (keeps running installed capabilities, refuses new installs and updates) (Q11).
- **OS updates:** unchanged. RAUC bundles through the same package endpoint (ADR 0004). After an OS update, the orchestrator re-validates installed capabilities against the new interface versions **before RAUC `mark-good`** (lifecycle §6.4).

### 5.15 SDK and developer and agent experience

**Crates** (SDK crates MPL-2.0 per the `system` README; services GPL-3.0-or-later):

| Crate | Content |
|---|---|
| `opendap-sdk` | Facade: service macros, lifecycle hooks, the permission and peer middleware, `Wake` helpers, structured logging to journald, OpenTelemetry hooks |
| `opendap-bus` | zlink wrappers, the bus style guide as traits (`Property<T>`, `Watch`), typed errors |
| `opendap-stream` | memfd ring and seqlock blackboard, producer and consumer, eventfd |
| `opendap-schema` | `#[derive(Schema)]`; generators for IDL, JSON Schema and docs; `schema check` rules |
| `opendap-manifest` | Manifest types, validation, signing and verification (Ed25519), canonical form |
| `opendap-schema-*` | First-party interface crates (media, audio, library, power, system, ui templates) |
| `opendap-ui` | Design-system Slint library (`@opendap/ui-components`), templates, binding layer, input profiles |
| `opendap-test` | Fake bus, fake orchestrator, fixtures (sysfs power, HID via uhid), golden-UI screenshot helpers |

**Tooling (the toolkit's `opendap` front door, CLI spec §5):**

| Command | Purpose |
|---|---|
| `opendap capability new --template service\|activity\|task\|driver` | Scaffold (cargo workspace, manifest, tests) |
| `opendap capability build` | Cross-build aarch64 (builder VM, lifecycle §2.4) → erofs + verity → manifest |
| `opendap capability sign` | Sign the manifest |
| `opendap capability run --target vm:default --watch` | Deploy as a developer-mode capability and stream logs |
| `opendap capability check` | Manifest schema, `schema check` against the registry, sandbox lint (declared vs observed syscalls and files from a test run) |
| `opendap bus list\|get\|call\|watch` | Generic Varlink access to any interface the caller may see; JSON in and out |
| `opendap activity list\|get\|start\|stop` | Lifecycle inspection and driving |

- **MCP:** `opendap mcp serve` exposes the bus directory as tools generated from interface IDL plus annotations (read, mutate or destructive, the same metadata the CLI uses), so an agent can, for example, `bus call org.opendap.audio.Engine.WatchSignalPath` with `--dry-run` semantics for mutating methods. This needs no extra device daemon beyond `dap-agent` and SSH (CLI spec §3, Q4 there).
- **Local loop:** edit, then `opendap capability run --watch` on the virtual T1 (QEMU/HVF), then `opendap screen capture` and `opendap input send` for UI checks, then `opendap verification check …`.

### 5.16 How each piece is verified on the virtual T1 (T1-026)

Levels as in the verification research (L0 static, L1 host, L2 virtual T1, L3/L4 hardware).

| Piece | Check (proposal) | Level | Evidence |
|---|---|---|---|
| Manifest and schemas | JSON Schema validation, signature round-trip, `schema check` minor/major rules with golden IDL pairs | L0/L1 | `cargo test` in `system` |
| Bus | Contract tests per interface (recorded request/reply goldens); introspection equals the published IDL hash; routing test: an undeclared interface socket does not exist in the unit namespace and a peer-cred check rejects a forged caller | L1/L2 | `opendap verification check bus.contract.*`, `bus.routing` |
| Bus performance gate | p50/p99 round-trip and CPU per 1,000 calls; memfd ring throughput | L2 now as a ratio, L3 on A55 as absolute | Report JSON |
| Orchestrator lifecycle | Scripted: launch → resume → background → (5 s) frozen (`FreezerState=frozen`, zero CPU) → relaunch restores state; memory-pressure kill order; quarantine after N failures | L2 | QMP input + `systemctl show` + screenshots |
| Contributions (SYS-007) | Install the two example capabilities; now-playing shows the heart and the Lyrics tab (golden screenshots, touch and focus profiles); tapping toggles and persists; recall removes both | L2 | `opendap screen capture` diffs; bus traces |
| Two-way binding (SYS-008) | Slider set → `SetVolume` → clamp by `dap-dose` → UI snaps back; latency budget | L2 | Bus trace + screenshot sequence |
| Endpoint-only fetch (SYS-011) | nftables blocks direct egress; install works via the package endpoint and fails without it; expired metadata refuses new installs | L2 | verification research §5.16 |
| Bit-perfect (OS-020) | `snd-aloop` capture with generated vectors; **`SignalPath.bitperfect` must equal the comparator result** for every positive and negative control (PipeWire path, −1 dB, S32 through F32) | L2 | verification research §3.2–§3.6 |
| DSD | DoP marker and payload check (`dop-check`); native DSD formats against a fake ALSA ioplug device (verification research §3.4) that models the CS43198 + RK3566 limits (DoP only on-board, DSD128 max) | L2 | Capture files + report |
| Hearing safety | Accelerated-clock dose test through the engine | L2 | verification research §5.1 |
| Power proxies | Wakeups/s and CPU per audio second per scenario; zero wakeups from frozen activities; no data-plane producer active with the screen off | L2 (ratios), L4 rundown | `org.opendap.power.Stats` |
| Faceplate input | `faceplate-emu` on uhid: each HID usage produces the right semantic action in `dap-input`, and the focus profile drives a template | L2 | verification research §5.10 |
| HAT capability | Emulated EEPROM fixture → `dap-hatd` → `dap-fetch` resolves via the package endpoint → driver unit starts with only its `DeviceAllow=` | L2 | Fixture + bus trace |
| OS update interplay | RAUC update to an image with a bumped interface major: incompatible capability flagged before `mark-good`; rollback path | L2 | lifecycle research §6.3 |
| Product feel (owner principle) | Install 20 test capabilities: the top-level navigation is unchanged (golden screenshot); per-slot caps hold; chips and toasts are rate-limited; no capability creates a top-level entry; a HAT mode appears only while the emulated EEPROM fixture is present | L2 | Screenshots + `ResolveSlots` traces |
| Design system (SYS-009, DS-001) | Templates rendered in every input profile against golden screenshots; contrast tokens; accessible names present in the AT-SPI tree (dumped from the guest) | L2 | Screenshots + AT-SPI dump |

---

## 6. Requirement mapping

| Requirement | How the proposal satisfies it |
|---|---|
| SYS-001 capabilities, not apps | The capability is the only unit of distribution (§5.3). Activities, services, tasks, handlers and drivers are all things a capability *provides* |
| SYS-002 manifest needs/provides | `capability.toml` (§5.9), schema-validated and signed; enforced by routing and sandboxing (§5.4, §5.12) |
| SYS-003 Unix-philosophy services, ephemeral or persistent, foreground or background | `kind = ephemeral` (`Accept=yes` or oneshot), `persistent` (socket-activated engine), `driver`; `class` = foreground or typed background slices (§4.5, §5.8) |
| SYS-004 bus with a standardised API | Varlink control plane + memfd data plane, one style guide, IDL generated from Rust, published in the registry (§5.4) |
| SYS-005 versioned schemas and UI components in a signed index | Registry layout, hashes and compatibility rules (§4.3, §5.6); templates and components are registry artifacts |
| SYS-006 activities with pause, resume, destroy | Activity state machine on the cgroup freezer and the `xdg_toplevel` suspended state (§5.8). Activities are product modes in a fixed information architecture, with no windows and no launcher (owner principle, §3.7) |
| SYS-007 capabilities add tasks to activities | Slots, tasks and templates (tier A), declarative components (tier B); like and lyrics walkthrough (§5.7, §5.10) |
| SYS-008 dynamic two-way binding | Slint `<=>` inside the UI; `Get`/`Set`/`Watch` with optimistic reconcile across processes (§5.5) |
| SYS-009 / DS-001 one design system | Host-rendered templates, stock icons, ui-components Slint library, tokens in the registry; tier-B components may import only the design system (§5.7) |
| SYS-010 no store; reverse-domain / well-known HTTP | `.well-known` publisher origin, reverse-domain IDs (§5.14) |
| SYS-011 always through the package endpoint; signed recalls | Devices fetch only via the package endpoint; signed expiring recalls (§5.14); verified with blocked egress (§5.16) |
| SYS-012 hearing safety (Draft) | `dap-dose` service bound to the engine's level and the volume property (§5.11) |
| OS-008 / HCS-013 host of all peripherals | `dap-chipd` and `driver`-kind capabilities with declared `DeviceAllow=`; HAT manifests resolve to capabilities (§5.2, §5.9) |
| OS-009 / OS-016 / OS-018 / OS-019 developer hosting and the virtual T1 | Dev-mode capabilities, `opendap capability run`, bus and activity commands over SSH, QMP-driven checks (§5.15, §5.16) |
| OS-020 bit-perfect PCM and DSD | Exclusive ALSA engine, DoP on-board, native DSD on USB, `SignalPath` cross-checked by capture (§5.11). **Gap: native DSD on the on-board I2S** (finding 8, Q6) |
| OS-021 minimal default image | Capabilities live outside the image; optional services socket-activated; developer tools as sysext (§4.6) |
| OS-022 SD/eMMC/RAM levels | `dap-library` owns the cache-level policy; the page cache is freed by destroying activities (§5.13). Detail is out of scope here |
| T1-008 audio first | The engine is a protected first-party service with RT, uclamp and large buffers; nothing else in the audio path (§5.11) |
| T1-009 battery first | Freezer, lazy start, idle exit, typed wake requests, timer coalescing, subscription-gated data plane (§5.13) |
| T1-011 standard, trick-less | systemd units, Varlink, memfd, PipeWire, Wayland, erofs/verity, Landlock: all upstream mechanisms. Custom code only for policy (orchestrator), the engine and the shell |
| T1-023 / T1-024 / ADR t1-0002 faceplate HID | `dap-input` maps HID usages (evdev) and the vendor page (hidraw) to semantic actions; input profiles in templates (§5.7) |
| T1-025 / TK-002 / TK-009 agentic CLI | `opendap bus`, `activity`, `capability` commands and MCP generated from bus IDL plus annotations (§5.15) |
| T1-026 every promise verified | Verification table (§5.16); every state is observable on the bus |
| HCS-007…011 / HCS-017 (Draft) signed HAT manifest via the repository | EEPROM holds the ID, hash and signature; resolution through the same package endpoint and bundle format (§5.9) |
| opendap-os ADR 0004 signed bundles on the data partition | erofs + verity + Ed25519 manifest at `/var/lib/dap/capabilities` (§4.6) |
| opendap-os ADR 0005 unlocked, HATs trusted | Verification on by default; locking is a later policy switch (§5.12) |
| system ADR 0001 Rust | Every custom component and the SDK in Rust; JSON and IDL keep third parties in other languages possible |

---

## 7. Alternatives considered and why rejected

| Alternative | Why rejected (for now) |
|---|---|
| **D-Bus (zbus) as the OpenDAP bus** | Viable runner-up: mature, has broadcast and bus policy. Rejected because every call pays a broker hop, its type system and XML policy are dated, and systemd itself is moving new APIs to Varlink. D-Bus stays for talking to BlueZ, logind, RAUC and WirePlumber |
| **A custom binary bus (postcard/rkyv over UDS, our own broker)** | Highest raw speed, but it means inventing introspection, tooling, policy and cross-language support, which are exactly what agents and third parties need. It also conflicts with T1-011. The speed is captured on the data plane anyway |
| **iceoryx2 as the bus** | No access control yet; aarch64 tier 2; not a Linux convention. Kept as a drop-in data-plane transport if measurements demand it |
| Zenoh, NNG, gRPC/tonic, tarpc, Cap'n Proto RPC | Network-oriented (Zenoh, gRPC), Rust-only (tarpc), immature Rust bindings (NNG 2), or a second schema ecosystem (Cap'n Proto) for no on-device gain |
| **rkyv or bitcode on the public API** | No schema evolution and Rust-only, which suits neither versioned contracts nor third parties |
| CBOR as the control encoding | No benefit over JSON at control-plane rates; loses `varlinkctl`/MCP readability. Possible later via Varlink upgrade |
| **WASM components (Wasmtime) for capabilities in v1** | Strong sandbox and WIT versioning, but aarch64 is tier 2, it adds a 1–19 MB runtime and per-instance RAM (UNVERIFIED), and drivers must be native anyway. systemd + Landlock give the needed isolation with standard tools. Revisit for pure-logic capabilities (§9 Q12) |
| **Single UI process hosting every activity** | Lower RAM, but a third-party crash takes down the whole UI, and per-activity freezing and accounting are lost (QtAM's own caveat). Kept as an optimisation for first-party activities only |
| **Launcher, app grid, app icons, notification shade, a settings app per capability** (the phone/desktop OS model, including Pebble's watchapp launcher and Rockbox's plugin list) | Contradicts the maintainers' "product, not OS" principle. Replaced by the fixed product navigation, slots with caps, chips and toasts, and the Extensions screen (§3.7, §5.7) |
| Installable third-party activities shown as top-level destinations | Same principle. Only HAT mode activities, tied to attached hardware, are allowed (Q5) |
| HAT mode activities as arbitrary Wayland clients (any toolkit) | Breaks SYS-009 (one design system) and the template guarantees. Activities are Slint on the OpenDAP host |
| Free-form `.slint` with Rust code in the UI process | Code in the UI process defeats isolation; ComponentContainer is experimental. Hence templates first, declarative-only later |
| Remote-view formats (Android Slices style) | The Slices lesson: a remote-UI format the host does not own decays. Templates are owned and versioned by OpenDAP |
| **Orchestrator as PID 1, or replacing systemd parts** | Rejected in image-composition §2.4: it re-implements supervision, cgroups, the freezer and sandboxing, and a bug panics the kernel |
| Portable services as the capability runtime | Overlaps with orchestrator-managed transient units; it uses the same `RootImage=` machinery, which the proposal uses directly |
| systemd-sysext for third-party capabilities | Merges into `/usr` for everyone. First-party and developer use only |
| `.deb`/apt, Flatpak, OCI containers for capabilities | See §4.6 |
| systemd-sysupdate for capabilities or OS | Back to experimental in 262; RAUC is decided for the OS |
| GStreamer as the audio engine | Harder to audit the path; the engine must own every stage to report `SignalPath` truthfully |
| MPD as the product engine | Good reference and test oracle in bring-up. As the product engine it would add a second control protocol and a C codebase outside the Rust SDK model |
| PipeWire-only audio | F32 graph: not exact for S32 or DoP (image-composition §5.1) |
| OCI registry on the device | Extra client and protocol for no gain; static HTTP + TUF is simpler |

---

## 8. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Slint's dynamic composition (`ComponentContainer`, named slots, `interface`) stays experimental or changes | Tier B delayed | Tier A templates need none of it; track issue #2390; isolate behind `opendap-ui` |
| Slint licensing for third-party `.slint` components interpreted in a GPLv3 process | Third parties may be unable to ship closed components | Owner and legal decision (Q2); commercial licence option; templates avoid the question |
| Native DSD on the on-board I2S needs a Rockchip I2S driver change | OS-020's "native" not met on the jacks without a kernel patch (OS-002) | DoP on-board; native over USB; upstream a patch to mainline so Debian picks it up (Q6); verify on hardware |
| CS43198 DSD limited to DSD128 by the driver | DSD256 files need conversion or refusal | Policy and `SignalPath` honesty; check the datasheet vs the driver at bring-up |
| Per-activity process memory (Slint + GLES context) on 4 GB | Fewer resident activities | Measure; LRU destroy; shared host for first-party activities |
| Varlink JSON latency on A55 under load | Sluggish bindings | Measurement gate; batched `Watch` structs; Varlink upgrade to binary framing (systemd 261+) |
| Trixie's systemd 257 lacks newer Varlink features (fd-passing APIs 258, registry 260, upgrade 261, `RestrictFileSystemAccess=` 261) | Some systemd-side tooling missing | zlink implements fd passing itself; own `/run/dap/bus` directory; reassess at forky or with a backport |
| Mainline TF-A suspend unknown (opendap-os pending) | Screen-off standby power | Runtime-idle playback path does not depend on it (image-composition §8.5) |
| Accessibility on a touch handheld has no Linux reference implementation | Weak screen-reader UX | AccessKit tree from day one; dedicated research (Q9) |
| Frozen activity + provider back-pressure bugs | Memory growth or stalls | Clients-only rule; providers drop slow subscribers; tested in L2 lifecycle checks |
| Third-party sandbox escapes through over-broad `DeviceAllow=` for drivers | Hardware misuse | Privileged permission class; driver capabilities reviewed; Landlock + seccomp |
| Package endpoint unreachable | Installs and updates stop | Devices keep running installed capabilities; metadata expiry fails safe for new installs only |
| One-person maintenance of orchestrator, shell and engine | Velocity | Lean on systemd, Smithay, Slint and PipeWire; keep custom code to policy |

---

## 9. Open questions for the maintainers

1. **Bus:** accept Varlink (zlink) for control plus memfd streams for data, subject to the A55 measurement gate? Or keep D-Bus (zbus) for everything?
2. **Slint licence:** GPLv3 (the default for `system`) or a commercial licence? Under GPLv3, what licence terms apply to third-party `.slint` components (tier B)?
3. **Contribution tiers:** start with host-rendered templates only (tier A), and add declarative `.slint` components (tier B) when Slint stabilises dynamic composition?
4. **Activity process model:** one process per activity (recommended), or a shared host for first-party activities after measuring?
5. **Activities beyond the product's own:** under the "product, not OS" principle, the proposal allows no third-party activities at all, with one exception: **HAT mode activities**, shown by the product navigation only while their HAT is attached and built from ui-components templates. Is that exception acceptable? Should official HAT modes be first-party only in v1?
6. **Native DSD on the 3.5 mm and 4.4 mm outputs:** OS-020 says native and DoP. On-board native DSD seems to need a Rockchip I2S driver change. Accept DoP on-board plus native over USB, or upstream the driver change (and wait for Debian)?
7. **System sounds during exclusive playback:** muted, or mixed by the engine (which breaks bit-perfect for that moment and says so)?
8. **Manifest format:** TOML (recommended, Rust-first) or JSON5/YAML?
9. **Accessibility:** what level of screen-reader support is required at launch?
10. **Permissions UX:** are the three classes (install-time, runtime prompt, privileged) right? Which permissions are user-visible?
11. **Recalls and offline devices:** when metadata expires (no network), should installed capabilities keep running (recommended) and only installs and updates stop?
12. **WASM:** keep it out of v1 and revisit for pure-logic capabilities, or plan for it now?
13. **Session model** (image-composition Q5): a `dap` user session (`user@.service`) for the shell, activities and PipeWire, or system units? The proposal works with either. The `user@` route fits PipeWire.
14. **Developer mode:** how is it entered on an unlocked device (setting, CLI pairing), and what indicator does the UI show?
15. **Interface governance:** is an RFC process (like the faceplate HID extensions) the way shared `org.opendap.*` interfaces evolve, with third parties free to publish under their own namespace?

---

## 10. Not covered and why

- **No measurements.** The bus latency, per-process memory, Slint renderer and power figures all need hardware or at least a Cortex-A55 board. They are listed as gates, not assumed.
- **Unverified this session** (search budget exhausted or pages unavailable): the Fuchsia session framework and element manager, Snap interface docs, Sailfish internals, Tizen status, the current `tough` version, Varlink generators for other languages, zlink's peer-credential API, Wasmtime per-instance RAM, the Landlock crate's support for ABI ≥ 8, MPD and Rockbox gapless documentation, GStreamer `playbin` defaults, HiBy/FiiO internals, Lyrion's version, a touch screen reader on Linux, the iPod menu structure, Rockbox plugin-menu placement and Teenage Engineering device internals (appliance benchmark, §3.7).
- **Out of scope:** the OS-022 cache-level design, network streaming sources, Bluetooth codec policy, the design system itself (not started), and RP2350 firmware protocol details (lifecycle research §7.7).

## 11. Sources (accessed 2026-10-21/22)

Repository context: `system`, `opendap-os` (research: image composition, OS lifecycle, base distribution), `toolkit` (CLI spec, agentic CLI and verification research), `t1` (faceplate spec, experiment review), `board`, `hat-standard`.

- **Android:** [activity lifecycle](https://developer.android.com/guide/components/activities/activity-lifecycle) · [memory management](https://developer.android.com/topic/performance/memory-management) · [cached apps freezer](https://source.android.com/docs/core/perf/cached-apps-freezer) · [fundamentals](https://developer.android.com/guide/components/fundamentals) · [FGS types](https://developer.android.com/develop/background-work/services/fgs/service-types) · [intents](https://developer.android.com/guide/components/intents-filters) · [stable AIDL](https://source.android.com/docs/core/architecture/aidl/stable-aidl) · [permissions](https://developer.android.com/guide/topics/permissions/overview) · [doze](https://developer.android.com/training/monitoring-device-state/doze-standby) · [wake locks](https://developer.android.com/develop/background-work/background-tasks/awake/wakelock) · [shortcuts](https://developer.android.com/develop/ui/views/launch/shortcuts) · [slice releases](https://developer.android.com/jetpack/androidx/releases/slice) · [media controls](https://developer.android.com/media/implement/surfaces/mobile) · [car apps](https://developer.android.com/training/cars/apps) · [template restrictions](https://developer.android.com/training/cars/apps/library/template-restrictions) · [AudioMixerAttributes](https://developer.android.com/reference/android/media/AudioMixerAttributes) · [preferred mixer attributes](https://source.android.com/docs/core/audio/preferred-mixer-attr)
- **Fuchsia:** [manifests](https://fuchsia.dev/fuchsia-src/concepts/components/v2/component_manifests) · [capabilities](https://fuchsia.dev/fuchsia-src/concepts/components/v2/capabilities) · [lifecycle](https://fuchsia.dev/fuchsia-src/concepts/components/v2/lifecycle) · [FIDL versioning](https://fuchsia.dev/fuchsia-src/reference/fidl/language/versioning) · [API evolution](https://fuchsia.dev/fuchsia-src/development/api/evolution) · [F31](https://fuchsia.dev/whats-new/release-notes/f31)
- **Shells and HMIs:** [Phosh](https://gitlab.gnome.org/World/Phosh/phosh/-/releases) · [Plasma Mobile](https://invent.kde.org/plasma/plasma-mobile/-/tags) · [Sailfish releases](https://docs.sailfishos.org/Support/Releases/) · [lomiri-app-launch](https://gitlab.com/ubports/development/core/lomiri-app-launch/-/blob/main/liblomiri-app-launch/jobs-base.cpp) · [qtmir](https://gitlab.com/ubports/development/core/qtmir/-/blob/main/src/modules/QtMir/Application/application.cpp) · [webOS OSE architecture](https://www.webosose.org/docs/guides/core-topics/architecture/architecture-overview/) · [Nura](https://nura.eco/) · [QtAM](https://doc.qt.io/QtApplicationManager/introduction.html), [manifest](https://doc.qt.io/QtApplicationManager/manifest.html), [single vs multi-process](https://doc.qt.io/QtApplicationManager/singlevsmultiprocess.html) · [AGL applaunchd](https://gerrit.automotivelinux.org/gerrit/plugins/gitiles/src/applaunchd/+/refs/heads/master/README.md) · [vsomeip](https://github.com/COVESA/vsomeip/releases) · [CommonAPI](https://github.com/COVESA/capicxx-core-runtime/releases)
- **Appliance UIs:** [Rockbox WPS tags](https://raw.githubusercontent.com/Rockbox/rockbox/master/manual/appendix/wps_tags.tex) · [Inside Playdate](https://sdk.play.date/inside-playdate/) · [PebbleOS](https://github.com/pebble-dev/pebble-firmware) · [Pebble timeline pins](https://developer.rebble.io/guides/pebble-timeline/pin-structure/) · [reMarkable docs](https://developer.remarkable.com/documentation), [Xochitl](https://developer.remarkable.com/documentation/xochitl)
- **Plugin models:** [VS Code contribution points](https://code.visualstudio.com/api/references/contribution-points), [when clauses](https://code.visualstudio.com/api/references/when-clause-contexts), [activation events](https://code.visualstudio.com/api/references/activation-events) · [WASI roadmap](https://wasi.dev/roadmap) · [Wasmtime releases](https://github.com/bytecodealliance/wasmtime/releases), [tiers](https://docs.wasmtime.dev/stability-tiers.html), [Pulley](https://docs.wasmtime.dev/examples-pulley.html), [minimal builds](https://docs.wasmtime.dev/examples-minimal.html) · [WIT](https://github.com/WebAssembly/component-model/blob/main/design/mvp/WIT.md) · [Zed extensions](https://zed.dev/docs/extensions/developing-extensions) · [wasmCloud](https://wasmcloud.com/docs/concepts/) · [HA manifest](https://contributors.home-assistant.io/core/integrations/manifest/), [quality scale](https://contributors.home-assistant.io/core/quality-scale/) · [Flatpak permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html), [conventions](https://docs.flatpak.org/en/latest/conventions.html) · [xdg-desktop-portal](https://flatpak.github.io/xdg-desktop-portal/docs/) · [RFC 8615](https://www.rfc-editor.org/rfc/rfc8615.html) · [Go modules](https://go.dev/ref/mod), [proxy](https://proxy.golang.org/) · [TUF](https://theupdateframework.io/docs/metadata/) · [tough](https://github.com/awslabs/tough) · [Sigstore](https://docs.sigstore.dev/about/overview/)
- **Bus and serialization:** [zbus](https://github.com/z-galaxy/zbus/releases), [zbus 6 upgrade](https://z-galaxy.github.io/zbus/upgrading-to-6.html) · [zlink](https://github.com/z-galaxy/zlink), [book](https://z-galaxy.github.io/zlink/) · [varlink.org](https://varlink.org/) · [systemd NEWS](https://github.com/systemd/systemd/blob/main/NEWS) · [D-Bus spec](https://dbus.freedesktop.org/doc/dbus-specification.html) · [dbus-broker](https://github.com/bus1/dbus-broker) · [iceoryx2](https://github.com/eclipse-iceoryx/iceoryx2), [0.10 notes](https://github.com/eclipse-iceoryx/iceoryx2/blob/main/doc/release-notes/iceoryx2-v0.10.0.md), [RPi 4 data](https://github.com/eclipse-iceoryx/iceoryx2/blob/main/internal/plots/benchmark_mechanism_comparison_rp4.dat) · [Zenoh](https://github.com/eclipse-zenoh/zenoh), [blog](https://zenoh.io/blog/2023-03-21-zenoh-vs-mqtt-kafka-dds/) · [NNG](https://github.com/nanomsg/nng/releases) · crates.io: [capnp-rpc](https://crates.io/crates/capnp-rpc), [tonic](https://crates.io/crates/tonic), [tarpc](https://crates.io/crates/tarpc), [postcard](https://crates.io/crates/postcard), [rkyv](https://crates.io/crates/rkyv), [bitcode](https://crates.io/crates/bitcode), [bincode 3.0.0](https://crates.io/crates/bincode/3.0.0), [flatbuffers](https://crates.io/crates/flatbuffers), [capnp](https://crates.io/crates/capnp), [minicbor](https://crates.io/crates/minicbor), [wincode](https://crates.io/crates/wincode), [schemars](https://github.com/GREsau/schemars), [typify](https://github.com/oxidecomputer/typify) · [postcard-rpc](https://github.com/jamesmunns/postcard-rpc/blob/main/docs/overview.md) · [rust_serialization_benchmark](https://github.com/djkoloski/rust_serialization_benchmark) · [memfd_create(2)](https://man7.org/linux/man-pages/man2/memfd_create.2.html) · [fcntl(2)](https://man7.org/linux/man-pages/man2/fcntl.2.html) · [unix(7)](https://man7.org/linux/man-pages/man7/unix.7.html) · [PipeWire native protocol](https://docs.pipewire.org/page_native_protocol.html) · [OCI image spec](https://github.com/opencontainers/image-spec/blob/main/manifest.md), [distribution spec](https://github.com/opencontainers/distribution-spec/blob/main/spec.md) · [ORAS](https://github.com/oras-project/oras) · [buf breaking rules](https://buf.build/docs/breaking/rules/)
- **UI:** [Slint 1.18.1](https://github.com/slint-ui/slint/releases/tag/v1.18.1), [LICENSE](https://github.com/slint-ui/slint/blob/v1.18.1/LICENSE.md), [interpreter API](https://github.com/slint-ui/slint/blob/v1.18.1/internal/interpreter/api.rs), [ComponentContainer](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/guide/experimental/component-container.mdx), [named slots](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/guide/experimental/named-slots.mdx), [interface](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/guide/experimental/interface.mdx), [backends](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/guide/backends-and-renderers/backends_and_renderers.mdx), [linuxkms](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/guide/backends-and-renderers/backend_linuxkms.mdx), [CHANGELOG](https://github.com/slint-ui/slint/blob/master/CHANGELOG.md), [embedded](https://slint.dev/embedded) · [Smithay](https://github.com/Smithay/smithay) · [cage](https://github.com/cage-kiosk/cage/releases) · [wlroots](https://gitlab.freedesktop.org/wlroots/wlroots/-/releases) · [cosmic-comp](https://github.com/pop-os/cosmic-comp/tags) · [niri](https://github.com/YaLTeR/niri/releases) · [wayland-protocols](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/releases), [xdg-shell.xml](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/stable/xdg-shell/xdg-shell.xml) · [AccessKit](https://github.com/AccessKit/accesskit) · [Orca](https://gitlab.gnome.org/GNOME/orca/-/tags)
- **Audio:** [MPD NEWS](https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/NEWS), [user docs](https://mpd.readthedocs.io/en/latest/user.html), [plugins](https://mpd.readthedocs.io/en/latest/plugins.html) · [Rockbox](https://github.com/Rockbox/rockbox) · [Volumio plugins](https://developers.volumio.com/plugins/plugins-overview) · [moOde](https://github.com/moode-player/moode/releases) · [HiBy RS6](https://store.hiby.com/products/hiby-rs6) · [FiiO M15S](https://www.fiio.com/m15s) · [Roon architecture](https://help.roonlabs.com/portal/en/kb/articles/architecture), [signal path](https://help.roonlabs.com/portal/en/kb/articles/signal-path) · [SlimProto](https://lyrion.org/reference/slimproto-protocol/) · [PipeWire NEWS](https://raw.githubusercontent.com/PipeWire/pipewire/master/NEWS), [props](https://github.com/PipeWire/pipewire/blob/master/doc/dox/config/pipewire-props.7.md) · [WirePlumber features](https://github.com/PipeWire/wireplumber/blob/master/docs/rst/daemon/configuration/features.rst) · [asound.h v7.2](https://github.com/torvalds/linux/blob/v7.2/include/uapi/sound/asound.h) · [cs43130.c v7.2](https://github.com/torvalds/linux/blob/v7.2/sound/soc/codecs/cs43130.c) · [rockchip_i2s_tdm.c v7.2](https://github.com/torvalds/linux/blob/v7.2/sound/soc/rockchip/rockchip_i2s_tdm.c) · [rockchip_i2s.c v7.2](https://github.com/torvalds/linux/blob/v7.2/sound/soc/rockchip/rockchip_i2s.c) · [DoP 1.1](https://dsd-guide.com/dop-open-standard) · [symphonia](https://github.com/pdeljanov/Symphonia) · crates.io [alsa](https://crates.io/crates/alsa), [cpal](https://crates.io/crates/cpal), [dsd-reader](https://crates.io/crates/dsd-reader), [gstreamer](https://crates.io/crates/gstreamer) · [GStreamer 1.24](https://gstreamer.freedesktop.org/releases/1.24/)
- **systemd and kernel:** [org.freedesktop.systemd1(5)](https://manpages.debian.org/trixie/systemd/org.freedesktop.systemd1.5.en.html) · [systemd.exec(5)](https://manpages.debian.org/trixie/systemd/systemd.exec.5.en.html) · [systemd.resource-control(5)](https://manpages.debian.org/trixie/systemd/systemd.resource-control.5.en.html) · [systemd-sysext(8)](https://manpages.debian.org/trixie/systemd/systemd-sysext.8.en.html) · [systemd-inhibit(1)](https://manpages.debian.org/trixie/systemd/systemd-inhibit.1.en.html) · [systemd.timer(5)](https://manpages.debian.org/trixie/systemd/systemd.timer.5.en.html) · [systemd releases](https://github.com/systemd/systemd/releases) · [cgroup-v2](https://github.com/torvalds/linux/blob/v7.2/Documentation/admin-guide/cgroup-v2.rst) · [PSI](https://github.com/torvalds/linux/blob/v7.2/Documentation/accounting/psi.rst) · [Landlock docs](https://docs.kernel.org/userspace-api/landlock.html), [syscalls.c v7.2](https://github.com/torvalds/linux/blob/v7.2/security/landlock/syscalls.c) · [kernel/power/Kconfig](https://github.com/torvalds/linux/blob/v7.2/kernel/power/Kconfig) · [RAUC](https://github.com/rauc/rauc/releases) · [landlock crate](https://crates.io/crates/landlock)
