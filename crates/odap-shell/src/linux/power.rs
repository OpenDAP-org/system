//! Requests to the power service (blackbird-os `odap-power.socket`): the shell's user cannot power
//! off or pick a boot mode itself; the service, as root, runs `odap-reboot` for it.

use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    time::Duration,
};

use tracing::warn;

const SOCKET: &str = "/run/opendap/power.sock";

/// One request (`list`, `poweroff`, `reboot MODE`) and the service's answer.
fn request(line: &str) -> Option<String> {
    let answer = (|| {
        let mut s = UnixStream::connect(SOCKET)?;
        s.set_read_timeout(Some(Duration::from_secs(2)))?;
        s.write_all(format!("{line}\n").as_bytes())?;
        s.shutdown(std::net::Shutdown::Write)?;
        let mut out = String::new();
        s.read_to_string(&mut out)?;
        Ok::<_, std::io::Error>(out)
    })();
    match answer {
        Ok(out) if out.starts_with("error") => {
            warn!(request = line, answer = out.trim(), "power service");
            None
        }
        Ok(out) => Some(out),
        Err(err) => {
            warn!(request = line, %err, "power service");
            None
        }
    }
}

/// The boot modes the device has.
pub fn modes() -> Vec<String> {
    request("list")
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_default()
}

pub fn poweroff() {
    request("poweroff");
}

pub fn reboot(mode: &str) {
    request(&format!("reboot {mode}"));
}
