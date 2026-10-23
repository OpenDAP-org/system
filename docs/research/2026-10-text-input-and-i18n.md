# Text input and internationalisation for Blackbird OS

- **Date:** 2026-10-22
- **Status:** Research report. Input to SYS-021 to SYS-029; nothing here is decided.
- **Question:** how the on-screen keyboard, its contextual variants, third-party keyboards, non-touch text entry and the i18n stack should work on the T1, given the architecture already accepted ([ADR 0002](../adr/0002-system-architecture.md): Smithay compositor, one Slint/winit process per activity, capabilities with permissions; [ADR 0005](../adr/0005-sdk-defined-services-components-and-schemas.md): SDK-defined components rendered by the host; [ADR 0006](../adr/0006-slint-as-host-renderer.md): Slint under GPLv3).
- **Inputs read:** [system requirements](../requirements/README.md) (SYS-021 to SYS-029 and their Context notes), ADRs 0002, 0004, 0005 and 0006, the [system architecture research](2026-10-system-architecture.md), the [Slint versus Iced research](2026-10-ui-toolkit-slint-vs-iced.md), `blackbird-ui` requirements (DS-006), the `t1` [faceplate interface spec](https://github.com/opendap-project/t1/blob/main/docs/specs/faceplate-interface.md) and the touch faceplate [display ADR](https://github.com/opendap-project/faceplate-touch/blob/main/docs/adr/0001-focus-lcds-e35gh-display.md).
- **Conventions:** versions and dates are those published on 2026-10-22, read from tags, protocol XML, crates.io and the projects' own documentation. Claims not checked against a primary source are marked **UNVERIFIED**. Nothing was measured on the RK3566.

## 1. Summary

1. **Protocols.** Apps speak `zwp_text_input_v3` (in wayland-protocols, `unstable/`; **version 2** since wayland-protocols 1.48, adding submit actions, show/hide input panel, IME language and preedit hints). Keyboards speak `zwp_input_method_v2` plus `zwp_virtual_keyboard_v1`, both of which live in the **wlroots** repository, not in wayland-protocols. Their successor, `xx-input-method-v2` (interface version 4) plus `xx-keyboard-filter-v1`, is in wayland-protocols' `experimental/` tree and has no Smithay support yet. Every Linux mobile stack checked uses this split, except Plasma, which still uses `input-method-v1` (§3).
2. **Smithay** already implements text-input-v3 (version 1 only), input-method-v2 and virtual-keyboard-v1, each manager global with a client filter, and allows **one input method per seat**. Master (unreleased since 0.7.0 of 2025-06-24) adds activation hooks that pass the field's content type to the compositor (§3.2).
3. **Slint is the weak link.** Slint 1.18.1 is on winit 0.30, which passes only `Normal`, `Password` or `Terminal` to the compositor, and Slint maps every `input-type` except `password` to `Normal`. winit 0.31 (beta) carries the full content purpose and hints, but still binds text-input-v3 version 1. Slint's upgrade to winit 0.31 is open ([#11243](https://github.com/slint-ui/slint/issues/11243)). Contextual variants therefore need upstream work in Slint, and version-2 features need it in winit as well (§3.3).
4. **Recommendation:** the system keyboard is a separate first-party process (`dap-keyboard`, Rust and Slint, design system) that speaks the same standard protocols as third-party keyboards. Its panel is a `wlr-layer-shell` surface (the standard Squeekboard, Stevia and wvkbd use), placed by the compositor. Only clients holding the `input.method` grant can see these globals. The compositor keeps the routing decision: sensitive fields always go to the system keyboard (§3.5).
5. **Variants** come from `content_purpose` and `content_hint` without app code. Eleven variants cover the protocol (§4). An SDK property for the action-key label is needed, because text-input-v3 only knows "submit".
6. **Third-party keyboards** follow the iOS model, which is stricter than Android's. They get no network, own storage only and no raw key injection (filtered virtual keyboard), and never serve password, PIN or `sensitive_data` fields. Each one is enabled in Settings behind an OS-owned warning (Android's wording is the precedent) (§5).
7. **Mini QWERTY:** the panel has 166 ppi (48.96 mm across 320 px), so a 10-column QWERTY gives 32 px (4.9 mm) keys. That is below Material's 48 dp (≈ 50 px) and Apple's 44 pt default, and above Apple's 28 pt minimum (≈ 28.5 px). Recommended: a 10-column QWERTY with 46 px rows, a 32 px candidate strip, long-press alternates and language-model hit-target correction. A two-letters-per-key layout stays as an alternative to test. For buttons-and-wheel faceplates, use a linear character strip (tvOS precedent) driven by `REL_DIAL`/`REL_WHEEL` and `KEY_SELECT` (§6).
8. **i18n stack:** ICU4X 2.4 (CLDR 49, Unicode 18; already in Slint's dependency tree through parley) with per-locale postcard blobs. Messages in Project Fluent, or gettext if one format with Slint matters more (Unicode MessageFormat 2 has no mature Rust implementation yet). Shaping through parley/HarfRust (a HarfBuzz port). **RTL layout mirroring is missing in Slint** ([#2294](https://github.com/slint-ui/slint/issues/2294), open), so the host's `.slint` generator mirrors layouts itself (§7).
9. **Languages and data:** Ethnologue's public chart confirms English, Mandarin Chinese, Hindi and Spanish as the top four; the fifth (Standard Arabic or French) is still **UNVERIFIED**. Mandarin through librime (BSD-3-Clause, C API) with pinyin and a 9-key option. Hindi through Latin transliteration on the default layout first. Dictionaries go into a memory-mapped FST (`fst` crate) per language, and each language is packaged as a data-only, verity-protected language pack loaded on demand (§8).

## 2. Requirements in scope

| ID | Ask | Where answered |
|---|---|---|
| SYS-021 | Own on-screen keyboard, part of the product UI system | §3.5, §6 |
| SYS-022 | Third-party keyboards installed and selected like capabilities | §5 |
| SYS-023 | Mini QWERTY default | §6 |
| SYS-024 | Contextual variants chosen by the field and the activity | §4 |
| SYS-025, DS-006 | i18n-first UI: strings, formats, plurals, RTL, fonts | §7 |
| SYS-026 | i18n-first keyboard: layouts per language, composing input methods, switching | §7.5, §8 |
| SYS-027 | English by default | §8.1 |
| SYS-028 | Five most spoken languages by total speakers | §8.1 |
| SYS-029 | Small core first, language data on demand | §8.4, §8.5 |

The display that drives the sizing is the touch faceplate's Focus LCDs E35GH-I-MW800-CB: 3.5", 320 × 480 portrait, active area 48.96 × 73.44 mm ([faceplate-touch ADR 0001](https://github.com/opendap-project/faceplate-touch/blob/main/docs/adr/0001-focus-lcds-e35gh-display.md), [display research](https://github.com/opendap-project/faceplate-touch/blob/main/docs/research/2026-10-display-selection.md)). Non-touch faceplates send the HID usages of the faceplate spec (Menu Up/Down/Left/Right, Menu Pick, AC Back, `REL_WHEEL`, `REL_DIAL`).

## 3. Input method on Wayland

### 3.1 Protocol state

| Protocol | Where it lives | Version and state | Role |
|---|---|---|---|
| `zwp_text_input_v3` (`text-input-unstable-v3`) | wayland-protocols, `unstable/text-input/` ([XML at 1.49](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/1.49/unstable/text-input/text-input-unstable-v3.xml)) | Interface **version 2**. Additions dated 2026-03-23 and released in 1.48 (2026-04-01): `set_available_actions` / `action` (only `none` and `submit`), `show_input_panel` / `hide_input_panel`, `language` (BCP 47, from the IME), `preedit_hint`, and the hints `on_screen_input_provided`, `no_emoji`, `preedit_shown` ([commit history](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/commits/main/unstable/text-input/text-input-unstable-v3.xml)) | App side: focus, surrounding text, content type, cursor rectangle |
| `zwp_input_method_v2` (`input-method-unstable-v2`) | wlroots `protocol/` ([XML](https://gitlab.freedesktop.org/wlroots/wlroots/-/blob/master/protocol/input-method-unstable-v2.xml)); wlroots 0.20.2 (2026-07-07) | Version 1, "experimental and backward incompatible changes may be made". Events: `activate`, `surrounding_text`, `content_type`, `done`; requests: `commit_string`, `set_preedit_string`, `delete_surrounding_text`, `get_input_popup_surface`, `grab_keyboard` | Keyboard side, the counterpart of text-input-v3 |
| `zwp_virtual_keyboard_v1` | wlroots `protocol/` ([XML](https://gitlab.freedesktop.org/wlroots/wlroots/-/blob/master/protocol/virtual-keyboard-unstable-v1.xml)) | Version 1. Sends a keymap and raw key events "as if they came from a physical keyboard"; error `unauthorized`; the compositor "should present an error when an untrusted client requests a new keyboard" | Keys that are not text: Enter, Backspace with no surrounding text, arrows, terminal modifiers |
| `xx-input-method-v2` (`xx_input_method_v1`, version 4) | wayland-protocols `experimental/xx-input-method/` ([XML at 1.49](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/1.49/experimental/xx-input-method/xx-input-method-v2.xml)) | Experimental: "Backwards incompatible major versions of the protocol are to be expected. Exposing this protocol without an opt-in mechanism is discouraged." Adds `perform_action`, `move_cursor`, feature announcement and a positioner for popups. Keyboard grabs move to `xx-keyboard-filter-v1`. Companion `xx-text-input-v3` | Possible successor; not usable today |
| `zwp_input_method_v1` / `zwp_input_panel_v1` | wayland-protocols `unstable/input-method/` | Old protocol; still used by KWin (below) | Has a panel surface role, unlike v2 |
| `wlr-layer-shell-unstable-v1` | wlroots `protocol/` | Used by every v2 keyboard checked to place its panel | Panel surface |

Input-method-v2 defines no surface for the keyboard itself, only a popup for candidates; keyboards place their panel with layer-shell.

### 3.2 Smithay

| Module | 0.7.0 (2025-06-24) | `master` (2026-10) |
|---|---|---|
| `wayland::text_input` | `zwp_text_input_manager_v3` at **version 1**, no client filter ([mod.rs](https://github.com/Smithay/smithay/blob/v0.7.0/src/wayland/text_input/mod.rs)) | Still version 1. New `TextInputActivation` hooks: `activated(content_type)` and `deactivated()` (commit "Hooks for text input activation and deactivation", 2026-10-06). New `set_compositor_input_method()`: "Have the compositor act as the input method for this seat" ([text_input_handle.rs](https://github.com/Smithay/smithay/blob/master/src/wayland/text_input/text_input_handle.rs)) |
| `wayland::input_method` | `zwp_input_method_manager_v2` version 1, **client filter**, popup surfaces, keyboard grab ([mod.rs](https://github.com/Smithay/smithay/blob/v0.7.0/src/wayland/input_method/mod.rs)) | "Only one input method per seat: a newcomer is made unavailable and inert" ([input_method_handle.rs](https://github.com/Smithay/smithay/blob/master/src/wayland/input_method/input_method_handle.rs)) |
| `wayland::virtual_keyboard` | `zwp_virtual_keyboard_manager_v1` version 1, client filter | Same |
| `wayland::shell::wlr_layer` | `new_with_filter` | Same |
| `xx-input-method`, text-input-v3 version 2 | None | None |

The architecture research already noted that cosmic-comp and niri pin Smithay git revisions; this area argues for doing the same.

### 3.3 Client side: winit and Slint

| Layer | Version | What reaches the compositor |
|---|---|---|
| winit 0.30.13 (2026-03-02) | Used by Slint | `ImePurpose` = `Normal`, `Password`, `Terminal` ([window.rs](https://github.com/rust-windowing/winit/blob/v0.30.13/src/window.rs)); no surrounding text |
| winit 0.31.0-beta.3 (2026-09-04) | Not yet in Slint | New `request_ime_update` API with `ImeCapabilities`, surrounding text (4000-byte limit), `ImePurpose` = Normal, Password, Terminal, Number, Phone, Url, Email, Pin, Date, Time, DateTime, and `ImeHint` bits matching text-input-v3's version-1 hints; "iOS / Android / Web / Windows / X11 / macOS / Orbital: Unsupported", so in practice Wayland only ([winit-core window.rs](https://github.com/rust-windowing/winit/blob/v0.31.0-beta.3/winit-core/src/window.rs)). The Wayland backend binds text-input-v3 `1..=1` and adds `SensitiveData` for Password and Pin ([text_input/mod.rs](https://github.com/rust-windowing/winit/blob/v0.31.0-beta.3/winit-wayland/src/seat/text_input/mod.rs)) |
| Slint 1.18.1 (2026-09-21) | winit `0.30.2` ([Cargo.toml](https://github.com/slint-ui/slint/blob/v1.18.1/internal/backends/winit/Cargo.toml)) | `InputType` is `Text`, `Password`, `Number`, `Decimal`, `Search` ([enums.rs](https://github.com/slint-ui/slint/blob/v1.18.1/internal/common/enums.rs)). The backend sends `Password` for `password` and **`Normal` for everything else** ([winitwindowadapter.rs](https://github.com/slint-ui/slint/blob/v1.18.1/internal/backends/winit/winitwindowadapter.rs)) |

Open Slint items: "Upgrade to winit 0.31" [#11243](https://github.com/slint-ui/slint/issues/11243) (a `feature/winit-0.31` branch exists, [#13710](https://github.com/slint-ui/slint/pull/13710)); "`input-type` in `TextInputInterface`" [#13820](https://github.com/slint-ui/slint/issues/13820); "text input that does not show virtual keyboard on focus" [#12418](https://github.com/slint-ui/slint/issues/12418); "virtual keyboard that supports inputting Chinese" [#6680](https://github.com/slint-ui/slint/issues/6680). Calling winit directly from the activity host to override the purpose (through Slint's winit accessor) is **UNVERIFIED**: Slint resets the purpose on each focus change.

Consequence: under ADR 0005 the host generates the `.slint` for every field from SDK data, so extending Slint's input purpose once covers every capability. The upstream work is (a) Slint on winit 0.31 with a richer `input-type` or separate purpose and hints, and (b) text-input-v3 version 2 in winit, for submit, show/hide and language.

### 3.4 How other mobile Linux stacks do it

| Stack | Compositor | Keyboard | Protocols |
|---|---|---|---|
| Phosh 0.58 | phoc (wlroots) | **Stevia** v0.58.0 (2026-10-04), "the default on-screen keyboard for Phosh", GPL-3.0-or-later. Word completion through presage, hunspell correction, experimental Chinese and Japanese through uim, Indic through varnam ([README](https://gitlab.gnome.org/World/Phosh/stevia/-/blob/main/README.md)) | `input-method-unstable-v2`, `virtual-keyboard-unstable-v1`, phoc's `phoc-device-state` ([protocol/](https://gitlab.gnome.org/World/Phosh/stevia/-/tree/main/protocol)) |
| Phosh (earlier) | phoc | **Squeekboard** v1.43.1 (2024-11-24), Rust and GTK3, YAML layouts, "input purpose dependent keyboard layouts", separate layout sets `email`, `number`, `pin`, `terminal`, `url` ([README](https://gitlab.gnome.org/World/Phosh/squeekboard/-/blob/main/README.md), [layouts](https://gitlab.gnome.org/World/Phosh/squeekboard/-/tree/main/data/keyboards)) | input-method-v2, text-input-v3, virtual-keyboard-v1, layer-shell |
| Plasma Mobile | KWin | **plasma-keyboard** v6.8.1 (2026-10-20): "uses the input-method-v1 Wayland protocol… can leverage KWin's fake-input protocol" ([README](https://invent.kde.org/plasma/plasma-keyboard/-/blob/master/README.md)). KWin starts the keyboard itself on a dedicated connection (`startInputMethod`, `createInputMethodConnection`) and implements text-input v2 and v3, including the v3 show/hide-panel requests ([inputmethod.cpp](https://invent.kde.org/plasma/kwin/-/blob/master/src/inputmethod.cpp)) | input-method-v1 + input-panel, text-input v2/v3 |
| Sxmo | sway (**UNVERIFIED**, not read this session) | **wvkbd** v0.20: virtual keyboard; auto show/hide "requires a wayland compositor that implements the zwp_input_method_v2 protocol" ([README](https://github.com/jjsullivan5196/wvkbd/blob/master/README.md)) | virtual-keyboard-v1, layer-shell, optional input-method-v2 |
| Lomiri (Ubuntu Touch) | Mir v2.30.0 (2026-09-24), which implements input-method v1 and v2, text-input v1/v2/v3 and virtual-keyboard v1 ([frontend_wayland](https://github.com/canonical/mir/tree/main/src/server/frontend_wayland)) | **lomiri-keyboard**, a Maliit plugin. Per-language plugins, purpose-specific layouts per language (`Keyboard_zh-hans_email.qml`, `_url.qml`, `_url_search.qml`), engines libpinyin, libchewing, anthy, presage, hunspell, with language data shipped as separate packages ([repository](https://gitlab.com/ubports/development/core/lomiri-keyboard), [debian/control](https://gitlab.com/ubports/development/core/lomiri-keyboard/-/blob/main/debian/control)) | Maliit over Mir |

Two patterns: the compositor launches and trusts a keyboard it knows (KWin), or any client may bind the input-method global and the compositor's policy decides (wlroots/phoc). Both keep text-input-v3 on the app side.

### 3.5 Recommendation: protocols and where the keyboard runs

- **App side:** `zwp_text_input_v3`. Target version 2 and contribute the version-2 support to Smithay (server) and winit (client).
- **Keyboard side:** `zwp_input_method_v2` for text, preedit and surrounding text, and `zwp_virtual_keyboard_v1` for non-text keys. Both are what Smithay, Mir, phoc, Stevia, Squeekboard and wvkbd implement. Track `xx-input-method` and move when it leaves `experimental/` and Smithay supports it.
- **Panel surface:** `wlr-layer-shell` as the privileged surface type, its global filtered to keyboard clients. The compositor forces placement (bottom, full width, height chosen by the compositor) and resizes the focused activity so the field stays visible. This reuses a standard protocol rather than a private one. A private `dap-input-panel-v1` is the fallback if the layer-shell semantics (layers, exclusive zones) get in the way.
- **Process:** a separate first-party `dap-keyboard` client, not drawn inside the compositor:
  - a crash in a composing engine (librime, transliteration) cannot take down the shell;
  - the system keyboard uses exactly the path third parties use, so that path gets tested every day;
  - Squeekboard, Stevia or wvkbd can run unchanged as test keyboards on the virtual T1.

  Like an activity, it can be frozen while hidden (cgroup freezer), which keeps it resident without CPU cost. Its memory cost is **UNVERIFIED** (one more Slint plus GLES context).
- **Routing:** Smithay holds one input method per seat. The compositor needs either a small Smithay change that keeps several bound input methods and activates one per field, or to act as the input method itself (`set_compositor_input_method`) and relay to the chosen keyboard over a private protocol. Recommended: the Smithay change, which keeps the wire protocol standard and can be upstreamed. The compositor picks the keyboard in its `activated(content_type)` hook.
- **Shell chrome fields** (rare, for example a consent dialog) go through an internal text-input context, the way KWin does with `InternalInputMethodContext`.

## 4. Contextual variants

### 4.1 Models compared

| Concept | text-input-v3 | Android | iOS |
|---|---|---|---|
| Field kind | `content_purpose`: normal, alpha, digits, number, phone, url, email, name, password, pin, date, time, datetime, terminal | `inputType` class: TEXT, NUMBER, PHONE, DATETIME, plus variations: URI, EMAIL_ADDRESS, EMAIL_SUBJECT, SHORT_MESSAGE, LONG_MESSAGE, PERSON_NAME, POSTAL_ADDRESS, PASSWORD, VISIBLE_PASSWORD, WEB_EDIT_TEXT, FILTER, PHONETIC, WEB_EMAIL_ADDRESS, WEB_PASSWORD, NUMBER_PASSWORD, DATE, TIME ([InputType.java](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/android/text/InputType.java)) | `UIKeyboardType`: default, asciiCapable, numbersAndPunctuation, URL, numberPad, phonePad, namePhonePad, emailAddress, decimalPad, twitter, webSearch, asciiCapableNumberPad ([UIKeyboardType](https://developer.apple.com/documentation/uikit/uikeyboardtype)) plus `UITextContentType` (about 50 semantic types such as username, password, newPassword, oneTimeCode, emailAddress, telephoneNumber, URL, postalCode, creditCardNumber) ([UITextContentType](https://developer.apple.com/documentation/uikit/uitextcontenttype)) |
| Behaviour flags | `content_hint` bits: completion, spellcheck, auto_capitalization, lowercase, uppercase, titlecase, hidden_text, sensitive_data, latin, multiline, and since v2 on_screen_input_provided, no_emoji, preedit_shown | TEXT_FLAG_CAP_CHARACTERS/WORDS/SENTENCES, AUTO_CORRECT, AUTO_COMPLETE, MULTI_LINE, NO_SUGGESTIONS; NUMBER_FLAG_SIGNED/DECIMAL | `UITextInputTraits`: autocapitalization, autocorrection, spellChecking, secureTextEntry and others |
| Action key | v2: `submit` only | `imeOptions`: GO, SEARCH, SEND, NEXT, DONE, PREVIOUS ([EditorInfo.java](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/android/view/inputmethod/EditorInfo.java)) | `UIReturnKeyType`: default, go, join, next, route, search, send, done, emergencyCall, continue, and two search-engine names ([UIReturnKeyType](https://developer.apple.com/documentation/uikit/uireturnkeytype)) |
| Privacy | `sensitive_data` ("typed text should not be stored") | `IME_FLAG_NO_PERSONALIZED_LEARNING`; the source says the flag "is not a guarantee, and some IMEs may not respect it" | Secure fields force the system keyboard (§5.1) |
| Language | v2 `language` event (IME → app) | `hintLocales` (app → IME) | — |

### 4.2 Proposed variant set

| Variant | Selected by | Layout | Prediction and learning |
|---|---|---|---|
| `text` | purpose normal or alpha or name; hints set casing | Mini QWERTY of the active language | On, unless `sensitive_data` |
| `multiline` | hint `multiline` | `text` with Enter as newline | On |
| `search` | SDK action `search` (no protocol purpose exists) | `text` with a Search action key; candidates may come from the activity (library titles) | On, no learning by default |
| `email` | purpose email | QWERTY with `@` and `.` on the base layer, no auto-capitalisation | Off |
| `url` | purpose url | QWERTY with `/`, `.`, `:` on the base layer | Off |
| `number` | purpose number | Numeric pad with locale decimal separator and sign (from ICU4X) | Off |
| `digits` | purpose digits | Numeric pad, digits only | Off |
| `phone` | purpose phone | Phone pad (`+`, `*`, `#`) | Off |
| `pin` | purpose pin | Numeric pad, system keyboard only | Off; never stored |
| `password` | purpose password or hint `hidden_text`/`sensitive_data` | QWERTY with every symbol reachable, no correction; system keyboard only | Off; never stored |
| `terminal` | purpose terminal | QWERTY plus Ctrl, Esc, Tab, arrows (virtual keyboard keys) | Off |

Date and time purposes open the design system's pickers rather than a keyboard. The action-key label (Search, Go, Send, Next, Done) and a language hint have no slot in text-input-v3. They travel as SDK component properties. The host passes them to the keyboard through the compositor, either in an `org.opendap` extension to text-input or as a context key (SYS-024's "activity's context").

## 5. Third-party keyboards

### 5.1 Security models elsewhere

- **Android.** An IME is a service protected by `BIND_INPUT_METHOD`, which has "Protection level: signature" so "only the system can bind to it" ([AndroidManifest.xml](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/res/AndroidManifest.xml), [creating an IME](https://developer.android.com/develop/ui/views/touch-and-input/creating-input-method)). The user enables it in settings, behind the warning "This input method may be able to collect all the text you type, including personal data like passwords and credit card numbers" ([SettingsLib strings](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/packages/SettingsLib/res/values/strings.xml)). The IME also serves password fields; the guidance is only advice ("Don't store passwords on a device").
- **iOS** ([custom keyboard guide](https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/CustomKeyboard.html)):
  - "When a user taps in a secure text input object, the system temporarily replaces your custom keyboard with the system keyboard"; phone pads behave the same way.
  - Apps may reject custom keyboards altogether.
  - There is no microphone access, the keyboard cannot select text, and it can draw only in its own view.
  - Without `RequestsOpenAccess`, there is no shared container and no file system beyond its own container. Network access requires open access, which the user switches on.

### 5.2 Proposal: the `input.method` permission and sandbox

| Aspect | Proposal |
|---|---|
| Declaration | A capability declares `provides = ["input.method"]` with its languages and layouts (CLDR keyboard XML, §8.3) and `needs = ["input.method"]` |
| Grant class | A fourth, explicit **enable** step, like Android's: installing never activates a keyboard; the user turns it on in Settings, behind an OS-owned warning in the product's words. It is not a silent install-time grant and not privileged (SYS-022 requires third parties) |
| Wayland access | The orchestrator tells the compositor which unit, identified by pidfd or cgroup, holds the grant. Smithay's client filters expose the input-method, virtual-keyboard and layer-shell globals only to that client. Only the keyboard the user selected is active |
| Sensitive fields | Purposes `password` and `pin`, and hints `sensitive_data` or `hidden_text`, are always routed to `dap-keyboard`. The third-party keyboard is deactivated and receives no surrounding text, as on iOS |
| Key injection | Virtual-keyboard events from a third-party keyboard go through a compositor allow-list (Enter, Backspace, Delete, Tab, arrows, Escape). There are no system shortcuts, no HID consumer keys and no modifier chords except in the `terminal` variant |
| Sandbox | The standard capability unit, plus: **no `net.*` grant may be combined with `input.method` in v1**; a private state directory only, for user dictionaries and learning; no `bus:*` interfaces except an `org.opendap.input` interface (language list, settings section); no microphone |
| Data | Dictionaries and models ship as data inside the signed bundle or as language packs (§8.5), not downloaded by the keyboard |
| Surface | Only the panel rectangle the compositor assigns; no screen capture, no clipboard globals |
| Opt-out | An activity may require the system keyboard for any field through an SDK property (iOS precedent) |
| Recovery | If the selected keyboard crashes or is quarantined, the compositor falls back to `dap-keyboard` |

Network access could later follow the iOS "full access" model, as a second user switch with its own warning. That is a decision for the maintainers.

## 6. Mini QWERTY on 320 × 480

### 6.1 Density and touch targets

The pixel pitch is 48.96 mm / 320 px = 0.153 mm, which gives **166 ppi** or 6.54 px/mm. The diagonal is 3.47".

| Guideline | Size | On this panel |
|---|---|---|
| Android: "at least 48dp×48dp" ([accessibility](https://developer.android.com/guide/topics/ui/accessibility/apps)); 1 dp = 1/160 in | 7.62 mm | **49.8 px** |
| Apple HIG: iOS default control size 44 × 44 pt, minimum 28 × 28 pt; watchOS the same ([HIG accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility)) | 44 pt ≈ 6.86 mm, 28 pt ≈ 4.36 mm, taking 1 pt = 1/163 in (**UNVERIFIED** conversion) | **44.8 px**, **28.5 px** |
| WCAG 2.2: 2.5.8 minimum 24 × 24 CSS px (AA), 2.5.5 enhanced 44 × 44 CSS px (AAA) ([WCAG 2.2](https://www.w3.org/TR/WCAG22/)) | CSS px are not physical | Not directly comparable |
| 10-column QWERTY across 320 px | 32 px pitch | **4.90 mm** wide (about 28 px key + 4 px gap) |

A 10-column row cannot meet 48 dp or 44 pt in width on a 49 mm-wide panel; it can in height. Phone keyboards make the same trade-off: the 4" iPhone SE (1st generation) was 320 pt wide with a 10-column QWERTY on a panel about 50 mm wide (**UNVERIFIED** recollection of 640 px at 326 ppi), which is almost exactly this panel's width.

### 6.2 Prior art for small screens

- **tvOS:** "a linear virtual keyboard when people select a text field using the Siri Remote", and a grid keyboard with other controllers ([HIG virtual keyboards](https://developer.apple.com/design/human-interface-guidelines/virtual-keyboards)).
- **watchOS:** "a text field can show a keyboard if the device screen is large enough. Otherwise, the system lets people use dictation or Scribble"; the keyboard type cannot be changed, only the content type (same source).
- **Multi-tap and T9:** the letter-to-digit assignment comes from ITU-T E.161 (02/01), with a 2014 amendment adding Korean ([E.161](https://www.itu.int/rec/T-REC-E.161)). Rime ships a 9-key pinyin schema (`t9.schema.yaml` in [rime-ice](https://github.com/iDvel/rime-ice)).
- **Flick input:** CLDR's Keyboard 3.0 includes a Japanese Hiragana flick layout (`ja-Hira-t-k0-flicks.xml`) with `flick`/`flickSegment` gestures, alongside `longPress` and `multiTap` ([CLDR release-49 keyboards](https://github.com/unicode-org/cldr/tree/release-49/keyboards/3.0), [UTS #35 Part 7](https://www.unicode.org/reports/tr35/tr35-keyboards.html)).
- **Two letters per key** (BlackBerry SureType style) with dictionary disambiguation gives 5 to 7 columns of 46 to 64 px (7 to 9.8 mm). It suffers on passwords, URLs and names (**UNVERIFIED** as a product reference; no primary source read).
- **Squeekboard and wvkbd** keep full QWERTY in portrait and use separate layouts per purpose (§3.4).
- **Rockbox** has a virtual keyboard for button-only players ([manual source](https://github.com/Rockbox/rockbox/blob/master/manual/advanced_topics/main.tex), which references `sec:virtual_keyboard`).

### 6.3 Recommended default layout

- A **10-column QWERTY** (SYS-023's "mini QWERTY") in portrait: three letter rows of 32 × 46 px (4.9 × 7.0 mm), and a bottom row with layer switch, language, space and the action key.
- A **32 px candidate strip** on top, which also holds composition candidates for pinyin and transliteration.
- Total height is about 216 px (45 % of 480), leaving about 264 px for the activity above.
- Letters show their alternates on long press (accents, digits on the top row, as on phones) instead of adding a number row.
- **Hit-target correction:** the effective touch area of each key grows with the language model's probability of the next letter. On a 4.9 mm key this matters more than visual size. It is **off** in `email`, `url`, `password`, `pin` and `terminal`.
- Haptics are not part of the T1's requirements; the visual key-press feedback (a popup above the finger) must fit inside the 216 px band.
- Gate: a usability test on the real panel comparing error rate and speed of the 10-column layout against a 7-column two-letters-per-key layout, before freezing the default.

### 6.4 Non-touch faceplates (buttons and wheel)

The same `dap-keyboard` process renders a different panel when the faceplate context key is not `touch`. The compositor routes the faceplate's semantic actions to the panel while it is shown.

- **Linear character strip** (tvOS precedent): `REL_DIAL` or `REL_WHEEL` moves through the current set, `KEY_SELECT` inserts, `KEY_BACK` deletes (and closes on an empty field), `KEY_LEFT`/`KEY_RIGHT` move the cursor, and `KEY_UP`/`KEY_DOWN` switch sets (letters, capitals, digits, symbols, language).
- **Candidate-first:** in `search`, candidates come from the activity (for example library entries matching the prefix), so a few characters plus the wheel over candidates is usually enough.
- **Grid** with D-pad focus for faceplates with a 4-way pad and no wheel.

All three are input-method clients, so activities need nothing extra.

## 7. i18n stack in Rust

### 7.1 Locale data and formatting: ICU4X

- **Version:** `icu` 2.4.0 (2026-10-22), Unicode-3.0 licence; "Updated to Unicode 18", "Updated to CLDR 49" ([CHANGELOG](https://github.com/unicode-org/icu4x/blob/main/CHANGELOG.md)). CLDR 49 was released 2026-10-21.
- **Already present:** parley 0.11 (Slint 1.18.1's text layout) depends on `icu_segmenter`, `icu_properties` and `icu_normalizer` 2.x, and Slint's core says "We're migrating our unicode handling over to the icu family of crates" ([i-slint-core Cargo.toml](https://github.com/slint-ui/slint/blob/v1.18.1/internal/core/Cargo.toml)).
- **Data size:** ICU4X's tutorial reports about 5 MB for a blob with every marker for one locale (`ja`), 19 KB with `--markers-for-bin`, and 8 KB with a narrower formatter ([data management tutorial](https://github.com/unicode-org/icu4x/blob/main/tutorials/data-management.md)). `BlobDataProvider` loads postcard blobs at runtime. `BlobDataProvider` "does not perform locale fallbacking" unless wrapped in `LocaleFallbackProvider`. 2.4 reduced dictionary-segmenter data (V2 structs).
- **Use:** dates, times, numbers, units, plural rules, collation (library sorting per locale), segmentation (cursor movement and word deletion in the keyboard, line breaking) and display names. Data is sliced by `--markers-for-bin` over the shipped binaries and split per locale (§8.5).

### 7.2 Messages

| Option | State | Fit |
|---|---|---|
| Slint `@tr()` with gettext `.mo` at runtime or bundled translations; `slint-tr-extractor` produces `.pot`; plural form `@tr("…" \| "…" % n)`; contexts ([translations guide](https://github.com/slint-ui/slint/blob/v1.18.1/docs/astro/src/content/docs/guide/development/translations.mdx)) | Stable in 1.18 | Natural for compiled first-party `.slint`. Plurals beyond two forms depend on the `.po` `Plural-Forms` header (Arabic has six forms) |
| Project Fluent, `fluent-bundle` 0.16.0 (2025-05-22), Apache-2.0 OR MIT, about 21.8 M downloads ([fluent-rs](https://github.com/projectfluent/fluent-rs)) | Mature, slow release cadence | Selectors on CLDR plural categories and on arbitrary variables; translators work per message; one catalogue format for the SDK and the system |
| Unicode MessageFormat 2 | "a stable part of CLDR", normative in TR35; some default functions still Draft ([MFWG](https://github.com/unicode-org/message-format-wg)). Rust: `mf2` 3.0.0 (2026-10-08) is a single-author crate with 76 downloads; ICU4X has no MF2 formatter in `icu_experimental` 0.7 | The standard to watch; too early for Rust |

Under ADR 0005, capability strings are data: the host resolves them before handing values to Slint, so the SDK's catalogue format does not have to be Slint's. Recommendation: **Fluent for SDK and system catalogues**, with the chrome receiving resolved strings as properties, and a migration path to MF2 when ICU4X ships it. If a single toolchain with Slint matters more, gettext works too, at the cost of weaker plural and selector handling.

### 7.3 Right-to-left

- **Text:** parley handles bidirectional text inside a paragraph (UAX #9 through ICU4X).
- **Layout:** layout mirroring is **not supported in Slint** ([#2294](https://github.com/slint-ui/slint/issues/2294), open since 2023, last update 2025-05-07). Also open: "Render ellipsis on the leading edge for RTL" ([#11750](https://github.com/slint-ui/slint/issues/11750)) and reverse ordering in layouts ([#13504](https://github.com/slint-ui/slint/issues/13504)).
- **Workaround:** because activity screens are generated `.slint` (Route 1 of the toolkit research), the generator can mirror: swap leading and trailing, reverse horizontal layouts, flip directional icons, and keep media transport controls unmirrored, as platform guidelines do (**UNVERIFIED** exact rule set). The keyboard mirrors its own layout.
- **Gate:** if Standard Arabic is the fifth language (§8.1), RTL is a launch requirement, not a later one.

### 7.4 Fonts and shaping

- **Shaping:** Slint 1.18.1 uses parley 0.11 with the `complex-scripts` feature. Parley 0.11 shapes with **HarfRust** ([dependencies](https://crates.io/crates/parley/0.11.1/dependencies)). HarfRust is "a Rust port of HarfBuzz" that "matches HarfBuzz v15.0.0" and "for the most part passes the HarfBuzz test and fuzzing suites", with no Graphite support ([README](https://github.com/harfbuzz/harfrust)); 0.15.0 is from 2026-10-22, MIT. Devanagari conjuncts and Arabic joining depend on it; rustybuzz (0.20.1, 2024-11) is the older fork. Font discovery is `fontique` and rasterisation `skrifa`/`swash`.
- **Font sizes:**

  | Font | Size |
  |---|---|
  | Noto Sans Regular (Latin, Greek, Cyrillic) | 0.62 MB |
  | Noto Sans Devanagari Regular | 0.24 MB |
  | Noto Sans Arabic Regular | 0.23 MB |
  | Noto Sans SC Regular, subset OTF | **8.33 MB** |
  | Noto Sans SC variable, OTF | 15.05 MB |

  Sources: [notofonts](https://github.com/notofonts/notofonts.github.io/tree/main/fonts), [noto-cjk Sans2.004](https://github.com/notofonts/noto-cjk/tree/Sans2.004/Sans).
- **Consequences:** CJK dominates font storage. The interface typeface is not chosen (DS-002 is withdrawn), but whichever family is chosen needs matching Devanagari, Arabic and CJK companions.

### 7.5 Composing input methods usable from Rust

| Engine | Language | Licence | Rust access | Notes |
|---|---|---|---|---|
| **librime** | Chinese (pinyin, double pinyin, 9-key, Zhuyin, Cantonese and others through schemas) | BSD-3-Clause ([README](https://github.com/rime/librime)) | C API (`rime_api.h`); crates `rime-api` 0.12.2 (2024-09) and `rime_api-sys` | Depends on Boost, LevelDB, marisa-trie, OpenCC and yaml-cpp. Data: `rime-luna-pinyin` (LGPL-3.0, 0.89 MB dictionary source), `rime-essay` (LGPL-3.0, 5.9 MB language model source), `rime-pinyin-simp` (Apache-2.0), `rime-ice` (GPL-3.0; `base.dict.yaml` 16.6 MB source, includes `t9.schema.yaml`) |
| libpinyin | Chinese pinyin | GPL-3.0 | C | 2.11.92 (2026-09-29); release tarball with data 20.6 MB. Used by lomiri-keyboard |
| **libchewing** | Chinese Zhuyin (Traditional) | LGPL-2.1 | **Written in Rust**; crate `chewing` 0.14.0-alpha.4 (2026-10-07) | v0.12.0 (2026-04-06) |
| libhangul | Korean | LGPL-2.1 | C | libhangul-0.2.0 |
| Mozc | Japanese | BSD-3-Clause (Google copyright; repo licence flagged as mixed) | C++, no stable C API (**UNVERIFIED**) | Active (2026-10-21) |
| Anthy (anthy-unicode) | Japanese | Mixed (**UNVERIFIED**) | C | Used by lomiri-keyboard |
| m17n-db / jquery.ime rules | Hindi and other Indic: `hi-inscript`, `hi-itrans`, `hi-phonetic`, `hi-remington` and others ([m17n-db MIM](https://git.savannah.nongnu.org/cgit/m17n/m17n-db.git/tree/MIM)) | LGPL (**UNVERIFIED** per file) | Rule tables, easy to port to Rust | Indic Keyboard (Apache-2.0) offers Hindi InScript and transliteration ([README](https://github.com/smc/indic-keyboard)) |
| varnam (govarnam) | Indic transliteration with learning | **UNVERIFIED** | Go | Used experimentally by Stevia |

Recommendation: librime behind a narrow Rust wrapper, run inside `dap-keyboard` (BSD-3-Clause engine; schema and data licences chosen per pack). Hangul and Indic transliteration as table-driven Rust code. Japanese is out of scope until it is a shipped language.

## 8. Languages and language data (SYS-027 to SYS-029)

### 8.1 The five languages

- **SYS-027:** English is the default; it is also the fallback locale for every missing string.
- **Ranking:** Ethnologue's public "Languages with the most speakers" chart (embedded on [What is the most spoken language?](https://www.ethnologue.com/insights/most-spoken-language/)) lists **English 1,528 M, Mandarin Chinese 1,184 M, Hindi 609 M, Spanish 558 M**. The chart's data year appears to be 2025 (**UNVERIFIED** from the chart metadata), and it stops at four.
- **Fifth language:** the Ethnologue 200 page cites the 29th edition (2026), but its ranked table is not in the public page. Standard Arabic and French therefore stay **UNVERIFIED**, as the requirements' Context already says. A maintainer with Ethnologue access can settle it in one lookup.
- **Planning:** the fifth language changes the work. Arabic brings RTL (§7.3), Arabic shaping and an Arabic keyboard; French brings only Latin diacritics. The plan below covers both.

### 8.2 Keyboard specifics per language

| Language | Default entry on the mini QWERTY | Notes |
|---|---|---|
| English | QWERTY | Long-press alternates for loanword accents |
| Spanish | QWERTY with ñ on the base layer, accents on long press | `es` keyboard data from CLDR or Squeekboard's `es.yaml` |
| Mandarin Chinese | **Pinyin on QWERTY** through librime: syllable segmentation, candidate strip, fuzzy pinyin. **9-key pinyin** as an alternative layout | Candidate strip doubles as the composition bar; Simplified first, Traditional through OpenCC or Zhuyin later |
| Hindi | **Latin transliteration on QWERTY** (ITRANS-like rules with candidates) first, then a Devanagari layout | InScript (BIS standard) needs about 4 rows of 11 to 13 keys plus a shift layer, which is too wide for 320 px (**UNVERIFIED** count). A compact Devanagari layout (consonants on the base, matras and conjuncts on long press or a popup) is a design task |
| Standard Arabic (if fifth) | Arabic layout, RTL panel and text | Squeekboard `ara.yaml` and wvkbd's Arabic layout as references |
| French (if fifth) | AZERTY or QWERTY with accents; the choice is a product decision | CLDR `fr.xml` keyboard |

Layout data format: **CLDR Keyboard 3.0** (UTS #35 Part 7, CLDR 49). It is a Unicode standard, covers touch layers, long press, multi-tap and flicks, and lets third-party keyboards add languages as data (SYS-026). CLDR itself ships only 13 Keyboard 3.0 layouts so far ([keyboards/3.0](https://github.com/unicode-org/cldr/tree/release-49/keyboards/3.0)), so most layouts will be authored by OpenDAP.

### 8.3 Dictionary and prediction data

| Source | Content | Licence | Size reference |
|---|---|---|---|
| AOSP-format dictionaries ([Helium314/aosp-dictionaries](https://codeberg.org/Helium314/aosp-dictionaries)) | Word lists with log frequency (`f`, 0 to 255) and optional next-word data; built from Leipzig corpora for the "experimental" lists | Leipzig-derived lists "under CC BY 4.0"; legacy AOSP lists vary; `main_hi` GPLv2 | Compiled `.dict`: en_US 2.96 MB (legacy, next-word data), en_US experimental 2.50 MB, es experimental 4.23 MB, ar experimental 4.90 MB, hi 0.53 MB (2014, no next-word data), fr 1.33 MB |
| Hunspell (LibreOffice dictionaries) | Spelling (affix and stem), no frequencies | en: SCOWL-derived; es: GPL-3.0+ / LGPL-3.0+ / MPL-1.1 triple; ar: GPL-2.0 / LGPL-2.1 / MPL-1.1; hi_IN: GPL-2.0 ([repository](https://github.com/LibreOffice/dictionaries)) | `hi_IN.dic` 5.0 MB, `ar.dic` 7.2 MB, `es_*.dic` about 0.7 MB |
| Rime dictionaries and grammar | Pinyin lexicon with weights, plus an n-gram model (`essay`) | LGPL-3.0 / Apache-2.0 / GPL-3.0 per repository (§7.5) | Sources 0.9 to 17 MB; compiled size **UNVERIFIED** |
| wordfreq | Multilingual frequencies | Repository licence not machine-detected; last push 2025-01-04 | Not assessed |

Formats:

- **`fst` 0.4.7** (MIT or Unlicense) stores "ordered sets and maps using finite state machines" with memory-mapped access and range queries ([README](https://github.com/BurntSushi/fst)). Its Levenshtein automaton fits typo correction. It is pure Rust and can be read straight from an erofs-backed file. It has had no release since 2021-06.
- **marisa-trie** (BSD-2-Clause or LGPL-2.1-or-later) is what librime uses; a Rust crate exists (`marisa-rs` 0.1.2).
- **Double-array tries** (`yada` 0.7.0, `daachorse` 5.1.0) suit fixed lexicons and transliteration tables.

Recommendation: one OpenDAP language-model format per language, built offline from corpora with compatible licences.

- **Core:** an `fst` map word → frequency class.
- **Next word:** a second `fst` keyed by `previous⟨sep⟩word` for bigrams.
- **User words:** a small writable store in the keyboard's state directory.

The AOSP sizes suggest 0.5 to 5 MB per language for unigram plus bigram data (**UNVERIFIED** for the `fst` encoding until built). Chinese stays in Rime's own compiled format. Data licences need a per-pack review (GPL-2.0-only data next to a GPL-3.0-or-later system is a question for the maintainers).

### 8.4 What the core contains

| Item | In the image (core) | On demand |
|---|---|---|
| UI strings | English plus the other four launch languages (catalogues are small; size **UNVERIFIED** until written) | Further languages |
| ICU4X data | Sliced blob for the five launch locales (tens of KB per locale per the tutorial's numbers, **UNVERIFIED** for our marker set) | Other locales |
| Fonts | Latin, Devanagari and Arabic (about 1.1 MB at one weight), plus a **name-only CJK subset** that can draw the language picker and a first boot in Chinese (**UNVERIFIED** size; a few hundred glyphs) | Full CJK font (8.3 MB per weight) on selecting Chinese |
| Keyboard layouts | All launch layouts (CLDR XML, a few KB each) | — |
| Dictionaries | English | Spanish, Hindi, Arabic or French, and Mandarin (librime data) when the language is enabled in the keyboard |

Whether the first boot in Chinese needs the full CJK font in the image is a product decision: on-demand loading needs the package endpoint, and a device used offline would stay on the subset.

### 8.5 Packaging for on-demand loading

The architecture already provides the mechanism: signed capability bundles with an erofs payload pinned by a verity root, fetched through the package endpoint ([architecture research §5.12, §5.14](2026-10-system-architecture.md)).

- **Language packs as data-only capabilities**, for example `org.opendap.lang.hi` providing:
  - `ui` (Fluent catalogues for system and templates),
  - `icu` (ICU4X blob),
  - `keyboard` (CLDR layouts and the `fst` dictionary),
  - `ime` (engine data such as Rime schemas),
  - `fonts` (a script font pack).

  They run no code, so they need no unit and no bus access; the orchestrator mounts the payload read-only.
- **Granularity:** split by use, so the UI language, the keyboard languages and fonts can be installed separately (a user can type Hindi with an English UI). Font packs are per script and shared between languages.
- **Loading:** consumers open data through memory maps (`fst`, ICU4X `BlobDataProvider` from a mapped file), so pages load lazily and the kernel can drop them under pressure.
- **Stacking:** an ICU4X provider forked per locale or marker combines the core blob with each pack's blob. Fluent bundles load per locale with fallback to English.
- **Updates and recalls:** the same path as any capability, so a fixed dictionary does not need an OS update.
- **Capabilities' own strings:** each capability ships its Fluent catalogues for the languages it supports, inside its bundle (SYS-025 applies to the SDK).

## 9. Proposed decisions (not decided)

1. App side `zwp_text_input_v3` (target version 2); keyboard side `zwp_input_method_v2` + `zwp_virtual_keyboard_v1`; panel through filtered `wlr-layer-shell`; revisit `xx-input-method` when it stabilises.
2. The system keyboard is a separate first-party client, `dap-keyboard`, on the same path as third-party keyboards. The compositor routes per field and always sends sensitive fields to it.
3. Contribute to Smithay (text-input-v3 version 2; several bound input methods with compositor selection), winit (text-input-v3 version 2) and Slint (winit 0.31; input purpose and hints on `TextInput`).
4. Variant set of §4.2. Action-key label and language hint as SDK component properties.
5. `input.method`: explicit user enable with an OS warning. No network in v1, filtered key injection, no sensitive fields, own storage only.
6. Default layout: a 10-column QWERTY with a candidate strip, long-press alternates and hit-target correction, frozen only after a usability test against a two-letters-per-key layout. Non-touch: a linear strip driven by the wheel.
7. ICU4X 2.x for locale data; Fluent for SDK and system catalogues; RTL mirroring in the host's `.slint` generator until Slint supports it.
8. Mandarin through librime (pinyin, plus 9-key). Hindi transliteration first, then a compact Devanagari layout.
9. Keyboard layouts as CLDR Keyboard 3.0 data; dictionaries as memory-mapped `fst` files.
10. Languages packaged as data-only capability packs; the core holds English data, the launch UI catalogues, the ICU4X slice, non-CJK fonts and a CJK name subset.

## 10. Open questions

1. Which is the fifth language: Standard Arabic or French (Ethnologue 29th edition)?
2. Is a CJK font in the image required for an offline first boot in Chinese?
3. Is the panel's key size acceptable in practice? This needs a usability test on the E35GH panel.
4. Should third-party keyboards ever get network access ("full access")?
5. Which data licences are acceptable inside language packs (GPL-2.0-only Hunspell and AOSP lists, GPL-3.0 rime-ice)?
6. What do `dap-keyboard` and a librime session cost in memory and wake-ups on the RK3566?
7. Is Fluent or gettext the single catalogue format, given Slint's `@tr` support?
