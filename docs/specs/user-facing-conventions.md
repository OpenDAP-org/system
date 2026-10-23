# User-facing conventions

- **Status:** Draft, 2026-10-23. Decided by Claude under the maintainers' delegation, from the benchmark of user-known formats and the [case studies](https://github.com/opendap-project/t1/blob/main/docs/research/ux/2026-10-ecosystem-case-studies.md). Items marked **UNVERIFIED** need a source before they become final.
- **Requirements:**
  - SYS-048: conventions people already know;
  - T1-065: USB storage by default;
  - SYS-042 to SYS-047: real-time playback sync, the household, providers, no accounts;
  - DESK-010, DESK-011: transcoding per device, sync through the client;
  - OS-017: A/B updates with rollback;
  - OS-029: the protocol syncs everything.
- **Scope:** what a listener sees on the T1, in OpenDAP Desktop and in files that leave the device. How OpenDAP stores things internally is separate and documented for developers. No listener ever needs to know it (SYS-048).

Each choice names the convention it follows.

## 1. Storage over USB

When the T1 is connected to a computer without OpenDAP Desktop or the CLI, it is a plain USB drive (T1-065).

| Item | Convention | Follows |
|---|---|---|
| File system | exFAT, the default of SDXC cards; FAT32 cards are read too (4 GB − 1 per file) | [SD card standard](https://en.wikipedia.org/wiki/ExFAT), [FAT](https://en.wikipedia.org/wiki/File_Allocation_Table) |
| Volume label | `OPENDAP` (at most 11 characters) | [exFAT specification §7.3](https://learn.microsoft.com/en-us/windows/win32/fileio/exfat-specification) |
| Top-level folders | `Music/`, `Podcasts/`, `Audiobooks/`, `Recordings/`, `Playlists/` | Android's public directories ([`Environment`](https://developer.android.com/reference/android/os/Environment)); Rockbox's `/Playlists` and `/Recordings` ([`rbpaths.h`](https://raw.githubusercontent.com/Rockbox/rockbox/master/firmware/export/rbpaths.h)) |
| Files dropped anywhere | Found and indexed wherever they are; the folder only gives the default category (a podcast under `Podcasts/` is not mixed with music) | Android's media scanner; Rockbox |
| Playlists | `.m3u8` files in `Playlists/`: UTF-8, `#EXTM3U` and `#EXTINF`, paths relative to the playlist, so the card plays in other players | [M3U](https://en.wikipedia.org/wiki/M3U) |
| Internals | Nothing hidden at the root: no system folder and no media database in the volume; the device's own state lives in its internal storage | The iPod's hidden database is the counter-example ([iPod](https://en.wikipedia.org/wiki/IPod)) |

**Proposed in [RFC 0004](https://github.com/opendap-project/rfcs/blob/main/text/0004-opendap-protocol.md) §3:** the T1 hands its music volume to the host exclusively, keeps playing from its cache levels, shows "Connected to a computer" with **Eject**, and re-indexes on release; with OpenDAP Desktop or the CLI present, USB networking is used instead. **Still open (engineering, `blackbird-os`):**

- which volumes the T1 exports (the card, a music area of the internal storage, or both);
- how playback and indexing behave while the computer holds the volume. One direction is to keep playing from the cache levels of OS-022 and to re-index changes on release.

## 2. Formats that leave the device

| What | Export | Import | Follows |
|---|---|---|---|
| Playlists | `.m3u8` (default); `.jspf` with open identifiers for lossless round trips | `.m3u`, `.m3u8`, `.pls`, `.xspf`, `.jspf` | [M3U](https://en.wikipedia.org/wiki/M3U), [PLS](https://en.wikipedia.org/wiki/PLS_(file_format)), [XSPF](https://www.xspf.org/), [JSPF](https://musicbrainz.org/doc/jspf) |
| Listening history | `.scrobbler.log` (Audioscrobbler portable player log 1.1), ListenBrainz JSON, CSV | The same, plus Spotify's "Download your data" JSON | [Rockbox scrobbler](https://raw.githubusercontent.com/Rockbox/rockbox/master/apps/plugins/lastfm_scrobbler.c), [ListenBrainz JSON](https://listenbrainz.readthedocs.io/en/latest/users/json.html), [Spotify](https://support.spotify.com/us/article/understanding-your-data/) |
| EQ and headphone corrections | `ParametricEQ.txt` and `GraphicEQ.txt` in the Equalizer APO syntax | The same, as AutoEq publishes them for most headphones | [Equalizer APO](https://sourceforge.net/p/equalizerapo/wiki/Configuration%20reference/), [AutoEq](https://github.com/jaakkopasanen/AutoEq) |
| A whole audio profile | One documented profile file (audio profiles spec §6), with the EQ inside in the APO syntax | The same | — |
| Library list | CSV (one row per track) | iTunes / Music `Library.xml` (**UNVERIFIED**: to check) | — |
| Backup | One file with a plain, dated name: `OpenDAP T1 backup 2026-10-23.zip`. Restored from OpenDAP Desktop or the CLI; the listener never opens it | The same file | iOS and Android backups expose no files ([Apple](https://support.apple.com/en-us/108366)) |

The audio files themselves are never converted when they leave (DESK-010 applies only to a device's copy, never to the library).

## 3. Pairing and devices

| Flow | Convention | Follows |
|---|---|---|
| The T1 with headphones, a phone or a car | Standard Bluetooth pairing; the 6-digit numeric comparison on screen | [Bluetooth Secure Simple Pairing](https://en.wikipedia.org/wiki/Bluetooth) |
| The T1 with OpenDAP Desktop or the CLI | The desktop finds the T1 (cable or network). The T1 shows a QR code and a short code (`XXXX-XXXX`) that expires. The listener confirms on the T1, by touch or the Action button | Chromecast's QR code with a typed fallback ([Google](https://support.google.com/chromecast/answer/2998456?hl=en)); the device code of [RFC 8628](https://www.rfc-editor.org/rfc/rfc8628) |
| The T1 with another OpenDAP device | The same as with the desktop | — |
| Devices page | On the T1 (Settings) and in the desktop: each paired device with name, kind, last seen, last sync and **Remove** | Apple's and Google's device lists ([Apple](https://support.apple.com/en-us/102649), [Google](https://support.google.com/accounts/answer/3067630?hl=en)) |
| Choosing where music plays | One **devices** button in Now Playing opens the list of places to play: this T1, the desktop, other paired devices, Bluetooth outputs, network renderers. The system's own AirPlay or Cast symbol appears only where that protocol is really used, never a look-alike | Spotify Connect's device picker ([Spotify](https://support.spotify.com/us/article/spotify-connect/)); Google Cast and Apple AirPlay guidelines ([Cast](https://developers.google.com/cast/docs/design_checklist/cast-button), [AirPlay](https://developer.apple.com/design/human-interface-guidelines/airplay)) |

Under the familiar surface, identity is the device's key, recorded at pairing; anything said before the secure channel is re-verified inside it (case studies, lesson 1).

## 4. Sync status

The same words and symbols on every surface (ecosystem research E8):

| State | Wording | Symbol | Follows |
|---|---|---|---|
| Synced | "Synced" (with the time, on detail) | Check | [Dropbox](https://help.dropbox.com/sync/sync-icons), [OneDrive](https://support.microsoft.com/en-us/office/what-do-the-onedrive-icons-mean-11143026-8000-44f8-aaa9-67c985aa49b3) |
| Syncing | "Syncing", with progress | Circular arrows | Same |
| Waiting | "Waiting for the T1" (or for charging, or for a link) | Clock | Dropbox's "waiting to be uploaded" |
| Paused | "Paused" | Pause | OneDrive |
| Cannot sync | "Can't sync", and the reason with the next step one step down | Cross, in the design system's error colour | Dropbox, OneDrive |

Per item (an album in the desktop's library), the same symbols say whether it is on the T1, on its way, or only on the computer. The colours are the design system's semantic colours, never new ones.

## 5. Deleting and undoing

| Action | Convention | Follows |
|---|---|---|
| Delete a track, an album or a playlist | It goes to **Recently Deleted** for 30 days, on every device, then is removed | Google Drive, Gmail, Google Photos, iCloud Photos: 30 days ([Drive](https://support.google.com/drive/answer/2375102?hl=en), [Photos](https://support.google.com/photos/answer/6128858?hl=en), [iCloud](https://support.apple.com/en-us/HT204247)) |
| Quick actions (remove from queue or playlist, a batch tag fix) | A short bar with **Undo**, dismissed without input | [Material snackbar](https://github.com/material-components/material-components-android/blob/master/docs/components/Snackbar.md); Gmail's Undo Send |
| Destructive device actions (erase, factory reset, remove all music) | Confirmed on the device itself, never only over the network | Western Digital My Book Live, 2021 (case studies §3.6) |

## 6. Storage

One bar per device, split by category: Music, Podcasts, Audiobooks, Recordings, Other, System, with detail per category and a warning when space runs low. Follows the iPhone and macOS storage screens ([iPhone](https://support.apple.com/en-us/108429), [Mac](https://support.apple.com/en-us/102624)).

## 7. Updates

| Item | Convention | Follows |
|---|---|---|
| Screen | **Software Update**: current version, "Update available" with short notes, **Download and Install** | iOS and Android ([Apple](https://support.apple.com/en-us/118575), [Android](https://support.google.com/android/answer/7680439?hl=en)) |
| Default | Downloaded in the background; installed overnight while charging, with a notice before and an opt-out; never during playback | iOS automatic updates; ecosystem research §7.9 |
| Failure | Automatic return to the previous version if the new one fails (OS-017) | [Android A/B updates](https://source.android.com/docs/core/ota/ab) |
| Going back | **Return to previous version**, visible while the previous version is kept. Phones do not offer this; the T1 does (T1-045, case studies lessons 10 and 12) | — |
| Deferring | Updates can be postponed by the listener | CrowdStrike, 2024 (case studies §3.4) |
| Release notes | Plain words, newest first, grouped as Added, Changed, Fixed, Removed, Security; a link to the full changelog | [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) |
| Versions | Numbers only, `1.2.3`, no codenames | [SemVer](https://semver.org/); Blackbird OS ADR 0011 |

## 8. Notifications

- Notifications are grouped by kind (sync, updates, battery, hearing safety, from other devices), and the listener chooses per kind what may interrupt. This follows Android's notification channels ([Android](https://developer.android.com/develop/ui/views/notifications/channels)).
- During playback, only safety notifications reach the centre (ecosystem research E10, E11).
- Notifications from other devices are off until the listener turns them on.

## 9. Still to verify

- Status LED colours and patterns that people know (charging, pairing, fault): to source before extending the LED code (T1-044). **UNVERIFIED**.
- Folder behaviour of current Sony, FiiO, HiBy and Astell&Kern players over USB. **UNVERIFIED**.
- The iTunes / Music `Library.xml` import, and the iCloud Drive and Google Drive status wording. **UNVERIFIED**.
