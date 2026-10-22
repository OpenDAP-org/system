# ADR 0003: Native binaries, no WASM

- **Status:** Accepted
- **Date:** 2026-10-22

## Context

Capabilities and services could be distributed as WebAssembly components or as native binaries.

## Decision

OpenDAP does not use WASM, in v1 or later. Services and capabilities ship as **binaries compiled for the board's specific target** (aarch64 Linux).
