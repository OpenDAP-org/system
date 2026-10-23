# System

Blackbird OS system software in Rust: bus, orchestrator, UI runtime, SDK and base services.

## Layout

- `crates/`: the Rust workspace
  - `opendap-device`: domain model of the device description (display, controls and their roles)
  - `odap-shell`: the compositor on Smithay (bring-up, milestone M1)
  - `odap-product`: the product host in Slint (bring-up: Now Playing)
  - `odap-boot`: the boot UI (splash screen, the hand-over to the shell)
  - `opendap-splash`: the splash screen's rasterization, shared by `odap-boot` and the shell
  - `opendap-workspace`: finding other repositories' checkouts from build scripts
- `schemas/`
- `docs/`

## Building

Development happens on the host; the virtual T1 only runs what is built ([development workflow](https://github.com/opendap-project/toolkit-host/blob/main/docs/specs/development-workflow.md)).

```sh
cargo test --workspace                 # domain and core crates, natively
cargo run -p odap-product              # the product UI in a window on the host
odap-sysroot install                   # once, and when the image's packages change (toolkit-image)
odap-deploy                            # cross-build and run on the running virtual device (toolkit-image)
```

`odap-product` and `opendap-splash` read Blackbird UI's published outputs (`export/`, `brand/svg/`), found through the workspace's `repos.tsv` or `ODAP_BLACKBIRD_UI_DIR`. `odap-deploy` takes its default crates and the units to restart from `[workspace.metadata.odap]` in `Cargo.toml`.

## License

This repository follows the [REUSE](https://reuse.software) layout: license texts live in `LICENSES/` and `REUSE.toml` maps each path to its license.

- Default: **GNU GPL v3.0 or later** (`GPL-3.0-or-later`)
- Markdown files: **Creative Commons Attribution-ShareAlike 4.0** (`CC-BY-SA-4.0`)
- `schemas/`: **Mozilla Public License 2.0** (`MPL-2.0`)
- `docs/`: **Creative Commons Attribution-ShareAlike 4.0** (`CC-BY-SA-4.0`)

SDK crates, meant for third-party capabilities, will be licensed **Mozilla Public License 2.0** (`MPL-2.0`) when they are created.

Copyright © 2026 The OpenDAP Project.
