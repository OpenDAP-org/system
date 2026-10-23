# Rust crate architecture

- **Status:** Accepted ([RFC 0002](https://github.com/opendap-project/rfcs/blob/main/text/0002-rust-crate-architecture.md))
- **Date:** 2026-10-22
- **Applies to:** every OpenDAP Rust repository: system (services, SDK), blackbird-os tools, toolkit (the `opendap` CLI and its plugins), OpenDAP Desktop, firmware where Rust is used.

The goal is reuse and portability: a capability is written once and reaches every surface (CLI, OpenDAP Desktop, device services, agents), and core code runs on every supported host and on the device.

## 1. Crates first

- Every capability is a **library crate**. A binary crate only parses input, wires dependencies and renders output; it holds no logic another surface could need.
- A crate has one purpose, a small public API and documentation on every public item.
- Prefer several focused crates over one large one; split when two consumers need different parts.

## 2. Layers

Dependencies point inward only. An inner layer never depends on an outer one.

| Layer | Contains | May depend on | Example |
|---|---|---|---|
| **Domain model** | Types and rules of the domain: devices, targets, modes, tracks, capabilities, errors as data. No I/O, no async runtime, no platform code | std, `serde`, small pure crates | `opendap-model` |
| **Application core** | Use cases expressed against **ports** (traits): discover devices, run a command, transfer a file, flash an image, build an image | domain model | `opendap-core` |
| **Adapters** | Implementations of the ports for one technology: SSH, USB/MaskROM (`rockusb`), QEMU/QMP, filesystem, network, Varlink bus | core, domain model, the technology's crates | `opendap-transport-ssh`, `opendap-transport-vm` |
| **Interfaces** | Surfaces for people and programs: CLI, desktop UI, device services, MCP server | everything above | `opendap-cli`, OpenDAP Desktop, device services |

Shared conventions of one interface kind (for example the CLI's output modes and error envelope) live in their own crate in the interface layer (`opendap-cli-kit`), not in the core.

## 3. Domain model

- One domain model per bounded context (host tooling, device system, audio engine, library), shared by every surface of that context. The CLI and OpenDAP Desktop use the same model and the same core; neither re-implements a use case.
- Types that cross the device–host boundary come from the bus schemas (system SDK, generated from the Rust types), so both sides compile against one definition.
- Errors are data: stable codes, typed fields, no stringly-typed errors in libraries.

## 4. Portability

- Domain and core crates are platform-agnostic: no POSIX-only or macOS-only assumptions, no hard-coded paths, no shelling out. Platform specifics live in adapters, selected by target or Cargo feature.
- This keeps Windows support possible for OpenDAP Desktop (DESK-005) and lets device and host share crates.
- `no_std` where it costs little (domain types used by firmware).
- Async: core APIs do not tie callers to one runtime unless the crate's purpose requires it.

## 5. Conventions

- Edition 2024; one Cargo workspace per repository; shared `[workspace.package]` metadata.
- Naming: `opendap-<area>[-<part>]` (`opendap-model`, `opendap-core`, `opendap-transport-usb`).
- Errors: `thiserror` in libraries, `anyhow` only in binaries.
- Features: minimal defaults; optional adapters behind features.
- No `unsafe` without a comment that justifies it and a test that exercises it.
- Semantic versioning for every library; breaking changes are listed in the changelog.
- **Licences:** libraries others build on are MPL-2.0; binaries and services are GPL-3.0-or-later; firmware and templates MIT (workspace licensing rule).
- Tests: unit tests in the domain and core against fake adapters; contract tests that every adapter of a port passes; end-to-end tests against the virtual T1.

## 6. Reuse across repositories

- Host-side crates (`opendap-model`, `opendap-core`, adapters) live in toolkit-cli today and are consumed by OpenDAP Desktop by version; they move to their own repository if a third consumer appears.
- Device-side crates live in system and are the SDK.
- A crate needed on both sides moves to the side that owns its domain and is consumed by version from the other.
