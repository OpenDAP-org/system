//! `dap-shell`: the Blackbird OS compositor (system ADR 0002, ADR 0012; shell and UI runtime spec).
//!
//! Bring-up scope (development workflow, milestone M1):
//! - drives the display the hardware offers through DRM/KMS, with GBM and GLES, on a logind
//!   session through libseat;
//! - shows the product host full screen, and starts it (`dap-shell -- <command>`) until the
//!   orchestrator does;
//! - forwards touch to it;
//! - turns button codes into roles from the device description, never from key names in code.
//!
//! Not yet: system layers, places and gestures, the bus, presentation and the other protocols of
//! spec §2. Those land milestone by milestone.

#[cfg(target_os = "linux")]
mod linux;

fn main() {
    #[cfg(target_os = "linux")]
    linux::main();
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("dap-shell drives DRM/KMS and runs on Linux only; run dap-product directly on this host.");
        std::process::exit(2);
    }
}
