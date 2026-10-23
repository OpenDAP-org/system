# System

Blackbird OS system software in Rust: bus, orchestrator, UI runtime, SDK and base services.

## Layout

- `crates/`: the Rust workspace
  - `opendap-device`: domain model of the device description (display, controls and their roles)
  - `dap-shell`: the compositor on Smithay (bring-up, milestone M1)
  - `dap-product`: the product host in Slint (bring-up: Now Playing)
  - `dap-inject`: development tool, touch contacts through uinput for automated tests
- `scripts/`: `cross.sh` (cross-build against the Blackbird OS sysroot), `deploy.sh` (development system extension to a running instance)
- `schemas/`
- `docs/`

## Building

Development happens on the host; the virtual T1 only runs what is built ([development workflow](https://github.com/opendap-project/toolkit-host/blob/main/docs/specs/development-workflow.md)).

```sh
cargo test --workspace                 # domain and core crates, natively
cargo run -p dap-product               # the product UI in a window on the host
../blackbird-os/build/dap-sysroot.sh   # once, and when the image's packages change
scripts/deploy.sh                      # cross-build and run on the virtual T1
```

`dap-product` reads Blackbird UI's generated tokens and fonts from a checkout next to this one, or from `BLACKBIRD_UI`.

## License

This repository follows the [REUSE](https://reuse.software) layout: license texts live in `LICENSES/` and `REUSE.toml` maps each path to its license.

- Default: **GNU GPL v3.0 or later** (`GPL-3.0-or-later`)
- Markdown files: **Creative Commons Attribution-ShareAlike 4.0** (`CC-BY-SA-4.0`)
- `schemas/`: **Mozilla Public License 2.0** (`MPL-2.0`)
- `docs/`: **Creative Commons Attribution-ShareAlike 4.0** (`CC-BY-SA-4.0`)

SDK crates, meant for third-party capabilities, will be licensed **Mozilla Public License 2.0** (`MPL-2.0`) when they are created.

Copyright © 2026 The OpenDAP Project.
