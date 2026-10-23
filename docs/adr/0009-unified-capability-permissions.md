# ADR 0009: One capability-based permission model for every principal

- **Status:** Accepted
- **Date:** 2026-10-23
- **Decided by:** Claude, under the maintainers' delegation

## Context

ADR 0002 routes interfaces into a capability's namespace and defines three consent classes. Since then, more principals call the same interfaces:

- paired devices and the phone app through the OpenDAP Protocol (RFC 0004);
- household members (SYS-043, SYS-044);
- HAT drivers;
- the developer's CLI.

SYS-032 asks for capability-based, granular permissions that never block capabilities before they exist. The benchmark is in [`../research/2026-10-capability-permissions-packages.md`](../research/2026-10-capability-permissions-packages.md): snapd, Fuchsia, Android, Apple, Flatpak, Genode and Matter.

## Decision

- **Principals:** the system, capabilities, HAT drivers, paired devices, household members and developer clients.
- **A grant** is (principal, interface@major, level, scope, limits, conditions).
  - **Levels:** View ⊂ Operate ⊂ Manage, from the interfaces' method annotations; Administer is granted on its own (Matter's privileges).
  - **Scopes** narrow the resources (a member's state, a folder, a host, the current track).
  - **Limits** are enforced by the server (Genode's session quotas).
  - **Conditions:** foreground, HAT attached, one time, expiry, local network only.
- **Two gates:** the signed manifest declares, with purpose strings for sensitive needs; the owner grants.
- **No static holes into the owner's data:** choosing a folder or file is the grant (Flatpak portals).
- **Roles** for one-of choices among providers (Android roles). Updates that need more are held until approved.
- **Consent classes:**
  - automatic;
  - at enable;
  - at use, with one-time and always options;
  - privileged: first-party, developer mode, or the owner's confirmation on the device.
- **One evaluator and one grant store** (`org.opendap.system.Grants`):
  - locally: socket routing plus peer-credential checks at the method's level;
  - remotely: `dap-peers` applies the same grants before forwarding a call.
- **Audit and reset:** grants, sensitive uses and refusals are recorded locally and shown to the owner. Unused "at use" grants return to "ask" after 90 days, listed in the audit, adjustable or off; other grants never reset by themselves. Revoking removes the socket.
- **SYS-032 holds:** enforcement is switched on per layer; until then capabilities simply work, and manifests do not change when it is.

Details: [system architecture spec](../specs/os-architecture.md) §8.

## Also decided (2026-10-23)

- Privileged declarations are signed by the image's signer: OpenDAP for Blackbird OS, a derivative for its own image; any owner may approve on their own device or enrol their keys (OS-025).
- Household members are local identities (a name, an optional PIN, a key in `dap-keystore`); no online account is needed (SYS-047).

## Consequences

- Interfaces must annotate every method's level; the SDK and `opendap schema check` enforce it.
- The phone app, Desktop and guests get exactly what the pairing screen said, and nothing else.
