# ADR 0013: Hardware described by data, served by adapters

- **Status:** Accepted
- **Date:** 2026-10-23
- **Decided by:** Claude, under the maintainers' delegation. The rule itself is theirs: the T1 is the reference hardware, but Blackbird OS must not be limited to it, and everything hardware-specific is subordinate to the OS (SYS-040, SYS-067, SYS-068).

## Context

The first specifications wrote the T1's parts and limits into OS and UI rules:

- the SoC's display planes;
- the panel's size and density;
- the button positions.

Blackbird OS must support any combination of SoC, board, components, faceplates, HATs and external peripherals, and optimise for each. Benchmark: [`../research/2026-10-hardware-support.md`](../research/2026-10-hardware-support.md).

## Decision

- **Six layers** (SoC, board, components, faceplate, HATs, external), each referring only to the named connectors of the layer below.
- **Two descriptions merged into one record.**
  - Devicetree carries what has a binding, with UCM2 and hwdb where they apply.
  - A signed, versioned **OpenDAP device description** (`opendap.device-description/v1`) carries layout, control roles and positions, display shape and distance class, outputs, audio and safety limits with sources.
  - The orchestrator merges them with probes into `org.opendap.device.Capabilities`.
- **One adapter per component class**, implementing `org.opendap.hw.<class>`, bound by declared matching rules; generic fallbacks for every class.
- **Board support packages** (L2): one image per board, the same OS for all (OS-004).
- **The UI and the design system** take scale, layout class, input profiles and the placement of physical controls from the record. They are optimised for each class, the T1's first (DS-022).
- **Compatibility** checked on update and on attach, in the manner of VINTF; interfaces only grow.
- **Nothing hardware-specific is hard-coded** outside the board sources and the component services (spec §9).

Details: [hardware support spec](../specs/hardware-support.md).

## Consequences

- The specifications that named T1 parts now present them as the T1 reference board's values:
  - the shell spec's planes and panel mode;
  - the design system's panel figures;
  - the mockups' button positions.
- The virtual T1 becomes one description among the test fixtures.
