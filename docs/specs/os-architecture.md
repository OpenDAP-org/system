# Blackbird OS system architecture

- **Status:** Draft, 2026-10-23. Decided by Claude under the maintainers' delegation, on top of what is already accepted; the new decisions are recorded in ADR 0009 to ADR 0011.
  - Already accepted: [ADR 0002](../adr/0002-system-architecture.md) (systemd orchestrator, Varlink bus, Smithay and Slint shell, activities, slots, manifests, consent classes), [ADR 0003](../adr/0003-native-binaries-no-wasm.md) (native binaries), [ADR 0004](../adr/0004-hat-mode-activities-use-the-product-ui-system.md) (HAT modes), [ADR 0005](../adr/0005-sdk-defined-services-components-and-schemas.md) (SDK-defined services, schemas and components), [ADR 0006](../adr/0006-slint-as-host-renderer.md) (Slint).
  - Background and benchmark: the [system architecture research](../research/2026-10-system-architecture.md) and the [capability permissions and package layers benchmark](../research/2026-10-capability-permissions-packages.md).
- **What this adds:** the architecture as one whole, with everything decided since ADR 0002:
  - the dedicated-instrument interface ([interface mockups](https://github.com/opendap-project/t1/blob/main/renders/web/ui-mockups.html), design system §8.1, §15.2);
  - providers for every third party (SYS-045), no accounts (SYS-047), households (SYS-043, SYS-044);
  - the OpenDAP Protocol ([RFC 0004](https://github.com/opendap-project/rfcs/blob/main/text/0004-opendap-protocol.md)) and the keystore (OS-032, BRD-059);
  - accessibility (SYS-061 to SYS-066), hardware-agnostic software (SYS-040) and derivatives (OS-028).
- **What this does not do:** choose new technologies. Where a choice was not made before, it is named as a port with candidates.

## 1. Summary

1. **Four kinds of thing, one trust model.**
   - **Packages** build the system: the image, Debian packages and system extensions.
   - **Capabilities** extend it: signed bundles on the data partition.
   - **Registry artifacts** describe it: interfaces, schemas, templates, components, tokens and language data.
   - **User data** belongs to the owner.

   Each has one format, one source and one update and rollback path (§3).
2. **Services follow the Unix philosophy on a typed bus.** One job per service; small, versioned interfaces; composition over the bus, not inside processes. Everything is reachable from the CLI, the MCP server and the remote protocol, because those are just other clients (§4).
3. **Capabilities feed the product before they draw on it** (ADR 0010).
   - A capability's first way to extend the T1 is to **implement a provider interface** that the product's own screens already use: a library source, lyrics, metadata, a store, a scrobbler, audio profile blocks.
   - Second, it fills **slots** the product defines, with host-rendered templates.
   - It never adds a destination. The only exception is a HAT mode activity while its HAT is attached.

   That keeps the T1 one instrument however many capabilities are installed (§5, §6).
4. **One permission model for every principal** (ADR 0009). Local capabilities, HAT drivers, paired devices, household members and the developer's CLI are all principals. A **grant** gives a principal an interface at a **privilege level** (view, operate, manage, administer), within a **scope**, under **conditions**. The same evaluator enforces it on local sockets and on remote sessions (§8).
5. **The registry is static, signed and replaceable.** OpenDAP hosts one; derivatives and the community may host theirs. Devices reach whichever the image configures (§7).
6. **Nothing needs the network, an account or a particular hardware.**
   - Online services are providers or endpoints (SYS-058).
   - Missing hardware disables a feature with its reason (SYS-040).
   - Every promise has an observable on the bus.

## 2. Principles

ADR 0002's principles stand. Seven more come from what was decided since:

| # | Principle | From |
|---|---|---|
| A1 | **A dedicated instrument.** The information architecture is OpenDAP's: Now Playing is home, the library below, the queue to its right, the status bar unfolding into its detail, "This T1" as the index of settings. Extensions never add destinations | SYS-017; design system §8.1 |
| A2 | **Data before UI.** A capability extends a first-party screen by providing data through a shared interface before it is allowed to contribute UI | Consistency (SYS-009); the Car App and Android media precedents (research §3) |
| A3 | **One model on and off the device.** The interfaces local clients call are the ones paired devices call through the OpenDAP Protocol, with the same grants | RFC 0004 §6.2; DESK-004 |
| A4 | **Third parties are providers.** Every outside service enters through a provider interface, opt-in, never in the core | SYS-045, SYS-047, SYS-058 |
| A5 | **Capabilities, not models.** Hardware and peers publish what they can do; features without the hardware are disabled with a reason | SYS-040 |
| A6 | **Bounded by construction.** Every service and contribution declares memory, time and rate budgets; the orchestrator enforces them | SYS-033, SYS-034, T1-046, T1-047 |
| A7 | **Accessible by construction.** Templates carry roles, names and states; voice guidance reads the same tree as AT-SPI | SYS-061, SYS-062 |

## 3. Packages and layers

### 3.1 Taxonomy

Decided in ADR 0011. "Verified" means covered by a signature the device checks before use.

| Layer | What | Format | Source | Updated by | Rollback |
|---|---|---|---|---|---|
| **L0 Boot and firmware** | U-Boot, signed FIT, the RP2350 coprocessor firmware, HAT MCU firmware | FIT; UF2/binaries flashed by `dap-chipd` (OS-008) | OpenDAP releases; HAT publishers for their firmware | RAUC slot for boot; `dap-chipd` for chips | A/B boot; chip firmware kept in its bundle's previous version |
| **L1 Base image** | Debian (pinned snapshot), OpenDAP's own packages, the product services and shell | erofs root, dm-verity, A/B (blackbird-os ADR 0004) | The image definition (mkosi), reproducible | RAUC bundle, deferrable (OS-030) | Automatic on failed boot (OS-017); "Return to previous version" while kept |
| **L2 OpenDAP packages** | OpenDAP services and libraries, optimised rebuilds of Debian packages (OS-023), patched drivers via DKMS | `.deb` in the OpenDAP APT repository (aptly snapshots) | `cargo-deb` and OpenDAP's package builds | Only through a new L1 image on the device; directly on development hosts and derivatives | With L1 |
| **L3 System extensions** | Optional system-level software outside the audiophile core: special HAT software, development tools (OS-015, OS-024) | systemd system extension images (`sysext`), signed, verity-protected, merged into `/usr` at runtime | The separate OpenDAP repository; derivatives' own | Downloaded through the package endpoint, enabled per extension; versioned against L1 | Disable or downgrade the extension; L1 untouched |
| **L4 Capabilities** | Everything that extends the product: services, providers, slot contributions, handlers | Signed bundle: `capability.toml` + signature + `payload.erofs` + verity (research §5.9) | Any publisher with a reverse domain (SYS-010); OpenDAP's first-party ones | Through the package endpoint; the orchestrator installs, upgrades, recalls | The previous version is kept until the new one is healthy |
| **L5 HAT bundles** | A capability whose manifest adds `[hardware]`: driver services, chip firmware, a HAT mode activity | L4 bundle, resolved from the HAT's EEPROM ID (HCS-017) | HAT publishers | As L4, triggered by attach | As L4 |
| **L6 Registry artifacts** | Interfaces (IDL), schemas, templates, tier-B components, design tokens, language data (SYS-029), provider descriptors | Static files with TUF-style metadata (§7) | The registry the image configures | Fetched on demand and cached; pinned per L1 for the system's own | Content-addressed: old versions stay valid |
| **L7 Owner data** | Library index, playlists, events, profiles, settings, household members, keys, capability state | Documented formats (SYS-054); state directories per capability | The owner | By the owner's actions and sync (OS-029) | Recently Deleted (SYS-050), backups (DESK-017) |

### 3.2 Rules

- **The verified root never changes at runtime.** L2 packages reach a device only inside an L1 image. Optional system software is an L3 extension, never `apt install` on the device. Development hosts and derivatives use L2 directly.
- **System or capability?**
  - Software that needs the system's privileges or `/usr` (a kernel module, a firmware loader, a system daemon with hardware access the capability sandbox cannot give) is an **L3 extension**.
  - Everything that can run in the capability sandbox is an **L4 capability**.
  - A HAT may need both. Its L5 bundle may declare an L3 extension as a dependency, installed with the owner's confirmation.
- **Signatures by layer.**
  - L0 to L3 are signed by OpenDAP or by a derivative's own keys (OS-028).
  - L4 and L5 are signed by their publisher, under the TUF metadata of the configured endpoint.
  - L6 is signed by the registry's keys.
- **Owner control.** The owner may enrol their own keys or turn enforcement off without losing data (OS-025), and developer mode accepts local, unsigned L3 and L4 with a persistent indicator (SYS-031).
- **Extensions match their OS.** An L3 extension declares the OS identity, version and architecture it is built for and is refused otherwise, as systemd checks extension images; a capability declares the interface versions it needs and is re-validated before an L1 update is marked good.
- **Derivatives** replace the package endpoint, the registry, the signing keys and the identity in their image definition (OS-028); nothing else changes.

## 4. Services

### 4.1 Unix philosophy on a typed bus

| Unix idea | On Blackbird OS |
|---|---|
| Do one thing well | One service per job. Its interface is small and named for its job (`org.opendap.audio.Engine`, `org.opendap.library.Index`) |
| Compose small programs | Composition happens on the bus. A service calls another's interface; no service embeds another's logic |
| Text streams as the universal interface | Varlink is JSON, introspectable with `varlinkctl` and `opendap bus`; bulk data stays in files and typed memfd streams, never in control messages |
| Programs that work together | Every interface is reachable by every client the grants allow: the shell, the activity hosts, capabilities, the CLI, the MCP server, paired devices (RFC 0004) |
| Silence is golden; fail loudly | Typed errors with stable codes; journald for logs; no chatty notifications |
| Ephemeral tools and long-running daemons | `ephemeral` services answer and exit (socket `Accept=yes` or oneshot); `persistent` engines exit when idle; `driver` services follow their hardware |

### 4.2 Kinds and classes

As in ADR 0002, with budgets made explicit:

| Kind | Starts | Stops | Budget declared in the manifest |
|---|---|---|---|
| `ephemeral` | On a call (socket activation) | After the reply | `time-max` per call, `memory-max` |
| `persistent` | On first call | After `idle-exit` | `memory-max`, `cpu-weight`, wakeups per hour |
| `driver` | When its hardware appears (udev, HAT attach) | When it goes | As `persistent`, plus `DeviceAllow` of its hardware only |

Background classes (`playback`, `transfer`, `device`, `short`) keep their typed budgets. The orchestrator stops work that exceeds them and says why in the capability's state (SYS-034).

### 4.3 First-party services

The system services of ADR 0002, with the ones decided since (new ones in bold):

| Service | Job | Main interfaces |
|---|---|---|
| `dap-orchestrator` | Lifecycle, capability index, slot resolution, permission evaluator, power policy, bus directory | `org.opendap.system.Capabilities`, `.Activities`, `.Slots`, `.Grants`, `.Power`, `org.opendap.bus.Directory` |
| `dap-audiod` | Playback engine, profiles and blocks, signal path, taps | `org.opendap.audio.Engine`, `.Output`, `.Profiles`, `org.opendap.media.Player` |
| `dap-library` | Index, search, artwork, events; replica of the owner's library (OS-029) | `org.opendap.library.Index`, `.Search`, `.Events`, `.Playlists` |
| `dap-dose` | Hearing-safety dose (SYS-012) | `org.opendap.hearing.Dose` |
| `dap-input` | Buttons and faceplate HID to semantic actions, timings (SYS-066) | `org.opendap.input.Actions` |
| `dap-power` | Battery, thermal, wake policy | `org.opendap.power.Battery`, `.Stats`, `.Wake` |
| `dap-fetch` | Downloads through the package endpoint; TUF metadata; recalls | `org.opendap.packages.Fetch` |
| `dap-hatd`, `dap-chipd` | HAT identification; chip flashing and debugging | `org.opendap.hat.Bus`, `org.opendap.chips.Chips` |
| **`dap-peers`** | The OpenDAP Protocol: discovery, pairing, sessions, the remote side of every interface (RFC 0004) | `org.opendap.peers.Devices`, `.Pairing`, `.Sessions` |
| **`dap-sync`** | State and index sync with peers; transfers; Recently Deleted (SYS-050 to SYS-053) | `org.opendap.sync.State`, `.Transfers` |
| **`dap-keystore`** | Every key behind one interface; filesystem today, TPM or TEE when present (OS-032, BRD-059) | `org.opendap.keys.Store` |
| **`dap-providers`** | The provider broker: which providers are enabled, for what, in which order (SYS-045) | `org.opendap.providers.Registry` |
| **`dap-voice`** | Voice guidance: speaks the focused element, changes and now playing, from the AT-SPI tree and the bus (SYS-062) | `org.opendap.a11y.Voice` |
| **`dap-notify`** | Notifications by kind, rate-limited, routed to the status panel and to peers that asked for them | `org.opendap.notify.Notifications` |
| **`dap-usb`** | The USB gadget composite and the music volume handover (blackbird-os ADR 0013) | `org.opendap.usb.Gadget` |
| `dap-agent` | The device side of the CLI and MCP (developer mode) | Introspection, input injection, screenshots |

### 4.4 Device capabilities

The hardware a device has is data, published once by the orchestrator from the board, the faceplate, the HATs and the probes of each service (SYS-040):

- an interface, `org.opendap.device.Capabilities`, lists features and their state (`present`, `absent`, `disabled: reason`);
- context keys expose the same facts to `when` predicates (`device.output.balanced`, `device.nfc`, `device.haptics`, `device.tpm`).

A feature whose hardware is absent is shown disabled with its reason, never hidden. Peers read the same interface through the protocol, so a newer Desktop knows what an older T1 can do.

## 5. Capabilities

### 5.1 What a capability can provide

In order of preference (ADR 0010):

| # | Provides | How the product uses it | Example |
|---|---|---|---|
| 1 | **A provider** implementing a shared `org.opendap.provider.*` interface | The product's own screens show its data, merged with others, ordered by the owner | A podcast directory as a library source; a lyrics source for the Lyrics view; a metadata service; a store; a scrobbler (SYS-045) |
| 2 | **Audio blocks** as data | Profiles use them like built-in blocks | A pack of headphone corrections (parametric EQ in the APO format), impulse responses for convolution |
| 3 | **A handler** | The product routes an intent to it | Open `text/x-lrc`; a `podcast:` URI |
| 4 | **A task in a slot** | The host renders it with a template | An action in Now Playing's More view; a settings section; a status line |
| 5 | **A choice for the Action long press** | Listed among the owner's choices (T1-061) | "Bookmark this moment" |
| 6 | **A driver** (HAT bundles) | Bound to its hardware | A radio tuner |
| 7 | **A HAT mode activity** | Listed only while its HAT is attached (ADR 0004) | An oscilloscope screen |

Not provided by third parties:

- code in the audio path. Processing in `dap-audiod` is first-party; third parties contribute blocks as data;
- code in the UI process;
- new destinations, top-level entries or replacements of the product's screens.

### 5.2 Manifest additions

ADR 0002's `capability.toml` gains three tables:

```toml
[[provides.provider]]              # 1: a provider
interface = "org.opendap.provider.LibrarySource@1"
service   = "podcastd"
kinds     = ["podcasts"]
network   = ["api.example.com:443"]   # opt-in: shown when the owner enables it

[[provides.audio-block]]           # 2: blocks as data
kind  = "parametric-eq"
data  = "blocks/hd800s-harman.txt" # Equalizer APO syntax (conventions spec §2)

[budgets]                          # A6
memory-max   = "48M"
wakeups-hour = 6
contributions-max = 4
```

### 5.3 Lifecycle and budgets

ADR 0002's service and activity lifecycles stand. Three rules are added:

- **Lazy by default.** Nothing of a capability runs until a contribution is shown, a provider is queried, a handler is invoked or its hardware appears.
- **Budgets are enforced, not advised.** Exceeding memory, time or wakeups stops the service and marks it, visible in "This T1 → Extensions" and `opendap capability get`.
- **Uninstall leaves nothing running.** Data is kept until the owner purges it.

## 6. Extending the interface

### 6.1 The product's activities

Fixed by OpenDAP (SYS-017, A1). Capabilities fill slots inside them:

| Activity | Screens | Slots |
|---|---|---|
| **Now Playing** (home) | Player; Queue (trailing edge); More (from the bottom): signal path, lyrics, credits | `nowplaying/more-views`, `nowplaying/more-actions` |
| **Library** | Albums, Artists, Playlists, More (tabs); album, artist, search | `library/more-entries`, `library/item-actions` |
| **Sound** | Profile axis, profile editor, signal chain, compare, hearing | `sound/blocks` (from audio-block providers) |
| **Status** | The unfolded status bar, sync line, notifications | `status/lines` |
| **This T1** | Sound, Library, Buttons and screen, Devices, Storage, Accessibility, Language, Software, Extensions, About | `settings/sections` |
| **System layers** | Volume, power, toasts, consent, pairing | none |

ADR 0002's "no notification shade" is refined by SYS-039: notifications live in the unfolded status panel, grouped by kind and rate-limited (`dap-notify`), never as a separate shade of capability content.

| **HAT mode** | Provided by a HAT bundle while attached | Its own |

### 6.2 Slots

Each slot names the control type of the design system §15.2 its contributions take, a quota and its templates:

| Slot | Control | Quota | Templates | Notes |
|---|---|---|---|---|
| `nowplaying/more-views` | Tab in More | 2 beyond the first-party views | `detail`, `synced-text`, `readout`, `list` | First-party views (signal path, lyrics, credits) come first |
| `nowplaying/more-actions` | List line | 4 | `action` | Never in the player's header or transport row, which stay first-party |
| `library/more-entries` | List line in Library → More | 8 | `action` (opens a provider's view) | A provider's view uses the library's own list and grid templates |
| `library/item-actions` | List line on an album or track's actions | 3 | `action` | — |
| `status/lines` | List line in the unfolded status | 2 | `status-line` (label, value, state) | For hardware the owner carries: a HAT battery, a tuner frequency |
| `settings/sections` | A section in its functional part and in Extensions | 1 per capability | `form`, `toggle-row`, `list` | Never a settings app of its own |
| `action/long-press` | A choice in Buttons and screen | 2 per capability | `action` | The owner picks one (T1-061) |

Rules:

- Over quota, contributions go to an overflow the owner orders.
- `when` predicates use context keys, including device capabilities (§4.4).
- A contribution that fails to bind is dropped and logged.
- Each template implements every input profile (touch, focus, dial) and carries accessible roles and names.

### 6.3 Templates

The host's catalogue, versioned in the registry: `list`, `grid` (covers), `detail`, `synced-text`, `readout` (mono values, right-aligned), `form`, `toggle-row`, `action`, `status-line`. Tier-B declarative components follow ADR 0002 once Slint's dynamic components are stable. A HAT that needs something new contributes it to the design system first (ADR 0004).

### 6.4 The same contributions on other surfaces

Templates are data, so OpenDAP Desktop and the phone app can render a capability's settings section or status line with their own renderers, through the protocol and the same grants (A3). One contribution is consistent across surfaces (the 3C model, ecosystem research E4).

## 7. Registry

- **Content:** the L6 artifacts of §3.1, each identified by name, version and hash:
  - interfaces with their IDL and generated docs;
  - schemas, templates and tier-B components;
  - design tokens and language data (SYS-029);
  - provider descriptors: which interfaces a provider kind implements and what it shows the owner before enabling it.
- **Form:** static files with TUF-style metadata (timestamp, snapshot, targets, expiry) and a signed recall list, as in research §5.6 and §5.14. Any static host can serve it.
- **Publishing:**
  - `org.opendap.*` interfaces and templates change through RFCs.
  - Publishers' own interfaces live under their reverse domain, validated by `opendap schema check`.
  - Components for the design system go through review (ADR 0004).
- **On the device:**
  - Artifacts the L1 image uses are pinned in it.
  - Others are fetched on first need and cached, content-addressed, so the device works offline with what it has.
  - Expired metadata stops new installs, never running ones.
- **Who hosts it:** the image configures one registry and one package endpoint (SYS-011). OpenDAP runs its own; a derivative or a community runs theirs (OS-028).

## 8. Permissions

Decided in ADR 0009. SYS-032 stands: until the full model is enforced, capabilities simply work, and nothing here may block them. The model is designed so that enforcement can be switched on per layer without changing manifests.

### 8.1 Principals

| Principal | Identity | Examples |
|---|---|---|
| **System** | The base image's services | `dap-audiod`, the shell |
| **Capability** | Reverse-domain ID and version; its signature | `com.example.podcasts@2.1` |
| **HAT driver** | Its capability, bound to an attached HAT | `com.vendor.radio/tunerd` |
| **Paired device** | Its key fingerprint, from pairing (RFC 0004 §5) | MacBook with OpenDAP Desktop; a phone |
| **Member** | A household member's identity, opt-in (SYS-044) | A second listener in the home |
| **Developer client** | The CLI or an agent, through SSH and developer mode | `opendap bus call …` |

### 8.2 Grants

A grant is **(principal, interface@major, level, scope, conditions)**:

- **Interface** is the unit, as in ADR 0002's routing: a principal without a grant cannot even see the socket. Needs are **required** or **optional**: a missing optional interface disables the feature that uses it, never the capability (SYS-040; Fuchsia's optional capabilities).
- **Level** comes from the method annotations the CLI already uses (read, mutate, destructive), in four steps:

  | Level | Allows | Example |
  |---|---|---|
  | **View** | Read and watch | Show now playing; list the library |
  | **Operate** | Everyday control | Play, pause, queue, volume within the dose |
  | **Manage** | Change what is stored | Edit playlists, write tags, install a capability |
  | **Administer** | Change trust and the device | Pair or remove devices, grant permissions, erase |

  The levels follow Matter's access control privileges and inherit the same way: Manage includes Operate and View; Administer is granted on its own. Multi-device, multi-person homes have a known precedent.
- **Scope** narrows the resources:
  - the owner's personal state or one member's (SYS-043);
  - the library or one folder;
  - one network host;
  - the current track only.
- **Limits:** a grant may carry session limits the server enforces, such as a rate, exclusive output, or a budget (Genode's session quotas).
- **Conditions:**
  - only while in the foreground;
  - only while its HAT is attached;
  - one time;
  - until a date;
  - only over the local network.

### 8.3 Consent

| Class | When | Shown | Examples |
|---|---|---|---|
| **Automatic** | View or Operate on non-sensitive interfaces a capability's own contributions need | Listed in its details | Read the current track for a lyrics view |
| **At enable** | Network hosts, providers, Manage on the owner's data | One screen before the capability or provider is enabled, in plain words | A podcast provider reaching its host; a tagger writing tags |
| **At use** | Sensitive resources | An OS-owned prompt at first use, with one-time and always options | Microphone, location, Bluetooth scanning, recording |
| **Privileged** | Audio exclusive access, chip flashing, raw HID, power policy, Administer | First-party, developer mode, or the owner's confirmation on the device itself (SYS-050) | A HAT's firmware update |

**Two gates.** A capability's signed manifest **declares** what it needs, with a purpose string for anything at use or privileged (the build refuses one without it, as Apple requires usage descriptions); the owner **grants** it. Neither alone is enough (Apple's entitlements and user consent).

**No static holes into the owner's data.** Access to the library, a folder or a file is granted by the owner choosing it, and the choice is the grant, recorded in the grant store (Flatpak's portals and permission store), not by a manifest line that opens the whole library.

**Roles.** Some choices are one-of: the default lyrics source, the default metadata source, the scrobbler, the default output for a kind of content. A role is a named bundle of grants the owner gives to one provider at a time, from the provider's settings (Android's roles).

**Updates.** A new version that needs more than the installed one is held until the owner approves what is new (Chrome's permission warnings on update).

For paired devices:

- Pairing grants **Operate** and **View** on playback, and **Manage** on sync, by default, as the pairing screen says (RFC 0004).
- **Administer** is asked separately.
- A guest's phone gets **Operate** on the queue only, when the owner allows it (ecosystem research §7.11).

### 8.4 Enforcement and audit

- **Locally:**
  - socket routing into the unit's namespace (ADR 0002);
  - the SDK middleware checks the peer's credentials against the grant, at the method's level, on every call;
  - Landlock and `IPAddressAllow` for network scope.
- **Remotely:** `dap-peers` evaluates the same grants for the session's principal before forwarding a call to the local interface. One evaluator, one grant store (`org.opendap.system.Grants`).
- **Audit:** every grant, use of a sensitive resource and refusal is recorded locally. It is shown in "This T1 → Extensions" and "Devices", and is readable by the CLI. It never leaves the device unless the owner exports it (SYS-055).
- **Revocation and reset:**
  - revoking removes the socket from the principal's namespace and takes effect at the next call;
  - grants unused for a long period are reset to "at use", in the manner of Android's auto-reset, with the period to be set;
  - removing a device or member revokes all its grants.

## 9. How it connects to everything else

| Part | Connection |
|---|---|
| **OpenDAP Protocol** (RFC 0004) | `dap-peers` exposes the same interfaces to paired devices, under §8's grants; `dap-sync` carries state and the index (OS-029) |
| **OpenDAP Desktop and the CLI** | Clients of the same interfaces, locally (CLI over SSH in developer mode) or through the protocol; they render capability contributions with their own renderers (§6.4); Desktop runs its own providers through the same provider interfaces (desktop research §5) |
| **Phone app** (T1-068) | A paired device: Operate and View by default, the same templates for what it shows |
| **Online services** | Providers (SYS-045) or endpoints the image configures (package endpoint, registry, a relay the owner chooses). Nothing requires one (SYS-058); their own design is outside the public repositories |
| **Interface patterns** | Slots map to the design system's control types (§6.2); quotas keep the instrument calm; first-party header and transport rows are never extended |
| **Accessibility** | Templates carry AT-SPI roles, names and states; `dap-voice` reads the tree and the bus; text size and contrast are system settings every template follows (SYS-061 to SYS-066) |
| **Languages** | Manifest labels per locale; language data as L6 artifacts loaded progressively (SYS-029) |
| **Keys** | Every key through `dap-keystore`: device identity, pairing keys, publisher keys the owner enrols (OS-025, OS-032) |
| **Updates** | L1 by RAUC, L3 and L4 through the package endpoint; capabilities re-validated against new interface versions before an L1 update is marked good (research §5.14) |
| **Derivatives** | Replace the endpoint, registry, keys and identity; keep every interface (OS-028) |

## 10. Examples

A podcast provider.

1. The owner enables `com.example.podcasts` in Library → More → Add a source. One screen says it will reach `api.example.com` and store subscriptions in the library (consent at enable).
2. It implements `org.opendap.provider.LibrarySource@1` for the kind `podcasts`. Library → More shows "Podcasts", and the episodes appear in the library's own lists, played by `dap-audiod`.
3. It contributes a settings section (refresh interval) in Library and in Extensions.
4. Nothing of it runs until the owner opens Podcasts or its refresh timer fires, coalesced with others.

A phone remote.

1. The phone pairs (RFC 0004).
2. Its principal gets Operate on `org.opendap.media.Player` and View on `org.opendap.library.Index`.
3. It plays, pauses and queues through `dap-peers`.
4. Asking to erase the T1 is refused: Administer is not granted.

A radio HAT.

1. Attaching it makes `dap-hatd` resolve its L5 bundle.
2. The owner confirms an L3 extension with its kernel module.
3. `tunerd` starts as a driver with `DeviceAllow` on the tuner only.
4. The HAT mode activity appears while the HAT is attached, and a status line shows the frequency.
5. Detaching removes all three; the grants stay in the store, conditioned on the HAT being attached.

A headphone-correction pack.

1. It provides audio blocks as data, with no service at all.
2. The Sound activity lists the corrections among its blocks, and profiles use them.
3. The signal path names the block, and the path is no longer bit-perfect.

## 11. Verification additions

Added to research §5.16:

| Piece | Check |
|---|---|
| Provider merge | Two library-source providers and the local library merged in one list; disabling one removes its items without touching the rest |
| Slot quotas and control types | 20 test capabilities: header and transport rows unchanged, quotas held, overflow ordered by the owner |
| Grants | A capability without a grant cannot see the socket; a View grant refuses a mutate call; a paired device's Operate grant refuses a destructive call through `dap-peers`; revocation takes effect at the next call |
| Budgets | A service exceeding `memory-max` or `wakeups-hour` is stopped and marked |
| Device capabilities | With NFC, haptics or TPM absent on the virtual T1, the features show disabled with their reasons |
| Accessibility | Every template's AT-SPI tree has roles and names; `dap-voice` reads a scripted flow |

## 12. Resolved points (2026-10-23)

Decided under the maintainers' delegation, from the project's rules and earlier decisions:

| Point | Decision | Why |
|---|---|---|
| **Reset of unused grants** | Grants of the "at use" class (microphone, location, Bluetooth scanning, recording) that go unused for **90 days** return to "ask". The reset is listed in the audit, the owner can change the period or turn it off, and grants of other classes never reset by themselves | Android resets after "a few months" without a number; 90 days is a visible, adjustable value. Nothing is silent (T1-045), and the owner keeps control (SYS-047) |
| **Who signs privileged declarations** | The signer of the image: OpenDAP for Blackbird OS, a derivative's own keys for its image (OS-028). Any owner can approve a privileged need on their own device with on-device confirmation, or enrol their own keys (OS-025). There is no central authority over other people's images | Openness is architectural; trust follows the image's root of trust, as in TUF and Ubuntu Core's model assertions, without locking owners out |
| **Household members on a shared device** | A member is a local identity: a name and an optional PIN, its key held by `dap-keystore`. Switching member is in the status panel and in This T1 → Devices; each member's own devices pair to their identity. No online account is needed; one is opt-in for reach beyond the home (SYS-044 stays a should, SYS-047) | No accounts by default; the shared collection stays one (SYS-043) and personal state stays apart |
| **OpenDAP Desktop's providers** | The same capability format, with one payload per target: the manifest lists payloads for `aarch64-linux` (the device) and host targets (macOS, Linux, Windows where supported). Desktop runs a host payload in its own sandbox, under the same provider interfaces and grants | Write once, reach every surface (Rust crate architecture §1; DESK-004); one provider model on device and desktop (A3, A4) |
| **Tier-B components** | Enabled by an ADR once two conditions hold: Slint releases `ComponentContainer`, named slots and `interface` as stable (not behind experimental flags), and OpenDAP's renderer conformance tests pass on the T1 within the frame, memory and energy budgets (ADR 0006). Until then, templates only | ADR 0002 and ADR 0006; no experimental dependency in the product path |
| **System extensions on trixie** | **Verified** on the Debian 13 build VM: systemd 257.13 ships `systemd-sysext` and `systemd-confext` in the `systemd` package, with `--image-policy` (to require verity and signatures) and a version check that only `--force` overrides. L3 extensions are built against each L1 release and merged by `systemd-sysext.service`; configuration overlays use `confext` | ADR 0011 |
