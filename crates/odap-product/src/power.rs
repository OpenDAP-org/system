//! Battery level from the kernel's power-supply class (`/sys/class/power_supply`), the generic
//! path every Linux fuel-gauge driver implements. A device without a battery, or a host that is
//! not Linux, has none: the status bar then leaves it out (hardware support spec §6).

/// A battery's level in percent and whether it is charging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    pub level: u8,
    pub charging: bool,
}

#[cfg(target_os = "linux")]
pub fn battery() -> Option<Battery> {
    let entries = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = std::fs::read_to_string(path.join("type")).unwrap_or_default();
        if kind.trim() != "Battery" {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(path.join("capacity"))
            && let Ok(level) = text.trim().parse::<u8>()
        {
            let status = std::fs::read_to_string(path.join("status")).unwrap_or_default();
            return Some(Battery {
                level: level.min(100),
                charging: status.trim() == "Charging",
            });
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
pub fn battery() -> Option<Battery> {
    None
}
