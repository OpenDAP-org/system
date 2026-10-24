//! The status bar's state (SYS-038) and the battery's warnings, from the kernel's own classes:
//! the clock, the microSD card (a block device of type SD, never the eMMC), Bluetooth and Wi-Fi
//! (rfkill and the network devices), the battery and the charger (power_supply). Kernel uevents
//! wake it, and the clock's minute; nothing polls.

use std::{fs, path::Path, time::Duration};

use smithay::reexports::calloop::{
    Interest, Mode, PostAction,
    generic::Generic,
    timer::{TimeoutAction, Timer},
};
use tracing::{info, warn};

use super::state::Shell;

/// Battery warnings, in percent: the status LED's thresholds (odap-statusled), Android's defaults
/// (decided under the maintainers' delegation, best UX: familiar, early enough to act, 2026-10-24).
const LOW_PERCENT: u8 = 15;
const CRITICAL_PERCENT: u8 = 5;
/// How long a battery warning shows.
const WARNING_FOR: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub time: String,
    pub microsd: bool,
    pub battery: Option<u8>,
    pub charging: bool,
    /// None: no such radio; Some(on).
    pub bluetooth: Option<bool>,
    pub wifi: Option<bool>,
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn clock() -> String {
    // SAFETY: time(2) and localtime_r(3) with valid pointers to stack values.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return String::new();
        }
        format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
    }
}

/// Whether a radio of `kind` (rfkill's `bluetooth`, `wlan`) exists, and if so whether it is on
/// (neither soft- nor hard-blocked).
fn radio(kind: &str) -> Option<bool> {
    let mut found = None;
    for r in fs::read_dir("/sys/class/rfkill").into_iter().flatten().flatten() {
        let p = r.path();
        if read(&p.join("type")).as_deref() == Some(kind) {
            let on = read(&p.join("soft")).as_deref() == Some("0")
                && read(&p.join("hard")).as_deref() == Some("0");
            found = Some(found.unwrap_or(false) || on);
        }
    }
    found
}

pub fn now() -> Status {
    let mut s = Status {
        time: clock(),
        ..Default::default()
    };
    for supply in fs::read_dir("/sys/class/power_supply").into_iter().flatten().flatten() {
        let p = supply.path();
        if read(&p.join("type")).as_deref() == Some("Battery") {
            s.battery = read(&p.join("capacity")).and_then(|c| c.parse().ok());
            s.charging = read(&p.join("status")).as_deref() == Some("Charging");
        }
    }
    s.microsd = fs::read_dir("/sys/block").into_iter().flatten().flatten().any(|b| {
        b.file_name().to_string_lossy().starts_with("mmcblk")
            && read(&b.path().join("device/type")).as_deref() == Some("SD")
    });
    s.bluetooth = radio("bluetooth");
    s.wifi = radio("wlan").or_else(|| {
        fs::read_dir("/sys/class/net")
            .into_iter()
            .flatten()
            .flatten()
            .any(|n| n.path().join("wireless").exists())
            .then_some(true)
    });
    s
}

/// The warning to show when the charge goes from `before` to `after`, discharging: once when it
/// crosses each threshold, never again until it rises above it.
pub fn warning(before: Option<u8>, after: Option<u8>, charging: bool) -> Option<String> {
    let (Some(before), Some(after)) = (before, after) else {
        return None;
    };
    if charging {
        return None;
    }
    if before > CRITICAL_PERCENT && after <= CRITICAL_PERCENT {
        Some(format!("Battery critically low: {after} %"))
    } else if before > LOW_PERCENT && after <= LOW_PERCENT {
        Some(format!("Battery low: {after} %"))
    } else {
        None
    }
}

/// Reads the state again and shows it; warns when the battery crosses a threshold.
pub fn refresh(shell: &mut Shell) {
    let status = now();
    let before = shell.status.battery;
    let note = warning(before, status.battery, status.charging);
    if status != shell.status {
        shell.status = status.clone();
        if let Some(layers) = shell.layers.as_mut() {
            layers.set_status(&status);
        }
    }
    if let Some(text) = note {
        info!(%text, "battery");
        if let Some(layers) = shell.layers.as_mut() {
            layers.show_toast(&text);
        }
        let _ = shell.handle.insert_source(Timer::from_duration(WARNING_FOR), |_, _, shell| {
            if let Some(l) = shell.layers.as_mut() {
                l.hide_toast();
            }
            redraw(shell);
            TimeoutAction::Drop
        });
    }
    redraw(shell);
}

fn redraw(shell: &mut Shell) {
    if let Some(b) = shell.backend.as_mut() {
        b.schedule();
    }
    super::backend::render_if_idle(shell);
}

/// Follows the kernel's uevents and the clock's minutes.
pub fn start(shell: &mut Shell) {
    refresh(shell);
    match uevents() {
        Ok(fd) => {
            let source = Generic::new(fd, Interest::READ, Mode::Level);
            let inserted = shell.handle.insert_source(source, |_, fd, shell| {
                let mut buf = [0u8; 8192];
                // SAFETY: reads at most the buffer's size into it from a valid descriptor.
                while unsafe {
                    libc::read(
                        std::os::fd::AsRawFd::as_raw_fd(&**fd),
                        buf.as_mut_ptr().cast(),
                        buf.len(),
                    )
                } > 0
                {}
                refresh(shell);
                Ok(PostAction::Continue)
            });
            if let Err(e) = inserted {
                warn!(%e, "status: uevents");
            }
        }
        Err(e) => warn!(%e, "status: uevents"),
    }
    // The clock: woken at the start of each minute.
    let _ = shell
        .handle
        .insert_source(Timer::from_duration(to_next_minute()), |_, _, shell| {
            refresh(shell);
            TimeoutAction::ToDuration(to_next_minute())
        });
}

fn to_next_minute() -> Duration {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Duration::from_secs(60) - Duration::from_millis((now.as_millis() % 60_000) as u64)
        + Duration::from_millis(20)
}

fn uevents() -> std::io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;
    // SAFETY: plain socket calls with a zeroed sockaddr_nl of the right size; the descriptor is
    // owned once created.
    unsafe {
        let fd = libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_DGRAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            libc::NETLINK_KOBJECT_UEVENT,
        );
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let owned = std::os::fd::OwnedFd::from_raw_fd(fd);
        let mut addr: libc::sockaddr_nl = std::mem::zeroed();
        addr.nl_family = libc::AF_NETLINK as u16;
        addr.nl_groups = 1;
        if libc::bind(
            fd,
            (&raw const addr).cast(),
            std::mem::size_of::<libc::sockaddr_nl>() as u32,
        ) < 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(owned)
    }
}

#[cfg(test)]
mod tests {
    use super::warning;

    #[test]
    fn warns_once_per_threshold_while_discharging() {
        assert_eq!(warning(Some(16), Some(15), false).as_deref(), Some("Battery low: 15 %"));
        assert_eq!(warning(Some(15), Some(14), false), None, "once");
        assert_eq!(warning(Some(6), Some(5), false).as_deref(), Some("Battery critically low: 5 %"));
        assert_eq!(warning(Some(16), Some(15), true), None, "not while charging");
        assert_eq!(warning(None, Some(10), false), None, "not at the first reading");
    }
}
