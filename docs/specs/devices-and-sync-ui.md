# Devices and sync on the T1

- **Status:** Draft, 2026-10-23. Decided by Claude under the maintainers' delegation; layouts are to be drawn in the design system and confirmed on the virtual T1 (OS-018).
- **Requirements:**
  - SYS-042: real-time playback sync;
  - SYS-043, SYS-044: the household and family members;
  - SYS-048: user-facing conventions;
  - SYS-050, SYS-051: recoverable deletion, visible sync state;
  - SYS-056: identity by key;
  - T1-065: USB storage;
  - DESK-011, DESK-017: sync through the client; device management.
- **Builds on:**
  - [RFC 0004](https://github.com/opendap-project/rfcs/blob/main/text/0004-opendap-protocol.md), the OpenDAP Protocol;
  - the [user-facing conventions](https://github.com/opendap-project/system/blob/main/docs/specs/user-facing-conventions.md);
  - the [Engineered Matter design system](https://github.com/opendap-project/blackbird-ui/blob/main/docs/specs/design-system.md): targets §8.3, gestures §8.5, type §9.3;
  - the [controls and status LED spec](https://github.com/opendap-project/t1/blob/main/docs/specs/controls-and-status-led.md) §2.2;
  - the [ecosystem experience research](https://github.com/opendap-project/t1/blob/main/docs/research/ux/2026-10-ecosystem-experience.md) E8, E10, E11, E13.

Everything here works with no account and no online service (SYS-047). Every touch flow has a button alternative (SYS-039), and every state the LED shows is also said in words on screen.

## 1. Where things are

| Place | What it holds | Reached from |
|---|---|---|
| **Quick settings** (status bar, swipe down) | One sync line: "Synced 2 min ago" / "Syncing…" / "Waiting for charging" / "Can't sync", tap for detail; the **devices** shortcut | The status bar (SYS-039) |
| **Now Playing** | The **devices** button: where music plays | Top row of Now Playing, beside like and the queue (the transport row keeps its five controls) |
| **Settings → Devices** | Paired devices, household members, pairing, links in use | Settings |
| **Settings → Storage** | The storage bar by category; Recently Deleted | Settings |
| **Settings → Software Update** | Version, update, return to the previous version | Settings |

The status bar stays as SYS-038 defines it. Sync is not a permanent status-bar item; the bar stays calm, and the sync line lives one swipe away (E11).

## 2. Pairing

**Starting from the T1:** Settings → Devices → **Pair a device**. **Starting from OpenDAP Desktop or the CLI:** the T1 shows a prompt when the other side asks to pair.

1. **Code screen.**
   - A QR code, and below it the code `XXXX-XXXX` in the monospace technical face (design system §9.4).
   - The name the T1 will show to the other side.
   - A countdown to expiry (10 minutes), and **Cancel**.
   - The LED blinks blue fast (pairing).
2. **Confirmation.** When the other side has the code, the T1 shows its name and kind ("MacBook · OpenDAP Desktop") with **Pair** (the primary action, also on Action) and **Cancel**. Pairing is confirmed only on the T1 (SYS-050 for anything that grants control).
3. **Done.** "Paired with MacBook", then back to where the listener was. With a computer, the first sync starts in the background (ecosystem research §7.1).

Bluetooth headphones and phones pair as Bluetooth devices, with the standard 6-digit comparison, from the same Devices page.

## 3. Devices page

- **This device:** name (editable), owner, household.
- **Paired devices:** one row each, with
  - the device's icon (computer, phone, T1, other) and its name;
  - "Last seen 5 min ago · Synced";
  - tap for detail: links in use, last sync, what it may do (manage the library, control playback, queue only), and **Remove**.

  **Remove** revokes the device here and on the owner's other devices at their next sync.
- **Household:** members and their own state. Adding members is opt-in and needs an identity of their own (SYS-044).
- **Bluetooth:** paired headphones, speakers, cars, with the codec in use while connected.

## 4. Where music plays

The **devices** button in Now Playing opens one list, in the Spotify Connect pattern:

- this T1;
- the owner's other devices that can play (another T1, OpenDAP Desktop);
- connected Bluetooth outputs;
- network renderers.

- **Choosing** moves playback with its position and queue (SYS-042). The T1's screen keeps showing the live state while another device plays, and its buttons control that device.
- **Feedback is immediate.** The choice shows at once and settles when the other side confirms (E13). If it fails, the list says why.
- **Live state from elsewhere.** When another device of the owner is playing, Now Playing on the T1 shows it with "Playing on MacBook", and **Play here** takes it over in one action.

## 5. Connected to a computer

When a computer without an OpenDAP client takes the music volume (blackbird-os ADR 0013):

- A full screen: "Connected to a computer". Below it: "Your music card is in use by the computer. Eject it there, or here, before unplugging." **Eject** is the primary action, also on Action.
- Playback continues from what the T1 holds in its cache; tracks that are not cached show as unavailable until release, with the reason.
- The LED blinks blue slowly (eject first).
- **On release:** "Ready · 24 new tracks found", then the library updates in the background.

When OpenDAP Desktop or the CLI is connected, no takeover happens; the sync line in quick settings shows the activity.

## 6. Sync detail

Tapping the sync line opens one screen:

- **The state,** in the conventions spec's words and symbols (synced, syncing, waiting, paused, can't sync).
- **Recent activity,** grouped: arrived, waiting, failed. Each failure has its reason and next step ("Not enough space: 2.1 GB needed. Choose what to remove on the computer").
- **Conflicts,** when a real one happened (SYS-052): "This playlist was changed on two devices. Both versions were kept." Choosing one is optional and can wait.
- **Sync now** and **Pause**.

## 7. Storage and Recently Deleted

- **Storage bar** split into Music, Podcasts, Audiobooks, Recordings, Other and System. The categories use the design system's existing colours, never new ones. Tap a category for detail.
- **Recently Deleted** lists items with the days left (30 at the start), with **Restore** and **Delete now**. Items deleted on another device appear here too (SYS-050).
- **Quick removals** (from the queue, from a playlist) show an Undo bar for a few seconds.

## 8. Software Update

- Current version (`1.2.3`), "Up to date" or "Update available" with short notes, and **Download and Install**.
- Default: downloaded in the background; installed overnight while charging, with a notice before and **Not tonight** (OS-030).
- **Return to previous version**, while the previous version is kept.
- During installation the LED breathes blue (do not power off), and the screen says so when on.

## 9. Open points

- First drawn layouts at 320 × 480 in the design system tokens: [`renders/web/devices-and-sync.html`](https://github.com/opendap-project/t1/blob/main/renders/web/devices-and-sync.html) (10 screens). Next: a usability check on the virtual T1 (ecosystem research §13.2).
- The wording of every message, in all five launch languages (SYS-028).
