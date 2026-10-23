# ADR 0010: Capabilities feed the product before they draw on it

- **Status:** Accepted
- **Date:** 2026-10-23
- **Decided by:** Claude, under the maintainers' delegation

## Context

The interface is a dedicated instrument with a fixed information architecture (SYS-017; design system §8.1; the [interface mockups](https://github.com/opendap-project/t1/blob/main/renders/web/ui-mockups.html)). Every third party enters as an opt-in provider (SYS-045). ADR 0002's slots let capabilities add tasks; with many installed, UI contributions alone would fragment the screens.

## Decision

- A capability extends the product in this order of preference:
  1. a **provider** of a shared `org.opendap.provider.*` interface, whose data the product's own screens show;
  2. **audio blocks as data**;
  3. a **handler**;
  4. a **task in a slot**, rendered by host templates;
  5. a **choice for the Action long press**;
  6. a **driver**;
  7. a **HAT mode activity** while its HAT is attached.
- **Slots** name the design system's control type, a quota and their templates:
  - `nowplaying/more-views`, `nowplaying/more-actions`;
  - `library/more-entries`, `library/item-actions`;
  - `status/lines`, `settings/sections`;
  - `action/long-press`.

  The player's header and transport rows are never extended.
- **Third parties never put code in the audio path or the UI process**, and never add destinations.
- Contributions are data, so OpenDAP Desktop and the phone app may render them too.

Details: [system architecture spec](../specs/os-architecture.md) §5 and §6.

## Consequences

- Shared provider interfaces (library source, lyrics, metadata, store, scrobbler, audio blocks) become first-class RFC subjects.
- First-party screens must merge provider data well, ordered by the owner.
