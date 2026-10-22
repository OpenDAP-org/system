# ADR 0008: Rust dependency set

- **Status:** Accepted
- **Date:** 2026-10-22
- **Decided by:** Claude, under the maintainers' delegation ("the more official, well supported and standard, the better")

## Context

The [Rust crate benchmark](../research/2026-10-rust-crate-benchmark.md) compared candidates in 34 areas by backing, maintenance, adoption, standards conformance, licence, portability, footprint and API stability, and recommended a consolidated set. It left seven choices open (O1–O7) and found four disagreements with earlier research.

## Decision

- **The recommended set in the benchmark's consolidated table is the default dependency set** for every OpenDAP Rust repository (RFC 0002). Adding a crate outside it needs a short justification against the same criteria.
- **O1, SSH:** the CLI's SSH adapter uses `openssh` (the system's OpenSSH). If OpenDAP Desktop ships on Windows, a second adapter on `russh` implements the same port; the core does not change (Rust crate architecture §4).
- **O2, Smithay:** the compositor is a binary, not a published library, so it may depend on Smithay pinned to a git revision until a release carries the input-method hooks; the pin is reviewed at every Smithay release.
- **O3, CBOR:** not used. JSON for control and configuration, `zerocopy` frames for the data plane.
- **O4, HTTP:** one stack only, `reqwest` with `rustls` (its blocking API where a tool is synchronous); no `ureq`.
- **O5, SQLite:** bundled (`rusqlite` with the bundled feature) on host and device, so FTS5 and the SQLite version are fixed and reproducible.
- **O6, messages:** **Project Fluent** is the single message format for the system, templates, capabilities and the desktop app. Slint strings are fed from Fluent by OpenDAP's binding, so there are never two translation formats.
- **O7, EQ:** OpenDAP's own RBJ biquads in the audio engine (small, audio-critical, bit-exact control, co-designed with the DAC volume path).
- **Logging:** `tracing` to journald on the device; no OpenTelemetry anywhere in the platform (the SDK description in the system architecture research is corrected).
- **QMP:** OpenDAP's own thin client over `serde_json`; no maintained crate exists.

## Consequences

- One dependency set to audit for licences and RustSec advisories, checked by the weekly security check.
- The Windows path for OpenDAP Desktop stays open without affecting the CLI.
