use std::{
    fs::{File, OpenOptions},
    os::fd::{AsFd, BorrowedFd},
    path::Path,
    time::{Duration, Instant},
};

use drm::{
    Device as _,
    buffer::Buffer as _,
    buffer::DrmFourcc,
    control::{Device as ControlDevice, ModeTypeFlags, connector},
};
use opendap_device::Description;
use opendap_splash::{COVER_ARMED_PATH, COVER_SHOWN_PATH, SHOWN_AT_PATH};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fs::inotify,
    time::{ClockId, clock_gettime},
};

/// `odap-boot handoff` creates this file to ask for the display...
const HANDOFF_PATH: &str = "/run/opendap/boot-handoff";
/// ...and the boot UI creates this one once it has released it.
const RELEASED_PATH: &str = "/run/opendap/boot-released";
/// Where the running boot UI records its process id, so a hand-over knows whether to wait.
const PID_PATH: &str = "/run/opendap/boot.pid";
/// The longest a hand-over waits for the release.
const HANDOFF_TIMEOUT: Duration = Duration::from_secs(2);
/// Where the description of the hardware is, first match wins (as the shell reads it).
const DESCRIPTIONS: &[&str] = &[
    "/run/opendap/device/description.toml",
    "/usr/share/opendap/device/description.toml",
];
/// The longest the boot UI waits for the shell before it gives the display up anyway.
const WAIT_FOR_SHELL: Duration = Duration::from_secs(60);
/// The longest a stopping shell's unit waits for the boot UI to be ready to cover it.
const COVER_TIMEOUT: Duration = Duration::from_secs(1);
/// The longest the boot UI waits for a stopping shell to let the display go.
const MASTER_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the framebuffer stays after the hand-over, for the shell's first frame to replace it.
const LINGER: Duration = Duration::from_secs(10);

struct Card(File);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl drm::Device for Card {}
impl ControlDevice for Card {}

/// Tells systemd the boot UI is ready (sd_notify, `Type=notify`): once the splash screen is on the
/// display, or once it is clear none will be shown. The shell's unit is ordered after this one, so
/// it starts only then. Sent once, when dropped at the latest.
struct Ready(bool);

impl Ready {
    fn now(&mut self) {
        if !self.0 {
            self.0 = true;
            if let Some(path) = std::env::var_os("NOTIFY_SOCKET") {
                let sock = std::os::unix::net::UnixDatagram::unbound();
                if let Err(err) = sock.and_then(|s| s.send_to(b"READY=1", &path)) {
                    log(&format!("cannot notify systemd: {err}"));
                }
            }
        }
    }
}

impl Drop for Ready {
    fn drop(&mut self) {
        self.now();
    }
}

fn log(msg: &str) {
    eprintln!("odap-boot: {msg}");
}

fn panel_size() -> Option<(u16, u16)> {
    let path = DESCRIPTIONS.iter().find(|p| Path::new(p).exists())?;
    let d = Description::parse(&std::fs::read_to_string(path).ok()?).ok()?;
    d.display.map(|d| (d.width_px as u16, d.height_px as u16))
}

fn open_card() -> Option<Card> {
    let mut cards: Vec<_> = std::fs::read_dir("/dev/dri")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    cards.sort();
    cards
        .into_iter()
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("card"))
        })
        .find_map(|p| {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(p)
                .ok()
                .map(Card)
        })
}

