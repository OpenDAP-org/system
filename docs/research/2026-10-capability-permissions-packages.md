# Capability permissions and package layers: benchmark

- **Date:** 2026-10-23
- **Status:** Research report, input to the [system architecture spec](../specs/os-architecture.md) and ADR 0009 to ADR 0011.

Research note, 2026-10-23. Sources were fetched by direct URL on that date. Anything not confirmed
from a fetched primary source is marked **UNVERIFIED**. The note gives no numbers that a source did
not state.

Baseline (ADR 0002): a Varlink bus with one Unix socket per interface. Capabilities are signed
erofs bundles with a TOML manifest that declares `needs`/`provides`. Routing works by binding only the
granted interface sockets into the capability's transient unit namespace. There are three consent
classes: install-time, runtime and privileged.

---

## 1. Ubuntu Core / snapd interfaces

**Facts (source: snapd `interfaces/builtin/README.md`, <https://raw.githubusercontent.com/canonical/snapd/master/interfaces/builtin/README.md>)**

- A snap declares a **plug** (consumer side) or a **slot** (provider side). Declarative rules control
  "what plugs or slots a snap is allowed to use, and ... what other slots/plugs can connect".
- Each interface has six policy keys: `allow/deny-installation`, `allow/deny-connection`,
  `allow/deny-auto-connection`. `allow-connection` also checks attribute matching. For example, the
  `content` interface requires the `content` attribute to match on both sides.
- **Auto-connect**: `allow-auto-connection` "is the final key considered". By default a plug
  auto-connects only when exactly one slot is a candidate. If there are several candidates, nothing
  auto-connects.
- **Super-privileged**: interfaces whose base declaration sets `allow-installation: false` for plugs
  "cannot be used at all without a snap-declaration assertion".
- **Base declaration**: these rules are hard-coded in snapd. Per-snap **snap-declaration assertions**,
  signed by the store, override them. So policy is split between the device (the defaults) and the
  store (the exceptions).
- **Enforcement**: security backends turn each connection into "low-level security artifacts". These
  are AppArmor, seccomp, mount namespace, udev and kernel-module snippets.
- **Snap types** (<https://documentation.ubuntu.com/core/explanation/core-elements/snaps-in-ubuntu-core/>):
  kernel, gadget (device properties, bootloader), base (common runtime libraries, tied to an LTS),
  snapd, and app. A **model assertion**, "produced and signed before the image is built", selects the
  kernel, gadget and base.
- Manual connection (`snap connect`) applies to connections that are allowed but do not auto-connect.
  Source: the CLI behaviour as commonly documented. The snapcraft.io pages now redirect behind a login,
  so this could not be re-fetched: **UNVERIFIED**.

Criticisms (UNVERIFIED, could not fetch primary sources: web search quota exhausted)

- The store is the policy authority, so auto-connect grants for a given snap are negotiated with
  Canonical's store review, not with the user.
- Some interfaces are broad, for example `home`, `removable-media` and `system-files`. Once connected,
  the user sees one switch, not a scope.
- Enforcement depends on AppArmor. On kernels without full AppArmor support, confinement is weaker
  ("devmode" or partial).

**Lesson for Blackbird OS**: this is the closest precedent to the current design. Keep the triple:
(a) a default policy shipped in the OS for each interface, (b) per-bundle exceptions signed by the
OpenDAP repository key, and (c) one compiler that turns a connection into concrete artifacts (socket
bind-mounts, unit properties, udev rules). Avoid two snapd traps. First, coarse interfaces with no
scope. Second, policy authority that only the store holds: the device owner must be able to see and
revoke every connection.

## 2. Fuchsia capability routing (beyond "routing via namespace")

**Facts (<https://fuchsia.dev/fuchsia-src/concepts/components/v2/capabilities>, …/capabilities/protocol, …/capabilities/storage)**

- Routable types are protocol, service, directory, storage, dictionary, resolver, runner, config and
  event.
- **offer** (parent to child), **expose** (child to parent) and **use** (into the component's own
  namespace). "A component can interact with the system ... only through the discoverable capabilities
  from its namespace." "Parent components play an important role in defining the sandboxes for their
  child components."
- `from: "framework"` capabilities (Realm, Binder) need no offer from the parent.
- **Availability**: a component can declare a capability as optional or required. The feature is
  documented; the exact semantics of each value were not fetched.
- **Storage** capability: "per-component isolated storage within a filesystem directory", backed by a
  directory capability. The subdirectory is keyed by moniker or by a static **instance ID**, a 256-bit
  value. Instance IDs "allow a component's storage to persist across changes to the component's
  moniker". The standard names are `data`, `cache` and `tmp`.
- **Dictionaries** bundle several capabilities so that they route as one unit.
- Component-manager security policy (allowlists of monikers allowed to use restricted capabilities): the
  page returned 404. **UNVERIFIED**.

**What it adds**: (1) Storage is a capability with a stable identity, not a path. (2) The route is
checked statically along the whole tree, not only at the leaf. (3) Optional versus required is
explicit. (4) Dictionaries group related interfaces.

**Lesson**: make per-bundle state a routed `storage` grant keyed by a stable bundle ID, so it survives
a rename or re-signing. Declare each `needs` entry as `required` or `optional`: a missing optional
interface must not stop the unit from starting (SYS-040 "disable, don't omit"). Add interface bundles
(a "dictionary"), such as `audio.playback` = {`org.opendap.Audio.Stream`, `org.opendap.Audio.Volume`}.

## 3. Android

**Facts (<https://developer.android.com/guide/topics/permissions/overview>, …/training/permissions/requesting, <https://source.android.com/docs/core/permissions/android-roles>, …/resources/app-languages)**

- **Install-time**: *normal* ("very little risk") and *signature*. Signature permissions are granted
  "only when the app is signed by the same certificate as the app or the OS that defines the
  permission".
- **Runtime** (*dangerous*): location, contacts, camera, microphone. The system prompts the user, and
  the app must check every time, never assume an earlier grant.
- **Special** permissions (`appop` protection level): "Only the platform and OEMs can define" them. The
  user grants them on the Settings > *Special app access* page (for example, draw over other apps).
  App Ops is the internal mechanism behind them. Internals beyond the `appop` level: **UNVERIFIED**.
- **Permission groups** reduce the number of dialogs. Groups "can change without notice".
- **One-time** (Android 11 / API 30): for location, microphone and camera there is "Only this time".
  Access continues while the app is visible, briefly once it is in the background, or until its
  foreground service stops. Revoking the permission kills the process.
- **Auto-reset** (API 30+): when an app "is not used for a few months", its sensitive runtime grants
  are reset. No exact period is given.
- **Repeated denial**: denying twice counts as "don't ask again" (`USER_FIXED`).
- **Roles** (RoleManager): "a unique name ... associated with certain permissions and privileges".
  Some roles the user selects (BROWSER, DIALER, SMS, HOME, ASSISTANT). Others "Only OEMs can grant"
  (SYSTEM_UI, ...). Some roles are exclusive. The version that introduced roles is not stated on the
  page (**UNVERIFIED**).
- **Per-app language** (Android 13 / API 33): a per-app setting stored by the OS and exposed in
  Settings. The app declares the locales it supports in `locale_config`. It is a precedent for
  OS-owned per-app settings, not for permissions.

**Lesson**: the three Blackbird classes map to normal, dangerous and signature/appop. Add four
Android ideas:

- one-time grants scoped to a session;
- auto-reset of unused runtime grants, with the period shown to the user and adjustable;
- a "don't ask again" state that the UI can always reverse;
- **roles** for exclusive system functions: default player, default library indexer, default output
  router, default remote-control endpoint. A role is a named bundle of grants plus exclusivity. That is
  cleaner than granting each interface separately.

## 4. iOS / macOS entitlements, TCC and purpose strings

Facts (<https://developer.apple.com/documentation/bundleresources/entitlements>, …/nsmicrophoneusagedescription)

- An entitlement is "a right or privilege that grants particular capabilities to an executable". It is
  stored "as key-value pairs embedded in the code signature". At signing time, Xcode combines the
  entitlements file and information from the developer account. Example: HomeKit needs the entitlement
  "along with explicit user consent". The entitlement and the consent are two separate gates.
- Purpose string: `NSMicrophoneUsageDescription` is "a message that tells people why the app is
  requesting access". The key "is required if your app uses APIs that access the device's microphone".
  That the system terminates an app that lacks the key: **UNVERIFIED** (not stated on the fetched page).
- TCC (Transparency, Consent and Control) is the macOS per-user consent database and its prompts. The
  internal details were not fetched: **UNVERIFIED**.

**Lesson**: there are two gates. (1) The signed manifest says the bundle *may* ask for an interface
(the entitlement). (2) The user's grant says it *does* get it (the consent). The manifest must carry a
**purpose string** for each runtime/privileged `needs` entry, shown verbatim in the prompt. Without
one, the bundle fails validation at build/sign time, not on the device.

## 5. Flatpak portals and the permission store

**Facts (<https://docs.flatpak.org/en/latest/sandbox-permissions.html>, …/desktop-integration.html, <https://flatpak.github.io/xdg-desktop-portal/docs/>, …/doc-org.freedesktop.impl.portal.PermissionStore.html)**

- By default an app has "extremely limited access": no network, no devices, filtered D-Bus.
- Static `finish-args` are "completely static". The docs advise portals "wherever possible", `:ro`
  access, and XDG directories (`xdg-music`) instead of full home access.
- Portals are D-Bus interfaces with desktop-specific backends. Picking a file in the file chooser
  *is* the grant: the user's action carries the consent.
- Permission store: "used by portals to store permissions ... to various resources". It holds tables
  of resource IDs and app IDs mapped to string arrays, and it "does not interpret" those strings.
- Camera and location portals exist in the portal API list. Their behaviour was not detailed in what
  was fetched.
- **What failed (partly UNVERIFIED)**: the docs themselves warn against `--filesystem=home` and
  `--device=all`. Many published apps still declare broad static holes, and once installed these
  bypass the portal model entirely. How common this is, is **UNVERIFIED**.

**Lesson**: give no static escape hatch to user data. Library and file access goes through a
**portal-style interface**: the user picks a folder or playlist, and the grant is that object, kept in
a permission store that is keyed by (principal, interface, object). Static grants should only exist
for interfaces with no user data.

## 6. Genode / seL4 (object capabilities)

Facts (<https://genode.org/documentation/genode-foundations/23.05/architecture/Recursive_system_structure.html>)

- A recursive tree. A new component holds only a "parent capability". Services are announced to the
  parent. Session requests go up to the parent, which routes, serves or escalates them. Every
  component in between "can impose its policy onto sessions". The parent also budgets resources (RAM
  quota) for the child.
- Trust asymmetry: a server "should generally not trust its clients".
- seL4: not fetched. Its formally verified capability kernel model is **UNVERIFIED** here.

**Lesson**: session arguments are policy too. A grant on `Audio.Stream` can carry *session labels*:
maximum sample rate, exclusive or shared output, quota. Every Varlink server validates its callers and
never trusts the manifest alone. Resource budgets (memory, CPU) belong in the grant, and in practice
in the transient unit's properties.

## 7. Matter access control (remote principals, multiple admins)

**Facts (connectedhomeip access-control guide, <https://raw.githubusercontent.com/project-chip/connectedhomeip/master/docs/guides/access-control-guide.md>)**

- Every Interaction Model operation is checked. If no entry grants it, the result is
  `0x7E Access Denied` (default deny).
- ACL entries are **fabric-scoped** and stored on endpoint 0. Admins of a fabric read and write only
  their own fabric's entries. Several fabrics (ecosystems) can administer one device: multi-admin.
- Privileges are ordered and inherit: Manage also covers Operate and View, but not Administer.
  (ProxyView also exists. Its exact place in the order was not covered by the fetched page:
  **UNVERIFIED**.)
- Auth modes are CASE (certificate-authenticated session) and Group. An entry applies only to its own
  mode. Subjects are node IDs or CATs (CASE Authenticated Tags: one tag covers a group of controllers).
  Targets are cluster, endpoint or device type. Endpoint and device type are mutually exclusive in one
  target.
- Commissioning installs an admin ACL entry. Every controller that holds the admin CAT is an admin.
- The minimum a device must support: "3 ACLs per fabric, 4 subjects per ACL, 3 targets per ACL".

**Lesson**: this is a direct template for remote principals and the household. A paired desktop,
phone or second T1 is a subject authenticated by its pairing key (the CASE equivalent). A household is
a group of subjects (the CAT equivalent). Grants are ordered privilege levels on interface targets,
with default deny. The household's admin entry is created at first setup and can never be deleted
down to zero admins.

## 8. Package layering precedents

| System | Layer | Format | Updated | Rollback | Source |
|---|---|---|---|---|---|
| systemd-sysext | Adds to `/usr`, `/opt` at runtime | Directory, DDI GPT, or raw erofs/squashfs/ext4 | Merge/unmerge with overlayfs. `extension-release` must match ID + VERSION_ID or SYSEXT_LEVEL + ARCHITECTURE | Unmerge. Read-only by default | <https://www.freedesktop.org/software/systemd/man/latest/systemd-sysext.html> |
| systemd-confext | Adds to `/etc` only. Mounted `nosuid`, `noexec` by default | Same | Same, reconfigures "without ... reboots" | Unmerge. Mutable modes: disabled (default), auto, enabled, import, ephemeral, ephemeral-import | <https://www.freedesktop.org/software/systemd/man/latest/systemd-confext.html> |
| Android APEX | Low-level system modules needed early in boot | Zip: `apex_manifest.json` + dm-verity ext4 `apex_payload.img` + `apex_pubkey`. Two signing keys | Play or adb. Active only "after reboot". Mounted at `/apex/<name>@<ver>` | A preinstalled copy in `/system` sits under the update in `/data` | <https://source.android.com/docs/core/ota/apex> |
| Android Mainline | 28+ modules (ART, Bluetooth, Media...) | APEX or APK | Google Play system updates or partner OTA. "Installs and rolls back atomically" as a set | Atomic as a set | <https://source.android.com/docs/core/ota/modular-system> |
| ChromeOS DLC | Optional features, installed on the stateful partition | Verifiable image plus metadata. A/B preallocated blocks | "Updated atomically with ChromeOS only if installed". Tied to the OS release | Follows the OS A/B (detail **UNVERIFIED**) | <https://chromium.googlesource.com/chromiumos/platform2/+/HEAD/dlcservice/docs/developer.md> |
| OSTree (Silverblue) | Whole OS tree, content-addressed | Object store, deployments | Atomic switch of deployment. 3-way merge for `/etc`, shared `/var` | Boot the previous deployment | <https://ostreedev.github.io/ostree/introduction/> (Fedora docs blocked by a bot wall. rpm-ostree layering and the Flatpak app split: **UNVERIFIED**) |
| Ubuntu Core | base / kernel / gadget / snapd / app snaps, chosen by a signed model assertion | squashfs snaps (format **UNVERIFIED**) | Store channels (**UNVERIFIED**) | Snap revert (**UNVERIFIED**) | <https://documentation.ubuntu.com/core/explanation/core-elements/snaps-in-ubuntu-core/> |
| Nix profiles | User environment as a generation of symlinks | Nix store | A new generation, switched by an atomic symlink ("atomic on Unix") | Switch back to any earlier generation | <https://nix.dev/manual/nix/2.24/package-management/profiles> |

Debian's own packaging of sysext tooling: **UNVERIFIED**. Check the `systemd` / `systemd-container`
packages in trixie before relying on it.

**Lessons**:

- sysext's `extension-release` matching is the right compatibility gate for development extensions.
- The stricter initrd image policy (`signed` only) is the model for production: use verity and a
  signature for every image outside development mode.
- APEX and Mainline show that low-level modules updated out of band still need a reboot and an atomic
  set.
- DLC shows the simplest choice: optional content versioned with the OS and updated in the same
  transaction. That matches HAT bundles better than independent channels.
- The Ubuntu Core model assertion is a precedent for a signed device model. It pins the kernel, the
  device tree overlay set and the base.

## 9. Extension UI precedents

Chrome (<https://developer.chrome.com/docs/extensions/develop/concepts/declare-permissions>)

- `permissions` / `host_permissions` are install-time, and the install shows warnings. The
  `optional_*` fields are "granted by the user at runtime".
- Adding host match patterns in an update "will also trigger a warning". Chrome recommends optional
  permissions so that updates do not alarm users.
- The per-site "site access" controls (on click / on specific sites / on all sites) and the details of
  disabling an extension on a permission increase: **UNVERIFIED** (the page returned 404).

VS Code Workspace Trust (<https://code.visualstudio.com/docs/editing/workspaces/workspace-trust>)

- Restricted Mode is the default for unknown folders. It disables or limits the terminal, tasks,
  debugging, AI agents, some workspace settings, and extensions that do not declare support.
- An extension declares its support for untrusted workspaces as supported, limited or unsupported.
- Trust is per folder, and trusting a parent folder applies to all its subfolders.

**Lesson**: (1) An update that adds a `needs` entry of a higher consent class must not be granted
silently. The bundle stays on the old version, or runs without the new interface, until the user
approves. (2) Bundles declare how they behave in a **restricted mode**, for example while signed out,
with an untrusted remote, or with a guest household member. Bundles that do not declare it are
disabled in that mode.

---

## Implications for Blackbird OS

Proposed model. These are options for the maintainers, not decisions. Everything below is derived from
the precedents above.

### Principals

| Principal | Authenticated by | Precedent |
|---|---|---|
| Local capability bundle | The bundle signature (OpenDAP key, or the user's developer key in development mode) plus a stable bundle ID | snap-declaration, APEX key |
| HAT driver bundle | Bundle signature, plus a match on the HAT's EEPROM ID (the grant is valid only while that HAT is present) | Ubuntu Core gadget, sysext `extension-release` |
| OS service | Part of the signed base image | Android `signature` |
| Remote device (desktop, phone, other T1) | Pairing key, mutually authenticated session | Matter CASE subject |
| Household member / group | A member key, or a group tag that covers several devices | Matter CAT, fabric |
| User on the device | Physical presence on the T1 UI (the root of consent) | Matter commissioning admin |

### Grants

- A grant is (principal, interface, **scope**, **privilege**, constraints, expiry, origin).
- **Interface**: one Varlink interface or an interface bundle (Fuchsia dictionary).
- **Privilege**: ordered `view < operate < manage < administer`, defined per interface. Example for
  `org.opendap.Audio`: view = read the state, operate = play and set volume, manage = change the output
  and DSP, administer = grant the interface to others (Matter).
- **Scope**: a concrete object. For example a library folder, a playlist, a HAT instance or an output
  device. Objects are granted through portal-style pickers, never as static holes (Flatpak).
- **Constraints**: session labels (sample rate, exclusive output) and resource budgets (Genode), applied
  as unit properties.
- Default deny. Grants are fabric-like for remote principals: each household's admin entries live
  separately, and there is always at least one local admin (Matter).

### Consent classes (keep the three, refine them)

| Class | Who decides | UI | Precedent |
|---|---|---|---|
| Install-time | Shown at install, from the OS default policy | A plain list of consequences | Android normal, snap auto-connect |
| Runtime | The user, at first use. The prompt shows the manifest's purpose string. Choices: once, while active, always | Prompt, plus Settings | Android dangerous / one-time, Apple purpose strings |
| Privileged | Needs a signed declaration from the OpenDAP repository, *and* explicit user approval on the device | Settings only, with a visible warning | snap super-privileged, Android signature/appop |
| Role (new) | The user picks one holder among the candidates | "Default player / indexer / output router" | Android RoleManager |

### Revocation, audit, auto-reset

- Revoking a grant removes the socket bind-mount and restarts or signals the unit. A revoked one-time
  grant ends the session (Android kills the process).
- Auto-reset: runtime grants unused for a user-visible, user-adjustable period go back to "ask". The
  period is a maintainer decision (**open**; Android gives no number).
- "Don't ask again" is a stored state that the user can always see and reverse.
- Audit: every grant, use (first use per boot), denial and revocation is logged to the journal with
  the principal and the interface. Settings shows "last used" for each grant. This visibility is a
  mitigation for the snapd store-authority criticism.
- Updates: a new version that needs more is held back until the user approves (Chrome). Manifests
  declare restricted-mode behaviour (VS Code).
- Manifest validation at sign time: every runtime/privileged `needs` entry must have a purpose
  string. Every `needs` entry is `required` or `optional`, and optional ones degrade without failing
  (Fuchsia availability, SYS-040).

### Package taxonomy

| Layer | Content | Format | Source | Updated how | Rollback | Precedent |
|---|---|---|---|---|---|---|
| Base image | Kernel, bootloader, init, core services, default interface policy, device model | Read-only rootfs image (dm-verity), RAUC bundle | OpenDAP release | RAUC A/B, whole image | Boot the other slot | OSTree, Ubuntu Core base/kernel/gadget, model assertion |
| Debian packages (standard) | Upstream Debian packages that OpenDAP mirrors or curates | `.deb` | OpenDAP APT repositories | Built into the base image. Installing at runtime on the device: **open** (conflicts with a read-only root) | With the image | Silverblue (rpm-ostree layering) |
| Optimised rebuilds | Debian packages rebuilt for the T1 SoC | `.deb`, same names with a higher version | OpenDAP optimised APT repository | Same as above | Same as above | — |
| Advanced repository | Experimental or engineer-facing packages | `.deb` | A separate APT repository, off by default | Opt-in | Remove the source | Debian experimental/backports pattern (**UNVERIFIED** as a fit) |
| Capability bundles | Apps and services (UI surfaces, integrations) | Signed erofs + TOML manifest | OpenDAP registry, or developer-signed | Independent channel, verity-checked, atomic per bundle | Keep N-1, switch back | Snap apps, Flatpak, APEX (signing) |
| HAT bundles | Drivers, DT overlay, HAT capability services | Signed erofs (sysext-like) matched to the HAT ID and the OS `SYSEXT_LEVEL` | OpenDAP or HAT vendor, signed | Versioned with the OS, updated in the same transaction when installed | With the OS slot | ChromeOS DLC, sysext `extension-release` |
| Registry artifacts | Varlink interfaces, schemas, templates, components, design tokens | Versioned, content-addressed files (format **open**) | OpenDAP registry | Pulled at build time by the SDK. On the device only as part of a bundle | Pin by version | Fuchsia/Android stable AIDL contracts (Mainline "stable interfaces") |
| OS updates | Base image + bundled HAT bundles | RAUC bundle | OpenDAP release channel | A/B, atomic | Automatic fallback on a failed boot | APEX/Mainline atomic sets |
| Development extensions | Debug tools (gdb, strace, perf), SDK runtime | systemd-sysext / confext images | Developer toolkit | Merge/unmerge at runtime. Only in development mode | Unmerge | systemd-sysext stated use case |
| Configuration overlays | Settings sets for a product or a household | confext images | OpenDAP or user | Merge without a reboot | Unmerge | systemd-confext |

### Open points for the maintainers

1. Whether apt install is allowed on a device with a read-only root, or `.deb`s only feed image
   builds.
2. The auto-reset period, and whether it is on by default.
3. Whether HAT bundles get an independent update channel (APEX style, reboot to activate) or follow
   the OS (DLC style).
4. Who can sign privileged declarations: only OpenDAP, or also a device owner's key for DIY builds.
   This interacts with "100 % of the hardware, never software limits".
5. Privilege levels: one global set (Matter) or per-interface vocabularies.

### Not covered

- The snapcraft.io and readthedocs snap pages now redirect to a login.
- Fedora docs are behind a bot wall.
- The Fuchsia security-policy page and the Chrome permissions/warnings page returned 404.
- Web search quota was exhausted, so no secondary sources were used for criticisms.

Every item marked **UNVERIFIED** above needs a second fetch before anyone cites it.
