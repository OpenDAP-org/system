# ADR 0002: System architecture

- **Status:** Accepted
- **Date:** 2026-10-22

## Context

The system layer must deliver capabilities instead of apps (SYS-001 to SYS-011), a product-like UI rather than an OS (SYS-017), bit-perfect audio (OS-020), battery life (T1-009) and standard, trick-less Linux software (T1-011). Proposal and benchmark: [`../research/2026-10-system-architecture.md`](../research/2026-10-system-architecture.md).

## Decision

The proposal in the research report is adopted, with these choices:

- **Orchestrator:** systemd stays PID 1. The OpenDAP orchestrator is a systemd service that runs services as transient units in slices, with sandboxing, socket activation, the cgroup freezer for paused activities, and logind inhibitors.
- **Bus:** Varlink (`zlink`) for control, plus sealed `memfd` rings with fixed-layout binary frames for data (audio, spectrograms). Subject to a measurement gate on the Cortex-A55; a capability's sandbox only contains the bus sockets its manifest declares.
- **UI:** a Smithay-based compositor drawing the product chrome in Slint, **one process per activity**, Android-like activity lifecycle mapped to the freezer. Capabilities fill bounded slots with **host-rendered templates** first; declarative `.slint` components later, when Slint's dynamic composition is stable.
- **Product, not OS:** no launcher, app grid, notification shade or per-capability settings. The only activities beyond the product's own are **HAT-mode activities**, shown only while their HAT is attached.
- **Slint licence:** GPLv3, like the rest of `system`.
- **Audio engine:** exclusive direct ALSA output with a typed signal-path status (including a bit-perfect flag that verification checks against captured bits); PipeWire for shared audio. **System sounds are muted during exclusive playback.**
- **Manifests:** TOML.
- **Session:** a `odap` user session (`user@.service`) for the composer (RFC 0012; first named the shell), activities and PipeWire.
- **Permissions:** three classes (install-time, runtime prompt, privileged).
- **Offline devices:** installed capabilities keep running when recall metadata expires; only installs and updates stop.
- **Interfaces:** shared `org.opendap.*` interfaces evolve through RFCs; third parties publish under their own namespace.

Still open: accessibility level at launch, and how developer mode is entered and shown.
