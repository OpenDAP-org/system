# Performance baselines of the UI primitives

- **Date:** 2026-10-24
- **Status:** baseline, measured on the host and the virtual board. **The T1 is not measured yet**: every number here is the point optimisations are compared against, never a claim about the device.
- **Why:** the primitives that exist to make large content fast (windowing, recycling, paging, caches, reveal, navigation) need a measured starting point before anything is optimised (T1-046, T1-047), so a change shows what it gained or cost.

## How to repeat

- **Microbenchmarks** (criterion, the benchmark crate in the dependency set): from the workspace, `cargo bench -p opendap-layout -p opendap-input -p opendap-media -p opendap-motion --bench layout --bench input --bench media --bench motion -- --warm-up-time 1 --measurement-time 3`. The benches are in each crate's `benches/`.
- **On the virtual board:** the UI primitives activity in the virtual T1; a script samples `/proc/<pid>/stat` (user and system time), `VmRSS` and the context switches of every thread of `odap-shell` and `odap-testbed` over a window, while the buttons drive one scenario (`odap-vdev input key … down`, held).

## Microbenchmarks

Host: Apple M3 Max, rustc 1.99.1, release profile. Median of criterion's estimate.

| Primitive | What | Time |
|---|---|---|
| `opendap-layout::extents` | offset of an item, 1 M measured items | 5.7 ns |
| | item at an offset, 1 M | 264 ns |
| | measure an item, 1 M | 2.3 ns |
| `window` | one frame's window while flinging, 1 M | 525 ns |
| `recycle` | slide a 20-key window by one item | 667 ns |
| `data::pages` | plan a window's pages, 1 M | 134 ns |
| `data::cache` | put within a budget of 60 | 59 ns |
| `focus` | a step and keep in view, 1 M | 18 ns |
| `opendap-input::arena` | a tap then a drag through three recognizers | 563 ns |
| `dispatch` | a tap down a five-node path, 32 listeners | 6.1 µs |
| `nav` | a step in a level of 1000 stops | 3.5 µs |
| `nested::reveal` | through two scroll areas | 14 ns |
| `opendap-media::plot` | a million samples to 300 columns | 91 µs |
| | push into a meter's ring | 0.96 ns |
| `fit` | decode size of a cover | 2.6 ns |
| `opendap-motion::spring` | sample | 1.7 ns |
| `easing` | sample the standard curve | 24 ns |
| `animator` | retarget and sample | 61 ns |

Every primitive is far inside a frame (16.7 ms at 60 Hz) on the host. The A55 cores of the T1 are several times slower: the device numbers come when it can be measured.

## Virtual board

Host as above; QEMU with HVF, 4 vCPUs; rendering with Mesa's llvmpipe (software OpenGL ES), so the CPU it shows includes drawing that a GPU does on hardware with one. Windows of 5–10 s.

| Scenario | `odap-shell` CPU | switches/s | `odap-testbed` CPU | switches/s | RSS shell / testbed |
|---|---|---|---|---|---|
| At rest (the list) | 0.1 % | 2.2 | 0.0 % | 0 | 171 / 163 MiB |
| Virtual list, Volume held (10 s) | 29.2 % | 1483 | 27.8 % | 909 | 174 / 162 MiB |
| Virtual list, at rest after | 0.0 % | 0 | 0.0 % | 0 | 174 / 163 MiB |
| Media: the spring thrown once a second | 14.8 % | 757 | 11.7 % | 432 | 174 / 169 MiB |
| Navigation: the 10 000-album shelf, Volume held (8 s, 139 albums) | 15.2 % | 718 | 9.1 % | 263 | 174 / 169 MiB |

- **At rest nothing runs:** no frames, no timers, no wakeups in the activity, which is what T1-047 needs for long sessions.
- CPU is a share of one vCPU.

## What the baselines point at

Candidates for optimisation, to confirm with measurements on the T1 before anything changes:

- **Item at an offset** (264 ns, against 5.7 ns for the offset of an item): the lookup searches prefix sums instead of descending the Fenwick tree, O(log² n) for O(log n).
- **A step in a navigation level** grows with the level (3.5 µs at 1000 stops): `nav` builds the level on every key. Large lists are not affected, since a virtual list is walked by index (`opendap-layout::focus`), but a very long screen of controls would be.
- **Memory:** about 170 MiB resident per process on the virtual board, with llvmpipe, fonts and Slint's caches; to break down and measure on the T1's GPU path.
- **Frames while scrolling:** about 1500 context switches a second in the shell while the list scrolls with the button held; to compare with the T1's display rate and the render path.
