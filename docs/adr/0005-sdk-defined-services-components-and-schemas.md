# ADR 0005: SDK-defined services, components, schemas and bindings

- **Status:** Accepted
- **Date:** 2026-10-22

## Context

Capabilities and HATs must bring versioned UI components at runtime (SYS-018), data schemas are versioned and shared through the registry (SYS-005), the UI runtime needs two-way data binding (SYS-008), and no WASM is used (ADR 0003). The UI toolkit (Slint or Iced) is still under evaluation.

## Decision

OpenDAP defines its own model, authored **in Rust through the official SDK**, for everything a capability provides:

- **Services:** created with SDK types and macros that implement the orchestrator lifecycle, the Varlink interfaces and the `memfd` data streams.
- **Data schemas:** Rust types in the SDK produce versioned, validated schemas published to the registry.
- **UI components:** declarative components written in Rust with SDK types and macros compile to versioned, schema-validated **data, not code**, shipped in the package or the registry. The host interprets them and renders them with the official `ui-components`; heavy visuals use host primitives (plot, canvas) fed by the data plane. The UI toolkit becomes an implementation detail of the host.
- **Two-way data binding:** component properties bind to service properties in both directions over the bus, through the SDK.

Using the SDK is how developers build for Blackbird OS, and it **enforces the project's optimizations, practices and patterns** (lifecycle, power behavior, sandboxing, schema versioning, design system).
