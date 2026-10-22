# ADR 0006: Slint as the host renderer

- **Status:** Accepted
- **Date:** 2026-10-22

## Context

UI components are SDK-defined data that the host renders ([ADR 0005](0005-sdk-defined-services-components-and-schemas.md)), so the toolkit is an implementation detail of the host. Slint and Iced were compared: [`../research/2026-10-ui-toolkit-slint-vs-iced.md`](../research/2026-10-ui-toolkit-slint-vs-iced.md).

## Decision

- **Slint, under GPLv3**, renders the shell chrome and the SDK-defined components.
- It sits behind an OpenDAP renderer interface with conformance tests, so switching toolkits stays possible.
- Memory, idle wake-ups and energy are measured on the device before committing further.
