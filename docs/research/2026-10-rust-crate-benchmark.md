# Rust crate benchmark for OpenDAP

- **Date:** 2026-10-22
- **Status:** Research report. Recommendations only; nothing here is decided. No ADR or code was changed.
- **Question:** which third-party Rust crates should OpenDAP depend on, area by area, for the device system (services, SDK, compositor, audio engine), the blackbird-os tools and the toolkit (the `opendap` CLI and OpenDAP Desktop)?
- **Inputs read:** the [Rust crate architecture spec](../specs/rust-crate-architecture.md); ADRs [0001](../adr/0001-rust-for-all-custom-software.md) to [0007](../adr/0007-language-data-licensing.md); the [system architecture research](2026-10-system-architecture.md), the [Slint versus Iced research](2026-10-ui-toolkit-slint-vs-iced.md) and the [text input and i18n research](2026-10-text-input-and-i18n.md); the blackbird-os research on [image composition](https://github.com/opendap-project/blackbird-os/blob/main/docs/research/2026-10-image-composition.md), [OS lifecycle](https://github.com/opendap-project/blackbird-os/blob/main/docs/research/2026-10-os-lifecycle.md) and [DSD512](https://github.com/opendap-project/blackbird-os/blob/main/docs/research/2026-10-dsd512.md); the toolkit [`opendap` CLI spec](https://github.com/opendap-project/cli/blob/main/docs/specs/opendap-cli.md) and [agentic CLI research](https://github.com/opendap-project/cli/blob/main/docs/research/2026-10-agentic-cli-design.md); the toolkit-cli `Cargo.toml` files (in use today: `clap` 4.6 with `derive` and `env`, `serde` 1, `serde_json` 1, `thiserror` 2).

## 1. Summary

1. **Most earlier choices hold.** `clap`, `thiserror`/`anyhow`, `tracing`, `serde`/`toml`/`schemars`, `zlink`, `zbus`, `slint`, `smithay`, `nusb`/`rockusb`, `tough`, `minisign-verify`, `ed25519-dalek`, `icu`, `fluent-bundle`, `symphonia`, `rubato`, `keyring`, `etcetera` and `rmcp` were all re-checked against crates.io, GitHub and RustSec on 2026-10-22. The versions quoted in the earlier research match the published ones.
2. **Open items from the earlier research can now be closed.** `tough` is 0.24.0 (2026-07-10). It resolves to 107 crates, including `tokio`, `rustls` and `aws-lc-rs`. `symphonia` is **MPL-2.0**, the same licence as OpenDAP's libraries.
3. **There is one async runtime.** `tokio` is already required by `zbus`, `zlink` (default feature), `russh`, `rmcp`, `tough` and `reqwest`. A second runtime would only add weight.
4. **There is one crypto provider.** `rustls` 0.23 defaults to `aws-lc-rs`, and so do `reqwest` 0.13, `tough` and `russh`. `aws-lc-rs` needs neither CMake nor bindgen for non-FIPS builds (its README). `ureq` would bring in `ring` as a second provider.
5. **SSH: use the `openssh` crate for the toolkit.** It drives the system OpenSSH client, so the user's `~/.ssh/config`, agent and `known_hosts` keep working, and it resolves to 25 crates (`russh`: 143). It is Unix-only (`compile_error!` on other targets), which matches the toolkit's supported hosts. Windows for OpenDAP Desktop remains an open choice (§6).
6. **Gaps with no maintained crate:**
   - **QMP:** `qapi` 0.15.0 is from 2024-09-09 and has one maintainer. Recommendation: a thin client of our own over `serde_json`.
   - **DSD:** no mature DSF/DFF reader and no DSD-to-PCM crate. This confirms the earlier plan to write our own.
   - **DSD tags:** `lofty` does not list DSF or DFF; `id3` covers their ID3v2 chunk.
   - **Biquad EQ:** `biquad` is small, with one maintainer. Writing our own is recommended (§4.24).
7. **Health warnings:**
   - `smithay` has not been released since 0.7.0 (2025-06-24), although its master branch is active (last push 2026-10-20).
   - `rmcp` released majors 1, 2 and 3 between March and July 2026.
   - `ciborium`, `rusb`, `xz2`, `claxon`, `soxr`, `sled`, `smol`, `figment`, `qapi` and `rust-tuf` have had no release in a year, or are abandoned.
   - `async-std`, `bincode`, `serde_yaml` and `serde_yml` are unmaintained or unsound according to RustSec.
8. **Minimum version floors, set by recent advisories:**
   - `rustls` ≥ 0.23.46: five advisories, 2026-09 to 2026-10.
   - `webpki-roots` ≥ 1.0.10: RUSTSEC-2026-0381.
   - `tokio` ≥ 1.54.1: RUSTSEC-2026-0384, published 2026-10-21.
   - `tar` ≥ 0.4.45, `anyhow` ≥ 1.0.103, `memmap2` ≥ 0.9.11, `russh` ≥ 0.60.3, `zip` ≥ 9.0.0.

## 2. Method

- **crates.io** (API `https://crates.io/api/v1/crates/<name>` and `/owners`, read once per second on 2026-10-22):
  - **Latest:** the highest stable version and the date it was published.
  - **Recent DL:** crates.io's `recent_downloads`, the last 90 days.
  - **Releases/yr:** the number of non-yanked versions published 2025-10-22 to 2026-10-22.
  - **MSRV:** the `rust-version` of the latest version, where one is declared.
  - **Backer:** the crates.io owners (people and GitHub teams) together with the repository owner.
- **GitHub** (`gh api repos/<owner>/<repo>`, 2026-10-22): stars, open issues plus PRs, and last push. These figures are quoted only where they inform a choice.
- **RustSec:** a clone of [rustsec/advisory-db](https://github.com/rustsec/advisory-db) at commit `640bf5a` (2026-10-22), read under `crates/<name>/`. Advisories from 2024 to 2026 are listed. Older ones are mentioned only when they matter.
- **Footprint:** the number of unique packages in the dependency tree. Measured with `cargo tree -e normal,build --target aarch64-unknown-linux-gnu` on a one-dependency project (cargo 1.98.1, resolved 2026-10-22), with default features unless noted.
  - It counts crates, not lines of code or binary size.
  - **Not measured:** compile time, binary size and the `unsafe` surface (no `cargo-geiger` run). These are **UNVERIFIED** throughout.
- **Maturity:** "1.x" means a 1.0 or later API under semver. "0.x" means minor releases may break. Where the history shows it, the pace of breaking releases is given.
- Nothing was built for or run on the RK3566.

## 3. Licence rules applied

- **Libraries (MPL-2.0)** may depend on MIT, Apache-2.0, BSD, ISC, Zlib, CC0, Unlicense, Unicode-3.0 and MPL-2.0 crates. They should not depend on LGPL or GPL crates. Such a dependency is legal, but it puts copyleft obligations on every downstream user, which defeats the point of MPL-2.0 for the SDK.
- **Binaries (GPL-3.0-or-later)** may additionally use LGPL-2.1-or-later, GPL-3.0 and MPL-2.0 code. Apache-2.0 is compatible with GPL-3.0 but not with GPL-2.0-only, and no GPL-2.0-only crate is recommended here.
- **Slint is `GPL-3.0-only`** (one of three options), so any binary that links it is effectively GPL-3.0-only, as the Slint research already noted.
- **Licences that need care:**
  - `minicbor` is BlueOak-1.0.0. It is permissive, but its GPL compatibility is **UNVERIFIED** (we did not find it on the FSF list).
  - The `soxr` binding crate is LGPL-2.1 itself, so binaries only.
  - The `systemd` crate is LGPL-2.1-or-later with a GCC exception, so binaries only.
  - `sd-journal` is AGPL-3.0-or-later, so avoid it.
  - `zeroconf` declares a non-standard licence, so avoid it.
  - `webpki-roots` data is CDLA-Permissive-2.0.
- `cargo deny` (already planned for the toolkit pre-commit hooks in the OS lifecycle research) can enforce these rules locally, with an allow-list per repository.

## 4. Areas

Every figure in this section is from 2026-10-22. "Recent DL" is the crates.io 90-day count.

### 4.1 CLI: argument parsing, completions, man pages

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `clap` | clap-rs org, rust-cli WG team | 4.6.7 (2026-09-14) | 262.4 M | MIT OR Apache-2.0 | 1.x (4.x), MSRV 1.85, 19 rel/yr | 17 crates with `derive`. Already in toolkit-cli |
| `clap_complete` | same | 4.6.11 (2026-09-15) | 33.1 M | MIT OR Apache-2.0 | 4.x | bash, zsh, fish, PowerShell, elvish |
| `clap_mangen` | same | 0.3.3 (2026-08-12) | 4.7 M | MIT OR Apache-2.0 | 0.x | roff man pages from the clap tree |
| `argh` | Google (Fuchsia) | 0.1.19 (2026-03-16) | 3.0 M | BSD-3-Clause | 0.x | Fuchsia conventions. No completions or man pages |
| `bpaf` | one maintainer | 0.9.28 (2026-09-17) | 2.1 M | MIT OR Apache-2.0 | 0.x | Smaller; one maintainer |
| `lexopt` | one maintainer | 0.3.2 (2026-02-28) | 3.0 M | MIT | 0.x | Bare lexer, no help or schema generation |

**Recommendation: `clap` (derive) with `clap_complete` and `clap_mangen`.** It is the de facto standard and has a WG team among its owners. The agentic CLI design walks the clap tree to produce schemas, MCP tools and conformance tests, and no other parser exposes a comparable command model.

### 4.2 CLI: colour, progress, prompts, terminal UI

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `anstream` / `anstyle` | rust-cli WG (clap's own) | 1.0.0 (2026-02-11) | 204.7 M | MIT OR Apache-2.0 | 1.x | Strips colour when the output is not a terminal, honours `NO_COLOR`/`CLICOLOR`. Already in clap's tree. RUSTSEC-2024-0404 fixed in 0.6.8 |
| `owo-colors` | two maintainers | 4.4.0 (2026-08-27) | 41.4 M | MIT | 4.x | Styling only, no stream handling |
| `termcolor` | BurntSushi | 1.4.1 (2024-01-10) | 87.5 M | Unlicense OR MIT | 1.x, 0 rel/yr | Stable, inactive |
| `indicatif` | console-rs (mitsuhiko, djc) | 0.18.6 (2026-07-01) | 52.9 M | MIT | 0.x, 6 rel/yr | 6 crates. Hides itself when stderr is not a terminal |
| `dialoguer` | console-rs | 0.12.0 (2025-08-23) | 18.4 M | MIT | 0.x, 0 rel/yr | Prompts and confirmations |
| `inquire` | one maintainer | 0.9.4 (2026-02-24) | 5.0 M | MIT | 0.x | Prompts |
| `ratatui` | ratatui org | 0.30.2 (2026-06-19) | 22.9 M | MIT | 0.x | 71 crates. Full TUI framework |
| `comfy-table` | one maintainer | 8.0.1 (2026-09-25) | 21.6 M | MIT | 8.x | Tables |

**Recommendation: `anstream`/`anstyle` for colour and `indicatif` for progress.** Both come from teams that already maintain crates we depend on, and both turn themselves off when the output is not a terminal, which the agent-friendly output modes need. Prompts, tables and a TUI are not needed yet. A `--yes`-driven confirmation can be written in a few lines, so leave `dialoguer` and `ratatui` out until a requirement asks for them.

### 4.3 Errors

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `thiserror` | dtolnay | 2.0.21 (2026-09-23) | 448.7 M | MIT OR Apache-2.0 | 2.x | Derive only; nothing at runtime. Already in toolkit-cli |
| `anyhow` | dtolnay | 1.0.104 (2026-07-18) | 253.1 M | MIT OR Apache-2.0 | 1.x | 1 crate. RUSTSEC-2026-0190 (unsound `downcast_mut`, fixed ≥ 1.0.103) |
| `miette` | one maintainer (zkat) | 7.6.0 (2025-04-27) | 22.1 M | Apache-2.0 | 7.x, 0 rel/yr | 53 crates with `fancy`; 115 open issues |
| `eyre` / `color-eyre` | eyre-rs org | 0.6.14 (2026-08-11) / 0.6.5 (2025-05-30) | 23.9 M / 14.8 M | MIT OR Apache-2.0 | 0.x | RUSTSEC-2024-0021 fixed in 0.6.12 |
| `snafu` | one maintainer | 0.9.2 (2026-07-21) | 23.0 M | MIT OR Apache-2.0 | 0.x | Used by `tough` |

**Recommendation: `thiserror` in libraries and `anyhow` in binaries**, as the [crate spec](../specs/rust-crate-architecture.md) §5 already requires. The CLI's error envelope is our own data type in `opendap-cli-kit`, so no report-rendering crate is needed.

### 4.4 Logging and structured logs (no OpenTelemetry)

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `tracing` | tokio-rs | 0.1.44 (2025-12-18) | 232.4 M | MIT | 0.1 line, stable in practice for years | Spans and structured fields |
| `tracing-subscriber` | tokio-rs | 0.3.23 (2026-03-13) | 168.4 M | MIT | 0.x | 23 crates with `env-filter` and `json`. RUSTSEC-2025-0055 (ANSI injection) fixed ≥ 0.3.20 |
| `tracing-journald` | tokio-rs | 0.3.2 (2025-11-26) | 2.2 M | MIT | 0.x | 9 crates. Writes native journal fields over the journald socket, without libsystemd |
| `systemd-journal-logger` | one maintainer (Codeberg) | 2.2.2 (2025-05-03) | 216 k | MIT OR Apache-2.0 | 2.x, 0 rel/yr | For the `log` facade |
| `log` / `env_logger` | rust-lang / rust-cli | 0.4.34 / 0.11.11 | 338.5 M / 89.3 M | MIT OR Apache-2.0 | 0.x | Simpler facade. `tracing` bridges it |
| `tracing-appender` | tokio-rs | 0.2.5 (2026-04-17) | 40.1 M | MIT | 0.x | File output for `--log-file` |

**Recommendation: `tracing` + `tracing-subscriber` (`fmt`, `env-filter`, `json`) everywhere, plus `tracing-journald` on the device.** One team maintains all three. Structured JSON logs come from the `fmt` JSON layer, and journald gets typed fields natively, so no OpenTelemetry crate is needed.

### 4.5 Serialisation and schemas

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `serde` | serde-rs (dtolnay) | 1.0.229 (2026-07-18) | 376.3 M | MIT OR Apache-2.0 | 1.x | Already in toolkit-cli |
| `serde_json` | serde-rs | 1.0.154 (2026-10-11) | 376.2 M | MIT OR Apache-2.0 | 1.x | 5 crates |
| `toml` / `toml_edit` | toml-rs (epage, rust-cli WG) | 1.1.8 / 0.25.17 (2026-10-09) | 271.0 M / 245.1 M | MIT OR Apache-2.0 | `toml` 1.x, TOML spec 1.1.0 | 7 crates. `toml_edit` keeps comments when `opendap config set` writes a file |
| `schemars` | one maintainer | 1.2.3 (2026-10-12) | 212.0 M | MIT | 1.x | JSON Schema 2020-12 from Rust types. 121 open issues |
| `jsonschema` | one maintainer | 0.60.0 (2026-10-22) | 32.2 M | MIT | 0.x, 69 rel/yr | Validation of manifests at install time, if needed |
| `minicbor` | one maintainer | 2.3.1 (2026-10-18) | 6.2 M | BlueOak-1.0.0 | 2.x | 6 crates. Licence: see §3 |
| `ciborium` | enarx org | 0.2.2 (2024-01-24) | 71.8 M | Apache-2.0 | 0.x, 0 rel/yr | Stale (the earlier research agrees) |
| `postcard` | one maintainer | 1.1.3 (2025-07-24) | 32.4 M | MIT OR Apache-2.0 | 1.x | The ICU4X blob format |
| `zerocopy` | Google | 0.8.63 (2026-10-13) | 291.3 M | BSD-2-Clause OR Apache-2.0 OR MIT | 0.x, 42 rel/yr | Checks at compile time that `#[repr(C)]` frames are safe to reinterpret |
| `bytemuck` | rust-bus | 1.25.2 (2026-07-19) | 119.8 M | Zlib OR Apache-2.0 OR MIT | 1.x | Simpler alternative to `zerocopy` |
| `bincode` | — | 3.0.0 (2025-12-16) | 71.9 M | MIT | — | **RUSTSEC-2025-0141: unmaintained** |
| `serde_yaml` / `serde_yml` | — | 0.9.34+deprecated / 0.0.13 | — | — | — | Deprecated; `serde_yml` is unsound (RUSTSEC-2025-0068) |

**Recommendation:**

- **`serde`, `serde_json`, `toml` (with `toml_edit` for config writes) and `schemars`** for the control plane, manifests (ADR 0002: TOML) and schemas.
- **`zerocopy`** for the fixed-layout data-plane frames (`opendap-stream`). Google maintains it and it checks layouts at compile time.
- **No CBOR crate for now**, because ADR 0002 keeps the control plane in JSON. If the Varlink upgrade path is ever taken, compare `minicbor` (after the licence check) with the then-current `ciborium`.
- **Avoid YAML and `bincode`.**

### 4.6 Async runtime and executors

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `tokio` | tokio-rs | 1.54.1 (2026-10-21) | 268.8 M | MIT | 1.x, MSRV 1.85, LTS lines | 11 crates with `rt,net,io-util,macros,time,sync`; 19 with `full`. RUSTSEC-2026-0384 (`Interval` race in 1.54.0, fixed 1.54.1), RUSTSEC-2025-0023 (fixed) |
| `smol` | smol-rs org | 2.0.2 (2024-09-07) | 7.0 M | Apache-2.0 OR MIT | 2.x, 0 rel/yr | 33 crates. `zlink` and `zbus` support it as an option |
| `async-std` | — | 1.13.2 (2025-08-15) | 11.7 M | — | — | **RUSTSEC-2025-0052: discontinued** |
| `futures` | rust-lang | 0.3.34 (2026-08-11) | 197.6 M | MIT OR Apache-2.0 | 0.3 | Runtime-neutral traits for core APIs |
| `calloop` | Smithay org | 0.14.5 (2026-10-01) | 20.5 M | MIT | 0.x | Smithay's event loop, used by the compositor |
| `monoio` / `glommio` | ByteDance / Datadog | 0.2.4 (2024-08) / 0.9.0 (2024-03) | 442 k / 24 k | MIT OR Apache-2.0 | 0.x, stale | io_uring thread-per-core; not relevant |

**Recommendation: `tokio`, with only the features each binary needs.** `zbus`, `zlink`, `russh`, `rmcp`, `tough` and `reqwest` already depend on it, so a second runtime would only add weight. The compositor runs on `calloop`, as Smithay requires. Per the crate spec §4, core crates expose `futures`/`std` traits and do not name `tokio` in their public API.

### 4.7 Process, signals and system calls

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `rustix` | Bytecode Alliance | 1.1.5 (2026-09-16) | 309.9 M | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | 1.x | 3 crates. Safe wrappers: `memfd_create` and seals, eventfd, `SCM_RIGHTS`, pidfd |
| `nix` | nix-rust org | 0.31.3 (2026-05-11) | 212.1 M | MIT | 0.x | 5 crates with `signal,process`; broader API |
| `libc` | rust-lang | 0.2.191 (2026-10-16) | 428.3 M | MIT OR Apache-2.0 | 0.2 | Raw FFI |
| `signal-hook` | rust-bus | 0.4.5 (2026-10-04) | 64.7 M | MIT OR Apache-2.0 | 0.x | 4 crates. Signals in synchronous programs |
| `ctrlc` | one maintainer | 3.5.2 (2026-02-10) | 24.1 M | MIT OR Apache-2.0 | 3.x | Ctrl-C only |
| `memfd` | lucab, nagisa | 0.6.6 (2026-09-01) | 14.9 M | MIT OR Apache-2.0 | 0.x | Thin; `rustix` covers the same ground |
| `memmap2` | four maintainers | 0.9.12 (2026-10-21) | 95.6 M | MIT OR Apache-2.0 | 0.x | RUSTSEC-2026-0186 fixed ≥ 0.9.11. For mmapped ICU4X and FST data |

**Recommendation:**

- **`std::process` and `tokio::process`** for child processes.
- **`rustix`** for Linux system calls (memfd rings, eventfd, fd passing). It is 1.x, maintained by the Bytecode Alliance, and small.
- **`tokio::signal`** in async binaries and **`signal-hook`** in synchronous ones.
- **`memmap2`** for memory-mapped data files.
- `nix` only where `rustix` lacks an API.

### 4.8 Configuration and directories

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `etcetera` | two maintainers | 0.11.0 (2025-10-28) | 41.6 M | MIT OR Apache-2.0 | 0.x | 2 crates. An XDG strategy that also applies on macOS |
| `directories` | one maintainer (soc) | 6.0.0 (2025-01-12) | 14.6 M | MIT OR Apache-2.0 | 6.x, 0 rel/yr | Apple paths on macOS. RUSTSEC-2020-0054 (old) |
| `dirs` | one maintainer (soc, Codeberg) | 7.0.0 (2026-09-05) | 95.3 M | MIT OR Apache-2.0 | 7.x | Same platform conventions as `directories` |
| `xdg` | one maintainer | 3.0.0 (2025-05-04) | 4.2 M | Apache-2.0 OR MIT | 3.x | Unix only |
| `config` | rust-cli WG | 0.15.27 (2026-09-30) | 20.9 M | MIT OR Apache-2.0 | 0.x | Layered configuration framework |
| `figment` | one maintainer | 0.10.19 (2024-05-17) | 12.8 M | MIT OR Apache-2.0 | 0.x, 0 rel/yr | Stale |

**Recommendation: `etcetera` with the XDG strategy**, as the agentic CLI research (§8.7) proposed. CLI users expect XDG paths on macOS too, and `directories`/`dirs` default to `~/Library`. The five-level precedence (flags, environment, project, user, system) is a small merge over `toml` + `serde` in `opendap-cli-kit`, so no framework is needed. On the device, paths come from systemd (`StateDirectory=`, `$STATE_DIRECTORY`), so no crate is needed there.

### 4.9 HTTP client

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `reqwest` | seanmonstar (hyperium) | 0.13.5 (2026-09-08) | 226.7 M | MIT OR Apache-2.0 | 0.x, 11 rel/yr | Async (tokio). 101 crates by default; 87 with `default-features = false, features = ["rustls"]`. Its default TLS is `rustls` + `aws-lc-rs` |
| `ureq` | two maintainers | 3.5.0 (2026-10-21) | 73.3 M | MIT OR Apache-2.0 | 3.x | Synchronous. 30 crates. Default TLS is `rustls` + **`ring`** |
| `hyper` | hyperium | 1.12.0 (2026-10-06) | 245.0 M | MIT | 1.x | Low-level; `reqwest` builds on it |
| `isahc` | one maintainer | 2.0.2 (2026-10-21) | 1.2 M | MIT | 2.x | libcurl |
| `attohttpc` | one maintainer | 0.31.0 (2026-05-25) | 7.1 M | MPL-2.0 | 0.x | Small, synchronous |

**Recommendation: `reqwest` with `rustls`.** It is the most adopted client and the hyperium stack. It shares `tokio` and `aws-lc-rs` with the rest of the set, and `tough`'s `http` feature is built on it (`reqwest` 0.13 plus `rustls-platform-verifier`, per tough's `Cargo.toml`). `ureq` is lighter (30 crates against 87), but on a host that already links `tokio`/`rustls`/`aws-lc-rs` it would bring a second crypto provider (`ring`). Only a strictly synchronous, size-sensitive tool justifies it (open choice O4).

### 4.10 TLS, signatures and hashes

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `rustls` | rustls org | 0.23.46 (2026-10-14) | 244.3 M | Apache-2.0 OR ISC OR MIT | 0.x, 16 rel/yr | 19 crates by default (with `aws-lc-rs`). RUSTSEC-2026-0285 (≥ 0.23.45), -0346 to -0349 (≥ 0.23.46) |
| `rustls-platform-verifier` | rustls org | 0.7.1 (2026-09-24) | 92.8 M | MIT OR Apache-2.0 | 0.x | Uses the OS trust store (macOS Security framework, Debian `ca-certificates`) |
| `webpki-roots` | rustls org | 1.0.10 (2026-10-19) | 222.5 M | CDLA-Permissive-2.0 | 1.x | Bundled Mozilla roots. RUSTSEC-2026-0381 (≥ 1.0.10) |
| `aws-lc-rs` | AWS | 1.18.2 (2026-10-12) | 103.7 M | ISC AND (Apache-2.0 OR ISC) | 1.x, 16 rel/yr | 13 crates. "CMake is never required", "Bindgen is never required" for non-FIPS builds (README). FIPS option |
| `ring` | one maintainer, plus rustls team access | 0.17.14 (2025-03-11) | 191.3 M | Apache-2.0 AND ISC | 0.17, 0 rel/yr | 8 crates. RUSTSEC-2025-0007 ("unmaintained") **withdrawn** 2025-02-22 |
| `openssl` | rust-openssl | 0.10.81 (2026-06-12) | 78.7 M | Apache-2.0 | 0.10 | 10 advisories since 2016. Complicates a static musl build |
| `ed25519-dalek` | dalek-cryptography org | 3.0.0 (2026-07-06) | 71.6 M | BSD-3-Clause | 3.x | 17 crates. `no_std`. RUSTSEC-2022-0093 (fixed) |
| `ed25519-compact` | jedisct1 | 2.6.0 (2026-09-25) | 3.4 M | MIT | 2.x | Smaller, one maintainer |
| `minisign-verify` | jedisct1 (minisign's author) | 0.3.0 (2026-09-25) | 7.8 M | MIT | 0.x | **1 crate** (no dependencies). Verification only |
| `minisign` | jedisct1 | 0.10.0 (2026-09-25) | 977 k | MIT | 0.x | Signing in Rust (not needed: release signing uses the `minisign` tool) |
| `sha2` | RustCrypto org | 0.11.1 (2026-10-12) | 320.1 M | MIT OR Apache-2.0 | 0.x | RUSTSEC-2021-0100 (old, fixed) |
| `blake3` | BLAKE3 team | 1.8.7 (2026-08-20) | 53.9 M | CC0-1.0 OR Apache-2.0 … | 1.x | Not needed: the formats chosen use SHA-256 |
| `zeroize` / `subtle` | RustCrypto / dalek | 1.9.1 / 2.6.1 | 217.0 M / 196.7 M | Apache-2.0 OR MIT / BSD-3-Clause | 1.x / 2.x | Pulled in transitively |

**Recommendation:**

- **`rustls` with the `aws-lc-rs` provider** and **`rustls-platform-verifier`**. One provider, which the HTTP, TUF and SSH crates already default to. AWS backs it, and it needs no CMake. The platform verifier is the standard trust store on each host and Debian's `ca-certificates` on the device.
- **`ed25519-dalek`** for capability and HAT manifest signatures. It is `no_std`, so it also suits firmware.
- **`minisign-verify`** for release artefacts, from minisign's own author.
- **`sha2`** for hashes.
- Version floors: `rustls` ≥ 0.23.46 and `webpki-roots` ≥ 1.0.10.

### 4.11 TUF client

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `tough` | AWS (awslabs, Bottlerocket) | 0.24.0 (2026-07-10) | 736 k | MIT OR Apache-2.0 | 0.x, 3 rel/yr, a breaking minor about every two months in 2026 | 107 crates (`tokio`, `rustls`, `aws-lc-rs`, `jiff`, `snafu`). 66 open issues. RUSTSEC-2020-0024 (old) |
| `tuf` (rust-tuf) | theupdateframework org (the TUF project) | 0.2.0 stable (2017); 0.3.0-beta14 (2025-10-20) | 1 k | MIT OR Apache-2.0 | beta for years | Effectively inactive |
| `sigstore` | Sigstore project | 0.14.0 (2026-05-22) | 381 k | Apache-2.0 | 0.x | Needs online verification; ruled out for the device by the image composition research |

**Recommendation: `tough`.** It is the only maintained Rust TUF client and AWS backs it. This closes the "version UNVERIFIED" note in the system architecture research §5.14. Most of its 107 crates (`tokio`, `rustls`, `aws-lc-rs`, `reqwest`) are already in the device set, so the extra footprint is small. The alternative raised there ("a deliberately smaller subset") is only worth reconsidering if the device's measured binary size demands it.

### 4.12 Compression

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `zstd` | one maintainer (gyscos); binds Meta's reference `libzstd` | 0.14.3 (2026-10-16) | 100.4 M | BSD-3-Clause | 0.x | 9 crates. Compression and decompression |
| `ruzstd` | one maintainer | 0.9.1 (2026-10-11) | 18.8 M | MIT | 0.x | Pure Rust. RUSTSEC-2024-0400 (out-of-bounds reads), RUSTSEC-2026-0344 (decompression bomb, 2026-10-11) |
| `liblzma` | Portable-Network-Archive org; binds the reference `liblzma` | 0.4.9 (2026-10-19) | 7.6 M | MIT OR Apache-2.0 | 0.x | Maintained successor of `xz2`. Default feature `bindgen` (needs libclang); `static` feature for musl |
| `xz2` | alexcrichton | 0.1.7 (2022-06-06) | 13.5 M | MIT OR Apache-2.0 | 0 rel/yr | Inactive |
| `lzma-rs` | one maintainer | 0.3.0 (2023-01-04) | 9.8 M | MIT | 0 rel/yr | Pure Rust; inactive |
| `flate2` | rust-lang org | 1.1.10 (2026-08-28) | 193.5 M | MIT OR Apache-2.0 | 1.x | gzip (`rockusb` uses it for `.img.gz`) |
| `lz4_flex` | one maintainer | 0.14.0 (2026-07-14) | 42.1 M | MIT | 0.x | RUSTSEC-2026-0041 (fixed). No current need |

**Recommendation:**

- **`zstd`** binds the reference implementation, whose format the image (`.img.zst`), erofs and RAUC already use.
- **`liblzma`** for xz, if a format needs it.
- **`flate2`** for gzip.
- `ruzstd` stays out: it had a decompression-bomb advisory eleven days ago.

### 4.13 Archives and image formats

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `tar` | composefs org (cgwalters) | 0.4.46 (2026-05-18) | 66.4 M | MIT OR Apache-2.0 | 0.4 | 8 crates. RUSTSEC-2026-0067/0068 fixed ≥ 0.4.45. 160 open issues |
| `zip` | zip-rs org | 9.0.5 (2026-10-17) | 89.0 M | MIT | 9.x, 24 rel/yr | Four extraction advisories, 2025-03 to 2026-10 (fixed ≥ 9.0.0) |
| `bmap-parser` | Collabora | 0.2.3 (2026-08-19) | 2 k | MIT AND Apache-2.0 | 0.x | bmap sparse images; used with `rockusb` |
| `cpio` | one maintainer | 0.4.1 (2024-11-08) | 267 k | MIT | 0 rel/yr | Not needed |

**Recommendation: `tar` (≥ 0.4.46) for release and SDK archives, and `bmap-parser` for flashing.** Do not add `zip` unless a format requires it, given its run of path-traversal and permission advisories.

### 4.14 SSH client

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `openssh` | openssh-rust org (jonhoo, NobodyXu) | 0.11.6 (2025-12-03) | 467 k | MIT OR Apache-2.0 | 0.x, 1 rel/yr | **25 crates.** Drives the system `ssh` binary through ControlMaster, so it honours `~/.ssh/config`, the agent and `known_hosts`. "**only works on unix**" (`compile_error!` otherwise) |
| `openssh-sftp-client` | openssh-rust org | 0.15.10 (2026-10-01) | 273 k | MIT | 0.x | SFTP over an `openssh` session |
| `russh` | one maintainer (Eugeny; repo Eugeny/russh) | 0.64.1 (2026-10-05) | 3.8 M | Apache-2.0 | 0.x, 28 rel/yr, a breaking minor about every six weeks | **143 crates**. Pure Rust client and server, on `aws-lc-rs` + `tokio`. RUSTSEC-2026-0154 (fixed ≥ 0.60.3) |
| `ssh2` | rust-lang org | 0.9.6 (2026-06-30) | 1.3 M | MIT OR Apache-2.0 | 0.x, 1 rel/yr | 17 crates. Binds libssh2 (C), with OpenSSL on Unix (**UNVERIFIED** for the default feature set); harder for static musl |
| `ssh-key` | RustCrypto | 0.6.7; 0.7.0-rc.11 (2026-06-29) | 7.0 M | Apache-2.0 OR MIT | 0.x | Key parsing and generation (pairing keys) |

**Recommendation: `openssh` for the toolkit.** It runs the user's own OpenSSH, which is the most standard and trick-less option and the one users already configure and trust. It is also the smallest tree. Its Unix-only limit matches the toolkit's supported hosts (CLAUDE.md: UNIX/POSIX only). Use `ssh-key` to generate the ed25519 pairing key (agentic CLI research §8.8). `russh` is the candidate if OpenDAP Desktop must run on Windows or needs an in-process SSH server. Its churn and single maintainer argue against making it the default (open choice O1).

### 4.15 USB and Rockchip MaskROM

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `nusb` | one maintainer (kevinmehall) | 0.2.8 (2026-10-14) | 788 k | Apache-2.0 OR MIT | 0.x (0.2 since 2025-05) | 8 crates. Pure Rust on usbfs, IOKit and WinUSB; no libusb. 19 open issues |
| `rusb` | one maintainer | 0.9.4 (2024-04-27) | 2.0 M | MIT | 0 rel/yr | libusb binding. RUSTSEC-2020-0098 (old) |
| `rockusb` | Collabora (rockchiprs) | 0.4.0 (2026-09-09) | 1 k | MIT OR Apache-2.0 | 0.x | 53 crates with `nusb`. MaskROM `download-sram`/`download-ddr`, `write-bmap` |

**Recommendation: `nusb` and `rockusb`**, unchanged from the OS lifecycle research. `rockusb` comes from Collabora, the maintainers of Rockchip mainline support, and builds on `nusb`. Together they need no libusb and no sudo on macOS. `nusb`'s single maintainer is the main risk. Mitigate it by keeping USB behind the transport port (crate spec §2).

### 4.16 QEMU QMP

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `qapi` / `qapi-qmp` | one maintainer (arcnmx) | 0.15.0 (2024-09-09) | 665 k / 669 k | MIT | 0.x, 0 rel/yr | 18 crates. Types generated from QEMU's QAPI schema. Last push 2025-08-25 |
| `qmp` | one maintainer | 0.1.1 (2026-01-07) | under 1 k | MIT | 0.x | Too new to judge |

**Recommendation: write a thin QMP client of our own** over `tokio::net::UnixStream` and `serde_json`, covering only the commands the virtual T1 uses (capabilities negotiation, `input-send-event`, `screendump`, `system_powerdown`, `query-status`). QMP is line-delimited JSON specified by QEMU itself. No maintained crate exists, and `qapi`'s generated types trail QEMU releases. `qapi-qmp` can serve as a reference for message shapes.

### 4.17 Serial (CDC-ACM console)

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `serialport` | serialport org + rust-bus | 4.10.1 (2026-09-08) | 3.6 M | **MPL-2.0** | 4.x | 17 crates. Port enumeration with USB VID/PID/serial (libudev on Linux by default, IOKit on macOS) |
| `serial2` | one maintainer | 0.2.38 (2026-07-31) | 12.8 M | BSD-2-Clause OR Apache-2.0 | 0.x | 3 crates. No USB metadata in enumeration (**UNVERIFIED**: checked from its API summary only) |
| `tokio-serial` / `mio-serial` | one maintainer | 5.5.0 / 5.0.7 (2026-06-15) | 1.8 M | MIT | 5.x | Async wrappers over `serialport` |

**Recommendation: `serialport`.** Two maintainer groups own it, its MPL-2.0 licence fits our libraries, and it enumerates ports by USB VID/PID, which `doctor` and device discovery need. On Linux, consider `default-features = false` to avoid linking libudev in the static musl CLI, with `nusb` providing the USB identity instead (**UNVERIFIED**: test enumeration without libudev).

### 4.18 Varlink

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `zlink` | z-galaxy (zeenix, the zbus author) | 0.7.1 (2026-09-09) | 130 k | MIT | 0.x, a breaking minor every 2–3 months in 2026 | 38 crates (with `tokio`); `smol` feature too. `#[service]`/`#[proxy]`, IDL, introspection, fd passing. 19 open issues |
| `varlink` | varlink org (the protocol's own reference implementation) | 13.0.0 (2025-10-20) | 5 k | MIT OR Apache-2.0 | 13.x, 0 rel/yr | Synchronous, code generator from `.varlink` files. Last push 2026-08-09 |

**Recommendation: `zlink`**, as ADR 0002 already decided. Note that the official `varlink` crate exists and is the protocol's reference implementation. It is far less active (no release in a year, 5 k recent downloads against 130 k) and generates code from IDL rather than from Rust types, which runs against ADR 0005's Rust-first model. `zlink` is pre-1.0 and breaks often, so pin it and wrap it in `opendap-bus`.

### 4.19 D-Bus

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `zbus` | z-galaxy (zeenix) | 6.0.0 (2026-10-16) | 33.6 M | MIT | 6.x | 32 crates. Pure Rust; used by `accesskit_unix`, `keyring` and many desktop crates |
| `dbus` | one maintainer (diwic) | 0.9.12 (2026-07-03) | 19.4 M | Apache-2.0 OR MIT | 0.x | 4 crates. Binds libdbus (C) |

**Recommendation: `zbus`**, unchanged, for logind, BlueZ, iwd, RAUC and WirePlumber. It shares its maintainer and style with `zlink`.

### 4.20 systemd integration

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `sd-notify` | one maintainer (lnicola) | 0.5.0 (2026-03-09) | 3.6 M | MIT OR Apache-2.0 | 0.x | 2 crates. `notify`, `notify_with_fds` (fd store), `listen_fds`, `listen_fds_with_names`, `watchdog_enabled`, `booted` (read from `src/lib.rs`) |
| `tracing-journald` | tokio-rs | 0.3.2 (2025-11-26) | 2.2 M | MIT | 0.x | See §4.4 |
| `libsystemd` | one maintainer (lucab) | 0.7.2 (2025-04-30) | 1.2 M | MIT OR Apache-2.0 | 0.x, 0 rel/yr | 32 crates. Pure Rust: notify, activation, journal, unit-name escaping |
| `systemd` | one maintainer | 0.10.1 (2025-07-19) | 536 k | LGPL-2.1-or-later WITH GCC-exception-2.0 | 0.x | Binds libsystemd; LGPL (§3) |
| `listenfd` | mitsuhiko | 1.0.2 (2025-01-19) | 1.4 M | Apache-2.0 | 1.x | Socket activation only |

**Recommendation: `sd-notify`** (readiness, watchdog, fd store, socket activation) and **`tracing-journald`**. Both are small, pure Rust and speak the documented systemd protocols directly. Transient units, slices and the freezer are driven over D-Bus (`org.freedesktop.systemd1` through `zbus`) or over systemd's Varlink interfaces where trixie's systemd 257 offers them. No extra crate is needed.

### 4.21 Wayland compositor

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `smithay` | Smithay org | 0.7.0 (2025-06-24) | 163 k | MIT | 0.x, **0 rel/yr** | 128 crates. Master is active (last push 2026-10-20; 287 open issues); cosmic-comp and niri track git |
| `wayland-server` / `wayland-protocols` | Smithay org | 0.31.14 / 0.32.13 (2026-06/07) | 1.5 M / 23.4 M | MIT | 0.x | Protocol bindings |
| `calloop`, `drm`, `gbm`, `input`, `udev`, `xkbcommon` | Smithay org / Smithay maintainers | 0.14.5, 0.15.0, 0.18.0, 0.10.0, 0.9.3, 0.9.0 | — | MIT | 0.x | Pulled in by `smithay`. `gbm` 0.18.0 is from 2024-12 |

**Recommendation: `smithay`**, unchanged. It is the only Rust compositor library, and Linux compositor projects use it in production. Its release gap means we must choose between pinning to a git revision of master (as cosmic-comp does) and staying on 0.7.0 (the text input research notes that master has input-method hooks 0.7.0 lacks) (open choice O2).

### 4.22 UI toolkit, text and accessibility

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `slint` / `slint-interpreter` | SixtyFPS GmbH (ogoffart, tronical) | 1.18.1 (2026-09-21) | 765 k / 17 k | GPL-3.0-only OR royalty-free OR commercial | 1.x, MSRV 1.92, 10 rel/yr | 345 crates with default features (the Slint research measured 306 with Wayland + FemtoVG only). 748 open issues |
| `accesskit` | AccessKit org | 0.25.1 (2026-09-25) | 8.6 M | MIT OR Apache-2.0 | 0.x | Accessibility tree; AT-SPI on Linux via `zbus` |
| `parley` / `skrifa` | Linebender / Google Fonts | 0.12.0 (2026-10-09) / 0.49.0 (2026-10-21) | 2.6 M / 21.4 M | Apache-2.0 OR MIT | 0.x | Text layout and font reading, already in Slint's tree (text input research) |
| `iced` | iced-rs | 0.14.0 (2025-12-07) | 692 k | MIT | 0.x | Not chosen (ADR 0006) |

**Recommendation: `slint`**, as ADR 0006 decided. Nothing in this benchmark changes the decision. It is the only 1.x toolkit in the comparison, and a company maintains it.

### 4.23 Image decoding (artwork)

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `image` | image-rs org | 0.25.10 (2026-03-10) | 65.8 M | MIT OR Apache-2.0 | 0.x | 93 crates by default (`rayon`, all formats); **21 crates** with `default-features = false, features = ["png","jpeg","webp"]`. Advisories from 2019/2020 only |
| `png` | image-rs org | 0.18.1 (2026-02-14) | 94.3 M | MIT OR Apache-2.0 | 0.x | 10 crates |
| `zune-jpeg` | one maintainer | 0.5.15 (2026-03-26) | 55.5 M | MIT OR Apache-2.0 OR Zlib | 0.x | `image`'s JPEG decoder since 0.25. 2 crates |
| `image-webp` | image-rs org | 0.2.4 (2025-08-27) | 36.9 M | MIT OR Apache-2.0 | 0.x | WebP decoder |
| `jpeg-decoder` | image-rs org | 0.3.2 (2025-06-21) | 12.6 M | MIT OR Apache-2.0 | 0 rel/yr | Superseded by `zune-jpeg` in `image` |
| `fast_image_resize` | one maintainer | 6.1.0 (2026-07-21) | 5.8 M | MIT OR Apache-2.0 | 6.x | SIMD resizing (NEON) for thumbnails |

**Recommendation: `image` with only `png`, `jpeg` and `webp` enabled.** It is one maintained front end (image-rs org) over `png`, `zune-jpeg` and `image-webp`, Slint already decodes images with it, and the trimmed feature set is 21 crates. Add `fast_image_resize` only if measured thumbnail generation needs it.

### 4.24 Audio I/O, decoding, resampling, DSP

**Output.**

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `alsa` | diwic, co-owned by the RustAudio cpal team | 0.12.1 (2026-07-31) | 8.6 M | Apache-2.0 OR MIT | 0.x | 9 crates. Binds alsa-lib: `hw:` PCM, `DSD_U*` formats, mixer controls |
| `pipewire` / `libspa` | PipeWire project (freedesktop GitLab) | 0.10.1 (2026-08-19) | 1.7 M | MIT | 0.x | 48 crates. The official Rust bindings |
| `cpal` | RustAudio org | 0.18.2 (2026-08-16) | 8.9 M | Apache-2.0 | 0.x | 8 crates (Linux). Portable output; adds no exclusive DSD control |
| `audio_thread_priority` | Mozilla | 0.38.0 (2026-09-17) | 330 k | MPL-2.0 | 0.x | Real-time thread priority via rtkit/`SCHED_FIFO`, as Firefox does |
| `rtrb` | one maintainer | 0.4.0 (2026-08-17) | 4.7 M | MIT OR Apache-2.0 | 0.x | Wait-free SPSC ring. RUSTSEC-2026-0274 (fixed 0.3.5 / 0.4.0) |

**Decoding.**

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `symphonia` | one main maintainer (pdeljanov) | 0.6.1 (2026-08-13) | 9.4 M | **MPL-2.0** | 0.x (0.5 → 0.6 took four years) | 31 crates with `all`. README status: FLAC "Excellent", ALAC "Great", WAV "Excellent", AIFF "Great", Vorbis, MP3, AAC-LC. Opus and WavPack listed without status. **No DSD** |
| `claxon` | one maintainer | 0.4.3 (2020-08-09) | 1.8 M | Apache-2.0 | inactive | FLAC only. RUSTSEC-2018-0004 (old) |
| `libflac-sys` | one maintainer | 0.3.4 (2025-10-04) | 20 k | BSD-3-Clause | 0.x | Binds the reference libFLAC |
| `dsd-reader` | one maintainer | 0.4.0 (2026-08-22) | under 1 k | MIT OR Apache-2.0 | 0.x | DSF/DFF reading; too new to judge |

**Resampling and DSP.**

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `rubato` | one maintainer (HEnquist, CamillaDSP author) | 5.0.1 (2026-10-01) | 5.5 M | MIT OR Apache-2.0 | 5.x | 20 crates. Sinc and polynomial; synchronous and asynchronous ratios |
| `soxr` | one maintainer | 0.6.0 (2024-01-25) | 45 k | **LGPL-2.1** | 0 rel/yr | Binds libsoxr (MPD's resampler) |
| `samplerate` | one maintainer | 0.2.4 (2021-01-14) | 465 k | BSD-2-Clause | inactive | libsamplerate |
| `biquad` | one maintainer | 0.6.0 (2026-03-22) | 144 k | MIT OR Apache-2.0 | 0.x | 4 crates. RBJ cookbook filters, `no_std` |
| `rustfft` / `realfft` | one maintainer each | 6.4.1 (2025-09-18) / 3.5.0 (2025-06-12) | 10.2 M / 5.1 M | MIT OR Apache-2.0 / MIT | 6.x / 3.x | NEON support for spectrograms (**UNVERIFIED** on the A55) |
| `dasp` | RustAudio | 0.11.0 (2020-05-29) | 1.1 M | MIT OR Apache-2.0 | inactive | — |

**Recommendation:**

- **`alsa`** for the exclusive bit-perfect engine and **`pipewire`** for shared audio. The official PipeWire bindings match ADR 0002's split.
- **`symphonia`** for PCM codecs. Its MPL-2.0 licence matches our libraries.
- **Our own DSF/DFF reader and DSD-to-PCM converter** (no mature crate exists).
- **`rubato`** for any non-bit-perfect conversion. Keep libsoxr only as the reference oracle for the verification vectors (DSD512 research §5.3), outside shipped libraries because of its LGPL binding.
- **`audio_thread_priority`** (Mozilla) for the real-time thread.
- **`realfft`** for spectrograms.
- **For EQ, write our own biquads** from the RBJ Audio EQ Cookbook in `f64`. Every stage of the signal path needs to be auditable for the `SignalPath` report, and the code is short. Use `biquad` as a test reference (open choice O7).
- `cpal` is for host-side previews only.

### 4.25 Metadata and tags

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `lofty` | one maintainer | 0.25.4 (2026-09-20) | 529 k | MIT OR Apache-2.0 | 0.x, MSRV 1.89 | 17 crates. Reads and writes ID3v1/v2, Vorbis comments, APE, MP4 ilst, RIFF INFO, AIFF text. Formats: AAC, APE, AIFF, FLAC, MP3, MP4, MPC, Opus, Vorbis, Speex, WAV, WavPack. **No DSF/DFF** in its list |
| `id3` | one maintainer (Codeberg) | 1.17.2 (2026-09-22) | 1.7 M | MIT | 1.x | 9 crates. ID3v2 on its own: DSF stores an ID3v2 chunk at the file end |
| `metaflac` | one maintainer | 0.2.8 (2025-01-25) | 38 k | MIT | 0 rel/yr | FLAC only |
| `mp4ameta` | one maintainer | 0.13.0 (2025-05-29) | 90 k | MIT OR Apache-2.0 | 0 rel/yr | MP4 only |
| `audiotags` | — | 0.5.0 (2024-02-01) | 9 k | MIT | inactive | Wrapper |

**Recommendation: `lofty` for every format it lists, plus `id3` to read the ID3v2 chunk of DSF (and DFF's unofficial ID3 chunk)** behind our own DSD reader. Both have a single maintainer, so keep tagging behind the library's port.

### 4.26 Library database and full-text search

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `rusqlite` | rusqlite org | 0.40.2 (2026-08-08) | 47.4 M | MIT | 0.x, about one breaking minor per quarter | 14 crates with `bundled`. SQLite is public domain and in Debian. RUSTSEC-2020-0014/2021-0128 (old) |
| `sqlx` | LaunchBadge | 0.9.0 (2026-05-21) | 48.8 M | MIT OR Apache-2.0 | 0.x, MSRV 1.94 | Async, compile-time-checked SQL; aimed at servers |
| `redb` | one maintainer | 4.4.0 (2026-10-13) | 6.2 M | MIT OR Apache-2.0 | 4.x (stable file format since 1.0) | 2 crates. Pure-Rust KV store, no SQL or FTS |
| `sled` | one maintainer | 0.34.7 (2021-09-12); 1.0 alphas | 3.8 M | MIT OR Apache-2.0 | inactive | — |
| `heed` | Meilisearch maintainer | 0.23.0 (2026-10-22) | 2.7 M | MIT | 0.x | LMDB |
| `turso` / `libsql` | Turso | 0.8.3 / 0.10.0 | 697 k / 845 k | MIT | 0.x, 125 rel/yr (turso) | SQLite rewrite or fork; too young |
| `tantivy` | Quickwit | 0.27.0 (2026-10-17) | 4.9 M | MIT | 0.x | **121 crates**. Lucene-style search engine |
| `fst` | BurntSushi | 0.4.7 (2021-06-06) | 8.0 M | Unlicense OR MIT | 0.4, inactive but complete | 1 crate. Keyboard dictionaries (text input research) |

**Recommendation: `rusqlite` on SQLite, with full-text search through SQLite's FTS5**, not `tantivy`. SQLite is the most standard embedded database on Linux, and the library can be inspected with the `sqlite3` tool. FTS5 keeps search in the same transactional store at a fraction of `tantivy`'s footprint. Whether to link Debian's `libsqlite3` (the standard, security-patched system library) or the `bundled` copy is open choice O5. Keep `fst` for keyboard dictionaries, as proposed.

### 4.27 File watching

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `notify` | notify-rs org | 8.2.0 (2025-08-03); 9.0.0-rc.6 (2026-10-17) | 47.4 M | CC0-1.0 | 8.x | 10 crates. inotify, FSEvents, ReadDirectoryChangesW |
| `notify-debouncer-full` | notify-rs org | 0.7.0 (2026-01-23) | 5.7 M | MIT OR Apache-2.0 | 0.x | Rename-aware debouncing |
| `inotify` | one maintainer | 0.11.5 (2026-08-14) | 48.5 M | ISC | 0.x | Linux only |

**Recommendation: `notify` with `notify-debouncer-full`**, on device and host. Move to 9.0 when it leaves release candidate.

### 4.28 Internationalisation

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `icu` (ICU4X) | Unicode Consortium | 2.4.0 (2026-10-22) | 710 k (`icu_locale` 16.1 M) | Unicode-3.0 | 2.x, MSRV 1.88 | 60 crates. CLDR 49, Unicode 18 |
| `fluent-bundle` | Project Fluent (Mozilla) | 0.16.0 (2025-05-22) | 7.8 M | Apache-2.0 OR MIT | 0.x, 0 rel/yr | 20 crates. 21.8 M total downloads, as the text input research quoted |
| `gettext-rs` | two maintainers | 0.8.0 (2026-08-05) | 652 k | MIT | 0.x | Binds GNU gettext. RUSTSEC-2026-0244 (unsound `setlocale`, fixed ≥ 0.8.0) |
| `gettext` | one maintainer | 0.4.0 (2019-06-04) | 435 k | MIT | inactive | Pure-Rust `.mo` reader |
| `i18n-embed` / `fluent-templates` / `rust-i18n` | single maintainers | 0.16.0 / 0.15.1 / 4.2.4 | 4.2 M / 236 k / 1.5 M | MIT (some dual) | 0.x | Convenience layers |
| `mf2` | one maintainer | 3.0.0 (2026-10-08) | under 1 k | MIT AND Unicode-3.0 | — | MessageFormat 2; too early (text input research agrees) |

**Recommendation: `icu` and `fluent-bundle`**, as the text input research and ADR 0007 propose. `fluent-bundle` has had no release in a year, which is acceptable for a stable format but worth watching. No convenience layer is needed: `opendap-sdk` owns loading per pack.

### 4.29 Keyring and secrets (host)

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `keyring` | open-source-cooperative org | 4.2.0 (2026-08-29) | 15.5 M | MIT OR Apache-2.0 | 4.x, MSRV 1.88 | 101 crates on Linux (Secret Service via `zbus`); macOS Keychain via `security-framework` |
| `secret-service` | same maintainers | 5.2.0 (2026-08-29) | 9.7 M | MIT OR Apache-2.0 | 5.x | Linux backend |
| `oo7` | GNOME (linux-credentials) | 0.6.0 (2026-02-21) | 2.0 M | MIT | 0.x | Linux only |
| `security-framework` | kornelski, djc | 3.7.0 (2026-02-20) | 101.1 M | MIT OR Apache-2.0 | 3.x | macOS only |

**Recommendation: `keyring`**, opt-in, as in the agentic CLI research. It is the only portable choice with an organisation behind it. Its Linux tree is large (101 crates), so keep it behind a Cargo feature of the credentials adapter.

### 4.30 MCP server

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `rmcp` | Model Context Protocol org (official Rust SDK) | 3.6.0 (2026-10-20) | 21.7 M | Apache-2.0 | 3.x, but **majors 1.0 (2026-03-03), 2.0 (2026-06-29), 3.0 (2026-07-23)**; 47 rel/yr | 60 crates with `server` + `transport-io`. RUSTSEC-2026-0189 (DNS rebinding in Streamable HTTP, fixed ≥ 1.4.0) |
| `rust-mcp-sdk` | one maintainer | 2.0.0 (2026-08-27) | 115 k | MIT | — | Unofficial |

**Recommendation: `rmcp`**, the official SDK, with stdio transport first. Contain its API churn inside one adapter module of `opendap-cli-kit`.

### 4.31 Testing

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `insta` | mitsuhiko, max-sixty | 1.49.0 (2026-10-03) | 33.6 M | Apache-2.0 | 1.x | 12 crates. Snapshot tests (JSON, help text) with `cargo insta review` |
| `proptest` | proptest-rs org | 1.12.0 (2026-10-10) | 61.2 M | MIT OR Apache-2.0 | 1.x | 25 crates. Shrinking property tests (frames, parsers) |
| `quickcheck` | BurntSushi | 1.1.0 (2026-02-10) | 13.6 M | Unlicense OR MIT | 1.x | Simpler, weaker shrinking |
| `assert_cmd` / `predicates` | rust-cli WG (assert-rs) | 2.2.2 / 3.1.4 | 20.1 M / 44.9 M | MIT OR Apache-2.0 | 2.x / 3.x | 12 crates. Run the built binary |
| `snapbox` / `trycmd` | rust-cli WG | 1.2.2 / 1.2.1 | 3.8 M / 2.3 M | MIT OR Apache-2.0 | 1.x | Snapshot-style CLI tests (`trycmd` reads `.md` cases) |
| `tempfile` / `assert_fs` | Stebalien / rust-cli WG | 3.27.0 / 1.1.4 | 218.6 M / 4.2 M | MIT OR Apache-2.0 | 3.x / 1.x | Temporary files and fixtures |
| `rstest` | one maintainer | 0.28.0 (2026-10-18) | 32.2 M | MIT OR Apache-2.0 | 0.x | Fixtures and parametrised tests |
| `mockall` | one maintainer | 0.15.0 (2026-06-28) | 31.3 M | MIT OR Apache-2.0 | 0.x | Not needed: the spec prefers fake adapters to mocks |

**Recommendation: `insta`, `proptest`, `assert_cmd` (with `predicates`) and `tempfile`.** These cover the spec's unit, contract and CLI tests with mature 1.x+ crates. `cargo-nextest` is a test runner (a tool, not a dependency) and is optional.

### 4.32 Benchmarking

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `criterion` | criterion-rs org + rust-bus | 0.8.2 (2026-02-04) | 69.2 M | Apache-2.0 OR MIT | 0.x | 52 crates. Statistical wall-clock benchmarks; the standard |
| `divan` | one maintainer | 0.1.21 (2025-04-10) | 4.3 M | MIT OR Apache-2.0 | 0.x, 0 rel/yr | 18 crates. Lighter, inactive |
| `gungraun` (formerly `iai-callgrind`) | two maintainers | 0.20.0 (2026-09-26) | 769 k | Apache-2.0 OR MIT | 0.x | Instruction counts via Valgrind; Linux only. Useful on the device for stable numbers (**UNVERIFIED** on aarch64) |

**Recommendation: `criterion`** for the bus and data-plane performance gates (system architecture research §5.16). Add `gungraun` later if wall-clock noise on the A55 makes the gate unreliable.

### 4.33 FFI and code generation

| Crate | Backer | Latest (date) | Recent DL | Licence | Maturity | Notes |
|---|---|---|---|---|---|---|
| `bindgen` | rust-lang org | 0.73.2 (2026-09-08) | 89.1 M | BSD-3-Clause | 0.x | C headers → Rust; needs libclang at build time |
| `cc` | rust-lang org | 1.9.0 (2026-10-19) | 330.3 M | MIT OR Apache-2.0 | 1.x | Builds C sources |
| `pkg-config` / `system-deps` | rust-lang org / GStreamer maintainers | 0.3.34 / 9.0.1 | 215.2 M / 24.8 M | MIT OR Apache-2.0 | 0.x / 9.x | Find system libraries (alsa-lib, libsqlite3, libpipewire) |
| `cxx` | dtolnay | 1.0.205 (2026-10-11) | 11.9 M | MIT OR Apache-2.0 | 1.x | Safe C++ interop. RUSTSEC-2026-0202 (fixed ≥ 1.0.195) |
| `cbindgen` | Mozilla | 0.29.4 (2026-06-09) | 18.0 M | MPL-2.0 | 0.x | Rust → C headers |
| `uniffi` | Mozilla | 0.32.2 (2026-09-23) | 5.2 M | MPL-2.0 | 0.x | Rust → Kotlin/Swift/Python |

**Recommendation: `bindgen`, `cc` and `pkg-config` when a C library has no maintained binding** (librime in the text input research is the known case). `cxx` is not needed: no C++ dependency is planned. `cbindgen` and `uniffi` only if a non-Rust SDK is ever opened as a topic. Per CLAUDE.md, that is not proposed here.

### 4.34 Added areas

These areas were not in the initial list but appear in the accepted architecture or the research.

| Need | Crate | Backer | Latest (date) | Recent DL | Licence | Notes |
|---|---|---|---|---|---|---|
| Time, dates, expiry (TUF, manifests, logs) | `jiff` | BurntSushi | 0.2.38 (2026-10-06) | 91.5 M | Unlicense OR MIT | 0.x, 22 rel/yr. Time-zone aware; already in `tough`'s tree. Alternatives: `time` 0.3.55 (one maintainer; RUSTSEC-2026-0009 fixed), `chrono` 0.4.45 (in `rmcp`'s tree) |
| Identifiers | `uuid` | uuid-rs, rust-lang owner | 1.28.0 (2026-10-10) | 222.7 M | Apache-2.0 OR MIT | 1.x |
| UTF-8 paths | `camino` | Manishearth, sunshowers | 1.2.6 (2026-09-15) | 75.9 M | MIT OR Apache-2.0 | 1.x. Optional |
| mDNS discovery (host) | `mdns-sd` | one maintainer | 0.21.6 (2026-10-19) | 3.0 M | Apache-2.0 OR MIT | 14 crates, pure Rust. The device side is avahi-daemon (OS lifecycle research) |
| Bluetooth (BlueZ) | `bluer` | BlueZ org (official) | 0.17.4 (2025-06-06) | 808 k | BSD-2-Clause | 65 crates with `full`; no release in a year. Only when Bluetooth is opened as a topic |
| GPIO / I²C / input | `gpiocdev` / `i2cdev` / `evdev` | warthog618 / rust-embedded WG / two maintainers | 0.8.0 / 0.6.2 / 0.13.2 | 339 k / 623 k / 793 k | Apache-2.0 OR MIT | Character-device GPIO uAPI v2 (no libgpiod), i2c-dev, evdev buttons and wheel |
| Landlock | `landlock` | landlock-lsm (the LSM's maintainers) | 0.4.7 (2026-07-27) | 6.1 M | MIT OR Apache-2.0 | Self-sandboxing where systemd unit options are not enough |
| Shared memory IPC (escalation path) | `iceoryx2` | Eclipse Foundation | 0.10.0 (2026-09-18) | 478 k | MIT OR Apache-2.0 | As already noted: aarch64 tier 2, no access control |

**Recommendation:**

- `jiff` for time.
- `uuid` for identifiers.
- `mdns-sd` for host discovery.
- `gpiocdev`, `i2cdev` and `evdev` for device hardware access through the kernel's standard character devices.
- `landlock` only where systemd sandboxing does not reach.
- `bluer` stays parked until Bluetooth is a topic. `iceoryx2` stays the escalation path, as already written.

## 5. Recommended dependency set

| Area | Crate(s) | Where | Licence |
|---|---|---|---|
| Argument parsing, completions, man | `clap` (derive), `clap_complete`, `clap_mangen` | toolkit | MIT OR Apache-2.0 |
| Colour, progress | `anstream`/`anstyle`, `indicatif` | toolkit | MIT OR Apache-2.0; MIT |
| Errors | `thiserror` (libraries), `anyhow` (binaries) | all | MIT OR Apache-2.0 |
| Logging | `tracing`, `tracing-subscriber` (`env-filter`, `json`), `tracing-journald` (device) | all | MIT |
| Serialisation, schema | `serde`, `serde_json`, `toml`, `toml_edit`, `schemars` | all | MIT OR Apache-2.0; MIT |
| Data-plane frames | `zerocopy` | system | BSD-2-Clause OR Apache-2.0 OR MIT |
| Async runtime | `tokio` (≥ 1.54.1, minimal features); `calloop` in the compositor | all | MIT |
| System calls, signals, mmap | `rustix`, `signal-hook` / `tokio::signal`, `memmap2` (≥ 0.9.11) | all | Apache-2.0 OR MIT family |
| Directories, config | `etcetera` (XDG strategy) + our own layering | toolkit | MIT OR Apache-2.0 |
| HTTP | `reqwest` (`rustls`, no default features) | toolkit, `dap-fetch` | MIT OR Apache-2.0 |
| TLS, crypto | `rustls` (≥ 0.23.46) + `aws-lc-rs`, `rustls-platform-verifier`, `ed25519-dalek`, `minisign-verify`, `sha2` | all | permissive (§4.10) |
| TUF | `tough` | device, toolkit | MIT OR Apache-2.0 |
| Compression | `zstd`, `flate2`, `liblzma` (if xz is needed) | toolkit, device | BSD-3-Clause; MIT OR Apache-2.0 |
| Archives, flashing images | `tar` (≥ 0.4.46), `bmap-parser` | toolkit | MIT OR Apache-2.0 |
| SSH | `openssh` (+ `ssh-key` for pairing keys) | toolkit (Unix hosts) | MIT OR Apache-2.0 |
| USB, MaskROM | `nusb`, `rockusb` | toolkit | Apache-2.0 OR MIT |
| QMP | our own thin client (`serde_json` + `tokio`) | toolkit | — |
| Serial | `serialport` | toolkit | MPL-2.0 |
| Varlink | `zlink` | system, toolkit | MIT |
| D-Bus | `zbus` | system | MIT |
| systemd | `sd-notify`, `tracing-journald`; units over `zbus` | system | MIT OR Apache-2.0; MIT |
| Wayland compositor | `smithay` (pin; see O2) | system | MIT |
| UI | `slint` (+ `accesskit` through it) | system | GPL-3.0-only |
| Images | `image` (`png`, `jpeg`, `webp` only) | system | MIT OR Apache-2.0 |
| Audio output | `alsa` (exclusive), `pipewire` (shared), `audio_thread_priority` | system | Apache-2.0 OR MIT; MIT; MPL-2.0 |
| Decoding | `symphonia` (PCM codecs) + our own DSF/DFF reader and DSD converter | system | MPL-2.0 |
| Resampling | `rubato` | system | MIT OR Apache-2.0 |
| EQ, spectra | our own RBJ biquads; `realfft` | system | — ; MIT |
| Tags | `lofty` + `id3` (DSF/DFF) | system | MIT OR Apache-2.0; MIT |
| Library database, search | `rusqlite` + SQLite FTS5 | system | MIT (SQLite: public domain) |
| Keyboard dictionaries | `fst` | system | Unlicense OR MIT |
| File watching | `notify` + `notify-debouncer-full` | system, toolkit | CC0-1.0; MIT OR Apache-2.0 |
| i18n | `icu`, `fluent-bundle` | system | Unicode-3.0; Apache-2.0 OR MIT |
| Keyring | `keyring` (opt-in feature) | toolkit | MIT OR Apache-2.0 |
| MCP | `rmcp` (stdio) | toolkit | Apache-2.0 |
| Time, IDs | `jiff`, `uuid` | all | Unlicense OR MIT; Apache-2.0 OR MIT |
| mDNS (host) | `mdns-sd` | toolkit | Apache-2.0 OR MIT |
| Hardware | `gpiocdev`, `i2cdev`, `evdev` | system | Apache-2.0 OR MIT |
| Testing | `insta`, `proptest`, `assert_cmd` + `predicates`, `tempfile` | all (dev) | Apache-2.0; MIT OR Apache-2.0 |
| Benchmarking | `criterion` | all (dev) | Apache-2.0 OR MIT |
| FFI | `bindgen`, `cc`, `pkg-config` (only for C libraries without bindings) | as needed | BSD-3-Clause; MIT OR Apache-2.0 |

## 6. Reconciliation with earlier research

**Agreements, re-checked on 2026-10-22 with the same numbers:**

- `zlink` 0.7.1 and `zbus` 6.0.0 (system architecture).
- `schemars` 1.2.3, `clap` 4.6.7, `clap_complete` 4.6.11, `clap_mangen` 0.3.3, `anstream` 1.0.0, `rmcp` 3.6.0, `keyring` 4.2.0, `etcetera` 0.11.0, `minisign-verify` 0.3.0, `nusb` 0.2.8 and `rockusb` 0.4.0 (agentic CLI research).
- `ed25519-dalek` 3.0.0 (OS lifecycle).
- `icu` 2.4.0 and `fluent-bundle` 0.16.0 (text input).
- `rubato` 5.0.1, `soxr` 0.6.0 (LGPL-2.1) and `dsd-reader` 0.4.0 (DSD512).
- `symphonia` 0.6.1 without DSD, and `ciborium` as stale (system architecture).
- `slint` 1.18.1 and `smithay` 0.7.0 (Slint research).

**Closed or corrected:**

1. **`symphonia` licence.** System architecture §4.7 left it "UNVERIFIED this session". It is **MPL-2.0** (crates.io, 0.6.1).
2. **`tough` version.** Left UNVERIFIED in system architecture §5.14 and §10. It is **0.24.0, 2026-07-10**, and resolves to 107 crates.
3. **"tracing … OpenTelemetry hooks" in `opendap-sdk`** (system architecture §5.15). This report keeps structured logs free of OpenTelemetry, and none of the recommended crates needs it. The SDK should expose `tracing` spans and fields and leave exporters out. Flagged so the two documents do not drift apart.
4. **`tough` versus "a deliberately smaller subset".** The concern about weight is valid in isolation (107 crates), but most of that tree is shared with `reqwest`, `rustls` and `tokio`, which the device needs anyway. The recommendation is `tough` without reservation.

**Disagreements and tensions to flag:**

1. **Varlink reference implementation.** The criteria of this report rank institutional backing first, including "the protocol's own reference implementation". That is the `varlink` crate (varlink org), not `zlink`. This report still recommends `zlink`, consistent with ADR 0002, because `varlink` is far less active and IDL-first. The reasoning is recorded here so the choice does not look like an oversight.
2. **Message format: Fluent or gettext.** The system architecture research (§4.4, "gettext-style `@tr()`") describes Slint's mechanism. The text input research recommends Fluent "or gettext if one format with Slint matters more". These are two formats for one product. This report recommends `fluent-bundle` for SDK and system strings and keeping `@tr()` out of the generated `.slint`. That is a maintainer decision (O6). `gettext-rs` also had an unsoundness advisory in 2026 (RUSTSEC-2026-0244, fixed in 0.8.0).
3. **SSH client.** The agentic CLI research names no SSH crate ("SSH client" in the size estimate). This report recommends `openssh` (system OpenSSH), which is Unix-only. That is fine for the toolkit but conflicts with "Windows-possible" for OpenDAP Desktop (crate spec §4) if the Desktop needs SSH (O1).
4. **QMP.** The CLI spec's backend table names `blackbird-os/build/dap-qmp.py`, and the research assumes a Rust QMP path. No maintained crate exists, so this report proposes our own small client rather than `qapi`.
5. **Smithay master versus release.** The text input research relies on input-method hooks that exist only on Smithay master. A git dependency conflicts with the "semantic versioning for every library" convention (crate spec §5) for any crate we publish (O2).

## 7. Open choices for the maintainers

- **O1. SSH on OpenDAP Desktop for Windows.** Either `openssh` everywhere with no Windows SSH, or `russh` behind the transport port for Windows only. `russh` brings 143 crates, has one maintainer and publishes a breaking minor about every six weeks.
- **O2. Smithay dependency.** Pin a git revision of master (gains the input-method hooks; cosmic-comp does the same), or stay on 0.7.0 until a release.
- **O3. CBOR.** Not needed while the control plane is JSON (ADR 0002). If it ever is: `minicbor` (active; BlueOak-1.0.0, GPL compatibility to confirm) or `ciborium` (Apache-2.0, stale).
- **O4. HTTP in synchronous tools.** `reqwest` everywhere (one stack), or `ureq` for small synchronous tools (30 crates, but it brings `ring` alongside `aws-lc-rs`).
- **O5. SQLite linkage on the device.** Debian's system `libsqlite3` (standard, patched by Debian security updates) or `rusqlite`'s `bundled` copy (version pinned by us, FTS5 guaranteed).
- **O6. One message format.** Fluent for everything (with `@tr()` unused in generated `.slint`), or gettext to align with Slint's built-in translation.
- **O7. Biquad EQ.** Our own `f64` RBJ cookbook implementation (recommended, for auditability) or the `biquad` crate.

## 8. Sources

All read on 2026-10-22.

- crates.io API: `https://crates.io/api/v1/crates/<name>` and `/owners` for every crate named above.
- GitHub API: `gh api repos/<owner>/<repo>` for the main repositories (stars, open issues, last push). READMEs and sources read for specific claims:
  - [openssh `src/lib.rs`](https://github.com/openssh-rust/openssh/blob/master/src/lib.rs) ("only works on unix", `compile_error!`);
  - [sd-notify `src/lib.rs`](https://github.com/lnicola/sd-notify/blob/master/src/lib.rs) (public functions);
  - [lofty README](https://github.com/Serial-ATA/lofty-rs) (supported formats);
  - [Symphonia README](https://github.com/pdeljanov/Symphonia) (format and codec status);
  - [aws-lc-rs README](https://github.com/aws/aws-lc-rs/blob/main/aws-lc-rs/README.md) (build requirements);
  - [tough `Cargo.toml`](https://github.com/awslabs/tough/blob/develop/tough/Cargo.toml) (`http` feature uses `reqwest` 0.13 and `rustls-platform-verifier`).
- RustSec: [advisory-db](https://github.com/rustsec/advisory-db) at commit `640bf5a` (2026-10-22), `crates/<name>/RUSTSEC-*.md`; [rustsec.org](https://rustsec.org/).
- Dependency trees: `cargo tree` (cargo 1.98.1), target `aarch64-unknown-linux-gnu`, resolved 2026-10-22.
