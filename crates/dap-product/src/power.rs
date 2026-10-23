//! Battery level from the kernel's power-supply class (`/sys/class/power_supply`), the generic
//! path every Linux fuel-gauge driver implements. A device without a battery, or a host that is
//! not Linux, has none: the status bar then leaves it out (hardware support spec §6).

#[cfg(target_os = "linux")]
pub fn battery() -> Option<u8> {
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
            return Some(level.min(100));
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
pub fn battery() -> Option<u8> {
    None
}
