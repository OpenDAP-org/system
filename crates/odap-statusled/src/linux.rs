//! The status LED on Linux: the LED class devices the description names, the battery through
//! `power_supply` uevents, requests through inotify on `/run/opendap/led/`.

use std::{
    ffi::CString,
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use opendap_device::{Description, IndicatorColors, IndicatorRole};

use crate::{Inputs, Show, decide, parse_developer};

/// Where the device description is found, first match wins (as the shell reads it).
const DESCRIPTIONS: &[&str] = &[
    "/run/opendap/device/description.toml",
    "/usr/share/opendap/device/description.toml",
];
const REQUESTS: &str = "/run/opendap/led";
const RUN: &str = "/run/opendap";
/// The shell's runtime directory: it writes `ui-ready` there at its first frame on screen, and
/// booting ends.
const SHELL: &str = "/run/opendap/shell";

/// One channel of the LED: its brightness file and its maximum.
struct Channel {
    path: PathBuf,
    max: u32,
    last: Option<u32>,
}

impl Channel {
    fn open(name: &str) -> io::Result<Self> {
        let dir = Path::new("/sys/class/leds").join(name);
        let max = fs::read_to_string(dir.join("max_brightness"))?
            .trim()
            .parse()
            .unwrap_or(255);
        // Nothing else drives it: no trigger left from the kernel's default.
        let _ = fs::write(dir.join("trigger"), "none");
        Ok(Channel {
            path: dir.join("brightness"),
            max,
            last: None,
        })
    }

    /// `level` 0–255, scaled to the channel; written only when it changes. False when the LED
    /// is gone (unplugged, its driver rebound).
    fn set(&mut self, level: f32) -> bool {
        let v = ((level / 255.0) * self.max as f32).round() as u32;
        if self.last == Some(v) {
            return true;
        }
        let ok = fs::write(&self.path, v.to_string()).is_ok();
        self.last = ok.then_some(v);
        ok
    }
}

enum Led {
    Rgb([Channel; 3]),
    Mono(Channel),
}

impl Led {
    /// False when the LED is gone.
    fn show(&mut self, rgb: [u8; 3], brightness: f32) -> bool {
        match self {
            Led::Rgb(ch) => ch
                .iter_mut()
                .zip(rgb)
                .fold(true, |ok, (c, v)| c.set(v as f32 * brightness) && ok),
            // One colour keeps the meaning through the pattern only (§2.2).
            Led::Mono(c) => c.set(if rgb == [0; 3] { 0.0 } else { 255.0 * brightness }),
        }
    }
}

fn description() -> io::Result<Description> {
    let text = DESCRIPTIONS
        .iter()
        .find_map(|p| fs::read_to_string(p).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no device description"))?;
    Description::parse(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

/// The LED class name the description gives, or with a trailing `*`, the first that matches
/// (a driver that numbers its devices, as hid-led does: `thingm*`).
fn resolve(name: &str) -> io::Result<String> {
    let Some(prefix) = name.strip_suffix('*') else {
        return Ok(name.to_string());
    };
    let mut found: Vec<String> = fs::read_dir("/sys/class/leds")?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter_map(|n| {
            let rest = n.strip_prefix(prefix)?;
            let digits = rest.split(':').next()?;
            (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                .then(|| format!("{prefix}{digits}"))
        })
        .collect();
    found.sort();
    found
        .into_iter()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no LED matches {name}")))
}

fn led(d: &Description) -> io::Result<Led> {
    let i = d.indicator(IndicatorRole::Status).ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "the description names no status LED")
    })?;
    let name = resolve(&i.linux)?;
    Ok(match i.colors {
        IndicatorColors::Rgb => Led::Rgb([
            Channel::open(&format!("{name}:red"))?,
            Channel::open(&format!("{name}:green"))?,
            Channel::open(&format!("{name}:blue"))?,
        ]),
        IndicatorColors::Mono => Led::Mono(Channel::open(&name)?),
    })
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// The battery, the charger and every request, as they are now.
fn inputs(booting: bool) -> Inputs {
    let mut i = Inputs {
        booting,
        ..Default::default()
    };
    for supply in fs::read_dir("/sys/class/power_supply").into_iter().flatten().flatten() {
        let p = supply.path();
        match read(&p.join("type")).as_deref() {
            Some("Battery") => {
                i.battery = read(&p.join("capacity")).and_then(|c| c.parse().ok());
                i.charging = read(&p.join("status")).as_deref() == Some("Charging");
            }
            Some("Mains" | "USB" | "USB_PD") => {
                i.external |= read(&p.join("online")).as_deref() == Some("1");
            }
            _ => {}
        }
    }
    let req = |name: &str| Path::new(REQUESTS).join(name).exists();
    i.fault = req("fault");
    i.updating = req("updating");
    i.recovery = req("recovery");
    i.eject_first = req("eject-first");
    i.pairing = req("pairing");
    i.activity = req("activity");
    i.developer = read(&Path::new(REQUESTS).join("developer")).and_then(|t| parse_developer(&t));
    i
}

fn uevents() -> io::Result<i32> {
    // SAFETY: plain socket calls with a zeroed sockaddr_nl of the right size.
    unsafe {
        let fd = libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_DGRAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            libc::NETLINK_KOBJECT_UEVENT,
        );
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut addr: libc::sockaddr_nl = std::mem::zeroed();
        addr.nl_family = libc::AF_NETLINK as u16;
        addr.nl_groups = 1;
        if libc::bind(
            fd,
            (&raw const addr).cast(),
            std::mem::size_of::<libc::sockaddr_nl>() as u32,
        ) < 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(fd)
    }
}

fn inotify(dirs: &[&str]) -> io::Result<i32> {
    // SAFETY: inotify calls with valid C strings.
    unsafe {
        let fd = libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK);
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        for d in dirs {
            let c = CString::new(*d).expect("no NUL");
            let mask = libc::IN_CREATE
                | libc::IN_DELETE
                | libc::IN_CLOSE_WRITE
                | libc::IN_MOVED_TO
                | libc::IN_MOVED_FROM;
            if libc::inotify_add_watch(fd, c.as_ptr(), mask) < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(fd)
    }
}

fn watch_dir(fd: i32, dir: &str) {
    let c = CString::new(dir).expect("no NUL");
    let mask = libc::IN_CREATE | libc::IN_DELETE | libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO;
    // SAFETY: a valid descriptor and C string; failure (no such directory yet) is fine.
    unsafe { libc::inotify_add_watch(fd, c.as_ptr(), mask) };
}

/// Empties a non-blocking descriptor: whatever woke it, the state is read again (cheap).
fn drain(fd: i32) {
    let mut buf = [0u8; 8192];
    // SAFETY: reads at most the buffer's size into it.
    while unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
}

pub fn run() -> io::Result<()> {
    let d = description()?;
    // Found again whenever it goes (a uevent wakes the loop when it comes back).
    let mut status = led(&d).ok();
    fs::create_dir_all(REQUESTS)?;
    let netlink = uevents()?;
    let watch = inotify(&[REQUESTS, RUN])?;
    let ready = || Path::new(SHELL).join("ui-ready").exists();
    let mut shown: Option<Show> = None;
    let mut since = Instant::now();
    loop {
        // The shell's directory comes and goes with the shell: watched whenever it exists (adding
        // a watch twice is harmless).
        watch_dir(watch, SHELL);
        let now = decide(&inputs(!ready()));
        if now != shown {
            shown = now;
            since = Instant::now();
        }
        if status.is_none() {
            status = led(&d).ok();
        }
        let (rgb, brightness, wait) = match shown {
            None => ([0; 3], 0.0, None),
            Some(s) => {
                let (brightness, stays) = s.pattern.at(since.elapsed());
                (s.rgb, brightness, stays)
            }
        };
        if let Some(l) = status.as_mut()
            && !l.show(rgb, brightness)
        {
            status = None;
        }
        let timeout = wait.map_or(-1, |w: Duration| w.as_millis().max(1) as i32);
        let mut fds = [
            libc::pollfd { fd: netlink, events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: watch, events: libc::POLLIN, revents: 0 },
        ];
        // SAFETY: a valid array of pollfd for its length.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
        }
        if fds[0].revents & libc::POLLIN != 0 {
            drain(netlink);
        }
        if fds[1].revents & libc::POLLIN != 0 {
            drain(watch);
        }
    }
}
