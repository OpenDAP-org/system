# ADR 0014: Composition over enumeration, and the test levels

- **Status:** Accepted (2026-10-24). The direction was set by the maintainers: a Unix-philosophy architecture of small modules, composition over inheritance, abstractions whose combinations emerge instead of each combination programmed by hand, applied to everything built so far, with the widest test coverage of every scenario at every level of integration. The design below was decided by Claude under their delegation.
- **Applies to:** the composer and its primitives (`opendap-input`, `-layout`, `-media`, `-motion`, `-forms`, `odap-composer`), Blackbird UI's components, the UI primitives activity.

## Context

The composer's primitives meet each other and their conditions (input source, focus shown or hidden, right to left, reduce motion, disabled, the blueprint, the display off) in more combinations than anyone can program one by one. Measured on 2026-10-24, the work had started doing exactly that:

| Where | Duplicated behaviour |
|---|---|
| Blackbird UI components | the focus report for the scroll area's reveal, 12 copies; the first key that only reveals a hidden focus, 12 copies; the disabled gate on focus, 12 copies; the lock of an adjustable, 3 implementations (Slider, RangeSlider, RadioStops); right to left handled by hand in 29 places |
| Screen and Overlay | the stops at the ends of the focus chain, written twice |
| The UI primitives activity | a key routed per experiment, by name, in one match |
| The composer's input | each condition a branch in one function (the display off, contacts that belong to the system, the activity not responding) |

Each copy is a place where one combination works and its neighbour does not; the combinatorial audit kept finding such gaps.

## Decision

1. **One behaviour, one implementation, composed.** Every behaviour lives once, with one purpose, and everything that needs it uses that one (Unix's small tools; composition over inheritance). In Blackbird UI: one focus target (report, reveal of a hidden focus, ring, disabled gate) that every control is built on; one adjustable (lock, nudge, arrows mirrored by direction, Volume never mirrored) shared by every adjustable; one end stop used by every scope; direction through `DirRow`, `Label`'s alignment and one mirroring rule, never per component.
2. **Orthogonal axes, one source of truth each.** Input flows through independent stages: raw input → the input domain's events → semantic actions → navigation (`opendap-input::nav`: the one model of where the focus is and at which level) → state → the view. Layout composes with direction; motion is a function of one progress; layers compose with their backdrop. A combination is then the product of axes that do not know about each other, and it works because each axis works on its own, not because someone wrote it.
3. **Stages, not branches, in the composer.** Input passes a chain of stages, each with one job (the display off and its wake gesture, contacts that belong to the system, an activity that does not respond, the system's combinations, the activity), so a new condition is a new stage, not a new branch in every path.
4. **Test levels.** Every primitive is tested at each level that can see it, and the combinatorial matrix (`docs/verification/`) records, case by case, at which levels it is covered:

   | Level | What | How |
   |---|---|---|
   | L0 unit | One primitive's rules | `cargo test` in each crate |
   | L1 property | Invariants over generated combinations: trees, inputs and conditions drawn at random and shrunk to the smallest failing case | `proptest` (the dependency set's choice) |
   | L2 interface | Blackbird UI's real components and the UI primitives activity driven headless, keys and touches in, state and focus out | Slint's testing backend (`i-slint-backend-testing`), no display |
   | L3 end to end | Scenarios on the virtual T1, scripted and versioned, with their evidence (screenshots, the focus ring's place, the journal) | `odap-vdev` input and screen, in a scenario runner |
   | L4 device | The same scenarios on a T1 | when there is one |

5. **The audit becomes a check.** Every gap found becomes, first, a test at the lowest level that can see it, then the fix.

## Consequences

- The refactor is retroactive: the duplicated behaviours above are replaced by their single implementations, each change covered by its tests before and after.
- The matrix gains the level each case is covered at; a case verified only by hand in the VM is not done until a level runs it again on demand.
- Adding a condition (a new input source, a display class) means writing its stage or its axis once, and its property tests, not visiting every component.
