# System requirements

Requirements for OpenDAP OS system software. Most of these record the intent from the project brief; the software stack is not defined yet.

| ID | Requirement | Priority | Status |
|---|---|---|---|
| SYS-001 | Developers shall extend the system with **capabilities** (business logic, user-space drivers, …) instead of standalone apps. | must | Draft |
| SYS-002 | Each capability shall declare in a manifest what it needs and what it provides. | must | Draft |
| SYS-003 | Services shall follow the Unix philosophy. They may be ephemeral (CLI-like) or persistent (engines), and run in the foreground or as background tasks. | must | Draft |
| SYS-004 | Services shall talk over a bus with a standardized API. | must | Draft |
| SYS-005 | Data schemas and UI components shall be versioned and published in a signed, versioned index. | must | Draft |
| SYS-006 | The UI shall show **activities** instead of windows. Activities pause, resume and are destroyed on demand as they enter or leave the foreground. | must | Draft |
| SYS-007 | Capabilities shall be able to add tasks to existing activities. Example: on the now-playing activity, one service adds a "like" action and another adds a lyrics tab. | must | Draft |
| SYS-008 | The UI runtime shall support dynamic two-way data binding. | must | Draft |
| SYS-009 | All UI shall follow the official design system. | must | Draft |
| SYS-010 | There shall be no app store. Packages are fetched over HTTP using a reverse-domain / well-known scheme. | must | Draft |
| SYS-011 | Package downloads shall always go through the package endpoint configured in the image, and the device shall verify signed, expiring metadata and honour signed recalls. | must | Draft |
| SYS-012 | Hearing safety (required for personal music players; compliance requirement): a local weekly sound-dose budget over a rolling seven days, with an 85 dB(A) limit and per-headphone profiles (EN 50332). | must | Draft |
| SYS-013 | When a HAT is connected, the UI shall show that it is verified, its signature status, power draw, permissions and where its capabilities appear. | must | Withdrawn |
| SYS-014 | Errors shall be explained in a human sentence with the next step, never blaming the user. | must | Withdrawn |
| SYS-015 | Button behavior: Action ×1/×2/×3 = play-pause / next / previous; holding Action captures an idea; Volume − always lowers the volume. | should | Withdrawn |
| SYS-016 | Activities instead of icons: Listen, Watch, Read, Create and Day, plus Advanced. | should | Withdrawn |
| SYS-017 | The UI shall not look or behave like an operating system (no launcher, app grid or app icons): it shall feel like the dedicated, purpose-specific interface of a consumer electronic product. | must | Confirmed |
| SYS-018 | Dynamic UI components are key: capabilities and HATs shall be able to bring versioned UI components from the registry at runtime, without rebuilding the shell, within the design system. | must | Confirmed |
| SYS-019 | Developers shall create services, UI components, data schemas and bindings in a simplified way, in Rust, through the official SDK. | must | Confirmed |
| SYS-020 | Using the SDK shall enforce the project's optimizations, practices and patterns. | must | Confirmed |
| SYS-021 | The UI shall provide its own on-screen keyboard, designed for the T1's display and interaction, as part of the product UI system. | must | Confirmed |
| SYS-022 | Third parties shall be able to provide custom keyboards, installed and selected like other capabilities. | must | Confirmed |
| SYS-023 | The default keyboard shall be a mini QWERTY layout. | must | Confirmed |

## Context

- SYS-021 to SYS-023 (2026-10-22), proposed direction, to be researched: the keyboard is an input method on the standard Wayland protocols (`zwp_input_method_v2` on the compositor side, `text-input-v3` in apps; Smithay and Slint support is **UNVERIFIED**), so every activity gets text entry without its own keyboard. Third-party keyboards are capabilities with a privileged `input.method` permission, because a keyboard sees everything typed; password and other sensitive fields always use the system keyboard, as on Android and iOS. Faceplates without touch (cassette, classic) need a non-touch text entry method on the same input-method interface (for example a character wheel driven by HID controls).
Direction from the project brief, not specified yet: a custom orchestrator coordinates services; the bus is Rust-native and high-performance, more modern than CBOR; the UI runtime is opinionated, reactive and based on Slint, running on Wayland.

- SYS-012 to SYS-016: Adopted from the earlier experiments on 2026-10-21 as **Draft**: consistent with the decisions already taken and with the product philosophy, pending the maintainers' confirmation. Part availability still has to be checked against the sourcing rule (T1-010).
- Withdrawn on 2026-10-21 after the [experiment review](https://github.com/opendap-project/t1/blob/main/docs/research/2026-10-experiment-review.md): SYS-015, SYS-016.
- Withdrawn on 2026-10-21: SYS-013 and SYS-014 came from the earlier UI prototype, which is discarded entirely together with its design system. SYS-012 stays because EN 50332 requires it, not because of the prototype.
- The SDK plays the role the Android SDK plays for Kotlin/Java, or SwiftUI and Apple's frameworks for Swift: Rust is the language; the OpenDAP SDK is the platform layer (services, schemas, bindings) and the declarative UI framework (components) that developers build with.
