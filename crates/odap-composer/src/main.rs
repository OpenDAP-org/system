//! `odap-composer`: the Blackbird OS composer (RFC 0012): the compositor, the input pipeline and the
//! interface primitives' runtime (system ADR 0002, ADR 0012; composer and UI runtime spec). The OS
//! Shell is a separate layer on top of it, not designed yet; the parts of it this process draws
//! today (the status bar, the power menu, toasts, the display's rules) are drafts that shape the
//! primitives.
//!
//! Bring-up scope (development workflow, milestone M1):
//! - drives the display the hardware offers through DRM/KMS, with GBM and GLES, on a logind
//!   session through libseat;
//! - shows the product host full screen, and starts it (`odap-composer -- <command>`) until the
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
        eprintln!(
            "odap-composer drives DRM/KMS and runs on Linux only; run odap-product directly on this host."
        );
        std::process::exit(2);
    }
}
