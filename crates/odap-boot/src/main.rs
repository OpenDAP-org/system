//! `odap-boot`: the boot UI of Blackbird OS (shell and UI runtime spec §9). It owns the screen from
//! the first moment DRM is up until the shell takes over, so nothing but the intended screens is
//! ever seen while the OS loads (OS-035).
//!
//! Today it shows the splash screen. The same program is where the boot-mode menu, progress of
//! updates that need a reboot, file-system checks and "battery too low to start" belong, drawn
//! with the software renderer and no compositor, as the spec's recovery UI.
//!
//! Hand-over, run by the shell's unit just before the shell starts: `odap-boot handoff` asks the
//! boot UI to let the display go and waits until it has (or until it is clear no boot UI runs).
//! The boot UI releases DRM master but keeps its framebuffer on screen, so the shell, whose first
//! frames show the same splash screen, takes the display without a blank frame.

#[cfg(target_os = "linux")]
mod linux;

fn main() {
    #[cfg(target_os = "linux")]
    std::process::exit(match std::env::args().nth(1).as_deref() {
        None => linux::main(),
        Some("handoff") => linux::handoff(),
        Some(other) => {
            eprintln!("odap-boot: unknown command {other}\nusage: odap-boot [handoff]");
            2
        }
    });
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("odap-boot drives DRM/KMS and runs on Linux only");
        std::process::exit(2);
    }
}
