# ADR 0012: Shell process model and system layers

- **Status:** Accepted
- **Date:** 2026-10-23
- **Decided by:** Claude, under the maintainers' delegation (they asked for the open points to be resolved with the project's philosophy and earlier decisions)
- **Refines:** [ADR 0002](0002-system-architecture.md) (process model, status line); amends nothing else

## Context

ADR 0002 runs one host process per activity, with a shared first-party host as a fallback, and has the compositor draw the status line. The dedicated-instrument interface makes Now Playing, Library, Sound and This T1 behave as one product with shared elements, and adds a status panel that unfolds from the status bar (SYS-039, design system §8.1). The [compositor and UI runtime research](../research/2026-10-compositor-ui-runtime.md) raised five questions.

## Decision

1. **Process model.** The first-party places (Now Playing, Library, Sound, This T1) share one product host, `odap-product`. Each HAT mode activity keeps its own host. Contributions remain data only (ADR 0010), so no third-party code runs in either. The process gate of ADR 0006 measures it against separate hosts; if the shared host costs more than it saves, a later ADR returns to one host per place.
2. **Status panel.** Drawn by `odap-shell`, as the unfolding of the status bar it already draws. `status/lines` contributions render only through the fixed `status-line` template, validated before they reach the shell.
3. **Accessibility of the shell.** `odap-shell` publishes its layers' semantic tree on the bus (`org.opendap.a11y.Tree`, the AT-SPI node model), and `odap-voice` merges it with the hosts' AT-SPI trees. Recovery speaks its states with fixed phrases, or plays documented earcons when no speech engine is present, driven by the buttons.
4. **Panel mode.** The driver's; on the T1 reference board, DSI video mode, as the mainline ILI9488 driver sets it. Command mode with self-refresh is a candidate power optimisation, adopted only through measurement and a carried driver patch (OS-023).
5. **Boot splash.**
   - No Plymouth.
   - U-Boot's logo where its DSI support allows.
   - The kernel keeps the framebuffer until the shell's first frame, which matches it and fades to Now Playing.

Details: [shell and UI runtime spec](../specs/shell-and-ui-runtime.md).

## Consequences

- The system architecture spec's Status entry moves from an activity to a shell layer.
- `odap-activity-v1` carries gestures and element rectangles as well as lifecycle hints.
- The measurement gates of the spec §5.1 join the release checks (T1-048).
