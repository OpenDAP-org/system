# Connecting a user's devices: towards an OpenDAP Protocol

- **Date:** 2026-10-23
- **Status:** Research report. Nothing here is decided, and no technology is chosen beyond what [ADR 0002](../adr/0002-system-architecture.md) already accepted for the device's own bus. It feeds a future RFC in `rfcs`, because the protocol crosses `system`, `blackbird-os`, `desktop`, `toolkit-cli` and every future device.
- **Question:** how should every device a listener owns (the T1, other OpenDAP devices, a computer with OpenDAP Desktop and the `opendap` CLI, a phone, speakers, a car, headphones) work together over USB, Wi-Fi and LAN, Bluetooth and the internet, in both directions and for every purpose? Does that need a protocol of OpenDAP's own?
- **Companion documents:**
  - the [ecosystem experience research](https://github.com/opendap-project/t1/blob/main/docs/research/ux/2026-10-ecosystem-experience.md) in `t1`, which derives from personas, jobs and journeys what each device does;
  - the [desktop companion research](https://github.com/opendap-project/desktop/blob/main/docs/research/2026-10-desktop-companion.md), on the library, sync and providers.
- **Conventions:** every claim about another protocol links its source; what was not checked against a primary source is **UNVERIFIED**.

## 1. Summary

1. **No existing protocol covers the whole job, and each one covers part of it well.**
   - KDE Connect: notifications, media control and file sharing.
   - Sendspin: synchronised playback with versioned roles.
   - Syncthing: peer-to-peer file sync.
   - OpenSubsonic: serving a library.
   - UPnP AV and OpenHome: renderer and server roles.
   - AVRCP: remote control over Bluetooth.
   - UAC2: the device as a USB DAC.

   What is missing is one authenticated, versioned, capability-negotiated channel. It would carry library state, settings and profiles, sync, control and handoff between devices of the same owner, over any transport (§3, §6).
2. **The best experience comes from a principle, not a feature list: the owner's devices are one system of replicas, not a master and its accessories.**
   - Each device keeps a full local copy of what it needs and works alone (local-first, §2).
   - Devices exchange changes when they meet, by whatever link is present.
   - Nothing in listening ever waits on another device.
3. **The protocol should be the device's own interfaces, carried off the device, not a second API.** Blackbird OS already defines its services as versioned `org.opendap.*` interfaces (ADR 0002), and the Desktop and the CLI share one core and one domain model (DESK-004, TK-009). Exposing the same interfaces between devices, behind owner-scoped trust, means a feature is defined once and is reachable from every surface.
4. **At the edges, be a good citizen of existing standards.**
   - Appear as a UAC2 USB DAC, an AVRCP target, a UPnP/DLNA renderer and server, an MPRIS player and an mDNS service.
   - Optionally serve a library to OpenSubsonic clients.

   Other people's devices then work with the T1 without OpenDAP software. Proprietary ecosystems (AirPlay, Cast, the "Connect" family, Roon Ready) are optional adapters, each evaluated on its own (OS-027).
5. **Capabilities, not models.** Every device advertises what it can do, and a missing capability is disabled, not designed out (SYS-040). A second T1, a future OpenDAP device or a third-party implementation joins the same system with whatever it supports.
6. **Openness needs governance.** An open specification, an extension namespace, a published licence and a conformance suite in `toolkit-verify`. KDE Connect shows the cost of skipping the last one: in CVE-2025-66270 five independent implementations shared the same impersonation flaw ([KDE advisory](https://kde.org/info/security/advisory-20251128-1.txt)).

## 2. Principles the protocol answers to

From the theory gathered in the [ecosystem experience research](https://github.com/opendap-project/t1/blob/main/docs/research/ux/2026-10-ecosystem-experience.md) §2, applied to device-to-device work:

| # | Principle | Source | What it asks of the protocol |
|---|---|---|---|
| P1 | **Local-first.** No spinners, work not trapped on one device, the network optional, data that outlives the vendor, privacy by default, the user in control | Kleppmann, Wiggins, van Hardenberg and McGranaghan, ["Local-first software"](https://www.inkandswitch.com/local-first/), Onward! 2019 | Every device holds a full replica of its state; sync exchanges changes, never asks permission to act; no central server is required for any device-to-device job |
| P2 | **Music never waits.** Playback, browsing and the buttons work with every other device absent | Case, *Calm Technology* (2015): "work even when it fails"; T1-057, SYS-036, SYS-037 | No protocol call sits on the input or audio path; remote control applies locally first |
| P3 | **Owner-scoped trust.** The devices are the owner's, and only devices the owner paired can see private state | Cavoukian, Privacy by Design (2009); Syncthing device IDs ([BEP v1](https://docs.syncthing.net/specs/bep-v1.html)) | Identity per device, membership per owner, revocation, no account required |
| P4 | **Strict inside, tolerant at the edges.** Accept messy libraries and other vendors' devices; keep the own protocol strict, versioned and loud on errors | Postel's robustness principle and its critique in [RFC 9413](https://www.rfc-editor.org/rfc/rfc9413) | Version and capability negotiation; typed errors (ADR 0002 already uses typed Varlink errors) |
| P5 | **Show the seams that matter.** Sync state, what is where, what is pending, in one glance | Chalmers and MacColl, ["Seamful and seamless design"](https://www.alandix.com/academic/conf/ubicomp-models/pdf/maccoll.pdf), UbiComp 2003 | Every sync and transfer exposes progress, last success, pending and conflicts as state, not only as logs |
| P6 | **Conflicts never lost, rarely asked.** | Terry et al., Bayou, SOSP 1995; Shapiro et al., CRDTs, SSS 2011 | Merge where intent is clear; keep both otherwise, and report once |
| P7 | **Capabilities, not models.** | SYS-040 | Devices advertise roles and features; peers adapt |
| P8 | **Bounded.** Large libraries and long sessions never degrade | T1-046, T1-047, SYS-033, SYS-034 | Incremental, resumable exchange; no message proportional to the library; bounded history |
| P9 | **Proprietary is optional.** | Workspace rule; OS-027 | Closed services are adapters behind the same interfaces; the protocol never requires them |

## 3. Who talks to whom

### 3.1 Device classes

| Class | Examples | What it brings | Owner's own? |
|---|---|---|---|
| **OpenDAP device** | The T1; a second T1; future OpenDAP devices | Playback, a library replica, profiles, the full protocol | Yes |
| **Computer** | OpenDAP Desktop, its background service (DESK-006), the `opendap` CLI | The master library for most users, curation, purchases, advanced settings, backups, updates | Yes |
| **Phone** | Any phone, with or without an OpenDAP app (none is decided) | A remote, a source over Bluetooth or the LAN, a screen in the pocket | Yes |
| **Home audio** | UPnP/DLNA and OpenHome renderers, Sendspin and Snapcast clients, AirPlay or Cast speakers | A place to send music to, or to play from | Usually |
| **Car and headphones** | Bluetooth headsets, car head units | Control (AVRCP) and audio (A2DP, LE Audio) | Yes |
| **Server** | A NAS with Navidrome, Jellyfin, MPD or a UPnP server; the user's own relay | A library to play from or sync with | Yes |
| **Cloud (optional)** | A relay, a backup or a store provider | Reach across the internet, off-site copies, purchases | A third party or the owner's own host, always optional |

### 3.2 Purposes and directions

Every purpose runs both ways where the hardware allows it (SYS-040). "In" means towards the T1.

| Purpose | In (T1 receives) | Out (T1 sends) |
|---|---|---|
| **Control** | A computer or phone plays, pauses, seeks, queues and sets volume on the T1 | The T1 controls another player or speaker (its buttons as a remote) |
| **Now playing** | Another device's playback shown on the T1 | The T1's playback shown on the computer, the phone or the car |
| **Library browse** | The T1 browses the computer's or a server's library | The computer or phone browses the T1's library |
| **Library sync** | Files and their tags arrive from the master library | New files bought or recorded on the T1 (or on a HAT) go back |
| **State sync** | Playlists, ratings, play counts, bookmarks and resume points from the other devices | The same, from the T1 |
| **Profiles and settings** | Audio profiles, headphone measurements and preferences edited on the computer | Profiles made on the T1, available everywhere |
| **Audio stream** | USB DAC (UAC2), Bluetooth receiver, network renderer | Bluetooth source, USB source to an external DAC, network source to speakers or another T1 |
| **Handoff** | Continue on the T1 what was playing on the computer, at the same point | Continue on the computer or a speaker what was playing on the T1 |
| **Files** | Any file, outside the library (documents, firmware for a HAT) | Recordings, logs, screenshots, measurement captures |
| **Device management** | Updates staged by the computer, backups restored, diagnostics requested | Backups, logs, health (battery, storage), crash reports the owner chooses to send |
| **Notifications** | From the computer or phone, filtered by the owner (off by default) | From the T1: sync done, battery low, update ready |
| **Measurement** | A computer drives a measurement session (engineer, SYS-041) | Signal-path state, meters, captures |

### 3.3 Channels

| Channel | Good at | Limits | Notes |
|---|---|---|---|
| **USB-C to a computer** | Bulk transfer, power, the USB DAC role, recovery | Cable; host OS support varies by function | USB networking (CDC NCM) would carry every network protocol over the cable ([kernel gadget functions](https://docs.kernel.org/usb/gadget-testing.html)); native macOS NCM support is **UNVERIFIED**. Mass storage gives the host the filesystem, so the T1 cannot index it at the same time ([kernel mass storage gadget](https://docs.kernel.org/usb/mass-storage.html)). macOS has no native MTP ([9to5Google](https://9to5google.com/2024/02/16/android-file-transfer-mac-hidden/)). The CLI already plans SSH over USB networking ([CLI spec](https://github.com/opendap-project/toolkit-cli/blob/main/docs/specs/opendap-cli.md)) |
| **USB-C to a DAC or a phone** | Bit-perfect audio out (host) or in (gadget) | One role at a time per port | T1-054; UAC2 ([USB-IF](https://www.usb.org/document-library/audio-device-class-specification-20)) |
| **Wi-Fi, same LAN** | Sync, browse, remote, streaming, handoff | Managed or guest networks may block multicast discovery | mDNS/DNS-SD, [RFC 6762](https://www.rfc-editor.org/rfc/rfc6762) and [RFC 6763](https://www.rfc-editor.org/rfc/rfc6763); a fallback is needed (KDE Connect keeps UDP broadcast) |
| **Wi-Fi, peer to peer** | Two devices with no network around (travel) | Platform support varies | **UNVERIFIED** on the T1's radio; listed as an option |
| **Bluetooth Classic** | Headphones and cars (A2DP, AVRCP), receiver mode | Lossy codecs; qualification required to ship | [Bluetooth SIG qualification](https://www.bluetooth.com/develop-with-bluetooth/qualification-listing/) |
| **Bluetooth LE** | Presence, wake-up, a lightweight remote, pairing bootstrap, LE Audio | Low throughput | LE Audio and Auracast ([bluetooth.com](https://www.bluetooth.com/auracast/)) |
| **Internet** | Reach when not at home: remote sync, backups | Needs a relay or a reachable host; privacy | Syncthing's relays and global discovery are a precedent ([BEP v1](https://docs.syncthing.net/specs/bep-v1.html)). Always optional, and the owner may run their own |
| **The microSD card** | Moving a large library with no link at all | Manual | A card prepared by the Desktop is a sync channel too: the same state, carried by hand |

**Choosing the link is the system's job, not the listener's.** The same exchange runs over whichever link is present and the fastest one wins (Tesler's law: the complexity stays in the system). The listener sees one state, "synced 2 min ago over USB", with the link as detail (P5).

## 4. Benchmark: what exists

Full findings, with every source, are summarised here by role.

| Role | Standards and products | Open? | Covers | Misses |
|---|---|---|---|---|
| **USB storage** | Mass storage ([USB-IF](https://www.usb.org/document-library/mass-storage-bulk-only-10)); MTP ([USB-IF](https://www.usb.org/document-library/media-transfer-protocol-v11-spec-and-mtp-v11-adopters-agreement)) | Class specs | Copying files without software | Mass storage blocks concurrent use; MTP is not native on macOS; neither knows what a library is |
| **USB channel** | ADB ([protocol](https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/main/protocol.txt)) | Open source | One multiplexed channel over USB or TCP, host keys | Developers only; no discovery for listeners |
| **USB audio** | UAC2 gadget ([configfs ABI](https://sre.ring0.de/linux/tree/Documentation/ABI/testing/configfs-usb-gadget-uac2)) | Class spec | The T1 as a USB DAC with no driver | Gadget maximum rates and DoP behaviour **UNVERIFIED** |
| **Discovery** | mDNS/DNS-SD | IETF | LAN presence and service browsing | Link-local only |
| **Renderer and server** | UPnP AV / DLNA ([OCF](https://openconnectivity.org/wp-content/uploads/2015/11/UPnP-av-AVArchitecture-v1-20020622.pdf)); OpenHome ([Linn](https://docs.linn.co.uk/wiki/index.php/Developer:Davaar), [code](https://github.com/openhome)) | Public specs; OpenHome code under MIT | Being played to; serving a library; OpenHome keeps the playlist on the renderer | Weak security; no two-way sync |
| **Multi-room playback** | Sendspin ([spec](https://github.com/Sendspin/spec)); Snapcast ([README](https://github.com/badaix/snapcast)); SlimProto ([Lyrion](https://lyrion.org/reference/slimproto-protocol/)) | Sendspin under the Community Specification License 1.0; Snapcast GPL-3.0 | Synchronised playback; Sendspin has versioned roles (`player@v1`, `controller@v1`, `metadata@v1`…), Noise encryption and mDNS | Library and settings sync; Sendspin's release status: a technical preview per [Music Assistant](https://www.music-assistant.io/player-support/sendspin), 1.0 per trade press (**UNVERIFIED**) |
| **Library API** | Subsonic and OpenSubsonic ([docs](https://opensubsonic.netlify.app/)) | Community spec | Browsing and streaming a server library from many clients; discoverable extensions | Legacy MD5 token auth; no sync of the client's own state |
| **Local control** | MPRIS ([spec](https://specifications.freedesktop.org/mpris/latest/)); MPD ([protocol](https://mpd.readthedocs.io/en/latest/protocol.html)) | Open | Desktop media keys and widgets; scriptable control | MPRIS is not a network protocol; MPD authenticates in plain text |
| **File sync** | Syncthing BEP ([spec](https://docs.syncthing.net/specs/bep-v1.html)); WebDAV with collection sync ([RFC 4918](https://www.rfc-editor.org/rfc/rfc4918), [RFC 6578](https://www.rfc-editor.org/rfc/rfc6578)) | Open | Peer-to-peer, offline-first, block-level, mutual TLS with certificate-hash device IDs, relays | Music semantics (tags, artwork, transcodes) |
| **State sync** | CRDTs such as Automerge ([concepts](https://automerge.org/docs/reference/concepts/)) | Open source | Offline edits on several replicas that converge | A domain model for library state; bounded history over years **UNVERIFIED** |
| **Multi-purpose device link** | KDE Connect, GSConnect, Valent ([protocol](https://valent.andyholmes.ca/documentation/protocol.html)) | Open | Pairing, notifications, media control through MPRIS, files, several independent implementations | Hi-res audio, library sync, device management |
| **Bluetooth** | A2DP codecs, AVRCP target and controller, LE Audio | SIG specs; LDAC decoder proprietary ([LDAC](https://en.wikipedia.org/wiki/LDAC_(codec))) | Headphones and cars | Settings-level control is vendor-private (HiBy Link, FiiO Control publish nothing, [HiBy guide](https://guide.hiby.com/en/docs/guides/hibylink/hibylink), [FiiO](https://www.fiio.com/newsinfo/385812.html)) |
| **Home** | Matter media clusters ([Amazon, Matter Casting](https://developer.amazon.com/docs/fire-tv/overview-of-matter-casting.html)) | SDK under Apache-2.0; certification for members ([NOTICE](https://pigweed.googlesource.com/third_party/github/project-chip/connectedhomeip/+/HEAD/NOTICE)) | Control from home systems | Not an audio transport (**UNVERIFIED**); certification cost |
| **Closed ecosystems** | AirPlay 2 ([MFi](https://mfi.apple.com/en/how-it-works)); Google Cast; Spotify Connect ([commercial hardware](https://developer.spotify.com/documentation/commercial-hardware/)); Tidal Connect; Qobuz Connect; Roon Ready ([KB](https://kb.roonlabs.com/Roon_Ready)) | Approval, SDK and certification gates | Large installed bases; Roon's RAAT documents bit-perfect hi-res | The open, reproducible build; the DIY edition; unofficial receivers carry legal risk ([librespot](https://github.com/librespot-org/librespot)) and, for AirPlay, no 96/192 kHz ([Shairport Sync](https://github.com/mikebrady/shairport-sync/blob/master/AIRPLAY2.md)) |

**Pairing patterns seen:**

- a certificate hash as identity (Syncthing);
- trust on first pairing with a visual confirmation (KDE Connect);
- a PAKE from a short code, then operational certificates (Matter, [Nordic](https://academy.nordicsemi.com/courses/matter-fundamentals/lessons/lesson-1-matter-introduction/topic/matter-security-model/));
- Noise with a pre-shared key (Sendspin);
- host keys accepted on the device (ADB).

## 5. What the best experience asks of it

Derived from the jobs and journeys in the [ecosystem experience research](https://github.com/opendap-project/t1/blob/main/docs/research/ux/2026-10-ecosystem-experience.md), whatever others do today.

1. **Pairing once, by looking at both screens.** The T1 shows a short code or a pattern and the other device shows the same one. Identity then lives in each device, with no account. The owner sees and revokes paired devices from any of them.
2. **Sync without a "sync" button.** When two of the owner's devices can reach each other, they exchange changes in the background, within the experience budgets (SYS-036), deferring heavy work to charging or idle. The listener only sees the state (P5). A manual "sync now" exists, as the visible alternative.
3. **Plugging in is meaningful.** A cable to a computer with OpenDAP Desktop means a fast sync, charging and, if the owner chose it, the USB DAC role. Without OpenDAP Desktop it means a standard device that works (§6.3).
4. **The T1 is a good guest anywhere.** In a hotel, a car or a friend's home, it plays to what is there by standards (Bluetooth, UPnP, a USB DAC), and nothing about the owner's private state leaves it.
5. **Handoff is one action.** "Continue here" on the receiving device, with the position, the queue and the profile's intent (a profile tied to a headphone stays with that headphone).
6. **Remote control feels local.** The controlling device shows the result at once and reconciles (Miller's and Card's 0.1 s band, cited in the theory section of the ecosystem research). The T1 never waits for the remote (P2).
7. **Several T1s or OpenDAP devices are one library.** Each one holds what its owner chose for it; playlists, ratings and history merge.
8. **Leaving is easy.** Every piece of state the protocol carries also exports in documented formats, so an owner can stop using any part of OpenDAP and keep everything (local-first ideal 5).

## 6. A shape for the protocol

A shape, not a stack. It follows the device's own architecture so the protocol is not a second system.

### 6.1 Layers

| Layer | Role | Built on (candidate direction, not decided) |
|---|---|---|
| **Identity and trust** | A key pair per device; an owner's set of paired devices; revocation; optional user-held recovery | Established patterns (§4): certificate-hash IDs, a PAKE from a short code |
| **Links** | USB networking, LAN, peer-to-peer Wi-Fi, BLE, internet through a relay; the card as an offline carrier | Standards in §3.3 |
| **Discovery and presence** | Find the owner's devices on each link; wake a sleeping T1 | mDNS/DNS-SD on IP links, BLE advertising, a relay's directory |
| **Session and negotiation** | Mutual authentication, protocol version, the capabilities each side offers | Version plus named capabilities, as OpenSubsonic's extensions and Sendspin's roles do |
| **Services** | The `org.opendap.*` interfaces the device already defines (player, library, profiles, update, diagnostics), made reachable to paired peers by permission | ADR 0002: Varlink interfaces, typed errors, Watch streams |
| **State sync** | Playlists, ratings, play counts, bookmarks, resume points, profiles, settings | A convergent model (CRDT or equivalent), bounded |
| **Bulk transfer** | Audio files, artwork, backups, updates; resumable, verified | Block-level, content-addressed exchange in the manner of Syncthing |
| **Media streams** | Audio in and out, bit-perfect where the path allows it, synchronised groups | Standards at the edges (UAC2, A2DP, UPnP, Sendspin, Snapcast); an own stream only where a gap is shown |

### 6.2 One model, every surface

The Desktop and the CLI share one core and one domain model (DESK-004, TK-009), and the device defines its services as versioned interfaces (ADR 0002). The protocol is the bridge, so:

- **a feature exists once.** "Set the active profile" is one interface method; the T1's screen, the Desktop, the CLI, an agent through the CLI's MCP server and a third-party client all call it;
- **permissions are per interface and per peer.** A paired computer may manage the library; a friend's phone, if the owner allows it, may only queue tracks;
- **versions move independently.** Each interface carries its version and hash (ADR 0002), so an older T1 and a newer Desktop agree on what both support and disable the rest (SYS-040).

### 6.3 Without OpenDAP software on the other side

The T1 must be useful to devices that know nothing about OpenDAP (P4, P9):

| The other side | What the T1 offers |
|---|---|
| A computer with no OpenDAP software | USB DAC (UAC2); a way to copy music that works on macOS and Linux with no vendor tool (the method is open, see §8); MPRIS on the T1's own session for desktop media keys over a future bridge |
| A phone | Bluetooth receiver (A2DP sink, AVRCP controller) and source; UPnP/DLNA renderer and server; optionally an OpenSubsonic-compatible library |
| Headphones and cars | A2DP source with the codec in use shown; AVRCP target with metadata, browsing and cover art |
| Home audio | UPnP/DLNA control point and renderer; Sendspin and Snapcast as open options |
| Proprietary ecosystems | Optional adapters, never in the image by default (OS-027) |

## 7. Governance, if OpenDAP publishes it

| Element | Precedent | For OpenDAP |
|---|---|---|
| **Specification** | MPRIS (freedesktop), Sendspin (Community Specification License 1.0, royalty-free patent grant) | A public specification in its own repository, with a licence suited to specifications; which licence is for the maintainers |
| **Process** | OpenSubsonic (GitHub discussions), ADR 0002 (`org.opendap.*` interfaces evolve through RFCs) | Changes through `rfcs`; third parties extend under their own reverse-domain namespace |
| **Versioning** | Sendspin per-role `@vN`; OpenSubsonic named extensions; interface hashes as in stable AIDL | Per-interface version and hash, negotiated per session |
| **Conformance** | Matter's mandatory certification; KDE Connect's missing one | A conformance suite in `toolkit-verify`, run by OpenDAP's own implementations first, available to anyone |
| **Name and mark** | Sendspin's name is trademarked by its foundation | "OpenDAP Protocol" in prose and branding, under the OpenDAP marks, so a "compatible" claim means passing the conformance suite. Technical identifiers use the short form `odap` (for example a DNS-SD service type such as `_odap._tcp`, a URI scheme, crate and package names), as the CLI already does with its `odap` alias (TK-003); the exact identifiers are for the RFC |

## 8. Implications

These become requirements only when the maintainers confirm them.

1. **One owner, many devices.** The unit of trust and sync is the owner's set of paired devices, not a pair of devices and not an account.
2. **Same interfaces on and off the device.** Off-device access reuses the `org.opendap.*` interfaces with per-peer permissions, rather than a separate remote API.
3. **Link-independent.** Every purpose in §3.2 works over every link that can carry it, chosen by the system. The card counts as a link.
4. **Never on the critical path.** No protocol work delays input, audio or the UI (SYS-036, SYS-037, T1-057); heavy work waits for idle or charging.
5. **Visible state.** Sync, transfer and pairing expose state that every surface shows the same way: what is where, what is pending, what conflicts, the last success.
6. **Good citizen.** UAC2, AVRCP, A2DP, mDNS, UPnP/DLNA and MPRIS at the edges, as listed in §6.3.
7. **Copying music without vendor software on macOS** is unsolved by the classic options (mass storage blocks concurrent use, MTP is not native). It needs a deliberate answer: USB networking with a standard file protocol, a dual-LUN or card-only mass storage mode, or the Desktop. That choice is for the maintainers.
8. **Conformance before compatibility.** A public test suite gates every implementation, OpenDAP's included.

## 9. Open points

### 9.1 To check before an RFC

- Native CDC NCM support on current macOS and on the Linux distributions in scope.
- UAC2 gadget: the highest PCM rate and DoP behaviour on the T1's USB controller.
- Peer-to-peer Wi-Fi on the T1's radio.
- Sendspin's release status and licence terms, as a candidate for the playback role instead of a new stream protocol.
- Whether LE Audio's standard media and volume control profiles can carry the BLE remote role without an own GATT service.
- Bluetooth qualification cost and process for the Commercial and DIY editions.
