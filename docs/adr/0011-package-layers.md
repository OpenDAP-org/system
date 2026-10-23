# ADR 0011: Package layers

- **Status:** Accepted
- **Date:** 2026-10-23
- **Decided by:** Claude, under the maintainers' delegation

## Context

Blackbird OS ships as a verified, read-only image updated with RAUC (blackbird-os ADR 0004). It needs, at the same time:

- OpenDAP's own and optimised packages (OS-023);
- optional system software kept apart (OS-015, OS-024);
- capabilities and HAT bundles (SYS-001, HCS-017);
- registry artifacts (SYS-005);
- derivatives (OS-028).

Precedents: systemd system extensions, Android APEX, ChromeOS DLC, OSTree, Ubuntu Core ([benchmark](../research/2026-10-capability-permissions-packages.md) §8).

## Decision

Eight layers, each with one format, source, update and rollback path:

| Layer | Content | Format |
|---|---|---|
| L0 | Boot and firmware | FIT; chip images |
| L1 | Base image | erofs + verity, A/B, RAUC |
| L2 | OpenDAP packages | `.deb` in OpenDAP's APT repository |
| L3 | System extensions | signed systemd `sysext` images, checked against the OS |
| L4 | Capabilities | signed bundles on the data partition |
| L5 | HAT bundles | L4 bundles with `[hardware]` |
| L6 | Registry artifacts | static files, TUF-style metadata |
| L7 | Owner data | documented formats |

- The verified root never changes at runtime: L2 reaches a device only inside L1; optional system software is L3, never `apt install` on the device.
- Software that needs system privileges or `/usr` is L3. Everything the capability sandbox can run is L4. A HAT bundle may depend on an L3 extension, installed with the owner's confirmation.
- Derivatives replace the endpoint, registry, keys and identity only.

Details: [system architecture spec](../specs/os-architecture.md) §3.

## Consequences

- The separate repository of OS-024 publishes L3 extensions built against each L1 release.
- Debian's and trixie's support for the needed sysext features must be verified on the image.