pub fn main() -> i32 {
    let mut ready = Ready(false);
    // A hand-over left from an earlier run must not release this one at once.
    for path in [
        HANDOFF_PATH,
        RELEASED_PATH,
        COVER_ARMED_PATH,
        COVER_SHOWN_PATH,
    ] {
        let _ = std::fs::remove_file(path);
    }
    let Some(card) = open_card() else {
        log("no display device: nothing to show");
        return 0;
    };
    let Ok(resources) = card.resource_handles() else {
        log("not a KMS device");
        return 0;
    };
    let Some(conn) = resources
        .connectors()
        .iter()
        .filter_map(|c| card.get_connector(*c, true).ok())
        .find(|c| c.state() == connector::State::Connected)
    else {
        log("no connected display");
        return 0;
    };
    let Some(crtc) = conn
        .encoders()
        .iter()
        .filter_map(|e| card.get_encoder(*e).ok())
        .flat_map(|e| resources.filter_crtcs(e.possible_crtcs()))
        .next()
    else {
        log("no CRTC for the display");
        return 0;
    };
    // The panel's own size from the description, then a kernel command-line mode, then the
    // preferred one: the same choice as the shell's, so the hand-over keeps the mode.
    let native = panel_size();
    let modes = conn.modes();
    let Some(mode) = modes
        .iter()
        .find(|m| Some(m.size()) == native)
        .or_else(|| {
            modes
                .iter()
                .find(|m| m.mode_type().contains(ModeTypeFlags::USERDEF))
        })
        .or_else(|| {
            modes
                .iter()
                .find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
        })
        .or_else(|| modes.first())
        .copied()
    else {
        log("the display has no modes");
        return 0;
    };
    let (w, h) = mode.size();
    let Ok(mut db) = card.create_dumb_buffer((w as u32, h as u32), DrmFourcc::Xrgb8888, 32) else {
        log("cannot allocate a buffer");
        return 1;
    };
    let pitch = db.pitch() as usize;
    match card.map_dumb_buffer(&mut db) {
        Ok(mut map) => {
            let pixels = opendap_splash::screen_xrgb(w as u32, h as u32, pitch);
            let n = pixels.len().min(map.as_mut().len());
            map.as_mut()[..n].copy_from_slice(&pixels[..n]);
        }
        Err(err) => {
            log(&format!("cannot map the buffer: {err}"));
            return 1;
        }
    }
    let Ok(fb) = card.add_framebuffer(&db, 24, 32) else {
        log("cannot create a framebuffer");
        return 1;
    };
    let _ = std::fs::create_dir_all("/run/opendap");
    let _ = std::fs::write(COVER_ARMED_PATH, "");
    // At boot nobody holds the display. When the boot UI covers a restart of the shell, DRM master
    // may still be on its way out of the old shell: wait for it, then show the splash screen.
    let deadline = Instant::now() + MASTER_TIMEOUT;
    while card.acquire_master_lock().is_err() {
        if Instant::now() >= deadline {
            log("the display stayed taken: nothing to show");
            return 0;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if let Err(err) = card.set_crtc(crtc, Some(fb), (0, 0), &[conn.handle()], Some(mode)) {
        log(&format!("cannot show the splash screen: {err}"));
        return 1;
    }
    let now = clock_gettime(ClockId::Monotonic);
    let _ = std::fs::write(PID_PATH, format!("{}\n", std::process::id()));
    let _ = std::fs::write(COVER_SHOWN_PATH, "");
    // The first showing of this boot starts the splash screen's minimum. A later one covers a
    // restart of the shell, which must not wait the minimum out again: the time stays.
    if !Path::new(SHOWN_AT_PATH).exists() {
        let _ = std::fs::write(
            SHOWN_AT_PATH,
            format!(
                "{}\n",
                now.tv_sec as u64 * 1000 + now.tv_nsec as u64 / 1_000_000
            ),
        );
    }
    ready.now();

    wait_for(HANDOFF_PATH, WAIT_FOR_SHELL);
    // Give DRM master up but keep the framebuffer: the shell becomes master and replaces it.
    if let Err(err) = card.release_master_lock() {
        log(&format!("cannot release the display: {err}"));
    }
    let _ = std::fs::write(RELEASED_PATH, "");
    std::thread::sleep(LINGER);
    // A later boot UI (covering a restart of the shell) may have started meanwhile: its files stay.
    let mine =
        std::fs::read_to_string(PID_PATH).is_ok_and(|p| p.trim() == std::process::id().to_string());
    if mine {
        for path in [PID_PATH, COVER_ARMED_PATH, COVER_SHOWN_PATH] {
            let _ = std::fs::remove_file(path);
        }
    }
    0
}

/// Run by the shell's unit when the shell stops: puts the boot UI on the display until the next
/// shell, unless one is already there or the system is shutting down. Before the shell is asked
/// to stop (ExecStop), it waits until the boot UI is armed, so the shell can hand the display over
/// without a blank frame; after a crash (ExecStopPost), the boot UI simply takes the free display.
pub fn cover() -> i32 {
    let stopping = std::process::Command::new("systemctl")
        .arg("is-system-running")
        .output()
        .is_ok_and(|o| o.stdout.starts_with(b"stopping"));
    // A boot UI still holding the display (not yet handed over) covers it already.
    if stopping || (boot_ui_running() && !Path::new(RELEASED_PATH).exists()) {
        return 0;
    }
    let started = std::process::Command::new("systemctl")
        .args(["start", "--no-block", "odap-boot-cover.service"])
        .status()
        .is_ok_and(|s| s.success());
    if !started {
        log("cannot start the boot UI to cover the shell");
        return 0;
    }
    if !wait_for(COVER_ARMED_PATH, COVER_TIMEOUT) {
        log("the boot UI was not ready in time to cover the shell");
    }
    0
}

/// Whether a boot UI is running: its process id is recorded and alive.
fn boot_ui_running() -> bool {
    std::fs::read_to_string(PID_PATH)
        .ok()
        .and_then(|p| p.trim().parse::<u32>().ok())
        .is_some_and(|pid| Path::new(&format!("/proc/{pid}")).exists())
}

/// The shell side of the hand-over: ask, then wait for the release, if a boot UI is running.
pub fn handoff() -> i32 {
    let _ = std::fs::create_dir_all("/run/opendap");
    let running = boot_ui_running();
    if std::fs::write(HANDOFF_PATH, "").is_err() {
        log("cannot ask for the display");
        return 0;
    }
    if running && !wait_for(RELEASED_PATH, HANDOFF_TIMEOUT) {
        log("the boot UI did not release the display in time");
    }
    0
}

/// Waits until `path` appears in /run/opendap, through inotify, or until `timeout` passes.
fn wait_for(path: &str, timeout: Duration) -> bool {
    if Path::new(path).exists() {
        return true;
    }
    let Ok(fd) = inotify::init(inotify::CreateFlags::CLOEXEC) else {
        std::thread::sleep(timeout);
        return Path::new(path).exists();
    };
    if inotify::add_watch(
        &fd,
        "/run/opendap",
        inotify::WatchFlags::CREATE | inotify::WatchFlags::MOVED_TO,
    )
    .is_err()
    {
        std::thread::sleep(timeout);
        return Path::new(path).exists();
    }
    let deadline = std::time::Instant::now() + timeout;
    let mut buf = [0u8; 1024];
    while !Path::new(path).exists() {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return false;
        }
        let timeout = Timespec {
            tv_sec: left.as_secs() as _,
            tv_nsec: left.subsec_nanos() as _,
        };
        let mut fds = [PollFd::new(&fd, PollFlags::IN)];
        if poll(&mut fds, Some(&timeout)).is_ok_and(|n| n > 0) {
            let _ = rustix::io::read(&fd, &mut buf);
        }
    }
    true
}
